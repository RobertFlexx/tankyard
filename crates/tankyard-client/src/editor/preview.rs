//! Logic preview: the arena's triggers run in the editor with the same rule
//! engine servers use, with a test tank at the pointer for area triggers.
use tankyard_content::{MAP_COUNTERS, MapDef, MapTrigger};
use tankyard_core::{DT, LogicEffect, MapLogic, Occupant, V2, WorldConfig};

/// Something the preview did, kept briefly for the canvas and the log.
pub struct Event {
    pub at: f32,
    pub text: String,
    /// Where it happened, for bosses and shapes.
    pub place: Option<V2>,
}

pub struct Preview {
    pub running: bool,
    pub time: f32,
    pub speed: f32,
    pub hidden: u32,
    pub counters: [i32; MAP_COUNTERS as usize],
    /// Round time when each trigger last acted.
    pub flashes: Vec<f32>,
    pub events: Vec<Event>,
    /// The latest announcement and when it was made.
    pub banner: Option<(f32, String)>,
    pub probe_team: u8,
    pub probe_bot: bool,
    logic: MapLogic,
    acted: Vec<u32>,
    triggers: Vec<MapTrigger>,
    seed: u64,
    carry: f32,
    effects: Vec<LogicEffect>,
}

impl Preview {
    pub fn new(map: &MapDef) -> Self {
        let mut preview = Self {
            running: false,
            time: 0.,
            speed: 1.,
            hidden: 0,
            counters: [0; MAP_COUNTERS as usize],
            flashes: Vec::new(),
            events: Vec::new(),
            banner: None,
            probe_team: 0,
            probe_bot: false,
            logic: MapLogic::default(),
            acted: Vec::new(),
            triggers: Vec::new(),
            seed: 0x7e57,
            carry: 0.,
            effects: Vec::new(),
        };
        preview.restart(map);
        preview
    }
    pub fn restart(&mut self, map: &MapDef) {
        self.time = 0.;
        self.carry = 0.;
        self.logic = MapLogic::default();
        self.hidden = map.starting_mask();
        self.counters = [0; MAP_COUNTERS as usize];
        self.flashes = vec![f32::NEG_INFINITY; map.triggers.len()];
        self.acted = vec![0; map.triggers.len()];
        self.events.clear();
        self.banner = None;
        self.triggers.clone_from(&map.triggers);
        self.seed = 0x7e57;
    }
    /// Whether the preview shows the map's starting state.
    pub fn at_start(&self) -> bool {
        self.time == 0.
    }
    /// Start over when the triggers or starting groups changed under it.
    pub fn follow(&mut self, map: &MapDef) {
        if self.triggers != map.triggers || (self.at_start() && self.hidden != map.starting_mask())
        {
            self.restart(map);
        }
    }
    /// The test tank, when it is on the canvas.
    fn tanks(&self, probe: Option<V2>) -> Vec<Occupant> {
        probe
            .map(|pos| Occupant {
                id: 1,
                pos,
                bot: self.probe_bot,
                team: self.probe_team,
            })
            .into_iter()
            .collect()
    }
    /// Carry out trigger `index` now, whatever its timing, and start the
    /// preview so its results play out.
    pub fn fire(&mut self, map: &MapDef, config: &WorldConfig, index: usize, probe: Option<V2>) {
        self.follow(map);
        // Time zero counts as untouched; acting there would be undone.
        self.time = self.time.max(DT);
        self.running = true;
        let tanks = self.tanks(probe);
        let seed = &mut self.seed;
        self.logic.act_now(
            index,
            map,
            config,
            self.time,
            &mut self.hidden,
            &tanks,
            &mut || next_random(seed),
            &mut self.effects,
        );
        self.record();
        self.counters = self.logic.counters;
    }
    /// Advance by `dt` seconds of wall time, with a test tank at `probe`.
    pub fn advance(&mut self, map: &MapDef, config: &WorldConfig, dt: f32, probe: Option<V2>) {
        if !self.running {
            return;
        }
        self.carry += dt.clamp(0., 0.1) * self.speed;
        let tanks = self.tanks(probe);
        // A bounded number of fixed steps keeps slow frames from spiralling.
        let mut steps = 0;
        while self.carry >= DT && steps < 64 {
            self.carry -= DT;
            steps += 1;
            self.time += DT;
            let seed = &mut self.seed;
            self.logic.step(
                map,
                config,
                self.time,
                &mut self.hidden,
                &tanks,
                &mut || next_random(seed),
                &mut self.effects,
            );
            self.record();
        }
        if steps == 64 {
            self.carry = 0.;
        }
        self.counters = self.logic.counters;
    }
    fn record(&mut self) {
        for (index, &count) in self.logic.acted.iter().enumerate() {
            if self.acted.get(index).is_some_and(|&seen| seen != count) {
                self.acted[index] = count;
                self.flashes[index] = self.time;
            }
        }
        for effect in self.effects.drain(..) {
            let (text, place) = match effect {
                LogicEffect::Announce(text) => {
                    self.banner = Some((self.time, text.clone()));
                    (format!("Announced \u{201c}{text}\u{201d}"), None)
                }
                LogicEffect::Boss { kind, at } => (format!("{} arrives", kind.name()), Some(at)),
                LogicEffect::Shapes { count, area } => (
                    format!("{count} shapes appear"),
                    Some(V2::new(area.x + area.w * 0.5, area.y + area.h * 0.5)),
                ),
                LogicEffect::Repair {
                    tanks,
                    percent,
                    area,
                } => (
                    format!("Repairs {percent}%{}", tank_count(tanks.len())),
                    Some(V2::new(area.x + area.w * 0.5, area.y + area.h * 0.5)),
                ),
                LogicEffect::Damage {
                    tanks,
                    percent,
                    area,
                } => (
                    format!("Deals {percent}% damage{}", tank_count(tanks.len())),
                    Some(V2::new(area.x + area.w * 0.5, area.y + area.h * 0.5)),
                ),
                LogicEffect::Award {
                    tanks,
                    points,
                    area,
                } => (
                    format!("Awards {points} points{}", tank_count(tanks.len())),
                    Some(V2::new(area.x + area.w * 0.5, area.y + area.h * 0.5)),
                ),
                // Scripts run on servers; the preview only says it happened.
                LogicEffect::Script {
                    name, tanks, area, ..
                } => (
                    format!(
                        "Script event \u{201c}{name}\u{201d}{}",
                        match tanks.len() {
                            0 => String::new(),
                            1 => " with the test tank".to_owned(),
                            n => format!(" with {n} tanks"),
                        }
                    ),
                    Some(V2::new(area.x + area.w * 0.5, area.y + area.h * 0.5)),
                ),
            };
            self.events.push(Event {
                at: self.time,
                text,
                place,
            });
        }
        if self.events.len() > 40 {
            self.events.drain(..self.events.len() - 40);
        }
    }
    /// When trigger `index` next acts on its own schedule, if it has one.
    pub fn next_at(&self, index: usize) -> Option<f32> {
        self.logic.next_at(index).filter(|&at| at > 0.)
    }
}

fn tank_count(count: usize) -> String {
    match count {
        0 => ", but no tank is inside".into(),
        1 => " to the test tank".into(),
        n => format!(" to {n} tanks"),
    }
}

/// A splitmix step: the preview's own repeatable dice.
fn next_random(seed: &mut u64) -> f32 {
    *seed = seed.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *seed;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    ((z ^ (z >> 31)) >> 40) as f32 / (1u32 << 24) as f32
}
