use crate::{effects::Effects, render, settings::Settings, ui::*};
use macroquad::prelude::*;
use std::collections::HashMap;
use tankyard_core::{Snapshot, V2, WorldConfig};
use tankyard_server::LocalSession;
pub struct Ambient {
    session: LocalSession,
    snapshot: Snapshot,
    effects: Effects,
    view: render::View,
    empty: HashMap<tankyard_core::EntityId, (V2, f32)>,
    accumulator: f32,
}
impl Ambient {
    pub fn new() -> Self {
        let mut session = LocalSession::new(
            WorldConfig {
                modded: true,
                mode: tankyard_core::Mode::Sandbox,
                width: 1500.,
                height: 1100.,
                target_count: 38,
                max_projectiles: 240,
                time_limit: 0.,
                score_limit: 0,
                seed: 91827,
                ..Default::default()
            },
            4,
        );
        for id in session.world.tanks.iter().map(|t| t.id).collect::<Vec<_>>() {
            session.world.set_practice_level(id, 18);
        }
        let snapshot = session.world.snapshot();
        Self {
            session,
            snapshot,
            effects: Effects::new(),
            view: render::View::new(),
            empty: HashMap::new(),
            accumulator: 0.,
        }
    }
    pub fn draw(&mut self, settings: &Settings) {
        if !settings.reduced_motion {
            // Half-rate simulation with no catch-up debt, particles, audio, mods,
            // network or personal progress. Four actual bots make every decision.
            self.accumulator = (self.accumulator + get_frame_time().min(0.08)).min(0.067);
            if self.accumulator >= 1. / 30. {
                self.accumulator %= 1. / 30.;
                self.session.step();
                self.session.world.events.clear();
                self.session.world.snapshot_into(&mut self.snapshot);
            }
        }
        self.view.center = V2::new(
            self.snapshot.config.width * 0.5,
            self.snapshot.config.height * 0.5,
        );
        self.view.zoom = (screen_width() / self.snapshot.config.width)
            .max(screen_height() / self.snapshot.config.height)
            * 1.12;
        render::world(
            &self.snapshot,
            &self.session.bundle.content,
            None,
            0,
            &self.view,
            &self.effects,
            settings,
            1.,
            self.snapshot.tick as f32 * tankyard_core::DT,
            self.snapshot.tick as f32 * tankyard_core::DT,
            &self.empty,
        );
        draw_rectangle(
            0.,
            0.,
            screen_width(),
            screen_height(),
            alpha(bg_color(), 0.60),
        );
    }
}
