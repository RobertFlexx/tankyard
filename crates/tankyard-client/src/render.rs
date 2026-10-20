use crate::ui::RED;
use crate::ui::VIOLET;
use crate::{effects::Effects, settings::Settings, ui::*};
use macroquad::prelude::*;
use std::collections::HashMap;
use tankyard_content::{ClassDef, Content, ProjectileKind};
use tankyard_core::{EntityId, Mode, Snapshot, TargetKind, V2};
const PLAYER_BLUE: Color = CYAN;
use crate::tank_model::OUTLINE;
const WORLD_TEXT: Color = Color::new(0.27, 0.33, 0.39, 1.0);

pub struct View {
    pub center: V2,
    pub zoom: f32,
    pub shake: Vec2,
}
impl View {
    pub fn new() -> Self {
        Self {
            center: V2::ZERO,
            zoom: 1.0,
            shake: Vec2::ZERO,
        }
    }
    pub fn screen(&self, p: V2) -> Vec2 {
        vec2(
            (p.x - self.center.x) * self.zoom + screen_width() * 0.5,
            (p.y - self.center.y) * self.zoom + screen_height() * 0.5,
        ) + self.shake
    }
    pub fn world(&self, p: Vec2) -> V2 {
        V2::new(
            (p.x - screen_width() * 0.5 - self.shake.x) / self.zoom + self.center.x,
            (p.y - screen_height() * 0.5 - self.shake.y) / self.zoom + self.center.y,
        )
    }
    pub fn visible(&self, p: V2, r: f32) -> bool {
        let s = self.screen(p);
        s.x + r * self.zoom > 0.0
            && s.y + r * self.zoom > 0.0
            && s.x - r * self.zoom < screen_width()
            && s.y - r * self.zoom < screen_height()
    }
    pub fn follow(&mut self, p: V2, dt: f32, shake: f32, smoothing: f32) {
        if self.center.distance(p) > 600.0 {
            self.center = p;
        }
        self.center = if smoothing <= 0.0 {
            p
        } else {
            self.center
                .lerp(p, 1.0 - (-dt * (24.0 - smoothing * 20.0)).exp())
        };
        let t = get_time() as f32;
        self.shake = vec2((t * 73.0).sin(), (t * 97.0).cos()) * shake;
    }
}
pub fn team_color(team: u8, local: bool, colorblind: bool) -> Color {
    if colorblind {
        use crate::accessibility::{BLUE, GREEN, ORANGE, PURPLE};
        return match team {
            0 if local => BLUE,
            0 => ORANGE,
            1 => BLUE,
            2 => ORANGE,
            3 => PURPLE,
            _ => GREEN,
        };
    }
    if team == 0 {
        if local { PLAYER_BLUE } else { RED }
    } else {
        match team {
            1 => PLAYER_BLUE,
            2 => RED,
            3 => VIOLET,
            _ => AMBER,
        }
    }
}
/// Geometric team marks retain identity independently of hue.
pub fn team_marker(team: u8, local: bool, p: Vec2, radius: f32) {
    let radius = radius.max(2.);
    let edge = (radius * 0.4).max(1.);
    let sides = match team {
        1 => 0,
        2 => 4,
        3 => 3,
        _ => 4,
    };
    let angle = if team >= 4 {
        std::f32::consts::FRAC_PI_4
    } else {
        0.
    };
    if team == 0 && !local {
        let p = crate::ui_layout::position(p);
        for c in [BLACK, WHITE] {
            let w = if c == BLACK { edge * 2.2 } else { edge };
            draw_line(p.x - radius, p.y - radius, p.x + radius, p.y + radius, w, c);
            draw_line(p.x - radius, p.y + radius, p.x + radius, p.y - radius, w, c);
        }
    } else {
        polygon(p, sides, radius + edge, angle, BLACK);
        polygon(p, sides, radius, angle, WHITE);
        polygon(
            p,
            sides,
            (radius - edge).max(0.),
            angle,
            team_color(team, local, true),
        );
    }
}
pub(crate) fn polygon(center: Vec2, sides: u8, radius: f32, angle: f32, color: Color) {
    let center = crate::ui_layout::position(center);
    if sides < 3 {
        draw_poly(
            center.x,
            center.y,
            crate::graphics::circle_sides(radius),
            radius,
            0.0,
            color,
        )
    } else {
        draw_poly(center.x, center.y, sides, radius, angle.to_degrees(), color)
    }
}
#[allow(clippy::too_many_arguments)]
pub fn class_model(
    class: &ClassDef,
    p: Vec2,
    r: f32,
    aim: f32,
    recoil: f32,
    flash: f32,
    color: Color,
    time: f32,
) {
    class_model_animated(
        class, p, r, aim, recoil, flash, color, time, None, None, None,
    );
}
#[allow(clippy::too_many_arguments)]
fn class_model_animated(
    class: &ClassDef,
    p: Vec2,
    r: f32,
    aim: f32,
    recoil: f32,
    flash: f32,
    color: Color,
    time: f32,
    barrel_recoils: Option<&[f32]>,
    barrel_aims: Option<&[f32]>,
    world_zoom: Option<f32>,
) {
    let time = crate::accessibility::motion_time(time);
    let recoil = if crate::accessibility::reduced_motion() {
        0.
    } else {
        recoil
    };
    crate::tank_model::class_model_geometry(
        class,
        p,
        r,
        aim,
        recoil,
        flash,
        color,
        time,
        barrel_recoils,
        barrel_aims,
        world_zoom,
        &mut |primitive| {
            draw_model_primitive(primitive);
            if let crate::tank_model::Primitive::Polygon {
                center,
                radius,
                angle,
                color: body_color,
                ..
            } = primitive
                && center == p
                && (radius - r).abs() < 0.001
                && body_color == color
                && let Some(texture) = &class.body_texture
            {
                crate::graphics::body(texture, center, radius, angle, color.a);
            }
        },
    );
}
pub(crate) fn draw_model_primitive(primitive: crate::tank_model::Primitive) {
    match primitive {
        crate::tank_model::Primitive::Polygon {
            center,
            sides,
            radius,
            angle,
            color,
        } => polygon(
            center,
            sides,
            radius,
            angle,
            crate::accessibility::palette(color),
        ),
        crate::tank_model::Primitive::Triangle { vertices, color } => draw_triangle(
            crate::ui_layout::position(vertices[0]),
            crate::ui_layout::position(vertices[1]),
            crate::ui_layout::position(vertices[2]),
            crate::accessibility::palette(color),
        ),
        crate::tank_model::Primitive::Quad { vertices, color } => {
            let vertices = vertices.map(crate::ui_layout::position);
            draw_triangle(
                vertices[0],
                vertices[1],
                vertices[2],
                crate::accessibility::palette(color),
            );
            draw_triangle(
                vertices[0],
                vertices[2],
                vertices[3],
                crate::accessibility::palette(color),
            );
        }
    }
}
pub fn class_preview(class: &ClassDef, rect: Rect, max_radius: f32, aim: f32, color: Color) {
    let max_radius = if class.hull_sides == 4 {
        max_radius * std::f32::consts::SQRT_2
    } else {
        max_radius
    };
    let (center, radius) = crate::tank_model::preview_placement(class, rect, max_radius, aim);
    class_model(class, center, radius, aim, 0.0, 0.0, color, 0.0);
}
fn cannon(p: Vec2, angle: f32, length: f32, width: f32, color: Color, recoil: f32) {
    crate::tank_model::cannon_profile(
        &mut draw_model_primitive,
        p,
        angle,
        length,
        width,
        color,
        recoil,
        1.0,
        None,
    );
}
#[allow(clippy::too_many_arguments)]
pub fn world(
    snapshot: &Snapshot,
    content: &Content,
    map: Option<&tankyard_content::MapDef>,
    local: EntityId,
    view: &View,
    effects: &Effects,
    settings: &Settings,
    interpolation: f32,
    animation_time: f32,
    local_animation_time: f32,
    previous_positions: &HashMap<EntityId, (V2, f32)>,
) {
    let dark = settings.dark_arena;
    clear_background(if dark {
        Color::new(0.14, 0.17, 0.20, 1.0)
    } else {
        BG
    });
    let grid_color = if dark {
        Color::new(0.20, 0.23, 0.27, 1.0)
    } else {
        Color::new(0.765, 0.765, 0.765, 1.0)
    };
    let z = view.zoom;
    let top = view.world(vec2(0.0, 0.0));
    let bottom = view.world(vec2(screen_width(), screen_height()));
    // Fine world-space grid, Diep-style. Remains crisp at arbitrary aspect ratios.
    let grid = 40.0;
    for x in ((top.x / grid).floor() as i32)..=((bottom.x / grid).ceil() as i32) {
        let p = view.screen(V2::new(x as f32 * grid, 0.0));
        draw_line(
            p.x.floor() + 0.5,
            0.0,
            p.x.floor() + 0.5,
            screen_height(),
            1.0,
            grid_color,
        );
    }
    for y in ((top.y / grid).floor() as i32)..=((bottom.y / grid).ceil() as i32) {
        let p = view.screen(V2::new(0.0, y as f32 * grid));
        draw_line(
            0.0,
            p.y.floor() + 0.5,
            screen_width(),
            p.y.floor() + 0.5,
            1.0,
            grid_color,
        );
    }
    let scale = effects.arena_scale.unwrap_or(snapshot.config.arena_scale);
    let bounds = tankyard_core::Rect {
        x: snapshot.config.width * (1. - scale) * 0.5,
        y: snapshot.config.height * (1. - scale) * 0.5,
        w: snapshot.config.width * scale,
        h: snapshot.config.height * scale,
    };
    let origin = view.screen(V2::new(bounds.x, bounds.y));
    let far = view.screen(V2::new(bounds.x + bounds.w, bounds.y + bounds.h));
    let outside = if dark {
        Color::new(0.10, 0.12, 0.14, 1.0)
    } else {
        Color::new(0.70, 0.70, 0.70, 1.0)
    };
    draw_rectangle(0.0, 0.0, origin.x.max(0.0), screen_height(), outside);
    draw_rectangle(
        far.x.max(0.0),
        0.0,
        (screen_width() - far.x).max(0.0),
        screen_height(),
        outside,
    );
    draw_rectangle(0.0, 0.0, screen_width(), origin.y.max(0.0), outside);
    draw_rectangle(
        0.0,
        far.y.max(0.0),
        screen_width(),
        (screen_height() - far.y).max(0.0),
        outside,
    );
    draw_rectangle_lines(
        origin.x,
        origin.y,
        bounds.w * z,
        bounds.h * z,
        4.0,
        if dark { line_color() } else { OUTLINE },
    );
    if snapshot.config.mode.teams() > 0 {
        for team in 1..=snapshot.config.mode.teams() {
            let position = match team {
                1 => V2::new(220.0, 220.0),
                2 => V2::new(
                    snapshot.config.width - 220.0,
                    snapshot.config.height - 220.0,
                ),
                3 => V2::new(snapshot.config.width - 220.0, 220.0),
                _ => V2::new(220.0, snapshot.config.height - 220.0),
            };
            let position = map
                .and_then(|map| map.team_bases.iter().find(|base| base.team == team))
                .map_or(position, |base| {
                    V2::new(
                        base.position[0] * snapshot.config.width,
                        base.position[1] * snapshot.config.height,
                    )
                });
            let home = V2::new(
                bounds.x + position.x / snapshot.config.width * bounds.w,
                bounds.y + position.y / snapshot.config.height * bounds.h,
            );
            let clearance = (home.x - bounds.x)
                .min(home.y - bounds.y)
                .min(bounds.x + bounds.w - home.x)
                .min(bounds.y + bounds.h - home.y);
            let fit = (snapshot.config.width / 3600.)
                .min(snapshot.config.height / 2600.)
                .clamp(0.35, 1.);
            let radius = (180. * fit * scale).min((clearance - 8. * scale).max(0.));
            let p = view.screen(home);
            let c = team_color(team, false, settings.colorblind);
            draw_circle(p.x, p.y, radius * z, alpha(c, 0.055));
            draw_circle_lines(p.x, p.y, radius * z, 2.0, alpha(c, 0.24));
        }
    }
    let wall_scale = scale / snapshot.config.arena_scale.max(0.5);
    let arena_center = V2::new(snapshot.config.width * 0.5, snapshot.config.height * 0.5);
    for wall in &snapshot.walls {
        let wall = tankyard_core::Rect {
            x: arena_center.x + (wall.x - arena_center.x) * wall_scale,
            y: arena_center.y + (wall.y - arena_center.y) * wall_scale,
            w: wall.w * wall_scale,
            h: wall.h * wall_scale,
        };
        if !view.visible(wall.center(), wall.w.max(wall.h)) {
            continue;
        }
        let p = view.screen(V2::new(wall.x, wall.y));
        tankyard_client::map_model::wall_geometry(
            Rect::new(p.x, p.y, wall.w * z, wall.h * z),
            tankyard_client::map_model::WallStyle::for_map(&snapshot.config.map),
            dark,
            settings.shape_quality,
            &mut draw_model_primitive,
        );
    }
    if snapshot.config.mode == Mode::Survival {
        let p = view.screen(V2::new(
            snapshot.config.width * 0.5,
            snapshot.config.height * 0.5,
        ));
        draw_circle_lines(
            p.x,
            p.y,
            snapshot.match_state.safe_radius * z,
            4.0,
            alpha(RED, 0.6),
        );
    }
    for objective in &snapshot.objectives {
        if !view.visible(objective.pos, objective.radius + 30.0) {
            continue;
        }
        let p = view.screen(objective.pos);
        let c = team_color(objective.team, true, settings.colorblind);
        draw_circle(p.x, p.y, objective.radius * z, alpha(c, 0.07));
        draw_circle_lines(p.x, p.y, objective.radius * z, 2.0, alpha(c, 0.45));
        draw_poly_lines(
            p.x,
            p.y,
            6,
            24.0 * z,
            crate::accessibility::motion_time(get_time() as f32) * 8.0,
            2.0 * z,
            c,
        );
        if objective.contested {
            center_label(
                "CONTESTED",
                p.x,
                p.y - (objective.radius + 12.0) * z,
                14.0 * z,
                if dark { AMBER } else { WORLD_TEXT },
            );
        }
        if objective.progress > 0.0 {
            crate::ui::progress(
                Rect::new(p.x - 32.0 * z, p.y + 36.0 * z, 64.0 * z, 4.0 * z),
                objective.progress.abs(),
                c,
            );
        }
    }
    for target in &snapshot.targets {
        let position = previous_positions
            .get(&target.id)
            .map_or(target.pos, |&(previous, _)| {
                crate::session::interpolate_position(previous, target.pos, interpolation)
            });
        if !view.visible(position, target.radius + 10.0) {
            continue;
        }
        let p = view.screen(position);
        let (sides, color) = match target.kind {
            TargetKind::Square => (4, AMBER),
            TargetKind::Triangle => (3, RED),
            TargetKind::Pentagon => (5, Color::new(0.463, 0.553, 0.988, 1.0)),
            TargetKind::Fortress => (6, Color::new(0.35, 0.53, 0.9, 1.0)),
            TargetKind::Rare => (7, Color::new(0.54, 0.92, 0.35, 1.0)),
            TargetKind::Mobile => (3, Color::new(1.0, 0.6, 0.35, 1.0)),
            TargetKind::Reinforced => (4, Color::new(0.64, 0.7, 0.75, 1.0)),
            TargetKind::Custom(sides) => (sides, VIOLET),
        };
        let radius = target.radius * z;
        let angle = previous_positions
            .get(&target.id)
            .map_or(target.angle, |&(_, previous)| {
                previous + (target.angle - previous) * interpolation
            });
        polygon(p, sides, radius + 2.5 * z, angle, OUTLINE);
        polygon(
            p,
            sides,
            radius,
            angle,
            crate::accessibility::palette(color),
        );
        if !settings.reduced_motion
            && let Some(flash) = effects.flashes.get(&target.id)
        {
            polygon(
                p,
                sides,
                radius,
                angle,
                alpha(WHITE, flash / 0.12 * 0.5 * settings.effects),
            );
        }
        if target.health < target.max_health {
            health_bar(p, radius, target.health / target.max_health, z, AMBER);
        }
    }
    // Ghosts are short-lived render-only echoes of expired / impacted bullets.
    // They never modify simulation state or require network protocol changes.
    for ghost in &effects.projectile_fades {
        if settings.reduced_motion || settings.effects <= 0. {
            break;
        }
        if !view.visible(ghost.pos, ghost.radius + 10.0) {
            continue;
        }
        let p = view.screen(ghost.pos);
        let t = (ghost.life / ghost.total).clamp(0.0, 1.0);
        let r = ghost.radius * z * (0.78 + 0.22 * t);
        let c = team_color(ghost.team, ghost.owner == local, settings.colorblind);
        if ghost.kind == ProjectileKind::Minion {
            crate::tank_model::minion_model_geometry(
                p,
                r,
                ghost.angle,
                0.0,
                alpha(c, t * 0.7),
                2.0 * z,
                &mut draw_model_primitive,
            );
        } else if matches!(
            ghost.kind,
            ProjectileKind::Drone | ProjectileKind::Trap | ProjectileKind::Mine
        ) {
            polygon(p, 3, r + 2.0 * z, ghost.angle, alpha(OUTLINE, t * 0.6));
            polygon(p, 3, r, ghost.angle, alpha(c, t * 0.7));
        } else {
            draw_circle(p.x, p.y, r + 2.0 * z, alpha(OUTLINE, t * 0.6));
            draw_circle(p.x, p.y, r, alpha(c, t * 0.7));
        }
    }
    for projectile in &snapshot.projectiles {
        // Fixed-step simulation / unlocked rendering: interpolate local bullets
        // between authoritative 60 Hz positions without altering collisions.
        let interpolated = projectile.previous.lerp(projectile.pos, interpolation);
        if !view.visible(interpolated, projectile.radius + 30.0) {
            continue;
        }
        let p = view.screen(interpolated);
        let r = projectile.radius * z;
        let boss = snapshot
            .bosses
            .iter()
            .find(|boss| boss.id == projectile.owner && !boss.mothership);
        let c = boss
            .map(|b| tankyard_client::boss_model::boss_color(b.kind))
            .unwrap_or_else(|| {
                team_color(
                    projectile.team,
                    projectile.owner == local,
                    settings.colorblind,
                )
            });
        if projectile.frozen.is_some() {
            draw_circle_lines(p.x, p.y, r + 4. * z, 1.5 * z, alpha(CYAN, 0.8));
            draw_line(p.x - r * 0.5, p.y, p.x + r * 0.5, p.y, z, WHITE);
        }
        // The lifetime field counts down on the authoritative simulation.
        let fade = (projectile.lifetime / 0.18).clamp(0.0, 1.0);
        let appear = (projectile.age / 0.045).clamp(0.45, 1.0);
        let opacity = fade * appear;
        match projectile.kind {
            ProjectileKind::Minion => {
                crate::tank_model::minion_model_geometry(
                    p,
                    r,
                    effects.drone_angle(projectile),
                    effects.minion_recoil(projectile.id) * z,
                    alpha(c, opacity),
                    2.0 * z,
                    &mut draw_model_primitive,
                );
            }
            ProjectileKind::Drone => {
                let angle = effects.drone_angle(projectile);
                let sides = if boss.is_some_and(|b| b.kind == tankyard_core::BossKind::Summoner) {
                    4
                } else {
                    3
                };
                if projectile.is_return_drone() {
                    draw_circle_lines(p.x, p.y, r + 5.0 * z, 2.0 * z, alpha(CYAN, opacity * 0.7));
                }
                polygon(p, sides, r + 2.0 * z, angle, alpha(OUTLINE, opacity));
                polygon(p, sides, r, angle, alpha(c, opacity));
                if projectile.is_return_drone() {
                    polygon(p, sides, r * 0.45, angle, alpha(WHITE, opacity * 0.6));
                }
            }
            ProjectileKind::Trap | ProjectileKind::Mine => {
                polygon(
                    p,
                    3,
                    r + 2.0 * z,
                    projectile.age * 0.1,
                    alpha(OUTLINE, opacity),
                );
                polygon(p, 3, r, projectile.age * 0.1, alpha(c, opacity));
            }
            ProjectileKind::Barrier => {
                draw_circle(p.x, p.y, r, alpha(c, opacity * 0.10));
                draw_poly_lines(p.x, p.y, 6, r, 0.0, 2.0 * z, alpha(c, opacity));
            }
            ProjectileKind::Turret => {
                cannon(p, projectile.vel.angle(), r * 1.5, r * 0.5, c, 0.0);
                draw_circle(p.x, p.y, r + 2.0 * z, alpha(OUTLINE, opacity));
                draw_circle(p.x, p.y, r, alpha(c, opacity));
            }
            _ => {
                draw_circle(p.x, p.y, r + 2.0 * z, alpha(OUTLINE, opacity));
                draw_circle(p.x, p.y, r, alpha(c, opacity));
            }
        }
    }
    for boss in &snapshot.bosses {
        let position = previous_positions
            .get(&boss.id)
            .map_or(boss.pos, |&(previous, _)| {
                crate::session::interpolate_position(previous, boss.pos, interpolation)
            });
        if !view.visible(position, boss.radius + 100.0) {
            continue;
        }
        let p = view.screen(position);
        let r = boss.radius * z;
        if boss.attack_windup > 0. {
            let progress = 1. - boss.attack_windup / 0.9;
            draw_circle_lines(
                p.x,
                p.y,
                r * (1.65 - progress * 0.35),
                2.5 * z,
                alpha(WHITE, 0.65),
            );
            draw_circle_lines(p.x, p.y, r * 1.12, 1.5 * z, alpha(AMBER, 0.6));
        }
        if boss.tactic == tankyard_core::BossTactic::Windup {
            let end = p + vec2(boss.aim.cos(), boss.aim.sin()) * r * 4.;
            draw_line(p.x, p.y, end.x, end.y, r * 0.55, alpha(RED, 0.12));
            draw_circle_lines(p.x, p.y, r * 1.25, 2., alpha(RED, 0.55));
        }
        tankyard_client::boss_model::boss_model_geometry(
            boss.kind,
            p,
            r,
            boss.aim,
            crate::accessibility::motion_time(snapshot.tick as f32 / 60.),
            boss.phase,
            boss.mothership
                .then(|| team_color(boss.team, false, settings.colorblind)),
            &mut draw_model_primitive,
        );
        health_bar(p, r, boss.health / boss.max_health, z, RED);
        center_label(
            if boss.mothership {
                "MOTHERSHIP"
            } else {
                boss.kind.name()
            },
            p.x,
            p.y - r - 14.0 * z,
            18.0 * z,
            if dark { text_color() } else { WORLD_TEXT },
        );
    }
    for tank in &snapshot.tanks {
        let position = previous_positions
            .get(&tank.id)
            .map_or(tank.pos, |&(previous, _)| {
                crate::session::interpolate_position(previous, tank.pos, interpolation)
            });
        if !tank.alive || !view.visible(position, tank.radius + 65.0) {
            continue;
        }
        let Some(class) = content.class(&tank.class) else {
            continue;
        };
        let p = view.screen(position);
        let visual = effects.hulls.get(&tank.id);
        let r = visual.map_or(tank.radius, |hull| hull.radius) * z;
        let color = team_color(tank.team, tank.id == local, settings.colorblind);
        let spawn_flash = if tank.spawn_immunity && tank.protection > 0.0 {
            if settings.reduced_motion {
                0.03
            } else {
                0.035 + (get_time() as f32 * std::f32::consts::TAU * 1.5).sin() * 0.025
            }
        } else {
            0.0
        };
        if tank.stasis > 0. {
            let field = tankyard_core::STASIS_RADIUS * z;
            draw_circle(p.x, p.y, field, alpha(CYAN, 0.035));
            let time = if settings.reduced_motion {
                0.
            } else {
                animation_time * 0.3
            };
            for arc in 0..48 {
                if arc % 6 == 0 {
                    continue;
                }
                let a = time + arc as f32 * std::f32::consts::TAU / 48.;
                let b = a + std::f32::consts::TAU / 56.;
                draw_line(
                    p.x + a.cos() * field,
                    p.y + a.sin() * field,
                    p.x + b.cos() * field,
                    p.y + b.sin() * field,
                    1.7 * z,
                    alpha(CYAN, 0.38),
                );
            }
            draw_circle_lines(p.x, p.y, r * 1.55, 2. * z, alpha(WHITE, 0.7));
        }
        let parrying = class.ability.is_parry() && tank.shield > 0.0;
        if tank.protection > 0.0 || tank.shield > 0.0 {
            let aura = if tank.spawn_immunity {
                WHITE
            } else if parrying {
                AMBER
            } else {
                color
            };
            draw_circle(
                p.x,
                p.y,
                r * 1.35,
                alpha(aura, if parrying { 0.16 } else { 0.04 }),
            );
            draw_circle_lines(
                p.x,
                p.y,
                r * 1.35,
                if parrying { 3.5 * z } else { 1.5 * z },
                alpha(
                    aura,
                    if settings.reduced_motion {
                        0.45
                    } else {
                        0.45 + (get_time() as f32 * 4.0).sin() * 0.1
                    },
                ),
            );
        }
        class_model_animated(
            class,
            p,
            r,
            *effects.display_aim.get(&tank.id).unwrap_or(&tank.aim),
            *effects.recoil.get(&tank.id).unwrap_or(&0.0),
            if settings.reduced_motion {
                spawn_flash
            } else {
                (effects.flashes.get(&tank.id).copied().unwrap_or(0.) * settings.effects)
                    .max(spawn_flash)
            },
            color,
            if tank.id == local {
                local_animation_time
            } else {
                animation_time
            },
            effects
                .barrels
                .get(&tank.id)
                .map(|visual| visual.recoil.as_slice()),
            Some(
                effects
                    .barrels
                    .get(&tank.id)
                    .map_or(tank.barrel_aims.as_slice(), |visual| visual.aim.as_slice()),
            ),
            Some(z),
        );
        if tank.id != local {
            arcade_player_label(tank, "", vec2(p.x, p.y - r - 14. * z), 320. * z, 15. * z);
        }
        if settings.colorblind {
            team_marker(
                tank.team,
                tank.id == local,
                vec2(p.x, p.y + r * 0.5),
                r * 0.19,
            );
        }
        health_bar(
            p,
            r,
            visual.map_or(tank.health / tank.max_health, |hull| hull.health),
            z,
            Color::new(0.45, 0.86, 0.33, 1.0),
        );
    }
    for particle in &effects.particles {
        if settings.reduced_motion || settings.effects <= 0. {
            break;
        }
        if !view.visible(particle.pos, 40.0) {
            continue;
        }
        let p = view.screen(particle.pos);
        let c = crate::accessibility::palette(Effects::particle_color(particle));
        if particle.ring {
            draw_circle_lines(
                p.x,
                p.y,
                (particle.size + (1.0 - particle.life / particle.total) * 60.0) * z,
                2.0 * z,
                c,
            );
        } else if particle.spark {
            let direction = particle.vel.normalized();
            let length = (particle.vel.length() * 0.035).clamp(4., 24.) * z;
            draw_line(
                p.x,
                p.y,
                p.x - direction.x * length,
                p.y - direction.y * length,
                (particle.size * 0.4 * z).max(0.6),
                c,
            );
            draw_circle(
                p.x,
                p.y,
                (particle.size * 0.32 * z).max(0.5),
                alpha(WHITE, c.a),
            );
        } else {
            draw_poly(
                p.x,
                p.y,
                3,
                particle.size * z,
                particle.spin.to_degrees(),
                c,
            );
        }
    }
    for popup in &effects.popups {
        let p = view.screen(popup.pos);
        center_label(
            &popup.text,
            p.x,
            p.y - 40.0 * z,
            18.0 * z,
            alpha(popup.color, (popup.life / 0.5).min(1.0)),
        );
    }
}
fn health_bar(p: Vec2, r: f32, ratio: f32, z: f32, color: Color) {
    if ratio >= 0.999 {
        return;
    }
    let w = (r * 1.8).max(28.0 * z);
    let h = 6.0 * z;
    arcade_track(
        Rect::new(p.x - w * 0.5, p.y + r + 8.0 * z, w, h),
        ratio,
        color,
    );
}

