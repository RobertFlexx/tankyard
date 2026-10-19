use crate::ui::RED;
use crate::{render, settings::Settings, ui::*};
use macroquad::prelude::*;
use tankyard_content::{Ability, ClassDef, Content, ProjectileKind};
use tankyard_core::{BossKind, EntityId, Mode, Snapshot, Tank, xp_for_level};

#[derive(Clone, Copy)]
struct StatHint {
    name: &'static str,
    description: &'static str,
}

// Names come from the actual gun mounts, not some hardcoded class ID bullshit.
fn stat_hints(class: Option<&ClassDef>) -> [StatHint; 8] {
    let mut hints = [
        StatHint {
            name: "Max Health",
            description: "Survive more incoming damage.",
        },
        StatHint {
            name: "Health Regen",
            description: "Recover health faster after combat.",
        },
        StatHint {
            name: "Body Damage",
            description: "Deal more damage when your tank hits something.",
        },
        StatHint {
            name: "Bullet Speed",
            description: "Your bullets travel faster.",
        },
        StatHint {
            name: "Bullet Penetration",
            description: "Your bullets survive more collisions.",
        },
        StatHint {
            name: "Bullet Damage",
            description: "Your bullets hit harder.",
        },
        StatHint {
            name: "Reload",
            description: "Fire shots more often.",
        },
        StatHint {
            name: "Movement Speed",
            description: "Accelerate and move more quickly.",
        },
    ];
    let Some(class) = class else { return hints };
    if class.barrels.is_empty() {
        hints[3] = StatHint {
            name: "Weapon Speed (N/A)",
            description: "This tank has no guns. Buying this won't make it move faster.",
        };
        hints[4] = StatHint {
            name: "Penetration (N/A)",
            description: "No projectiles to strengthen on this tank.",
        };
        hints[5] = StatHint {
            name: "Shot Damage (N/A)",
            description: "No guns, no shot damage. Body Damage is separate.",
        };
        hints[6] = StatHint {
            name: "Reload (N/A)",
            description: "Nothing to reload on this tank.",
        };
        return hints;
    }

    let first = class.barrels[0].projectile;
    let same_kind = class
        .barrels
        .iter()
        .all(|barrel| barrel.projectile == first);
    if same_kind {
        let (speed, health, damage, reload) = match first {
            ProjectileKind::Drone => (
                StatHint {
                    name: "Drone Speed",
                    description: "Your drones fly and chase faster. This doesn't increase the drone cap.",
                },
                StatHint {
                    name: "Drone Health",
                    description: "Your drones withstand more hits before being destroyed.",
                },
                StatHint {
                    name: "Drone Damage",
                    description: "Your drones deal more contact damage.",
                },
                StatHint {
                    name: "Drone Spawn Rate",
                    description: "Replace destroyed drones faster. The maximum drone count stays the same.",
                },
            ),
            ProjectileKind::Minion => (
                StatHint {
                    name: "Minion Speed",
                    description: "Your little tanks move faster, and their own bullets fly faster too.",
                },
                StatHint {
                    name: "Minion Health",
                    description: "Minions survive more hits; their bullets also penetrate more.",
                },
                StatHint {
                    name: "Minion Damage",
                    description: "Minions hit harder and fire stronger bullets.",
                },
                StatHint {
                    name: "Minion Reload",
                    description: "Spawn replacements and fire minion guns faster. Doesn't increase the class's minion cap.",
                },
            ),
            ProjectileKind::Trap => (
                StatHint {
                    name: "Trap Range",
                    description: "Launch traps farther before they settle.",
                },
                StatHint {
                    name: "Trap Health",
                    description: "Placed traps withstand more damage.",
                },
                StatHint {
                    name: "Trap Damage",
                    description: "Traps deal more damage when enemies run into them.",
                },
                StatHint {
                    name: "Trap Reload",
                    description: "Launch new traps more often.",
                },
            ),
            ProjectileKind::Mine => (
                StatHint {
                    name: "Mine Launch Speed",
                    description: "Mines launch faster before settling into position.",
                },
                StatHint {
                    name: "Mine Health",
                    description: "Mines survive more hits before being destroyed.",
                },
                StatHint {
                    name: "Mine Damage",
                    description: "Mines deal more damage, including their explosions.",
                },
                StatHint {
                    name: "Mine Reload",
                    description: "Lay mines more often.",
                },
            ),
            ProjectileKind::Turret => (
                StatHint {
                    name: "Turret Speed (N/A)",
                    description: "Turrets are stationary; a speed upgrade won't move them.",
                },
                StatHint {
                    name: "Turret Health",
                    description: "Turrets withstand more hits.",
                },
                StatHint {
                    name: "Turret Damage",
                    description: "Turrets deal more damage on contact.",
                },
                StatHint {
                    name: "Turret Deploy Rate",
                    description: "Deploy new turrets more often.",
                },
            ),
            ProjectileKind::Barrier => (
                StatHint {
                    name: "Barrier Speed (N/A)",
                    description: "Barriers are stationary; upgrading shot speed won't move them.",
                },
                StatHint {
                    name: "Barrier Health",
                    description: "Defensive barriers withstand more damage.",
                },
                StatHint {
                    name: "Barrier Damage",
                    description: "Barriers deal more contact damage.",
                },
                StatHint {
                    name: "Barrier Deploy Rate",
                    description: "Place new barriers more often.",
                },
            ),
            ProjectileKind::Shell => (
                StatHint {
                    name: "Shell Speed",
                    description: "Heavy shells travel faster.",
                },
                StatHint {
                    name: "Shell Penetration",
                    description: "Shells survive more collisions.",
                },
                StatHint {
                    name: "Shell Damage",
                    description: "Heavy shells hit harder.",
                },
                StatHint {
                    name: "Shell Reload",
                    description: "Fire heavy shells more often.",
                },
            ),
            ProjectileKind::Piercing => (
                StatHint {
                    name: "Piercing Speed",
                    description: "Piercing rounds travel faster.",
                },
                StatHint {
                    name: "Piercing Power",
                    description: "Your rounds survive more collisions and can hit more targets.",
                },
                StatHint {
                    name: "Piercing Damage",
                    description: "Piercing rounds hit harder.",
                },
                StatHint {
                    name: "Reload",
                    description: "Fire piercing rounds more often.",
                },
            ),
            ProjectileKind::Scatter => (
                StatHint {
                    name: "Pellet Speed",
                    description: "Your scattered pellets travel faster.",
                },
                StatHint {
                    name: "Pellet Penetration",
                    description: "Pellets survive more collisions.",
                },
                StatHint {
                    name: "Pellet Damage",
                    description: "Each pellet deals more damage.",
                },
                StatHint {
                    name: "Reload",
                    description: "Fire pellet volleys more often.",
                },
            ),
            ProjectileKind::Homing => (
                StatHint {
                    name: "Homing Speed",
                    description: "Guided shots travel faster.",
                },
                StatHint {
                    name: "Homing Penetration",
                    description: "Guided shots survive more collisions.",
                },
                StatHint {
                    name: "Homing Damage",
                    description: "Guided shots deal more damage.",
                },
                StatHint {
                    name: "Reload",
                    description: "Launch guided shots more often.",
                },
            ),
            ProjectileKind::Explosive | ProjectileKind::Fragment => (
                StatHint {
                    name: "Explosive Speed",
                    description: "Explosive projectiles travel faster.",
                },
                StatHint {
                    name: "Explosive Penetration",
                    description: "Explosive projectiles survive more collisions.",
                },
                StatHint {
                    name: "Explosive Damage",
                    description: "Direct hits and explosions deal more damage.",
                },
                StatHint {
                    name: "Reload",
                    description: "Launch explosives more often.",
                },
            ),
            ProjectileKind::Bullet => return hints,
        };
        hints[3] = speed;
        hints[4] = health;
        hints[5] = damage;
        hints[6] = reload;
        return hints;
    }

    let has_units = class.barrels.iter().any(|b| {
        matches!(
            b.projectile,
            ProjectileKind::Drone
                | ProjectileKind::Minion
                | ProjectileKind::Trap
                | ProjectileKind::Mine
                | ProjectileKind::Barrier
                | ProjectileKind::Turret
        )
    });
    let has_shots = class.barrels.iter().any(|b| {
        !matches!(
            b.projectile,
            ProjectileKind::Drone
                | ProjectileKind::Minion
                | ProjectileKind::Trap
                | ProjectileKind::Mine
                | ProjectileKind::Barrier
                | ProjectileKind::Turret
        )
    });
    if has_units && has_shots {
        hints[3] = StatHint {
            name: "Weapon Speed",
            description: "Faster bullets and drone movement, or faster launches for moving deployables. Stationary units stay put.",
        };
        hints[4] = StatHint {
            name: "Pen. / Unit Health",
            description: "Shots penetrate more; drones, traps, and other deployables take more hits.",
        };
        hints[5] = StatHint {
            name: "Weapon Damage",
            description: "Boost both shot damage and contact damage from your deployables.",
        };
        hints[6] = StatHint {
            name: "Reload / Spawn",
            description: "Fire shots and replace or deploy units more often. Doesn't raise unit caps.",
        };
    } else if has_units {
        hints[3] = StatHint {
            name: "Unit Speed",
            description: "Moving units travel faster; stationary turrets and barriers stay put.",
        };
        hints[4] = StatHint {
            name: "Unit Health",
            description: "Drones, minions, traps, and defenses last longer under fire. Minion bullets also gain penetration.",
        };
        hints[5] = StatHint {
            name: "Unit Damage",
            description: "Units hit harder, including bullets fired by your minions.",
        };
        hints[6] = StatHint {
            name: "Unit Reload",
            description: "Deploy and replace units faster; minions also shoot faster. Unit caps stay unchanged.",
        };
    } else {
        hints[3] = StatHint {
            name: "Projectile Speed",
            description: "All your weapon projectiles travel faster.",
        };
        hints[4] = StatHint {
            name: "Penetration",
            description: "Your projectiles withstand more collisions.",
        };
        hints[5] = StatHint {
            name: "Projectile Damage",
            description: "All your weapons deal more damage.",
        };
        hints[6] = StatHint {
            name: "Reload",
            description: "Fire each weapon more often.",
        };
    }
    if class
        .barrels
        .iter()
        .any(|b| b.projectile == ProjectileKind::Trap)
    {
        hints[3] = StatHint {
            name: "Speed / Trap Range",
            description: "Faster shots and longer trap launches.",
        };
    }
    hints
}

