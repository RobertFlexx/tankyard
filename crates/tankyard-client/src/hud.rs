mod boss_bar;
mod sandbox;

pub(crate) use boss_bar::Bar as BossBar;

use crate::graphics::{draw_circle, draw_poly};
use crate::ui::RED;
use crate::{render, settings::Settings, ui::*};
use macroquad::prelude::*;
use std::fmt::Write as _;
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
    ToggleBoard,
}

/// How much of a scoreboard row its little tank takes, at scale 1.
const BOARD_ICON: f32 = 24.;
/// Scoreboard rows that clear what the tank menu docks under the board.
const DOCKED_ROWS: usize = 9;

/// One line of the scoreboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BoardRow {
    /// Place in the standings, from 0.
    place: usize,
    /// Index into the snapshot's tanks.
    tank: usize,
    /// Index into the content's classes, looked up once per tick.
    class: Option<usize>,
}

/// Where the scoreboard and the tab that folds it away sit. At `fold` 1 the
/// board has gone past the right edge and only the tab is left on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
struct BoardLayout {
    x: f32,
    y: f32,
    width: f32,
    tab: Rect,
}

fn board_layout(screen: f32, s: f32, fold: f32) -> BoardLayout {
    let open = screen - 242. * s;
    let x = open + (screen - open) * fold.clamp(0., 1.);
    let y = 22. * s;
    let (tab_width, tab_height) = (20. * s, 28. * s);
    BoardLayout {
        x,
        y,
        width: 220. * s,
        tab: Rect::new(x - tab_width - 6. * s, y + s, tab_width, tab_height),
    }
}

/// The top of scoreboard row `row`. The eleventh row is the local player's
/// when they're outside the top ten, set slightly apart from it.
fn board_row_y(y: f32, row: usize, s: f32) -> f32 {
    y + (30. + row as f32 * 24. + if row >= 10 { 5. } else { 0. }) * s
}

/// Who gets a line: the top ten, plus you if you're further down.
fn board_rows(
    ranking: &[usize],
    is_local: impl Fn(usize) -> bool,
    class: impl Fn(usize) -> Option<usize>,
    rows: &mut Vec<BoardRow>,
) {
    rows.clear();
    let row = |(place, &tank): (usize, &usize)| BoardRow {
        place,
        tank,
        class: class(tank),
    };
    rows.extend(ranking.iter().enumerate().take(10).map(row));
    rows.extend(
        ranking
            .iter()
            .enumerate()
            .skip(10)
            .find(|(_, tank)| is_local(**tank))
            .map(row),
    );
}

/// The lines to draw with `room` of them to spare, in order: the top of the
/// board and always your own. The tank menu leaves room for fewer, because it
/// docks the chat and party rail right under the board.
fn shown_rows(
    rows: &[BoardRow],
    room: usize,
    is_local: impl Fn(&BoardRow) -> bool,
) -> impl Iterator<Item = &BoardRow> {
    let own = rows.iter().skip(room).find(|line| is_local(line));
    let top = if own.is_some() {
        room.saturating_sub(1)
    } else {
        room
    };
    rows.iter().take(top).chain(own)
}