pub fn menu_background(time: f32) {
    let time = crate::accessibility::motion_time(time);
    clear_background(bg_color());
    let grid = if is_dark() {
        Color::new(0.17, 0.20, 0.23, 1.0)
    } else {
        Color::new(0.765, 0.765, 0.765, 1.0)
    };
    // Crisp one-pixel grid, rendered in framebuffer coordinates on every resize.
    for x in (0..screen_width() as i32).step_by(32) {
        draw_line(
            x as f32 + 0.5,
            0.0,
            x as f32 + 0.5,
            screen_height(),
            1.0,
            grid,
        );
    }
    for y in (0..screen_height() as i32).step_by(32) {
        draw_line(
            0.0,
            y as f32 + 0.5,
            screen_width(),
            y as f32 + 0.5,
            1.0,
            grid,
        );
    }
    // A few quiet geometric targets around the edge, not behind the controls.
    // Their movement is cosmetic and disabled by the reduced-motion setting.
    let w = screen_width();
    let h = screen_height();
    let border = if is_dark() {
        Color::new(0.33, 0.35, 0.37, 1.0)
    } else {
        Color::new(0.43, 0.43, 0.44, 1.0)
    };
    let geometry = [
        (0.10, 0.29, 4_u8, 18.0, 0.45, AMBER),
        (0.86, 0.27, 3, 21.0, -0.27, RED),
        (0.14, 0.79, 5, 27.0, 0.31, VIOLET),
        (0.87, 0.76, 4, 22.0, -0.45, AMBER),
        (0.77, 0.88, 3, 17.0, 0.42, RED),
        (0.29, 0.90, 4, 19.0, 0.36, AMBER),
    ];
    for (i, (nx, ny, sides, r, angle, c)) in geometry.into_iter().enumerate() {
        let drift = (time * 0.23 + i as f32).sin() * 5.0;
        let x = nx * w + drift;
        let y = ny * h + drift * 0.5;
        let c = crate::accessibility::palette(c);
        let fill = if is_dark() {
            mix(c, bg_color(), 0.54)
        } else {
            mix(c, bg_color(), 0.22)
        };
        polygon(vec2(x, y), sides, r + 3.0, angle, border);
        polygon(vec2(x, y), sides, r, angle, fill);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn colorblind_palette_distinguishes_ffa_and_every_team() {
        assert_ne!(team_color(0, true, true), team_color(0, false, true));
        for first in 1..=4 {
            assert_eq!(
                team_color(first, true, true),
                team_color(first, false, true)
            );
            for second in 1..=4 {
                if first != second {
                    assert_ne!(
                        team_color(first, false, true),
                        team_color(second, false, true)
                    );
                }
            }
        }
        assert_ne!(team_color(0, true, true), team_color(0, true, false));
        assert_ne!(team_color(0, false, true), team_color(0, false, false));
    }
}