pub enum HudAction {
    Respawn,
    Spectate(i8),
    Stat(usize),
    Evolve(String),
    ToggleTheme,
    Encyclopedia,
    Pause,
    Controls,
    Menu,
    Reconnect,
    SandboxLevel(u8),
    SandboxClass(String),
    Bot,
    Boss(BossKind),
    Invulnerable(bool),
    Speed(f32),
}
pub struct Hud {
    pub spectating: Option<EntityId>,
    pub practice: bool,
    pub evolution: bool,
    pub sandbox: bool,
    pub sandbox_page: usize,
    pub invulnerable: bool,
    pub boss_index: usize,
    pub speed: f32,
    pub local_simulation: bool,
    pub sandbox_allowed: bool,
    sandbox_level_pending: Option<u8>,
    sandbox_level_at: f64,
    stat_visibility: f32,
    offered_class: String,
    ranking_tick: Option<(u64, usize)>,
    ranking: Vec<usize>,
}
impl Hud {
    pub fn new() -> Self {
        Self {
            spectating: None,
            practice: false,
            evolution: false,
            sandbox: false,
            sandbox_page: 0,
            invulnerable: false,
            boss_index: 0,
            speed: 1.0,
            local_simulation: false,
            sandbox_allowed: false,
            sandbox_level_pending: None,
            sandbox_level_at: 0.,
            stat_visibility: 0.0,
            offered_class: String::new(),
            ranking_tick: None,
            ranking: Vec::with_capacity(256),
        }
    }
    pub fn captures_pointer(
        &self,
        snapshot: &Snapshot,
        content: &Content,
        local: EntityId,
        s: f32,
    ) -> bool {
        let Some(tank) = snapshot.tanks.iter().find(|tank| tank.id == local) else {
            return false;
        };
        if !tank.alive || snapshot.match_state.finished || self.sandbox {
            return true;
        }
        let m = 22.0 * s;
        if contains(Rect::new(m, m, 226.0 * s, 56.0 * s)) {
            return true;
        }
        if tank.points > 0
            && contains(Rect::new(
                m,
                screen_height() - m - 225.0 * s,
                246.0 * s,
                225.0 * s,
            ))
        {
            return true;
        }
        if self.evolution
            && let Some(class) = content.class(&tank.class)
        {
            let cards = evolution_cards(class.upgrades.len(), screen_width(), screen_height(), s);
            let card_scale =
                evolution_scale(class.upgrades.len(), screen_width(), screen_height(), s);
            if cards.iter().any(|r| contains(*r))
                || contains(Rect::new(
                    m,
                    110. * s - 29. * card_scale,
                    290. * card_scale,
                    29. * card_scale,
                ))
                || cards.last().is_some_and(|r| {
                    contains(Rect::new(
                        m,
                        r.y + r.h + 8. * card_scale,
                        134. * card_scale,
                        24. * card_scale,
                    ))
                })
            {
                return true;
            }
        }
        contains(Rect::new(
            screen_width() - 242.0 * s,
            m,
            220.0 * s,
            270.0 * s,
        )) || contains(Rect::new(
            screen_width() - 206.0 * s,
            screen_height() - 154.0 * s,
            184.0 * s,
            132.0 * s,
        ))
    }
    fn update_skill_panel(&mut self, tank: &Tank, finished: bool, dt: f32, reduced_motion: bool) {
        let target = if tank.points > 0 && tank.alive && !finished {
            1.0
        } else {
            0.0
        };
        let easing = if reduced_motion {
            1.0
        } else {
            1.0 - (-dt.clamp(0.0, 0.1) * 18.0).exp()
        };
        self.stat_visibility += (target - self.stat_visibility) * easing;
        if (self.stat_visibility - target).abs() < 0.005 {
            self.stat_visibility = target;
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        snapshot: &Snapshot,
        content: &Content,
        local: EntityId,
        settings: &Settings,
        diagnostics: Option<(f64, f64, f64, f64)>,
        s: f32,
        view: &render::View,
    ) -> Vec<HudAction> {
        pointer_scope(100);
        let mut actions = Vec::new();
        let w = screen_width();
        let h = screen_height();
        let m = 22.0 * s;
        let Some(tank) = snapshot.tanks.iter().find(|t| t.id == local) else {
            center_label("Connecting…", w * 0.5, h * 0.5, 26.0 * s, CYAN);
            return actions;
        };
        let sandbox_controls = snapshot.config.mode == Mode::Sandbox && !self.practice;
        if !self.sandbox {
            self.sandbox_level_pending = None;
        }
        if !sandbox_controls || !tank.alive || snapshot.match_state.finished {
            self.sandbox = false;
            self.sandbox_level_pending = None;
        }
        let modal = self.sandbox || !tank.alive || snapshot.match_state.finished;
        if !tank.alive {
            self.evolution = false;
            self.offered_class.clear();
        }
        let class = content.class(&tank.class);
        if tank.alive
            && !modal
            && let Some(class) = class
            && let Some(progress) = tankyard_core::reload_progress(tank, class, &snapshot.config)
        {
            let p = view.screen(tank.pos);
            let width = 52. * s;
            let y = p.y + tank.radius * view.zoom + 23. * s;
            let rect = Rect::new(p.x - width * 0.5, y, width, 5. * s);
            arcade_track(
                rect,
                progress,
                if progress >= 0.999 {
                    alpha(CYAN, 0.75)
                } else {
                    CYAN
                },
            );
        }
        let available = class.is_some_and(|class| {
            class
                .upgrades
                .iter()
                .filter_map(|id| content.class(id))
                .any(|next| next.min_level <= tank.level)
        });
        if available && self.offered_class != tank.class && tank.alive {
            self.offered_class.clone_from(&tank.class);
            self.evolution = true;
        }
        if !available {
            self.offered_class.clear();
        }
        if small_button(
            Rect::new(m, m, 110.0 * s, 26.0 * s),
            "Tanks [Tab]",
            s * 0.76,
        ) && !modal
        {
            self.evolution = !self.evolution;
        }
        if small_button(
            Rect::new(m + 116.0 * s, m, 110.0 * s, 26.0 * s),
            "Menu [Esc]",
            s * 0.76,
        ) && !modal
        {
            actions.push(HudAction::Pause);
        }
        if small_button(
            Rect::new(m, m + 32.0 * s, 110.0 * s, 24.0 * s),
            if settings.dark_arena {
                "Dark [F9]"
            } else {
                "Light [F9]"
            },
            s * 0.72,
        ) && !modal
        {
            actions.push(HudAction::ToggleTheme);
        }
        if small_button(
            Rect::new(m + 116.0 * s, m + 32.0 * s, 110.0 * s, 24.0 * s),
            if sandbox_controls {
                "Sandbox [T]"
            } else {
                "Help [F1]"
            },
            s * 0.72,
        ) && tank.alive
            && !snapshot.match_state.finished
        {
            if sandbox_controls {
                self.sandbox = !self.sandbox;
            } else if !modal {
                actions.push(HudAction::Controls);
            }
        }
        arcade_center(
            snapshot
                .config
                .mode_name
                .as_deref()
                .unwrap_or(snapshot.config.mode.name()),
            w * 0.5,
            m + 16.0 * s,
            19.0 * s,
        );
        if snapshot.config.mode.teams() > 1 {
            for i in 0..snapshot.config.mode.teams() {
                label(
                    &format!("{}", snapshot.match_state.scores[i as usize]),
                    w * 0.5 - (snapshot.config.mode.teams() as f32 * 25.0) * s
                        + i as f32 * 50.0 * s,
                    m + 44.0 * s,
                    22.0 * s,
                    render::team_color(i + 1, false, settings.colorblind),
                );
            }
        }
        if !snapshot.match_state.announcement.is_empty()
            && snapshot.match_state.announcement != snapshot.config.mode.name()
        {
            arcade_center(
                &snapshot.match_state.announcement,
                w * 0.5,
                m + 72.0 * s,
                17.0 * s,
            );
        }
        // Saturated horizontal segmented stats, ordered like the classic
        // interface. The display order is NOT the authoritative stat index.
        let stats_y = h - m - 207.0 * s;
        let stat_w = 246.0 * s;
        let stat_order = STAT_ORDER;
        self.update_skill_panel(
            tank,
            snapshot.match_state.finished,
            get_frame_time(),
            settings.reduced_motion,
        );
        let stats_x = m - (1.0 - self.stat_visibility) * (stat_w + m + 10.0 * s);
        let hints = stat_hints(class);
        let stat_colors = [
            Color::new(0.91, 0.43, 0.91, 1.0),
            Color::new(0.99, 0.68, 0.46, 1.0),
            Color::new(0.59, 0.43, 0.88, 1.0),
            Color::new(0.43, 0.64, 0.88, 1.0),
            Color::new(1.0, 0.88, 0.41, 1.0),
            RED,
            Color::new(0.72, 0.94, 0.41, 1.0),
            Color::new(0.42, 0.91, 0.89, 1.0),
        ];
        let mut tooltip = None;
        if self.stat_visibility > 0.0 {
            arcade_label(
                &format!("Skill points: {}", tank.points),
                stats_x + 6.0 * s,
                stats_y - 9.0 * s,
                16.0 * s,
            );
            for (row, &index) in stat_order.iter().enumerate() {
                let r = Rect::new(stats_x, stats_y + row as f32 * 26.0 * s, stat_w, 22.0 * s);
                let hovered = contains(r) && !modal && !self.evolution && tank.points > 0;
                if hovered {
                    tooltip = Some(index);
                }
                let color = stat_colors[index];
                let levels_width = r.w - 46.0 * s;
                let edge = (r.h * 0.16).clamp(0.7, 3.0);
                arcade_track_tinted(
                    r,
                    (levels_width - edge) / (r.w - edge * 2.0) * f32::from(tank.stats[index]) / 7.0,
                    color,
                    mix(color, if settings.dark_arena { BLACK } else { WHITE }, 0.28),
                );
                for point in 1..7 {
                    let px = r.x + levels_width * point as f32 / 7.0;
                    draw_line(
                        px,
                        r.y + 3.0 * s,
                        px,
                        r.y + r.h - 3.0 * s,
                        1.4 * s,
                        alpha(BLACK, 0.30),
                    );
                }
                let can_upgrade = tank.points > 0 && tank.stats[index] < 7 && !modal;
                let controls_x = r.x + r.w - 43.0 * s;
                draw_rectangle(
                    controls_x,
                    r.y + 3.0 * s,
                    27.0 * s,
                    r.h - 6.0 * s,
                    Color::new(0.43, 0.43, 0.43, 1.0),
                );
                capsule(
                    Rect::new(
                        controls_x + 27.0 * s,
                        r.y + 3.0 * s,
                        13.0 * s,
                        r.h - 6.0 * s,
                    ),
                    if can_upgrade && hovered {
                        color
                    } else {
                        Color::new(0.50, 0.50, 0.50, 1.0)
                    },
                );
                arcade_center(
                    hints[index].name,
                    r.x + levels_width * 0.5,
                    r.y + 16.0 * s,
                    13.0 * s,
                );
                arcade_center(
                    &format!("[{}]", row + 1),
                    controls_x + 12.0 * s,
                    r.y + 15.0 * s,
                    11.0 * s,
                );
                arcade_center(
                    if tank.stats[index] == 7 { "=" } else { "+" },
                    r.x + r.w - 10.0 * s,
                    r.y + 16.0 * s,
                    15.0 * s,
                );
                if hovered && can_upgrade && is_mouse_button_pressed(MouseButton::Left) {
                    actions.push(HudAction::Stat(index));
                }
            }
        }
        if let Some(index) = tooltip {
            let x = stats_x;
            let y = stats_y - 130. * s;
            panel(Rect::new(x, y, 264.0 * s, 112.0 * s));
            label(
                hints[index].name,
                x + 12.0 * s,
                y + 24.0 * s,
                16.0 * s,
                text_color(),
            );
            wrap(
                hints[index].description,
                x + 12.0 * s,
                y + 45.0 * s,
                238.0 * s,
                13.0 * s,
                text_color(),
            );
            label(
                &format!("{} / 7", tank.stats[index]),
                x + 12.0 * s,
                y + 101.0 * s,
                12.0 * s,
                muted_color(),
            );
        }
        let leader_score = self.leaderboard(snapshot, local, settings, s);
        self.minimap(snapshot, local, settings, s, view);
        if tank.alive && !self.sandbox {
            let xp_width = (w * 0.36).min(430.0 * s);
            let xp_x = (w - xp_width) * 0.5;
            let next = xp_for_level((tank.level + 1).min(tankyard_core::MAX_LEVEL));
            let prev = xp_for_level(tank.level);
            let fraction = if tank.level >= tankyard_core::MAX_LEVEL {
                1.0
            } else {
                tank.xp.saturating_sub(prev) as f32 / (next - prev).max(1) as f32
            };
            let xp_y = h - 33.0 * s;
            arcade_player_label(
                tank,
                "",
                vec2(w * 0.5, xp_y - 45. * s),
                xp_width - 12. * s,
                27. * s,
            );
            arcade_track(
                Rect::new(
                    xp_x + 18.0 * s,
                    xp_y - 32.0 * s,
                    xp_width - 36.0 * s,
                    22.0 * s,
                ),
                tank.score as f32 / leader_score as f32,
                Color::new(0.55, 0.87, 0.37, 1.0),
            );
            arcade_center(
                &format!("Score: {}", tank.score),
                w * 0.5,
                xp_y - 16.0 * s,
                14.0 * s,
            );
            arcade_track(Rect::new(xp_x, xp_y, xp_width, 24.0 * s), fraction, AMBER);
            arcade_center(
                &fitted_label(
                    &format!(
                        "Lvl {} {}",
                        tank.level,
                        class.map(|c| c.name.as_str()).unwrap_or("Tank")
                    ),
                    16.0 * s,
                    xp_width - 16.0 * s,
                ),
                w * 0.5,
                xp_y + 17.0 * s,
                16.0 * s,
            );
            // Stack status lines above the name, clear of both progress bars.
            let mut status_y = xp_y - 74.0 * s;
            if let Some(class) = class
                && class.ability != Ability::None
            {
                let ready = tank.ability_cooldown <= 0.0;
                let parrying = class.ability.is_parry() && tank.shield > 0.0;
                center_label(
                    &format!(
                        "[{}] {}  {}",
                        settings.bindings[4],
                        class.ability.name(),
                        if parrying {
                            "ACTIVE".into()
                        } else if ready {
                            "READY".into()
                        } else {
                            format!("{:.1}s", tank.ability_cooldown)
                        }
                    ),
                    w * 0.5,
                    status_y,
                    15.0 * s,
                    if parrying {
                        AMBER
                    } else if ready {
                        CYAN
                    } else {
                        muted_color()
                    },
                );
                status_y -= 24.0 * s;
            }
            if let Some(class) = class.filter(|class| class.has_drones()) {
                let name = if class
                    .barrels
                    .iter()
                    .any(|barrel| barrel.projectile == tankyard_content::ProjectileKind::Minion)
                {
                    "Minions"
                } else {
                    "Drones"
                };
                center_label(
                    &format!("{name}: {} / {}", tank.deployed_drones, class.drone_limit),
                    w * 0.5,
                    status_y,
                    12.0 * s,
                    if tank.deployed_drones == class.drone_limit {
                        CYAN
                    } else {
                        muted_color()
                    },
                );
                status_y -= 21.0 * s;
            }
            if settings.auto_fire {
                center_label(
                    &format!("Auto-fire [{}]", settings.bindings[5]),
                    w * 0.5,
                    status_y,
                    12.0 * s,
                    muted_color(),
                );
                status_y -= 21.0 * s;
            }
            if tank.spawn_immunity && tank.protection > 0.0 {
                center_label(
                    &format!(
                        "Spawn protection · {}s · move to engage",
                        tank.protection.ceil() as u32
                    ),
                    w * 0.5,
                    status_y,
                    12.0 * s,
                    CYAN,
                );
            }
        }
        if settings.show_fps {
            label(
                &format!("{} FPS  ·  tick {} / 60 Hz", get_fps(), snapshot.tick),
                m,
                h - 5.0 * s,
                11.0 * s,
                muted_color(),
            );
            if let Some((ping, loss, sent, received)) = diagnostics {
                label(
                    &format!(
                        "{ping:.0}ms  ·  loss {:.1}%  ·  ↑{:.1} ↓{:.1} KiB/s",
                        loss * 100.0,
                        sent / 1024.0,
                        received / 1024.0
                    ),
                    m,
                    h - 319.0 * s,
                    12.0 * s,
                    CYAN,
                );
            }
        } else if let Some((ping, _, _, _)) = diagnostics {
            right_label(
                &format!("{ping:.0} ms"),
                w - m,
                m + 224.0 * s,
                12.0 * s,
                muted_color(),
            );
        }
        if self.evolution && tank.alive && !snapshot.match_state.finished && !self.sandbox {
            self.evolution_overlay(tank, content, &mut actions, s);
        }
        if sandbox_controls && self.sandbox {
            self.sandbox_overlay(tank, content, &mut actions, s);
        }
        if !tank.alive && !snapshot.match_state.finished {
            if let Some(watched) = snapshot
                .tanks
                .iter()
                .find(|t| Some(t.id) == self.spectating && t.alive)
            {
                self.spectator_bar(tank, watched, snapshot.config.mode, &mut actions, s);
            } else {
                self.death(tank, snapshot.config.mode, &mut actions, s);
            }
        }
        if snapshot.match_state.finished {
            draw_rectangle(0.0, 0.0, w, h, alpha(bg_color(), 0.77));
            center_label("Game over", w * 0.5, h * 0.4, 44.0 * s, AMBER);
            let result = match (snapshot.config.mode, snapshot.match_state.winner) {
                (Mode::CoopPve, Some(1)) => "All ten waves cleared".into(),
                (Mode::CoopPve, _) => "Squad eliminated".into(),
                (_, Some(0)) => "The arena has a champion".into(),
                (_, Some(team)) => format!("Team {team} wins"),
                (_, None) => "Match finished".into(),
            };
            center_label(&result, w * 0.5, h * 0.4 + 44.0 * s, 22.0 * s, text_color());
            if button(
                Rect::new(w * 0.5 - 130.0 * s, h * 0.4 + 86.0 * s, 260.0 * s, 46.0 * s),
                "Return to menu",
                true,
                true,
                s,
            ) {
                actions.push(HudAction::Menu);
            }
        }
        actions
    }
    fn leaderboard(
        &mut self,
        snapshot: &Snapshot,
        local: EntityId,
        settings: &Settings,
        s: f32,
    ) -> u32 {
        let x = screen_width() - 242.0 * s;
        let y = 22.0 * s;
        let width = 220.0 * s;
        let coop = snapshot.config.mode == Mode::CoopPve;
        arcade_center(
            if coop { "Squad" } else { "Leaderboard" },
            x + width * 0.5,
            y + 18.0 * s,
            19.0 * s,
        );
        let revision = (snapshot.tick, snapshot.tanks.len());
        if self.ranking_tick != Some(revision) {
            self.ranking.clear();
            self.ranking.extend(
                snapshot
                    .tanks
                    .iter()
                    .enumerate()
                    .filter(|(_, tank)| !coop || tank.team == 1)
                    .map(|(index, _)| index),
            );
            self.ranking.sort_unstable_by(|&a, &b| {
                snapshot.tanks[b]
                    .score
                    .cmp(&snapshot.tanks[a].score)
                    .then_with(|| snapshot.tanks[a].id.cmp(&snapshot.tanks[b].id))
            });
            self.ranking_tick = Some(revision);
        }
        let leading = self
            .ranking
            .first()
            .map_or(1, |&index| snapshot.tanks[index].score.max(1));
        for (row, &index) in self.ranking.iter().take(10).enumerate() {
            let tank = &snapshot.tanks[index];
            let by = y + 30.0 * s + row as f32 * 24.0 * s;
            let color = render::team_color(tank.team, tank.id == local, settings.colorblind);
            arcade_track(
                Rect::new(x, by, width, 20.0 * s),
                tank.score as f32 / leading as f32,
                color,
            );
            let score = format!(" - {}", tank.score);
            arcade_player_label(
                tank,
                &score,
                vec2(x + width * 0.5, by + 14. * s),
                width - 12. * s,
                12. * s,
            );
        }
        leading
    }
    fn minimap(
        &self,
        snapshot: &Snapshot,
        local: EntityId,
        settings: &Settings,
        s: f32,
        view: &render::View,
    ) {
        let width = 184.0 * s;
        let height = 132.0 * s;
        let x = screen_width() - width - 22.0 * s;
        let y = screen_height() - height - 78.0 * s;
        draw_rectangle(x, y, width, height, alpha(surface_color(), 0.55));
        draw_rectangle_lines(
            x,
            y,
            width,
            height,
            3.0 * s,
            Color::new(0.333, 0.333, 0.333, 1.0),
        );
        right_label(
            "Tank Yard",
            screen_width() - 22.0 * s,
            y - 10.0 * s,
            16.0 * s,
            muted_color(),
        );
        let sx = (width - 12.0 * s) / snapshot.config.width;
        let sy = (height - 12.0 * s) / snapshot.config.height;
        let point = |p: tankyard_core::V2| vec2(x + 6.0 * s + p.x * sx, y + 6.0 * s + p.y * sy);
        let bounds = snapshot.config.bounds();
        let edge = point(tankyard_core::V2::new(bounds.x, bounds.y));
        draw_rectangle_lines(
            edge.x,
            edge.y,
            bounds.w * sx,
            bounds.h * sy,
            1.5 * s,
            alpha(CYAN, 0.8),
        );
        let top = view.world(vec2(0.0, 0.0));
        let bottom = view.world(vec2(screen_width(), screen_height()));
        let a = point(tankyard_core::V2::new(
            top.x.clamp(0.0, snapshot.config.width),
            top.y.clamp(0.0, snapshot.config.height),
        ));
        let b = point(tankyard_core::V2::new(
            bottom.x.clamp(0.0, snapshot.config.width),
            bottom.y.clamp(0.0, snapshot.config.height),
        ));
        draw_rectangle(
            a.x,
            a.y,
            (b.x - a.x).max(0.0),
            (b.y - a.y).max(0.0),
            alpha(text_color(), 0.07),
        );
        draw_rectangle_lines(
            a.x,
            a.y,
            (b.x - a.x).max(0.0),
            (b.y - a.y).max(0.0),
            1.0 * s,
            alpha(text_color(), 0.28),
        );
        for wall in &snapshot.walls {
            draw_rectangle(
                x + 6.0 * s + wall.x * sx,
                y + 6.0 * s + wall.y * sy,
                wall.w * sx,
                wall.h * sy,
                line_color(),
            );
        }
        for objective in &snapshot.objectives {
            let p = point(objective.pos);
            draw_circle_lines(p.x, p.y, 5.0 * s, 1.0, AMBER);
        }
        for boss in &snapshot.bosses {
            let p = point(boss.pos);
            draw_poly(p.x, p.y, 4, 4.0 * s, 45.0, RED);
        }
        for tank in snapshot.tanks.iter().filter(|t| t.alive) {
            let p = point(tank.pos);
            let color = render::team_color(tank.team, tank.id == local, settings.colorblind);
            draw_circle(
                p.x,
                p.y,
                if tank.id == local { 3.5 * s } else { 1.8 * s },
                color,
            );
            if tank.id == local {
                draw_circle_lines(p.x, p.y, 6.0 * s, 1.0, alpha(text_color(), 0.6));
            }
        }
    }
    fn evolution_overlay(
        &mut self,
        tank: &Tank,
        content: &Content,
        actions: &mut Vec<HudAction>,
        s: f32,
    ) {
        let Some(current) = content.class(&tank.class) else {
            return;
        };
        let next: Vec<_> = current
            .upgrades
            .iter()
            .filter_map(|id| content.class(id))
            .collect();
        let cards = evolution_cards(next.len(), screen_width(), screen_height(), s);
        let x = 22.0 * s;
        let y = 110.0 * s;
        let s = evolution_scale(next.len(), screen_width(), screen_height(), s);
        arcade_label(
            if next.is_empty() {
                "No more upgrades"
            } else {
                "Upgrade"
            },
            x,
            y - 8.0 * s,
            18.0 * s,
        );
        if small_button(
            Rect::new(x + 207.0 * s, y - 27.0 * s, 83.0 * s, 23.0 * s),
            "Hide [Tab]",
            s * 0.66,
        ) {
            self.evolution = false;
        }
        for (slot, (class, r)) in next.iter().zip(&cards).enumerate() {
            let unlocked = tank.level >= class.min_level;
            let hovered = contains(*r) && unlocked;
            let fill = if unlocked {
                card_color(slot)
            } else {
                mix(card_color(slot), bg_color(), 0.70)
            };
            draw_rectangle(
                r.x,
                r.y,
                r.w,
                r.h,
                mix(fill, WHITE, if hovered { 0.40 } else { 0.20 }),
            );
            draw_rectangle(
                r.x,
                r.y + r.h * 0.58,
                r.w,
                r.h * 0.42,
                mix(fill, BLACK, 0.10),
            );
            draw_rectangle_lines(
                r.x,
                r.y,
                r.w,
                r.h,
                if hovered { 3.0 * s } else { 2.5 * s },
                Color::new(0.333, 0.333, 0.333, 1.0),
            );
            render::class_preview(
                class,
                Rect::new(r.x + 7.0 * s, r.y + 6.0 * s, r.w - 14.0 * s, r.h - 30.0 * s),
                16.0 * s,
                -0.45,
                if unlocked { CYAN } else { muted_color() },
            );
            let size =
                (13.0 * s).min((r.w - 8.0 * s) / (class.name.chars().count().max(1) as f32 * 0.55));
            arcade_center(&class.name, r.x + r.w * 0.5, r.y + r.h - 9.0 * s, size);
            if !unlocked {
                arcade_center(
                    &format!("Lvl {}", class.min_level),
                    r.x + r.w * 0.5,
                    r.y + r.h - 25.0 * s,
                    11.0 * s,
                );
            }
            if hovered && is_mouse_button_pressed(MouseButton::Left) {
                actions.push(HudAction::Evolve(class.id.clone()));
                self.evolution = false;
            }
        }
        let bottom = cards.last().map_or(y + 30.0 * s, |r| r.y + r.h + 8.0 * s);
        if small_button(
            Rect::new(x, bottom, 134.0 * s, 24.0 * s),
            "Tank tree",
            s * 0.72,
        ) {
            actions.push(HudAction::Encyclopedia);
        }
    }
    fn sandbox_overlay(
        &mut self,
        tank: &Tank,
        content: &Content,
        actions: &mut Vec<HudAction>,
        s: f32,
    ) {
        let (panel_rect, s) = sandbox_layout(screen_width(), screen_height(), s);
        let Rect { x, y, w: width, .. } = panel_rect;
        panel(panel_rect);
        label("Sandbox", x + 20. * s, y + 32. * s, 24. * s, CYAN);
        right_label(
            "PgUp / PgDn · Shift: 15",
            x + width - 20. * s,
            y + 30. * s,
            10. * s,
            muted_color(),
        );
        let interactive = set_interactive(false);
        set_interactive(interactive && self.sandbox_allowed);
        if self.sandbox_level_pending == Some(tank.level) {
            self.sandbox_level_pending = None;
        }
        let mut level = f32::from(self.sandbox_level_pending.unwrap_or(tank.level));
        if slider_value(
            Rect::new(x + 20. * s, y + 53. * s, width - 40. * s, 48. * s),
            "Level",
            &mut level,
            [1., f32::from(tankyard_core::MAX_LEVEL)],
            1.,
            s * 0.85,
            |value| format!("{value:.0}"),
        ) {
            self.sandbox_level_pending = Some(level.round() as u8);
        }
        let step = (width - 40. * s) / tankyard_core::UPGRADE_LEVELS.len() as f32;
        for (index, level) in tankyard_core::UPGRADE_LEVELS.into_iter().enumerate() {
            if small_button(
                Rect::new(
                    x + 20. * s + index as f32 * step,
                    y + 109. * s,
                    step - 7. * s,
                    28. * s,
                ),
                &level.to_string(),
                s * 0.85,
            ) {
                self.sandbox_level_pending = Some(level);
            }
        }
        if let Some(level) = self.sandbox_level_pending
            && get_time() - self.sandbox_level_at >= 0.11
            && self.sandbox_allowed
        {
            self.sandbox_level_at = get_time();
            actions.push(HudAction::SandboxLevel(level));
        }
        let page_count = content.classes.len().div_ceil(6);
        self.sandbox_page = self.sandbox_page.min(page_count.saturating_sub(1));
        label(
            "Choose tank",
            x + 20. * s,
            y + 159. * s,
            12. * s,
            muted_color(),
        );
        for (row, class) in content
            .classes
            .iter()
            .skip(self.sandbox_page * 6)
            .take(6)
            .enumerate()
        {
            if button(
                Rect::new(
                    x + 20. * s,
                    y + 174. * s + row as f32 * 37. * s,
                    width - 40. * s,
                    31. * s,
                ),
                &class.name,
                tank.class == class.id,
                self.sandbox_allowed,
                s * 0.75,
            ) {
                actions.push(HudAction::SandboxClass(class.id.clone()));
            }
        }
        if small_button(
            Rect::new(x + 20. * s, y + 407. * s, 65. * s, 30. * s),
            "‹",
            s,
        ) {
            self.sandbox_page = self.sandbox_page.saturating_sub(1);
        }
        center_label(
            &format!("{} / {}", self.sandbox_page + 1, page_count),
            x + width * 0.5,
            y + 428. * s,
            14. * s,
            muted_color(),
        );
        if small_button(
            Rect::new(x + width - 85. * s, y + 407. * s, 65. * s, 30. * s),
            "›",
            s,
        ) {
            self.sandbox_page = (self.sandbox_page + 1).min(page_count.saturating_sub(1));
        }
        if toggle(
            Rect::new(x + 20. * s, y + 456. * s, width - 40. * s, 30. * s),
            "Invulnerability",
            &mut self.invulnerable,
            s,
        ) {
            actions.push(HudAction::Invulnerable(self.invulnerable));
        }
        set_interactive(interactive && self.local_simulation);
        let previous_speed = self.speed;
        if slider_value(
            Rect::new(x + 20. * s, y + 501. * s, width - 40. * s, 48. * s),
            if self.local_simulation {
                "Simulation speed"
            } else {
                "Server controls speed"
            },
            &mut self.speed,
            [0.25, 2.],
            0.25,
            s * 0.85,
            |value| format!("{value:.2}×"),
        ) && self.speed != previous_speed
        {
            actions.push(HudAction::Speed(self.speed));
        }
        set_interactive(interactive && self.sandbox_allowed);
        if small_button(
            Rect::new(x + 20. * s, y + 563. * s, 180. * s, 31. * s),
            "Add AI tank",
            s * 0.85,
        ) {
            actions.push(HudAction::Bot);
        }
        set_interactive(interactive && self.local_simulation);
        if small_button(
            Rect::new(x + 220. * s, y + 563. * s, 180. * s, 31. * s),
            "Reset speed",
            s * 0.85,
        ) {
            self.speed = 1.;
            actions.push(HudAction::Speed(1.));
        }
        set_interactive(interactive && self.sandbox_allowed);
        let bosses = BossKind::ALL;
        if small_button(
            Rect::new(x + 20. * s, y + 611. * s, 240. * s, 31. * s),
            bosses[self.boss_index].name(),
            s * 0.8,
        ) {
            self.boss_index = (self.boss_index + 1) % bosses.len();
        }
        if small_button(
            Rect::new(x + 280. * s, y + 611. * s, 120. * s, 31. * s),
            "Spawn boss",
            s * 0.85,
        ) {
            actions.push(HudAction::Boss(bosses[self.boss_index]));
        }
        label(
            if self.local_simulation {
                "Speed changes affect this offline session."
            } else if self.sandbox_allowed {
                "Level and spawning changes are applied by the server."
            } else {
                "Sandbox tools are disabled by this server."
            },
            x + 20. * s,
            y + 674. * s,
            11. * s,
            muted_color(),
        );
        set_interactive(interactive);
        if small_button(
            Rect::new(x + 20. * s, y + 694. * s, width - 40. * s, 27. * s),
            "Close [T]",
            s * 0.85,
        ) {
            self.sandbox = false;
            self.sandbox_level_pending = None;
        }
    }
    fn death(&self, tank: &Tank, mode: Mode, actions: &mut Vec<HudAction>, s: f32) {
        let waiting_wave = mode == Mode::CoopPve;
        draw_rectangle(
            0.0,
            0.0,
            screen_width(),
            screen_height(),
            alpha(bg_color(), if waiting_wave { 0.25 } else { 0.6 }),
        );
        let x = screen_width() * 0.5;
        let y = screen_height() * 0.42;
        center_label("You died", x, y, 48.0 * s, RED);
        center_label(
            &format!("Score: {}    Kills: {}", tank.score, tank.kills),
            x,
            y + 44.0 * s,
            19.0 * s,
            text_color(),
        );
        let eliminated = tank.lives == 0 && mode == Mode::Survival;
        let status = if waiting_wave {
            "Reviving after this wave".to_owned()
        } else if eliminated {
            "Out of lives".to_owned()
        } else if tank.respawn_timer > 0.0 {
            format!("Respawn in {:.1}s", tank.respawn_timer)
        } else {
            "Press Enter or click to respawn".to_owned()
        };
        center_label(&status, x, y + 82.0 * s, 18.0 * s, CYAN);
        if !eliminated && !waiting_wave {
            center_label(
                &format!(
                    "Restart at level {}  ·  {} skill points",
                    tank.level, tank.points
                ),
                x,
                y + 108.0 * s,
                15.0 * s,
                text_color(),
            );
        }
        let action_y = y + 134.0 * s;
        if !eliminated
            && !waiting_wave
            && tank.respawn_timer <= 0.0
            && button(
                Rect::new(x - 134.0 * s, action_y, 268.0 * s, 43.0 * s),
                "Respawn",
                true,
                true,
                s,
            )
        {
            actions.push(HudAction::Respawn);
        }
        if small_button(
            Rect::new(x - 110.0 * s, action_y + 54.0 * s, 220.0 * s, 36.0 * s),
            "Return to menu",
            s,
        ) {
            actions.push(HudAction::Menu);
        }
    }
    fn spectator_bar(
        &self,
        tank: &Tank,
        watched: &Tank,
        mode: Mode,
        actions: &mut Vec<HudAction>,
        s: f32,
    ) {
        let width = 440. * s;
        let x = (screen_width() - width) * 0.5;
        let y = screen_height() - 140. * s;
        panel(Rect::new(x, y, width, 118. * s));
        center_label(
            &fitted_label(
                &format!(
                    "Spectating {}",
                    fitted_player_name(
                        watched,
                        18. * s,
                        width - 32. * s - label_width("Spectating ", 18. * s)
                    )
                ),
                18. * s,
                width - 28. * s,
            ),
            x + width * 0.5,
            y + 27. * s,
            18. * s,
            CYAN,
        );
        let status = if mode == Mode::CoopPve {
            "Reviving after this wave".to_owned()
        } else if mode == Mode::Survival && tank.lives == 0 {
            "Out of lives".to_owned()
        } else if tank.respawn_timer > 0. {
            format!("Respawn in {:.1}s", tank.respawn_timer)
        } else {
            "Enter to respawn".to_owned()
        };
        center_label(
            &format!("Left / Right to switch  ·  {status}"),
            x + width * 0.5,
            y + 49. * s,
            13. * s,
            text_color(),
        );
        if small_button(
            Rect::new(x + 14. * s, y + 66. * s, 92. * s, 34. * s),
            "< Previous",
            s * 0.75,
        ) {
            actions.push(HudAction::Spectate(-1));
        }
        let can_respawn = mode != Mode::CoopPve
            && !(mode == Mode::Survival && tank.lives == 0)
            && tank.respawn_timer <= 0.;
        if button(
            Rect::new(x + 116. * s, y + 66. * s, 208. * s, 34. * s),
            "Respawn",
            true,
            can_respawn,
            s * 0.8,
        ) {
            actions.push(HudAction::Respawn);
        }
        if small_button(
            Rect::new(x + width - 106. * s, y + 66. * s, 92. * s, 34. * s),
            "Next >",
            s * 0.75,
        ) {
            actions.push(HudAction::Spectate(1));
        }
    }
}
pub(crate) fn boss_banner_layout(width: f32, s: f32) -> Rect {
    let left = 320. * s;
    let right = width - 320. * s;
    let available = (right - left).max(1.);
    let w = (600. * s).min(available);
    Rect::new((left + right - w) * 0.5, 112. * s, w, 62. * s)
}
fn sandbox_layout(width: f32, height: f32, scale: f32) -> (Rect, f32) {
    let left = (22. + 246. + 8.) * scale;
    let right = width - (380. + 8.) * scale;
    let top = 178. * scale;
    let available = (right - left).max(1.);
    let vertical = (height - top - 16.).max(1.);
    let s = scale.min(available / 420.).min(vertical / 744.);
    let w = 420. * s;
    let h = 744. * s;
    (
        Rect::new(
            left + (available - w) * 0.5,
            top + (vertical - h) * 0.5,
            w,
            h,
        ),
        s,
    )
}

fn evolution_scale(count: usize, width: f32, height: f32, requested: f32) -> f32 {
    let rows = count.div_ceil(3).max(1) as f32;
    let available = (height - 370. * requested).max(1.);
    requested
        .min(width / 920.)
        .min(available / (rows * 99. + 25.))
}
fn evolution_cards(count: usize, width: f32, height: f32, requested: f32) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    let s = evolution_scale(count, width, height, requested);
    let columns = 3;
    let gap = 7.0 * s;
    let size = 92.0 * s;
    (0..count)
        .map(|index| {
            Rect::new(
                22.0 * requested + (index % columns) as f32 * (size + gap),
                110.0 * requested + (index / columns) as f32 * (size + gap),
                size,
                size,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_core::{World, WorldConfig};

    #[test]
    fn sandbox_and_boss_panels_fit_between_other_hud_regions_at_all_supported_scales() {
        for (width, height) in [
            (800_f32, 600_f32),
            (1024., 768.),
            (1280., 800.),
            (1920., 1080.),
            (2560., 1440.),
            (3440., 1440.),
            (800., 1200.),
        ] {
            for scale in [0.8, 1., 1.4] {
                let s = crate::ui_layout::scale(width, height, scale);
                let (sandbox, _) = sandbox_layout(width, height, s);
                let boss = boss_banner_layout(width, s);
                assert!(
                    sandbox.x >= 0.
                        && sandbox.y >= 0.
                        && sandbox.x + sandbox.w <= width
                        && sandbox.y + sandbox.h <= height - 16.
                );
                assert!(
                    sandbox.y + 0.001 >= boss.y + boss.h + 4. * s,
                    "{width}x{height} at {scale}"
                );
                assert!((boss.x + boss.w * 0.5 - width * 0.5).abs() < 0.001);
                assert!(boss.x >= 312. * s + 4. * s);
                assert!(boss.x + boss.w <= width - 242. * s - 12. * s);
                assert!(sandbox.x + 0.001 >= (22. + 246.) * s + 8. * s);
                assert!(sandbox.x + sandbox.w <= width - 380. * s - 8. * s + 0.001);
            }
        }
    }

    #[test]
    fn skill_panel_follows_earned_points_and_hides_after_spending() {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Sandbox,
                target_count: 0,
                ..Default::default()
            },
            Content::builtin(),
        );
        let player = world.add_player("Skills", false);
        world.sandbox_level(player, 1);
        let mut hud = Hud::new();
        hud.update_skill_panel(&world.tanks[0], false, 1.0 / 60.0, true);
        assert_eq!(hud.stat_visibility, 0.0);
        world.sandbox_level(player, 2);
        hud.update_skill_panel(&world.tanks[0], false, 1.0 / 60.0, true);
        assert_eq!(hud.stat_visibility, 0.0, "level two has no skill point");
        world.sandbox_level(player, 3);
        hud.update_skill_panel(&world.tanks[0], false, 1.0 / 60.0, true);
        assert_eq!(hud.stat_visibility, 1.0);
        assert!(world.upgrade_stat(player, STAT_ORDER[0]));
        assert_eq!(world.tanks[0].stats[1], 1, "key 1 upgrades regeneration");
        hud.update_skill_panel(&world.tanks[0], false, 1.0 / 60.0, true);
        assert_eq!(hud.stat_visibility, 0.0);
        world.sandbox_level(player, 4);
        hud.update_skill_panel(&world.tanks[0], false, 1.0 / 60.0, false);
        assert!(hud.stat_visibility > 0.0);
        for _ in 0..60 {
            hud.update_skill_panel(&world.tanks[0], false, 1.0 / 60.0, false);
        }
        assert_eq!(hud.stat_visibility, 1.0);
        world.tanks[0].alive = false;
        hud.update_skill_panel(&world.tanks[0], false, 1.0 / 60.0, true);
        assert_eq!(hud.stat_visibility, 0.0);
    }

    #[test]
    fn skill_names_follow_the_weapon_and_not_the_class_name() {
        let content = Content::builtin();
        let names = |id: &str| stat_hints(content.class(id));
        let drone = names("hive");
        assert_eq!(drone[3].name, "Drone Speed");
        assert_eq!(drone[4].name, "Drone Health");
        assert_eq!(drone[5].name, "Drone Damage");
        assert_eq!(drone[6].name, "Drone Spawn Rate");
        let factory = names("foundry");
        assert_eq!(factory[3].name, "Minion Speed");
        assert_eq!(factory[4].name, "Minion Health");
        assert!(factory[6].description.contains("Spawn"));
        assert!(factory[6].description.contains("fire"));
        let trap = names("anchor");
        assert_eq!(trap[4].name, "Trap Health");
        assert_eq!(names("monolith")[4].name, "Barrier Health");
        assert_eq!(names("vector")[3].name, "Bullet Speed");
    }

    #[test]
    fn mixed_classes_account_for_both_weapons_and_deployables() {
        let content = Content::builtin();
        let mixed = stat_hints(content.class("satellite"));
        assert_eq!(mixed[4].name, "Pen. / Unit Health");
        assert!(mixed[4].description.contains("Shots"));
        assert!(mixed[4].description.contains("drones"));
        let fabricator = stat_hints(content.class("assembly"));
        assert_eq!(fabricator[4].name, "Unit Health");
        assert!(fabricator[3].description.contains("stationary"));
        let smasher = stat_hints(content.class("bulwark"));
        assert!(smasher[3].description.contains("no guns"));
    }

    #[test]
    fn class_choices_fit_small_and_large_windows() {
        for (width, height) in [
            (800., 600.),
            (1280., 800.),
            (1920., 1080.),
            (3840., 2160.),
            (800., 1200.),
        ] {
            for preference in [0.8, 1., 1.4] {
                let scale = crate::ui_layout::scale(width, height, preference);
                let card_scale = evolution_scale(8, width, height, scale);
                let cards = evolution_cards(8, width, height, scale);
                assert_eq!(cards.len(), 8);
                for card in &cards {
                    assert!(
                        card.x >= 0.
                            && card.y >= 0.
                            && card.x + card.w < width
                            && card.y - 29. * card_scale >= 78. * scale
                            && card.y + card.h + 32. * card_scale <= height - 260. * scale + 0.001
                    );
                }
                for pair in cards.windows(2) {
                    assert!(!pair[0].overlaps(&pair[1]));
                }
            }
        }
    }
}
