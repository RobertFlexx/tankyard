//! Public-address discovery and leased UDP forwarding. No invite or credential
//! is sent to a lookup service. Router HTTP and XML stay off the render thread.
use std::{
    io::Read,
    net::{IpAddr, SocketAddr, UdpSocket},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

const LEASE: u32 = 600;
const RESPONSE_LIMIT: u64 = 32 * 1024;
static RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug)]
pub struct Status {
    pub address: Option<SocketAddr>,
    pub message: String,
}

pub struct Discovery {
    updates: Receiver<Status>,
    stop: Option<SyncSender<()>>,
}
impl Discovery {
    pub fn start(local: SocketAddr) -> Self {
        let (sender, updates) = mpsc::sync_channel(4);
        let (stop, cancelled) = mpsc::sync_channel(1);
        if RUNNING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            let _ = sender.try_send(Status { address: None, message: "Previous router request is finishing. Set a public IP:PORT manually, or recreate the party shortly.".into() });
            return Self {
                updates,
                stop: None,
            };
        }
        let failure = sender.clone();
        if std::thread::Builder::new()
            .name("tankyard-party-network".into())
            .spawn(move || {
                struct Reset;
                impl Drop for Reset {
                    fn drop(&mut self) {
                        RUNNING.store(false, Ordering::Release);
                    }
                }
                let _reset = Reset;
                run(local, &sender, &cancelled);
            })
            .is_err()
        {
            RUNNING.store(false, Ordering::Release);
            let _=failure.try_send(Status { address:None,message:"Internet setup could not start. Enter your public IP:PORT and configure UDP forwarding manually.".into() });
        }
        Self {
            updates,
            stop: Some(stop),
        }
    }
    pub fn poll(&self) -> Option<Status> {
        self.updates.try_iter().last()
    }
}
impl Drop for Discovery {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.try_send(());
        }
    }
}

fn cancelled(stop: &Receiver<()>) -> bool {
    !matches!(stop.try_recv(), Err(mpsc::TryRecvError::Empty))
}
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(4)))
        .max_redirects(0)
        .max_response_header_size(8192)
        .proxy(None)
        .build()
        .into()
}
fn body(response: &mut ureq::http::Response<ureq::Body>) -> Result<String, ()> {
    let mut text = String::new();
    response
        .body_mut()
        .as_reader()
        .take(RESPONSE_LIMIT + 1)
        .read_to_string(&mut text)
        .map_err(|_| ())?;
    if text.len() as u64 > RESPONSE_LIMIT {
        return Err(());
    }
    Ok(text)
}
pub fn public_ipv4(ip: IpAddr) -> bool {
    let IpAddr::V4(ip) = ip else {
        return false;
    };
    let [a, b, c, _] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_documentation()
        || a == 0
        || a >= 240
        || (a == 100 && (64..=127).contains(&b))
        || (a == 198 && (18..=19).contains(&b))
        || (a == 192 && b == 0 && c == 0))
}
fn lookup(stop: &Receiver<()>) -> Option<IpAddr> {
    for url in ["https://api4.ipify.org", "https://ipv4.icanhazip.com"] {
        if cancelled(stop) {
            return None;
        }
        if let Ok(mut response) = agent().get(url).call()
            && let Ok(text) = body(&mut response)
            && text.len() <= 64
            && let Ok(ip) = text.trim().parse::<IpAddr>()
            && public_ipv4(ip)
        {
            return Some(ip);
        }
    }
    None
}

struct Gateway {
    addr: SocketAddr,
    control_url: String,
    service: String,
}

