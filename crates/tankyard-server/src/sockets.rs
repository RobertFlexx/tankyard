use std::net::UdpSocket;

/// Absorb short scheduling bursts without requiring privileged OS tuning.
/// OS limits vary; failing to enlarge a buffer must not prevent a connection.
pub(crate) fn tune_udp(socket: &UdpSocket, bytes: usize) {
    let socket = socket2::SockRef::from(socket);
    for result in [
        socket.set_recv_buffer_size(bytes),
        socket.set_send_buffer_size(bytes),
    ] {
        if let Err(error) = result {
            log::debug!("Could not enlarge UDP buffer: {error}");
        }
    }
}
