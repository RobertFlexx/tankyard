use crate::{NetError, ServerSettings, UdpServer};
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

enum HostCommand {
    Shutdown,
    Admin(String, SyncSender<String>),
    PlanParty(PartyPlan, SyncSender<Result<(), NetError>>),
}

/// What only the party leader's own computer can give the next round: the
/// packs on its disk, and which bosses a Boss Rush fights. It takes effect
/// with the leader's next `ConfigureParty`, all together or not at all.
#[derive(Clone, Debug, Default)]
pub struct PartyPlan {
    pub mods: Vec<PathBuf>,
    pub boss_rush: Vec<tankyard_core::BossKind>,
}

/// Player-hosted matches run exactly the dedicated-server transport and
/// simulation on a small worker thread. A bounded command queue prevents a
/// stalled host from accumulating arbitrary UI work.
pub struct HostedServer {
    pub address: SocketAddr,
    sender: SyncSender<HostCommand>,
    worker: Option<JoinHandle<()>>,
    error: Arc<Mutex<Option<String>>>,
}
impl HostedServer {
    pub fn start(settings: ServerSettings) -> Result<Self, NetError> {
        Self::start_sharing(settings, None)
    }

    /// Like [`Self::start`], with bots learning into `memory` when given.
    pub fn start_sharing(
        settings: ServerSettings,
        memory: Option<tankyard_ai::SharedKnowledge>,
    ) -> Result<Self, NetError> {
        let mut server = UdpServer::bind(settings)?;
        if let Some(memory) = memory {
            server.share_bot_memory(memory);
        }
        let address = server.address;
        let (sender, receiver) = mpsc::sync_channel(32);
        let error = Arc::new(Mutex::new(None));
        let worker_error = error.clone();
        let worker = thread::Builder::new()
            .name("tankyard-server".into())
            .spawn(move || {
                let mut previous = Instant::now();
                loop {
                    for command in receiver.try_iter().take(32) {
                        match command {
                            HostCommand::Shutdown => {
                                server.shutdown();
                                return;
                            }
                            HostCommand::Admin(text, reply) => {
                                let result = server.admin_command(&text);
                                let _ = reply.try_send(result.message);
                                if result.shutdown {
                                    return;
                                }
                            }
                            HostCommand::PlanParty(plan, reply) => {
                                let _ = reply.try_send(server.plan_party(plan));
                            }
                        }
                    }
                    let now = Instant::now();
                    if let Err(e) = server.update(now.duration_since(previous)) {
                        if let Ok(mut error) = worker_error.lock() {
                            *error = Some(e.to_string());
                        }
                        log::error!("hosted server stopped: {e}");
                        break;
                    }
                    previous = now;
                    thread::sleep(Duration::from_millis(2));
                }
            })?;
        Ok(Self {
            address,
            sender,
            worker: Some(worker),
            error,
        })
    }
    pub fn command(&self, command: &str) -> Result<String, NetError> {
        self.command_async(command)?
            .recv_timeout(Duration::from_secs(2))
            .map_err(|e| NetError(format!("host did not respond: {e}")))
    }
    pub fn command_async(&self, command: &str) -> Result<mpsc::Receiver<String>, NetError> {
        if command.len() > 512 {
            return Err(NetError("host command is too long".into()));
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        self.sender
            .try_send(HostCommand::Admin(command.into(), sender))
            .map_err(|e| NetError(format!("host command could not be queued: {e}")))?;
        Ok(receiver)
    }
    /// Line up `plan` for the party's next `ConfigureParty`.
    pub fn plan_party(&self, plan: PartyPlan) -> Result<(), NetError> {
        self.plan_party_async(plan)?
            .recv_timeout(Duration::from_secs(6))
            .map_err(|e| NetError(format!("host did not respond: {e}")))?
    }

    /// Queue the plan without blocking the client while the server loads
    /// packs. The reply confirms preparation; it never starts a round.
    pub fn plan_party_async(
        &self,
        plan: PartyPlan,
    ) -> Result<mpsc::Receiver<Result<(), NetError>>, NetError> {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.sender
            .try_send(HostCommand::PlanParty(plan, sender))
            .map_err(|e| NetError(format!("host command could not be queued: {e}")))?;
        Ok(receiver)
    }
    pub fn error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|e| e.clone())
    }
    pub fn shutdown(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = self.sender.send(HostCommand::Shutdown);
            let _ = worker.join();
        }
    }
}
impl Drop for HostedServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}
