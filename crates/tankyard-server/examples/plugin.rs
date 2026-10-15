use tankyard_core::{
    World,
    modding::{Commands, HookEvent, ServerPlugin},
};
use tankyard_mods::{Effect, Trigger};
use tankyard_server::{ServerSettings, UdpServer};

struct HealOnKill;
impl ServerPlugin for HealOnKill {
    fn id(&self) -> &str {
        "heal_on_kill"
    }
    fn on_event(
        &mut self,
        _world: &World,
        event: HookEvent,
        commands: &mut Commands,
    ) -> Result<(), String> {
        if event.trigger == Trigger::Kill {
            commands.push(event.actor, Effect::Heal { amount: 15.0 })?;
        }
        Ok(())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut server = UdpServer::bind(ServerSettings::default())?;
    server.register_plugin(|| Box::new(HealOnKill))?;
    let mut previous = std::time::Instant::now();
    loop {
        let now = std::time::Instant::now();
        server.update(now.duration_since(previous))?;
        previous = now;
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
