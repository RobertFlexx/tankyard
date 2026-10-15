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
                    name: "Trap Launch Speed",
                    description: "Launch traps farther and faster; placed traps don't move on their own.",
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
    hints
}

pub enum HudAction {
    Respawn,
    Stat(usize),
    Evolve(String),
    ToggleTheme,
    Encyclopedia,
    Pause,
    Controls,
    Menu,
    SandboxLevel(u8),
    SandboxClass(String),
    Bot,
    Boss(BossKind),
    Invulnerable(bool),
    Speed(f32),
}
pub struct Hud {
    pub practice: bool,
    pub evolution: bool,
    pub sandbox: bool,
    pub sandbox_page: usize,
    pub invulnerable: bool,
    pub boss_index: usize,
    pub speed: f32,
    stat_visibility: f32,
    offered_class: String,
    ranking_tick: Option<u64>,
    ranking: Vec<usize>,
}
impl Hud {
    pub fn new() -> Self {
        Self {
            practice: false,
            evolution: false,
            sandbox: false,
            sandbox_page: 0,
            invulnerable: false,
            boss_index: 0,
            speed: 1.0,
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
            if cards.iter().any(|r| contains(*r))
                || contains(Rect::new(m, 77.0 * s, 290.0 * s, 29.0 * s))
                || cards.last().is_some_and(|r| {
                    contains(Rect::new(m, r.y + r.h + 8.0 * s, 134.0 * s, 24.0 * s))
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
        let mut actions = Vec::new();
        let w = screen_width();
        let h = screen_height();
        let m = 22.0 * s;
        let Some(tank) = snapshot.tanks.iter().find(|t| t.id == local) else {
            center_label("Connecting…", w * 0.5, h * 0.5, 26.0 * s, CYAN);
            return actions;
        };
        let modal = self.sandbox || !tank.alive || snapshot.match_state.finished;
        let sandbox_controls = snapshot.config.mode == Mode::Sandbox && !self.practice;
        if !tank.alive {
            self.evolution = false;
            self.offered_class.clear();
        }
        let class = content.class(&tank.class);
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
        if snapshot.config.mode.teams() > 0 {
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
                let hovered = contains(r) && !modal && tank.points > 0;
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
            let x = (stats_x + stat_w + 10.0 * s).min(w - 274.0 * s);
            let row = stat_order.iter().position(|&i| i == index).unwrap_or(0);
            let y = (stats_y + row as f32 * 26.0 * s).min(h - 125.0 * s);
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
        let xp_width = (w * 0.36).min(430.0 * s);
        let xp_x = (w - xp_width) * 0.5;
        let next = xp_for_level((tank.level + 1).min(45));
        let prev = xp_for_level(tank.level);
        let fraction = if tank.level >= 45 {
            1.0
        } else {
            tank.xp.saturating_sub(prev) as f32 / (next - prev).max(1) as f32
        };
        let xp_y = h - 33.0 * s;
        arcade_center(
            &fitted_label(&tank.name, 27.0 * s, xp_width - 12.0 * s),
            w * 0.5,
            xp_y - 45.0 * s,
            27.0 * s,
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
            center_label(
                &format!(
                    "[{}] {:?}  {}",
                    settings.bindings[4],
                    class.ability,
                    if ready {
                        "READY".into()
                    } else {
                        format!("{:.1}s", tank.ability_cooldown)
                    }
                ),
                w * 0.5,
                status_y,
                15.0 * s,
                if ready { CYAN } else { muted_color() },
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
            self.sandbox_overlay(content, &mut actions, s);
        }
        if !tank.alive && !snapshot.match_state.finished {
            self.death(
                tank,
                snapshot.config.mode == Mode::Survival,
                &mut actions,
                s,
            );
        }
        if snapshot.match_state.finished {
            draw_rectangle(0.0, 0.0, w, h, alpha(bg_color(), 0.77));
            center_label("Game over", w * 0.5, h * 0.4, 44.0 * s, AMBER);
            let result = match snapshot.match_state.winner {
                Some(0) => "The arena has a champion".into(),
                Some(team) => format!("Team {team} wins"),
                None => "Match finished".into(),
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
        arcade_center("Leaderboard", x + width * 0.5, y + 18.0 * s, 19.0 * s);
        if self.ranking_tick != Some(snapshot.tick) || self.ranking.len() != snapshot.tanks.len() {
            self.ranking.clear();
            self.ranking.extend(0..snapshot.tanks.len());
            self.ranking.sort_unstable_by(|&a, &b| {
                snapshot.tanks[b]
                    .score
                    .cmp(&snapshot.tanks[a].score)
                    .then_with(|| snapshot.tanks[a].id.cmp(&snapshot.tanks[b].id))
            });
            self.ranking_tick = Some(snapshot.tick);
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
            let name = fitted_label(
                &tank.name,
                12.0 * s,
                width - 12.0 * s - label_width(&score, 12.0 * s),
            );
            arcade_center(
                &format!("{name}{score}"),
                x + width * 0.5,
                by + 14.0 * s,
                12.0 * s,
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
        let y = screen_height() - height - 22.0 * s;
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
        let s = s.min(screen_height() / 760.0).min(screen_width() / 920.0);
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
        let y = 106.0 * s;
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
    fn sandbox_overlay(&mut self, content: &Content, actions: &mut Vec<HudAction>, s: f32) {
        let s = s.min(screen_height() / 760.0).min(screen_width() / 500.0);
        let width = 380.0 * s;
        let x = (screen_width() - width) * 0.5;
        let y = 65.0 * s;
        let height = 650.0 * s;
        panel(Rect::new(x, y, width, height));
        label("Sandbox", x + 20.0 * s, y + 32.0 * s, 24.0 * s, CYAN);
        label(
            "SET LEVEL",
            x + 20.0 * s,
            y + 63.0 * s,
            12.0 * s,
            muted_color(),
        );
        for (index, level) in [1, 15, 30, 45].into_iter().enumerate() {
            if small_button(
                Rect::new(
                    x + 20.0 * s + index as f32 * 85.0 * s,
                    y + 101.0 * s,
                    75.0 * s,
                    32.0 * s,
                ),
                &level.to_string(),
                s,
            ) {
                actions.push(HudAction::SandboxLevel(level));
            }
        }
        let page_count = content.classes.len().div_ceil(6);
        self.sandbox_page = self.sandbox_page.min(page_count.saturating_sub(1));
        label(
            "Choose tank",
            x + 20.0 * s,
            y + 145.0 * s,
            12.0 * s,
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
                    x + 20.0 * s,
                    y + 159.0 * s + row as f32 * 37.0 * s,
                    width - 40.0 * s,
                    31.0 * s,
                ),
                &class.name,
                false,
                true,
                s * 0.75,
            ) {
                actions.push(HudAction::SandboxClass(class.id.clone()));
            }
        }
        if small_button(
            Rect::new(x + 20.0 * s, y + 390.0 * s, 65.0 * s, 30.0 * s),
            "‹",
            s,
        ) {
            self.sandbox_page = self.sandbox_page.saturating_sub(1);
        }
        center_label(
            &format!("{} / {}", self.sandbox_page + 1, page_count),
            x + width * 0.5,
            y + 411.0 * s,
            14.0 * s,
            muted_color(),
        );
        if small_button(
            Rect::new(x + width - 85.0 * s, y + 390.0 * s, 65.0 * s, 30.0 * s),
            "›",
            s,
        ) {
            self.sandbox_page = (self.sandbox_page + 1).min(page_count.saturating_sub(1));
        }
        if toggle(
            Rect::new(x + 20.0 * s, y + 437.0 * s, width - 40.0 * s, 30.0 * s),
            "Invulnerability",
            &mut self.invulnerable,
            s,
        ) {
            actions.push(HudAction::Invulnerable(self.invulnerable));
        }
        if small_button(
            Rect::new(x + 20.0 * s, y + 480.0 * s, 155.0 * s, 31.0 * s),
            "Add AI tank (host)",
            s * 0.9,
        ) {
            actions.push(HudAction::Bot);
        }
        if small_button(
            Rect::new(x + 190.0 * s, y + 480.0 * s, 170.0 * s, 31.0 * s),
            &format!("Speed {:.1}× (local)", self.speed),
            s * 0.9,
        ) {
            self.speed = if self.speed < 1.0 {
                1.0
            } else if self.speed < 2.0 {
                2.0
            } else {
                0.5
            };
            actions.push(HudAction::Speed(self.speed));
        }
        let bosses = [
            BossKind::Sentinel,
            BossKind::Prism,
            BossKind::Foundry,
            BossKind::Tempest,
            BossKind::Bastion,
            BossKind::Eclipse,
        ];
        if small_button(
            Rect::new(x + 20.0 * s, y + 529.0 * s, 210.0 * s, 31.0 * s),
            &format!("Boss: {:?}", bosses[self.boss_index]),
            s * 0.9,
        ) {
            self.boss_index = (self.boss_index + 1) % 6;
        }
        if small_button(
            Rect::new(x + 242.0 * s, y + 529.0 * s, 118.0 * s, 31.0 * s),
            "Spawn boss",
            s * 0.9,
        ) {
            actions.push(HudAction::Boss(bosses[self.boss_index]));
        }
        label(
            "Online tools require server sandbox permission.",
            x + 20.0 * s,
            y + 592.0 * s,
            12.0 * s,
            muted_color(),
        );
        if small_button(
            Rect::new(x + 20.0 * s, y + 609.0 * s, width - 40.0 * s, 27.0 * s),
            "Close [T]",
            s * 0.9,
        ) {
            self.sandbox = false;
        }
    }
    fn death(&self, tank: &Tank, survival: bool, actions: &mut Vec<HudAction>, s: f32) {
        draw_rectangle(
            0.0,
            0.0,
            screen_width(),
            screen_height(),
            alpha(bg_color(), 0.6),
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
        let eliminated = tank.lives == 0 && survival;
        let status = if eliminated {
            "Out of lives".to_owned()
        } else if tank.respawn_timer > 0.0 {
            format!("Respawn in {:.1}s", tank.respawn_timer)
        } else {
            "Press Enter or click to respawn".to_owned()
        };
        center_label(&status, x, y + 82.0 * s, 18.0 * s, CYAN);
        if !eliminated {
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
}
fn evolution_cards(count: usize, width: f32, height: f32, s: f32) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    let s = s.min(width / 920.0).min(height / 760.0);
    let columns = 3;
    let gap = 7.0 * s;
    let size = 92.0 * s;
    (0..count)
        .map(|index| {
            Rect::new(
                22.0 * s + (index % columns) as f32 * (size + gap),
                106.0 * s + (index / columns) as f32 * (size + gap),
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
        assert_eq!(hud.stat_visibility, 1.0);
        assert!(world.upgrade_stat(player, STAT_ORDER[0]));
        assert_eq!(world.tanks[0].stats[1], 1, "key 1 upgrades regeneration");
        hud.update_skill_panel(&world.tanks[0], false, 1.0 / 60.0, true);
        assert_eq!(hud.stat_visibility, 0.0);
        world.sandbox_level(player, 3);
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
        for (width, height, scale) in [(800., 600., 0.7), (1280., 800., 1.), (3840., 2160., 2.6)] {
            let cards = evolution_cards(8, width, height, scale);
            assert_eq!(cards.len(), 8);
            for card in &cards {
                assert!(
                    card.x >= 0.
                        && card.y >= 0.
                        && card.x + card.w < width
                        && card.y + card.h < height * 0.64
                );
            }
            for pair in cards.windows(2) {
                assert!(!pair[0].overlaps(&pair[1]));
            }
        }
    }
}