pub struct Hud {
    pub spectating: Option<EntityId>,
    /// The local tank's most recent killer, from reliable kill events.
    pub killed_by: Option<EntityId>,
    /// The local tank's death count, from the same events.
    pub deaths: u32,
    /// The death the killer card last played for.
    cards_shown: u32,
    /// The killer card on screen: who, and when it started.
    card: Option<(EntityId, f64)>,
    /// When the local tank last died.
    died_at: f64,
    /// The current map's palette accent, which tints minimap cover.
    pub palette: Color,
    pub practice: bool,
    pub editor_playtest: bool,
    pub evolution: bool,
    pub sandbox: bool,
    pub sandbox_page: usize,
    sandbox_search: String,
    sandbox_search_active: bool,
    sandbox_tier: Option<u8>,
    sandbox_scroll: f32,
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
    /// The lines the scoreboard is showing.
    board: Vec<BoardRow>,
    /// How far the scoreboard is folded away, 0 to 1. Unset until the first
    /// frame, so that a board hidden last session doesn't slide out on load.
    board_fold: Option<f32>,
    /// How many of its lines the board drew last frame.
    board_shown: usize,
    board_tab_glow: f32,
    /// Axiom and the headline boss keep separate health and targeting fades.
    boss_bars: [boss_bar::Animation; 2],
}
impl Hud {
    /// A fresh HUD for new content mid-game, which still remembers the
    /// deaths it already showed a killer card for.
    pub fn rebuilt(&self) -> Self {
        Self {
            deaths: self.deaths,
            cards_shown: self.cards_shown,
            card: self.card,
            editor_playtest: self.editor_playtest,
            board_fold: self.board_fold,
            board_shown: 0,
            ..Self::new()
        }
    }
    pub fn new() -> Self {
        Self {
            spectating: None,
            killed_by: None,
            deaths: 0,
            cards_shown: 0,
            card: None,
            died_at: 0.,
            palette: tankyard_client::map_model::palette("slate"),
            practice: false,
            editor_playtest: false,
            evolution: false,
            sandbox: false,
            sandbox_page: 0,
            sandbox_search: String::new(),
            sandbox_search_active: false,
            sandbox_tier: None,
            sandbox_scroll: 0.,
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
            board: Vec::with_capacity(11),
            board_fold: None,
            board_shown: 0,
            board_tab_glow: 0.,
            boss_bars: Default::default(),
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
        if (tank.points > 0 || tank.creator_boss)
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
        let board = board_layout(screen_width(), s, self.board_fold.unwrap_or(0.));
        contains(board.tab)
            // Only as tall as the rows it has, so empty arena stays shootable.
            || contains(Rect::new(
                board.x,
                board.y,
                board.width,
                board_row_y(board.y, self.board_shown, s) - board.y,
            ))
            || contains(Rect::new(
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
        map_remaining: Option<u32>,
        s: f32,
        view: &render::View,
    ) -> Vec<HudAction> {
        pointer_scope(100);
        let mut actions = Vec::new();
        let w = screen_width();
        let h = screen_height();
        let m = 22.0 * s;
        let Some(tank) = snapshot.tanks.iter().find(|t| t.id == local) else {
            center_label("Connecting…", w * 0.5, h * 0.5, 26.0 * s, accent());
            return actions;
        };
        let sandbox_controls = snapshot.config.mode == Mode::Sandbox && !self.practice;
        if !self.sandbox {
            self.sandbox_search_active = false;
            self.sandbox_level_pending = None;
        }
        if !sandbox_controls || !tank.alive || snapshot.match_state.finished {
            self.sandbox = false;
            self.sandbox_level_pending = None;
        }
        let modal = self.sandbox || !tank.alive || snapshot.match_state.finished;
        if !tank.alive || tank.creator_boss {
            self.evolution = false;
            self.offered_class.clear();
        }
        let class = content.class(&tank.class);
        if tank.alive
            && !modal
            && let Some(class) = class
            && tank.pocket <= 0.
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
                    alpha(accent(), 0.75)
                } else {
                    accent()
                },
            );
        }
        let available = !tank.creator_boss
            && class.is_some_and(|class| {
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
        let corner_text = text_after_shapes();
        if small_button(
            Rect::new(m, m, 110.0 * s, 26.0 * s),
            "Tanks [Tab]",
            s * 0.76,
        ) && !modal
            && !tank.creator_boss
        {
            self.evolution = !self.evolution;
        }
        if small_button(
            Rect::new(m + 116.0 * s, m, 110.0 * s, 26.0 * s),
            if self.editor_playtest {
                "Editor [Esc]"
            } else {
                "Menu [Esc]"
            },
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
        drop(corner_text);
        let header_width = (w - 548. * s).max(120. * s);
        let mode_name = if tank.pocket > 0. {
            "Backrooms"
        } else {
            snapshot
                .config
                .mode_name
                .as_deref()
                .unwrap_or(snapshot.config.mode.name())
        };
        let title = if tank.pocket <= 0. && snapshot.config.mode == Mode::Infection {
            format!(
                "{mode_name} · {}",
                if tank.team == 2 {
                    "Infected"
                } else {
                    "Survivor"
                }
            )
        } else {
            mode_name.to_owned()
        };
        arcade_center(
            &fitted_label(&title, 19. * s, header_width),
            w * 0.5,
            m + 16. * s,
            19. * s,
        );
        if matches!(snapshot.config.mode, Mode::Infection | Mode::KingOfTheHill) {
            let remaining = snapshot.match_state.arena.remaining.ceil().max(0.) as u32;
            let text = if snapshot.config.mode == Mode::Infection {
                format!(
                    "Survivors {} · Infected {} · {}:{:02}",
                    snapshot.match_state.scores[0],
                    snapshot.match_state.scores[1],
                    remaining / 60,
                    remaining % 60
                )
            } else {
                format!(
                    "{} : {} · Hill moves in {}:{:02}",
                    snapshot.match_state.scores[0],
                    snapshot.match_state.scores[1],
                    remaining / 60,
                    remaining % 60
                )
            };
            arcade_center(
                &fitted_label(&text, 15. * s, header_width),
                w * 0.5,
                m + 44. * s,
                15. * s,
            );
        } else if snapshot.config.mode.teams() > 1 {
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
                &fitted_label(&snapshot.match_state.announcement, 17. * s, header_width),
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
        if self.stat_visibility > 0.0 && !tank.creator_boss {
            arcade_label(
                &format!("Skill points: {}", tank.points),
                stats_x + 6.0 * s,
                stats_y - 9.0 * s,
                16.0 * s,
            );
            let covered = self.evolution
                && class.is_some_and(|class| {
                    evolution_cards(class.upgrades.len(), w, h, s)
                        .iter()
                        .any(|card| contains(*card))
                });
            let _text = text_after_shapes();
            for (row, &index) in stat_order.iter().enumerate() {
                let r = Rect::new(stats_x, stats_y + row as f32 * 26.0 * s, stat_w, 22.0 * s);
                let hovered = contains(Rect::new(r.x, r.y, r.w, 25. * s))
                    && !modal
                    && !covered
                    && tank.points > 0;
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
                if hovered && can_upgrade && take_click() {
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
        let leader_score = self.leaderboard(snapshot, content, local, settings, s);
        if self.board_tab(snapshot, local, settings, s) {
            actions.push(HudAction::ToggleBoard);
        }
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
            let bars_text = text_after_shapes();
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
                    &if tank.creator_boss {
                        "Axiom · Event boss".to_owned()
                    } else {
                        format!(
                            "Lvl {} {}",
                            tank.level,
                            class.map(|c| c.name.as_str()).unwrap_or("Tank")
                        )
                    },
                    16.0 * s,
                    xp_width - 16.0 * s,
                ),
                w * 0.5,
                xp_y + 17.0 * s,
                16.0 * s,
            );
            drop(bars_text);
            // Stack status lines above the name, clear of both progress bars.
            let mut status_y = xp_y - 74.0 * s;
            if let Some(class) = class
                && class.ability != Ability::None
            {
                let ready = tank.ability_cooldown <= 0.0;
                let parrying = class.ability.is_parry() && tank.shield > 0.0;
                // A rift gun shows which end it opens next, then how long
                // its pair stays open.
                let open_pair = snapshot
                    .rifts
                    .iter()
                    .filter(|rift| rift.owner == tank.id && rift.partner != 0 && rift.life > 0.)
                    .map(|rift| rift.life)
                    .reduce(f32::min);
                let rift_armed = tank.rift_shots > 0;
                let status = format!(
                    "[{}] {}  {}",
                    settings.bindings[4],
                    class.ability.name(),
                    if tank.pocket > 0. {
                        format!("{:.1}s · REPAIRING", tank.pocket)
                    } else if parrying {
                        "ACTIVE".into()
                    } else if rift_armed {
                        format!(
                            "FIRE THE {} · {:.1}s",
                            match crate::rifts::next_end(snapshot, tank.id) {
                                tankyard_core::RiftEnd::Entry => "ENTRY",
                                tankyard_core::RiftEnd::Exit => "EXIT",
                            },
                            tank.rift_armed
                        )
                    } else if let Some(life) = open_pair {
                        format!(
                            "{} · rifts close in {:.0}s",
                            if ready {
                                "READY".to_owned()
                            } else {
                                format!("{:.1}s", tank.ability_cooldown)
                            },
                            life.ceil()
                        )
                    } else if ready {
                        "READY".into()
                    } else {
                        format!("{:.1}s", tank.ability_cooldown)
                    }
                );
                if tank.pocket > 0. {
                    // The outline stays readable over both carpet and lamps.
                    arcade_center(&status, w * 0.5, status_y, 15.0 * s);
                } else {
                    center_label(
                        &status,
                        w * 0.5,
                        status_y,
                        15.0 * s,
                        if parrying {
                            AMBER
                        } else if rift_armed {
                            crate::rifts::color(crate::rifts::next_end(snapshot, tank.id), false)
                        } else if ready {
                            accent()
                        } else {
                            muted_color()
                        },
                    );
                }
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
                        accent()
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
            if snapshot
                .config
                .base_protected(tank.team, tank.pos, tank.radius)
            {
                center_label(
                    "Home base protection",
                    w * 0.5,
                    status_y,
                    12.0 * s,
                    accent(),
                );
            } else if tank.spawn_immunity && tank.protection > 0.0 {
                center_label(
                    &format!(
                        "Spawn protection · {}s · move to engage",
                        tank.protection.ceil() as u32
                    ),
                    w * 0.5,
                    status_y,
                    12.0 * s,
                    accent(),
                );
            }
        }
        if settings.show_fps || map_remaining.is_some() {
            let footer = match map_remaining {
                Some(seconds) => format!(
                    "Map in {}:{:02}:{:02}{}",
                    seconds / 3600,
                    seconds / 60 % 60,
                    seconds % 60,
                    if settings.show_fps {
                        format!("  ·  {} FPS", get_fps())
                    } else {
                        String::new()
                    }
                ),
                None => format!("{} FPS  ·  tick {} / 60 Hz", get_fps(), snapshot.tick),
            };
            label(&footer, m, h - 5.0 * s, 11.0 * s, muted_color());
        }
        if settings.show_fps {
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
                    accent(),
                );
            }
        } else if let Some((ping, _, _, _)) = diagnostics {
            right_label(
                &format!("{ping:.0} ms"),
                w - m,
                h - 66.0 * s,
                10.0 * s,
                muted_color(),
            );
        }
        if self.evolution && tank.alive && !snapshot.match_state.finished && !self.sandbox {
            self.evolution_overlay(tank, content, &mut actions, s);
        }
        if sandbox_controls && self.sandbox {
            self.sandbox_overlay(tank, content, &mut actions, s);
        }
        if tank.alive {
            self.card = None;
        } else if self.deaths != self.cards_shown {
            self.cards_shown = self.deaths;
            self.died_at = get_time();
            self.card = self
                .killed_by
                .filter(|&killer| killer != local && snapshot.tanks.iter().any(|t| t.id == killer))
                .map(|killer| (killer, get_time()));
        }
        let card = self.card.and_then(|(killer, started)| {
            let age = (get_time() - started) as f32;
            let killer = snapshot.tanks.iter().find(|t| t.id == killer)?;
            (age < CARD_END).then_some((killer, age))
        });
        if !tank.alive && !snapshot.match_state.finished {
            if let Some((killer, age)) = card {
                killer_card(
                    tank,
                    killer,
                    snapshot.config.mode,
                    content,
                    settings,
                    age,
                    s,
                );
            } else if let Some(watched) = snapshot
                .tanks
                .iter()
                .find(|t| Some(t.id) == self.spectating && t.alive)
            {
                self.spectator_bar(tank, watched, snapshot.config.mode, &mut actions, s);
                self.gloat(snapshot, content, s);
            } else {
                self.death(tank, snapshot, content, &mut actions, s);
            }
        }
        if snapshot.match_state.finished {
            draw_rectangle(0.0, 0.0, w, h, alpha(bg_color(), 0.77));
            center_label("Game over", w * 0.5, h * 0.4, 44.0 * s, AMBER);
            let result = match (snapshot.config.mode, snapshot.match_state.winner) {
                (Mode::CoopPve, Some(1)) => "All ten waves cleared".into(),
                (Mode::CoopPve, _) => "Squad eliminated".into(),
                (Mode::Infection, Some(1)) => "The survivors contained the outbreak".into(),
                (Mode::Infection, Some(2)) => "The infected took the arena".into(),
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
        content: &Content,
        local: EntityId,
        settings: &Settings,
        s: f32,
    ) -> u32 {
        let coop = snapshot.config.mode == Mode::CoopPve;
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
            board_rows(
                &self.ranking,
                |index| snapshot.tanks[index].id == local,
                |index| {
                    content
                        .classes
                        .iter()
                        .position(|class| class.id == snapshot.tanks[index].class)
                },
                &mut self.board,
            );
            self.ranking_tick = Some(revision);
        }
        let leading = self
            .ranking
            .first()
            .map_or(1, |&index| snapshot.tanks[index].score.max(1));
        let target = if settings.leaderboard_hidden { 1. } else { 0. };
        let fold = self.board_fold.get_or_insert(target);
        ease_toward(fold, target, 15.);
        let fold = *fold;
        if fold >= 1. {
            return leading;
        }
        let BoardLayout { x, y, width, .. } = board_layout(screen_width(), s, fold);
        arcade_center(
            if coop { "Squad" } else { "Leaderboard" },
            x + width * 0.5,
            y + 18.0 * s,
            19.0 * s,
        );
        let text = text_after_shapes();
        let mut score = String::with_capacity(24);
        let room = if self.evolution || self.sandbox {
            DOCKED_ROWS
        } else {
            self.board.len()
        };
        let lines = shown_rows(&self.board, room, |line| {
            snapshot.tanks[line.tank].id == local
        });
        let mut drawn = 0;
        for (row, line) in lines.enumerate() {
            drawn = row + 1;
            let tank = &snapshot.tanks[line.tank];
            let by = board_row_y(y, row, s);
            let color = render::team_color(tank.team, tank.id == local, settings.colorblind);
            // The tank itself sits ahead of its bar, on the arena it plays on.
            let bar_x = x + BOARD_ICON * s;
            let bar_width = width - BOARD_ICON * s;
            arcade_track(
                Rect::new(bar_x, by, bar_width, 20.0 * s),
                tank.score as f32 / leading as f32,
                color,
            );
            let icon = vec2(x + 9.5 * s, by + 10.5 * s);
            let body = if tank.alive { color } else { muted_color() };
            if tank.creator_boss {
                tankyard_client::boss_model::axiom_geometry(
                    icon,
                    6.4 * s,
                    -0.5,
                    0.,
                    &mut render::draw_model_primitive,
                );
            } else if let Some(class) = line.class.and_then(|class| content.classes.get(class)) {
                render::class_model(class, icon, 7. * s, -0.5, 0., 0., body, 0.);
            }
            score.clear();
            let _ = if line.place >= 10 {
                write!(score, " - {} (#{})", tank.score, line.place + 1)
            } else {
                write!(score, " - {}", tank.score)
            };
            arcade_player_label(
                tank,
                &score,
                vec2(bar_x + bar_width * 0.5, by + 14. * s),
                bar_width - 12. * s,
                12. * s,
            );
        }
        drop(text);
        self.board_shown = drawn;
        leading
    }
    /// The little tab with the arrow that folds the scoreboard away. Returns
    /// true when clicked.
    fn board_tab(
        &mut self,
        snapshot: &Snapshot,
        local: EntityId,
        settings: &Settings,
        s: f32,
    ) -> bool {
        let fold = self.board_fold.unwrap_or(0.);
        let rect = board_layout(screen_width(), s, fold).tab;
        let hovered = contains(rect);
        ease_toward(&mut self.board_tab_glow, f32::from(u8::from(hovered)), 18.);
        let glow = self.board_tab_glow;
        crate::hud_widgets::rounded(rect, 7. * s, Color::new(0.333, 0.333, 0.333, 1.));
        let edge = (2.4 * s).max(1.);
        crate::hud_widgets::rounded(
            Rect::new(
                rect.x + edge,
                rect.y + edge,
                rect.w - edge * 2.,
                rect.h - edge * 2.,
            ),
            5. * s,
            mix(Color::new(0.40, 0.40, 0.40, 1.), accent(), glow * 0.75),
        );
        // It points where the board will go: right to put it away, left to
        // fetch it back, and turns over on the way.
        let heading = fold * std::f32::consts::PI;
        let way = vec2(heading.cos(), heading.sin());
        let side = vec2(-way.y, way.x);
        let size = (5.6 + 0.9 * glow) * s;
        let nudge = rect.center() + way * (0.7 + 0.7 * glow) * s;
        let corner = |reach: f32| {
            [
                nudge + way * reach,
                nudge - way * reach * 0.72 + side * reach * 0.94,
                nudge - way * reach * 0.72 - side * reach * 0.94,
            ]
        };
        if !crate::accessibility::contrast() {
            let [a, b, c] = corner(size + 2.3 * s);
            draw_triangle(a, b, c, Color::new(0.15, 0.15, 0.16, 1.));
        }
        let [a, b, c] = corner(size);
        draw_triangle(a, b, c, WHITE);
        // The arrow alone doesn't say what it does, or where you stand.
        let free = (0..6).all(|index| settings.key(index) != KeyCode::L);
        let note = if hovered {
            let word = if settings.leaderboard_hidden {
                "Show"
            } else {
                "Hide"
            };
            Some(if free {
                format!("{word} [L]")
            } else {
                word.to_owned()
            })
        } else if fold > 0.6 && snapshot.config.mode != Mode::CoopPve {
            self.ranking
                .iter()
                .position(|&index| snapshot.tanks[index].id == local)
                .map(|place| format!("#{}", place + 1))
        } else {
            None
        };
        if let Some(note) = note {
            let size = 15. * s;
            arcade_label(
                &note,
                rect.x - 7. * s - label_width(&note, size),
                rect.y + rect.h * 0.5 + 5.5 * s,
                size,
            );
        }
        hovered && take_click()
    }
    fn minimap(
        &self,
        snapshot: &Snapshot,
        local: EntityId,
        settings: &Settings,
        s: f32,
        view: &render::View,
    ) {
        if snapshot
            .tanks
            .iter()
            .any(|tank| tank.id == local && tank.pocket > 0.)
        {
            return;
        }
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
            alpha(accent(), 0.8),
        );
        for team in 1..=snapshot.config.mode.teams() {
            if let Some(base) = snapshot.config.team_base(team) {
                let p = point(tankyard_core::V2::new(base.x, base.y));
                draw_rectangle(
                    p.x,
                    p.y,
                    base.w * sx,
                    base.h * sy,
                    alpha(render::team_color(team, false, settings.colorblind), 0.25),
                );
            }
        }
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
        let wall_color = mix(line_color(), self.palette, 0.35);
        for wall in &snapshot.walls {
            draw_rectangle(
                x + 6.0 * s + wall.x * sx,
                y + 6.0 * s + wall.y * sy,
                wall.w * sx,
                wall.h * sy,
                wall_color,
            );
        }
        for objective in &snapshot.objectives {
            let p = point(objective.pos);
            if objective.team > 0 && crate::accessibility::team_symbols() {
                render::team_marker(objective.team, false, p, 3.5 * s);
            } else {
                draw_circle_lines(p.x, p.y, 5.0 * s, 1.0, AMBER);
            }
        }
        for boss in &snapshot.bosses {
            let p = point(boss.pos);
            draw_poly(p.x, p.y, 4, 4.0 * s, 45.0, RED);
        }
        for tank in snapshot.tanks.iter().filter(|t| t.tangible()) {
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
                if unlocked { accent() } else { muted_color() },
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
            if hovered && take_click() {
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
    /// True while the killer card holds the camera on the wreck.
    pub fn holding_camera(&self) -> bool {
        self.card
            .is_some_and(|(_, started)| ((get_time() - started) as f32) < CARD_HOLD)
    }
    fn death(
        &self,
        tank: &Tank,
        snapshot: &Snapshot,
        content: &Content,
        actions: &mut Vec<HudAction>,
        s: f32,
    ) {
        let mode = snapshot.config.mode;
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
        if let Some(killer) = self
            .killed_by
            .and_then(|id| killer_name(snapshot, content, id))
        {
            center_label(
                &fitted_label(
                    &format!("Destroyed by {killer}"),
                    15.0 * s,
                    screen_width() - 40.0 * s,
                ),
                x,
                y - 46.0 * s,
                15.0 * s,
                muted_color(),
            );
        }
        if let Some(line) = self.killed_by.and_then(|id| parting_shot(snapshot, id)) {
            center_label(line, x, y + 21.0 * s, 13.0 * s, muted_color());
        }
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
        } else if mode == Mode::Infection {
            format!("Rejoining the outbreak in {:.1}s", tank.respawn_timer)
        } else if eliminated {
            "Out of lives".to_owned()
        } else if tank.respawn_timer > 0.0 {
            format!("Respawn in {:.1}s", tank.respawn_timer)
        } else {
            "Press Enter or click to respawn".to_owned()
        };
        center_label(&status, x, y + 82.0 * s, 18.0 * s, accent());
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
    /// A boss gets no killer card, and the camera moves on to someone else
    /// right away, so it says its piece over the spectator bar instead.
    fn gloat(&self, snapshot: &Snapshot, content: &Content, s: f32) {
        let age = (get_time() - self.died_at) as f32;
        let Some(id) = self.killed_by.filter(|_| age < GLOAT_SECONDS) else {
            return;
        };
        let (Some(line), Some(killer)) = (
            parting_shot(snapshot, id),
            killer_name(snapshot, content, id),
        ) else {
            return;
        };
        let fade = ((GLOAT_SECONDS - age) / 0.4).min(1.);
        let width = 440. * s;
        let x = (screen_width() - width) * 0.5;
        let y = screen_height() - 194. * s;
        draw_rectangle(x, y, width, 47. * s, alpha(bg_color(), 0.84 * fade));
        center_label(
            &fitted_label(&format!("Destroyed by {killer}"), 13. * s, width - 24. * s),
            x + width * 0.5,
            y + 19. * s,
            13. * s,
            alpha(RED, fade),
        );
        center_label(
            line,
            x + width * 0.5,
            y + 38. * s,
            13. * s,
            alpha(text_color(), fade),
        );
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
            accent(),
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
/// The killer card shows for `CARD_HOLD` seconds, then fades as the camera
/// moves over to the killer, gone by `CARD_END`.
const CARD_HOLD: f32 = 2.3;
const CARD_END: f32 = 3.0;

/// Who destroyed the local tank: their tank pops in, then their name, then
/// their score counts up.
#[allow(clippy::too_many_arguments)]
fn killer_card(
    tank: &Tank,
    killer: &Tank,
    mode: Mode,
    content: &Content,
    settings: &Settings,
    age: f32,
    s: f32,
) {
    let calm = settings.reduced_motion;
    let fade_in = (age / 0.2).min(1.);
    let leaving = ((age - CARD_HOLD) / (CARD_END - CARD_HOLD)).clamp(0., 1.);
    let shown = fade_in * (1. - leaving);
    let w = screen_width();
    let h = screen_height();
    draw_rectangle(0., 0., w, h, alpha(bg_color(), 0.6 * shown));
    let x = w * 0.5;
    let y = h * 0.44;
    let color = render::team_color(killer.team, false, settings.colorblind);
    let step = |from: f32, length: f32| ((age - from) / length).clamp(0., 1.);

    center_label("DESTROYED BY", x, y - 104. * s, 14. * s, alpha(RED, shown));
    // A ring bursts out as the tank arrives.
    let burst = step(0.05, 0.7);
    if !calm && burst < 1. {
        draw_circle_lines(
            x,
            y,
            (44. + 110. * ease_out(burst)) * s,
            3. * s,
            alpha(color, (1. - burst) * 0.8 * shown),
        );
    }
    draw_circle(x, y, 66. * s, alpha(color, 0.12 * shown));
    let pop = if calm {
        1.
    } else {
        ease_out_back(step(0., 0.45))
    };
    let size = 44. * s * pop * (1. - ease_in(leaving));
    if size > 0.5
        && let Some(class) = content.class(&killer.class)
    {
        let time = get_time() as f32;
        let aim = -0.6 + if calm { 0. } else { (time * 0.8).sin() * 0.25 };
        render::class_model(class, vec2(x, y), size, aim, 0., 0., color, time);
    }

    // The name slides up, then the score counts.
    let name_in = if calm { 1. } else { ease_out(step(0.25, 0.35)) };
    let name_alpha = name_in * shown;
    let rise = (1. - name_in) * 18. * s;
    center_label(
        &fitted_player_name(killer, 30. * s, w - 80. * s),
        x,
        y + 92. * s + rise,
        30. * s,
        alpha(text_color(), name_alpha),
    );
    let class = content
        .class(&killer.class)
        .map_or(killer.class.as_str(), |class| class.name.as_str());
    center_label(
        &format!("Level {}  ·  {class}", killer.level),
        x,
        y + 118. * s + rise,
        14. * s,
        alpha(muted_color(), name_alpha),
    );
    let counting = if calm { 1. } else { ease_out(step(0.6, 0.8)) };
    if age >= 0.6 || calm {
        // A small bump when the count lands.
        let landed = if calm {
            0.
        } else {
            (1. - (age - 1.4) / 0.25).clamp(0., 1.)
        };
        let bump = if age >= 1.4 { landed } else { 0. };
        center_label(
            "SCORE",
            x,
            y + 152. * s,
            11. * s,
            alpha(muted_color(), shown),
        );
        let score = (killer.score as f64 * counting as f64).round() as u64;
        let size = 34. * s * (1. + 0.22 * bump);
        center_label(
            &grouped(score),
            x,
            y + 190. * s,
            size,
            alpha(accent(), shown),
        );
    }
    let hint_alpha = step(1.3, 0.3) * shown;
    if hint_alpha > 0. {
        let next = if tankyard_core::can_spectate(tank, killer, mode) {
            "Spectating them next  ·  Left / Right to switch"
        } else if tank.respawn_timer > 0. {
            "Respawning soon"
        } else {
            "Press Enter to respawn"
        };
        center_label(
            next,
            x,
            y + 226. * s,
            13. * s,
            alpha(muted_color(), hint_alpha),
        );
    }
}

fn ease_out(t: f32) -> f32 {
    1. - (1. - t).powi(3)
}

fn ease_in(t: f32) -> f32 {
    t * t * t
}

/// Overshoots a little, then settles: a pop.
fn ease_out_back(t: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.;
    1. + c3 * (t - 1.).powi(3) + c1 * (t - 1.).powi(2)
}

/// 12345 as "12,345".
pub(crate) fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// A readable name for whatever destroyed the local tank.
fn killer_name(snapshot: &Snapshot, content: &Content, id: EntityId) -> Option<String> {
    if id == 0 {
        return None;
    }
    if tankyard_core::base_guard_team(id).is_some() {
        return Some("the enemy base guardians".into());
    }
    if let Some(tank) = snapshot.tanks.iter().find(|tank| tank.id == id) {
        let class = content
            .class(&tank.class)
            .map_or(tank.class.as_str(), |class| class.name.as_str());
        return Some(format!("{} · {class}", tank.display_name()));
    }
    snapshot
        .bosses
        .iter()
        .find(|boss| boss.id == id)
        .map(|boss| format!("the {}", boss.kind.name()))
}
/// How long a boss's last word stays up over the spectator bar.
const GLOAT_SECONDS: f32 = 6.;
/// The last word, for whoever just lost to a boss or wandered into a base.
fn parting_shot(snapshot: &Snapshot, id: EntityId) -> Option<&'static str> {
    if id != 0 && tankyard_core::base_guard_team(id).is_some() {
        return Some("They did ask you to leave.");
    }
    let boss = snapshot.bosses.iter().find(|boss| boss.id == id)?;
    Some(boss_gloat(boss.kind))
}
fn boss_gloat(kind: BossKind) -> &'static str {
    match kind {
        BossKind::Sentinel => "It saw you first. It always does.",
        BossKind::Prism => "Pretty, though.",
        BossKind::Foundry => "You have been recycled.",
        BossKind::Tempest => "Should have brought an umbrella.",
        BossKind::Bastion => "The wall won.",
        BossKind::Eclipse => "Lights out.",
        BossKind::Guardian => "The pentagons thank you for visiting.",
        BossKind::Summoner => "It brought friends. You brought a cannon.",
        BossKind::Defender => "To be fair, you started it.",
        BossKind::FallenOverlord => "Fallen, but not far enough.",
        BossKind::FallenBooster => "It has never once used the brakes.",
        BossKind::SiegeEngine => "There was a big red line. We did draw it.",
        BossKind::Skirmisher => "Hit and run. Mostly hit.",
        BossKind::Trapper => "It put those there on purpose.",
        BossKind::Railgunner => "Distance was not your friend.",
        BossKind::Sunderworm => "The ground was hungry.",
    }
}
/// Where an arrow pointing `direction` from the middle of the screen meets its
/// edge, kept clear of the title above and the score bars below.
pub(crate) fn edge_point(direction: Vec2, s: f32) -> Vec2 {
    let (w, h) = (screen_width(), screen_height());
    let (inset, top, bottom) = (44. * s, 92. * s, 150. * s);
    let edge = if direction.y < 0. { top } else { bottom };
    let reach = ((w * 0.5 - inset) / direction.x.abs().max(0.001))
        .min((h * 0.5 - edge) / direction.y.abs().max(0.001));
    vec2(w, h) * 0.5 + direction * reach
}
/// The most bosses out of view that get an arrow, nearest first.
const MOST_BOSS_ARROWS: usize = 4;
/// Arrows at the screen's edge toward bosses out of view, each with its name
/// and health, so a fight is easy to find again after a respawn.
pub(crate) fn boss_arrows(snapshot: &Snapshot, view: &render::View, s: f32, colorblind: bool) {
    let (w, h) = (screen_width(), screen_height());
    let center = vec2(w, h) * 0.5;
    let mut out_of_view: Vec<(&tankyard_core::Boss, Vec2, f32)> = snapshot
        .bosses
        .iter()
        .filter(|boss| !boss.mothership && !view.visible(boss.pos, boss.radius))
        .filter_map(|boss| {
            let offset = view.screen(boss.pos) - center;
            Some((boss, offset.try_normalize()?, offset.length_squared()))
        })
        .collect();
    out_of_view.sort_unstable_by(|a, b| a.2.total_cmp(&b.2));
    for &(boss, d, _) in out_of_view.iter().take(MOST_BOSS_ARROWS) {
        let tip = edge_point(d, s);
        let n = vec2(-d.y, d.x);
        let size = 13. * s;
        let tint = render::boss_tint(boss, colorblind);
        let wing =
            |grow: f32, side: f32| tip - d * (size * 0.75 + grow) + n * side * (size * 0.85 + grow);
        let notch = tip - d * (size * 0.34);
        let outline = alpha(tankyard_client::tank_model::OUTLINE, 0.9);
        let reach = tip + d * (size + 3. * s);
        draw_triangle(reach, wing(2.5 * s, 1.), notch, outline);
        draw_triangle(reach, notch, wing(2.5 * s, -1.), outline);
        let point = tip + d * size;
        draw_triangle(point, wing(0., 1.), notch, tint);
        draw_triangle(point, notch, wing(0., -1.), tint);
        let name = fitted_label(boss.kind.name(), 11. * s, 104. * s);
        let half = (label_width(&name, 11. * s) * 0.5).max(26. * s);
        let at = tip - d * 34. * s;
        // A minimized window can be zero wide; `clamp` would panic.
        let x = tankyard_core::within(at.x, half + 6. * s, w - half - 6. * s);
        let y = tankyard_core::within(at.y, 16. * s, h - 14. * s);
        center_label(&name, x, y, 11. * s, text_color());
        arcade_track(
            Rect::new(x - 26. * s, y + 4. * s, 52. * s, 5. * s),
            boss.health / boss.max_health.max(1.),
            RED,
        );
    }
}
/// How much room a big boss bar takes at the top of the screen.
pub(crate) const RAIL_BAR_HEIGHT: f32 = 61.;

/// What the Railgunner is up to, for the line under its bar, and how urgent
/// that is: 0 for routine, 1 for get moving, 2 for now. Nothing when there's
/// nothing to say.
fn rail_status(boss: &tankyard_core::Boss, marked: bool) -> Option<(&'static str, u8)> {
    if boss.venting() {
        Some(("Venting. Hit it now.", 1))
    } else if boss.beaming() {
        Some((
            if marked {
                "Beam's on you. Move."
            } else {
                "Beam's on."
            },
            2,
        ))
    } else if boss.attack_windup > 0. {
        if boss.attack_windup < tankyard_core::RAIL_LOCK {
            Some((
                if marked {
                    "Locked on to you."
                } else {
                    "Locked on."
                },
                2,
            ))
        } else {
            Some((
                match boss.attack {
                    1 => "Charging the rail",
                    2 => "Charging the beam",
                    _ => "Loading sabots",
                },
                1,
            ))
        }
    } else if marked {
        Some(("You're the furthest away. Sorry.", 1))
    } else if boss.target == 0 {
        Some(("Looking for someone far away", 0))
    } else {
        None
    }
}

/// The line under a boss's bar. Only the Railgunner narrates its whole attack.
fn bar_status(boss: &tankyard_core::Boss, marked: bool) -> Option<(&'static str, u8)> {
    if boss.kind == BossKind::Sunderworm {
        use tankyard_core::WormAct;
        let sign = tankyard_core::WormSign::of(boss);
        match (sign.act, sign.stage) {
            (WormAct::Rise, _) => return Some(("The ground is moving.", 1)),
            (WormAct::Lunge, 0) => return Some(("Get out of its path.", 2)),
            (WormAct::Erupt, 0) => return Some(("The floor's breaking. Move.", 2)),
            (WormAct::Tear, _) => return Some(("It's tearing in two.", 2)),
            _ => {}
        }
    }
    if boss.kind == tankyard_core::BossKind::Railgunner {
        return rail_status(boss, marked);
    }
    marked.then_some(("It's after you.", 1))
}

pub(crate) fn boss_banner_layout(width: f32, s: f32) -> Rect {
    let left = 320. * s;
    let right = width - 320. * s;
    let available = (right - left).max(1.);
    let w = (600. * s).min(available);
    Rect::new((left + right - w) * 0.5, 112. * s, w, 62. * s)
}
fn sandbox_layout(width: f32, height: f32, scale: f32) -> (Rect, f32) {
    let s = scale
        .min((width - 40.).max(1.) / 920.)
        .min((height - 40.).max(1.) / 640.);
    let size = vec2(920., 640.) * s;
    (
        Rect::new(
            (width - size.x) * 0.5,
            (height - size.y) * 0.5,
            size.x,
            size.y,
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

    #[test]
    fn the_scoreboard_folds_clean_off_screen_and_leaves_its_tab() {
        for s in [0.7, 1., 1.6] {
            let screen = 1600.;
            let open = board_layout(screen, s, 0.);
            assert!(
                (open.x - (screen - 242. * s)).abs() < 0.01,
                "opens where it always was"
            );
            assert!(
                open.tab.x + open.tab.w <= open.x,
                "the tab doesn't cover a row"
            );
            let shut = board_layout(screen, s, 1.);
            assert!(shut.x >= screen, "nothing of the board is left");
            for fold in [0., 0.25, 0.5, 0.9, 1., 7., -1.] {
                let tab = board_layout(screen, s, fold).tab;
                assert!(tab.x >= 0. && tab.x + tab.w <= screen, "{fold}");
            }
        }
    }

    #[test]
    fn the_board_lists_the_top_ten_and_you_if_you_are_further_down() {
        let ranking: Vec<_> = (0..14).collect();
        let mut rows = Vec::new();
        board_rows(&ranking, |tank| tank == 3, |_| None, &mut rows);
        assert_eq!(rows.len(), 10, "already on it");
        board_rows(
            &ranking,
            |tank| tank == 12,
            |tank| Some(tank + 100),
            &mut rows,
        );
        assert_eq!(rows.len(), 11);
        assert_eq!(
            rows[10],
            BoardRow {
                place: 12,
                tank: 12,
                class: Some(112)
            }
        );
        board_rows(&ranking[..2], |_| false, |_| None, &mut rows);
        assert_eq!(rows.len(), 2);
        assert!(
            board_row_y(0., 10, 1.) > board_row_y(0., 9, 1.) + 24.,
            "set apart"
        );
    }

    #[test]
    fn with_the_tank_menu_open_the_board_clears_the_dock_and_keeps_you() {
        let ranking: Vec<_> = (0..14).collect();
        let mut rows = Vec::new();
        board_rows(&ranking, |tank| tank == 12, |_| None, &mut rows);
        let shown = |own: usize, room: usize| -> Vec<usize> {
            shown_rows(&rows, room, |line| line.tank == own)
                .map(|line| line.tank)
                .collect()
        };
        let docked = shown(12, DOCKED_ROWS);
        assert_eq!(docked.len(), DOCKED_ROWS);
        assert_eq!(docked[..8], [0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(docked[8], 12, "you stay on it");
        assert_eq!(shown(3, DOCKED_ROWS), (0..9).collect::<Vec<_>>());
        assert_eq!(shown(12, rows.len()).len(), 11, "the whole board");
        let bottom = board_row_y(22., DOCKED_ROWS - 1, 1.) + 20.;
        assert!(bottom <= 270., "the dock starts at 270: {bottom}");
    }

    use tankyard_core::{World, WorldConfig};

    #[test]
    fn every_boss_gloats_in_its_own_words_on_one_line() {
        let lines: std::collections::HashSet<_> =
            BossKind::ALL.iter().map(|&kind| boss_gloat(kind)).collect();
        assert_eq!(lines.len(), BossKind::ALL.len());
        assert!(lines.iter().all(|line| line.len() <= 44), "{lines:?}");
    }

    #[test]
    fn killer_card_easing_and_score_grouping() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(12_345_678), "12,345,678");
        for f in [ease_out, ease_in, ease_out_back] {
            assert!(f(0.).abs() < 1e-5 && (f(1.) - 1.).abs() < 1e-5);
        }
        // The pop overshoots before it settles.
        assert!((0..100).any(|i| ease_out_back(i as f32 / 100.) > 1.0));
        const { assert!(CARD_HOLD < CARD_END) };
    }

    #[test]
    fn sandbox_is_centred_and_boss_banner_fits_at_all_supported_scales() {
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
                assert!((sandbox.x + sandbox.w * 0.5 - width * 0.5).abs() < 0.001);
                assert!((sandbox.y + sandbox.h * 0.5 - height * 0.5).abs() < 0.001);
                assert!((boss.x + boss.w * 0.5 - width * 0.5).abs() < 0.001);
                assert!(boss.x >= 312. * s + 4. * s);
                assert!(boss.x + boss.w <= width - 242. * s - 12. * s);
                assert!(sandbox.x >= 19.99 && sandbox.y >= 19.99);
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