fn parse_xml(xml: &str) -> Result<xmltree::Element, ()> {
    if xml.contains("<!DOCTYPE") || xml.contains("<!ENTITY") {
        return Err(());
    }
    let mut depth = 0usize;
    for (index, event) in xml::reader::EventReader::new(xml.as_bytes())
        .into_iter()
        .enumerate()
    {
        if index > 4096 {
            return Err(());
        }
        match event.map_err(|_| ())? {
            xml::reader::XmlEvent::StartElement { .. } => {
                depth += 1;
                if depth > 32 {
                    return Err(());
                }
            }
            xml::reader::XmlEvent::EndElement { .. } => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    xmltree::Element::parse(xml.as_bytes()).map_err(|_| ())
}

/// Accept only numeric HTTP endpoints on the responding LAN device. A forged
/// SSDP Location cannot make this worker fetch arbitrary public/loopback URLs.
fn device_url(url: &str, source: IpAddr) -> Option<(SocketAddr, String)> {
    if url.len() > 1024 || !matches!(source,IpAddr::V4(ip) if ip.is_private() || ip.is_link_local())
    {
        return None;
    }
    let uri: ureq::http::Uri = url.parse().ok()?;
    if uri.scheme_str() != Some("http") || uri.host()?.parse::<IpAddr>().ok()? != source {
        return None;
    }
    let port = uri.port_u16().unwrap_or(80);
    if port == 0 {
        return None;
    }
    Some((
        SocketAddr::new(source, port),
        uri.path_and_query()
            .map_or("/", |path| path.as_str())
            .to_owned(),
    ))
}
fn service(root: &xmltree::Element) -> Option<(&str, String)> {
    if root.name == "service" {
        let kind = root.get_child("serviceType")?.get_text()?;
        if [
            "urn:schemas-upnp-org:service:WANIPConnection:1",
            "urn:schemas-upnp-org:service:WANIPConnection:2",
            "urn:schemas-upnp-org:service:WANPPPConnection:1",
        ]
        .contains(&kind.as_ref())
        {
            return Some((
                root.get_child("serviceType")?.children.first()?.as_text()?,
                root.get_child("controlURL")?.get_text()?.into_owned(),
            ));
        }
    }
    root.children
        .iter()
        .find_map(|node| node.as_element().and_then(service))
}
fn discover(local: SocketAddr, stop: &Receiver<()>) -> Option<Gateway> {
    let socket = UdpSocket::bind(SocketAddr::new(local.ip(), 0)).ok()?;
    socket.set_multicast_ttl_v4(2).ok()?;
    socket.send_to(b"M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\n\r\n","239.255.255.250:1900").ok()?;
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut visited = std::collections::HashSet::new();
    for _ in 0..16 {
        if cancelled(stop) || Instant::now() >= deadline {
            break;
        }
        socket
            .set_read_timeout(Some(
                deadline
                    .saturating_duration_since(Instant::now())
                    .max(Duration::from_millis(1)),
            ))
            .ok()?;
        let mut packet = [0u8; 2048];
        let (count, source) = socket.recv_from(&mut packet).ok()?;
        let Ok(text) = std::str::from_utf8(&packet[..count]) else {
            continue;
        };
        let Some(location) = text.lines().find_map(|line| {
            let (header, value) = line.split_once(':')?;
            header
                .eq_ignore_ascii_case("location")
                .then_some(value.trim())
        }) else {
            continue;
        };
        if !visited.insert(location.to_owned()) {
            continue;
        }
        let Some((addr, path)) = device_url(location, source.ip()) else {
            continue;
        };
        let Ok(mut response) = agent().get(format!("http://{addr}{path}")).call() else {
            continue;
        };
        let Some(root) = body(&mut response)
            .ok()
            .and_then(|xml| parse_xml(&xml).ok())
        else {
            continue;
        };
        let Some((kind, control)) = service(&root) else {
            continue;
        };
        let url = if control.starts_with("http://") {
            control
        } else if control.starts_with('/') {
            format!("http://{addr}{control}")
        } else {
            format!(
                "http://{}{}/{}",
                addr,
                path.rsplit_once('/').map_or("", |(base, _)| base),
                control
            )
        };
        let Some((addr, control_url)) = device_url(&url, source.ip()) else {
            continue;
        };
        return Some(Gateway {
            addr,
            control_url,
            service: kind.into(),
        });
    }
    None
}
fn soap(gateway: &Gateway, action: &str, args: &str) -> Result<xmltree::Element, ()> {
    let payload = format!(
        "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:{action} xmlns:u=\"{}\">{args}</u:{action}></s:Body></s:Envelope>",
        gateway.service
    );
    let mut response = agent()
        .post(format!("http://{}{}", gateway.addr, gateway.control_url))
        .header("SOAPAction", format!("\"{}#{action}\"", gateway.service))
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .config()
        .http_status_as_error(false)
        .build()
        .send(payload)
        .map_err(|_| ())?;
    let xml = body(&mut response)?;
    parse_xml(&xml)
}
fn request(gateway: &Gateway, action: &str, args: &str) -> Result<xmltree::Element, ()> {
    let root = soap(gateway, action, args)?;
    if value(&root, "Fault").is_some() || value(&root, &format!("{action}Response")).is_none() {
        return Err(());
    }
    Ok(root)
}
fn value(root: &xmltree::Element, name: &str) -> Option<String> {
    if root.name == name {
        return Some(root.get_text().unwrap_or_default().into_owned());
    }
    root.children
        .iter()
        .find_map(|node| node.as_element().and_then(|child| value(child, name)))
}
struct Mapping {
    gateway: Gateway,
    port: u16,
    local: SocketAddr,
    description: String,
}
impl Mapping {
    fn leased(&self) -> bool {
        request(&self.gateway, "GetSpecificPortMappingEntry", &self.args()).is_ok_and(|root| {
            value(&root, "NewLeaseDuration")
                .and_then(|text| text.parse::<u32>().ok())
                .is_some_and(|lease| (1..=LEASE).contains(&lease))
        })
    }
    fn vacant(&self) -> bool {
        soap(&self.gateway, "GetSpecificPortMappingEntry", &self.args())
            .is_ok_and(|root| value(&root, "errorCode").as_deref() == Some("714"))
    }
    fn args(&self) -> String {
        format!(
            "<NewRemoteHost></NewRemoteHost><NewExternalPort>{}</NewExternalPort><NewProtocol>UDP</NewProtocol>",
            self.port
        )
    }
    fn renew(&self) -> Result<(), ()> {
        let args = format!(
            "{}<NewInternalPort>{}</NewInternalPort><NewInternalClient>{}</NewInternalClient><NewEnabled>1</NewEnabled><NewPortMappingDescription>{}</NewPortMappingDescription><NewLeaseDuration>{LEASE}</NewLeaseDuration>",
            self.args(),
            self.local.port(),
            self.local.ip(),
            self.description
        );
        request(&self.gateway, "AddPortMapping", &args).map(|_| ())
    }
    fn owns(&self) -> bool {
        request(&self.gateway, "GetSpecificPortMappingEntry", &self.args()).is_ok_and(|root| {
            value(&root, "NewInternalClient").as_deref() == Some(&self.local.ip().to_string())
                && value(&root, "NewInternalPort").as_deref()
                    == Some(&self.local.port().to_string())
                && value(&root, "NewPortMappingDescription").as_deref() == Some(&self.description)
        })
    }
}
impl Drop for Mapping {
    fn drop(&mut self) {
        if self.owns() {
            let _ = request(&self.gateway, "DeletePortMapping", &self.args());
        }
    }
}
fn run(local: SocketAddr, updates: &SyncSender<Status>, stop: &Receiver<()>) {
    let send = |address, message: String| {
        let _ = updates.try_send(Status { address, message });
    };
    let public = lookup(stop);
    if cancelled(stop) {
        return;
    }
    if let Some(ip) = public {
        send(
            None,
            format!(
                "Public IP {ip} found. Checking router forwarding for UDP {}…",
                local.port()
            ),
        );
    }
    let gateway = discover(local, stop);
    if cancelled(stop) {
        return;
    }
    if let Some(gateway) = gateway {
        let router_ip = request(&gateway, "GetExternalIPAddress", "")
            .ok()
            .and_then(|root| value(&root, "NewExternalIPAddress"))
            .and_then(|text| text.parse::<IpAddr>().ok());
        let ip = router_ip.filter(|ip| public_ipv4(*ip)).or(public);
        // A private/CGNAT WAN address proves another NAT exists upstream; a
        // mapping on this router alone cannot make the invite public.
        if router_ip.is_some_and(public_ipv4) && public.is_none_or(|seen| Some(seen) == router_ip) {
            let mut random = [0u8; 4];
            if ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut random).is_ok()
            {
                let port = 49152 + u16::from_le_bytes([random[0], random[1]]) % 16384;
                let mapping = Mapping {
                    gateway,
                    port,
                    local,
                    description: format!("TankYard-{:08x}", u32::from_le_bytes(random)),
                };
                // Random high ports avoid claiming another application's usual
                // listening port. Never fall back to a permanent router lease.
                if !cancelled(stop)
                    && mapping.vacant()
                    && mapping.renew().is_ok()
                    && mapping.owns()
                    && mapping.leased()
                {
                    let address = SocketAddr::new(ip.expect("public router address"), port);
                    send(Some(address), "Router forwarded this party's UDP port automatically. Firewall/ISP restrictions may still block incoming traffic.".into());
                    loop {
                        match stop.recv_timeout(Duration::from_secs(u64::from(LEASE / 2))) {
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                if !mapping.owns() || mapping.renew().is_err() || !mapping.leased()
                                {
                                    send(
                                        Some(SocketAddr::new(address.ip(), local.port())),
                                        format!(
                                            "Router forwarding expired. Forward UDP {} to {local} manually, or recreate the party.",
                                            local.port()
                                        ),
                                    );
                                    return;
                                }
                            }
                            _ => return,
                        }
                    }
                }
            }
        }
        if cancelled(stop) {
            return;
        }
        if let Some(ip) = ip {
            send(
                Some(SocketAddr::new(ip, local.port())),
                format!(
                    "Public invite ready; automatic forwarding unavailable. Forward UDP {} to {local}. Double NAT/CGNAT needs an upstream forward or a public IP from your ISP.",
                    local.port()
                ),
            );
            return;
        }
    }
    let message = if public.is_some() {
        format!(
            "Public invite ready; router did not allow automatic forwarding. Forward UDP {} to {local}, and allow Tank Yard through your firewall.",
            local.port()
        )
    } else {
        format!(
            "Public IP discovery failed. Enter your public IP:PORT and forward UDP {} to {local}. LAN play still works with {local}.",
            local.port()
        )
    };
    send(public.map(|ip| SocketAddr::new(ip, local.port())), message);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    fn reply(action: &str, fields: &str) -> String {
        format!(
            "<s:Envelope xmlns:s='s'><s:Body><{action}Response>{fields}</{action}Response></s:Body></s:Envelope>"
        )
    }
    fn router(responses: Vec<(&'static str, String)>) -> (Gateway, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let gateway = Gateway {
            addr: listener.local_addr().unwrap(),
            control_url: "/control".into(),
            service: "urn:schemas-upnp-org:service:WANIPConnection:1".into(),
        };
        let thread = std::thread::spawn(move || {
            for (action, response) in responses {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(&mut socket);
                let mut headers = String::new();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((key, value)) = line.split_once(':')
                        && key.eq_ignore_ascii_case("content-length")
                    {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                    headers.push_str(&line);
                }
                assert!(headers.contains(action), "{headers}");
                assert!(length < 8192);
                let mut payload = vec![0; length];
                reader.read_exact(&mut payload).unwrap();
                if action == "AddPortMapping" {
                    let xml = String::from_utf8(payload).unwrap();
                    assert!(xml.contains("<NewProtocol>UDP</NewProtocol>"));
                    assert!(xml.contains("<NewLeaseDuration>600</NewLeaseDuration>"));
                }
                let _ = write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.len(),
                    response
                );
            }
        });
        (gateway, thread)
    }
    #[test]
    fn forwarding_requests_are_leased_verified_and_deleted_only_when_owned() {
        let fields = "<NewInternalClient>192.168.1.5</NewInternalClient><NewInternalPort>5000</NewInternalPort><NewPortMappingDescription>TankYard-test</NewPortMappingDescription><NewLeaseDuration>598</NewLeaseDuration>";
        let entry = reply("GetSpecificPortMappingEntry", fields);
        let absent = "<s:Envelope xmlns:s='s'><s:Body><s:Fault><errorCode>714</errorCode></s:Fault></s:Body></s:Envelope>";
        let (gateway, worker) = router(vec![
            ("GetSpecificPortMappingEntry", absent.into()),
            ("AddPortMapping", reply("AddPortMapping", "")),
            ("GetSpecificPortMappingEntry", entry.clone()),
            ("GetSpecificPortMappingEntry", entry.clone()),
            ("GetSpecificPortMappingEntry", entry),
            ("DeletePortMapping", reply("DeletePortMapping", "")),
        ]);
        let mapping = Mapping {
            gateway,
            port: 56000,
            local: "192.168.1.5:5000".parse().unwrap(),
            description: "TankYard-test".into(),
        };
        assert!(mapping.vacant());
        mapping.renew().unwrap();
        assert!(mapping.owns());
        assert!(mapping.leased());
        drop(mapping);
        worker.join().unwrap();
        let foreign = reply(
            "GetSpecificPortMappingEntry",
            &fields.replace("TankYard-test", "Other-app"),
        );
        let (gateway, worker) = router(vec![
            ("GetSpecificPortMappingEntry", foreign.clone()),
            ("GetSpecificPortMappingEntry", foreign),
        ]);
        let mapping = Mapping {
            gateway,
            port: 56000,
            local: "192.168.1.5:5000".parse().unwrap(),
            description: "TankYard-test".into(),
        };
        assert!(!mapping.owns());
        drop(mapping);
        worker.join().unwrap();
    }
    #[test]
    fn router_response_bytes_are_bounded_and_permanent_leases_are_rejected() {
        let (gateway, worker) = router(vec![(
            "GetExternalIPAddress",
            "x".repeat(RESPONSE_LIMIT as usize + 1),
        )]);
        assert!(request(&gateway, "GetExternalIPAddress", "").is_err());
        worker.join().unwrap();
        let permanent = reply(
            "GetSpecificPortMappingEntry",
            "<NewLeaseDuration>0</NewLeaseDuration>",
        );
        let (gateway, worker) = router(vec![
            ("GetSpecificPortMappingEntry", permanent.clone()),
            ("GetSpecificPortMappingEntry", permanent),
        ]);
        let mapping = Mapping {
            gateway,
            port: 56000,
            local: "192.168.1.5:5000".parse().unwrap(),
            description: "TankYard-test".into(),
        };
        assert!(!mapping.leased());
        drop(mapping);
        worker.join().unwrap();
    }
    #[test]
    fn router_urls_and_xml_cannot_escape_lan_or_expand_entities() {
        let source = "192.168.1.1".parse().unwrap();
        assert!(device_url("http://192.168.1.1:1900/root.xml", source).is_some());
        for url in [
            "http://example.com/root.xml",
            "http://127.0.0.1/root.xml",
            "http://1.1.1.1/root.xml",
            "http://192.168.1.2/root.xml",
            "https://192.168.1.1/root.xml",
            "http://192.168.1.1:0/root.xml",
        ] {
            assert!(device_url(url, source).is_none(), "{url}");
        }
        assert!(parse_xml("<!DOCTYPE x [<!ENTITY boom 'expanded'>]><x>&boom;</x>").is_err());
        assert!(parse_xml(&("<x>".repeat(33) + &"</x>".repeat(33))).is_err());
        assert!(parse_xml("<!-- normal router comment --><x>1</x>").is_ok());
    }
    #[test]
    fn public_address_validation_excludes_lan_cgnat_and_special_ranges() {
        for ip in [
            "0.1.2.3",
            "10.0.0.1",
            "100.64.0.1",
            "100.127.255.255",
            "127.0.0.1",
            "169.254.1.2",
            "172.20.0.1",
            "192.168.1.2",
            "192.0.2.1",
            "198.18.1.1",
            "198.51.100.1",
            "203.0.113.2",
            "224.0.0.1",
            "255.255.255.255",
            "::1",
        ] {
            assert!(!public_ipv4(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["1.1.1.1", "8.8.8.8", "100.128.0.1"] {
            assert!(public_ipv4(ip.parse().unwrap()));
        }
    }
    #[test]
    fn small_soap_reply_is_parsed_by_element_not_substring() {
        let root = xmltree::Element::parse(&b"<s:Envelope xmlns:s='s'><s:Body><x:Reply xmlns:x='x'><NewExternalIPAddress>1.2.3.4</NewExternalIPAddress></x:Reply></s:Body></s:Envelope>"[..]).unwrap();
        assert_eq!(
            value(&root, "NewExternalIPAddress").as_deref(),
            Some("1.2.3.4")
        );
        assert!(value(&root, "Missing").is_none());
    }
}
