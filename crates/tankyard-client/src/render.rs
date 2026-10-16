use crate::ui::RED;
use crate::ui::VIOLET;
use crate::{effects::Effects, settings::Settings, ui::*};
use macroquad::prelude::*;
use std::collections::HashMap;
use tankyard_content::{ClassDef, Content, ProjectileKind};
use tankyard_core::{BossKind, EntityId, Mode, Snapshot, TargetKind, V2};
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
    if team == 0 {
        if local { PLAYER_BLUE } else { RED }
    } else {
        match team {
            1 => PLAYER_BLUE,
            2 => {
                if colorblind {
                    AMBER
                } else {
                    RED
                }
            }
            3 => VIOLET,
            _ => AMBER,
        }
    }
}
fn polygon(center: Vec2, sides: u8, radius: f32, angle: f32, color: Color) {
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
fn draw_model_primitive(primitive: crate::tank_model::Primitive) {
    match primitive {
        crate::tank_model::Primitive::Polygon {
            center,
            sides,
            radius,
            angle,
            color,
        } => polygon(center, sides, radius, angle, color),
        crate::tank_model::Primitive::Triangle { vertices, color } => {
            draw_triangle(vertices[0], vertices[1], vertices[2], color)
        }
        crate::tank_model::Primitive::Quad { vertices, color } => {
            draw_triangle(vertices[0], vertices[1], vertices[2], color);
            draw_triangle(vertices[0], vertices[2], vertices[3], color);
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
    let origin = view.screen(V2::ZERO);
    let far = view.screen(V2::new(snapshot.config.width, snapshot.config.height));
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
        snapshot.config.width * z,
        snapshot.config.height * z,
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
            let p = view.screen(position);
            let c = team_color(team, false, settings.colorblind);
            draw_circle(p.x, p.y, 180.0 * z, alpha(c, 0.055));
            draw_circle_lines(p.x, p.y, 180.0 * z, 2.0, alpha(c, 0.24));
        }
    }
    for wall in &snapshot.walls {
        if !view.visible(wall.center(), wall.w.max(wall.h)) {
            continue;
        }
        let p = view.screen(V2::new(wall.x, wall.y));
        draw_rectangle(
            p.x,
            p.y,
            wall.w * z,
            wall.h * z,
            if dark {
                Color::new(0.105, 0.15, 0.21, 1.0)
            } else {
                Color::new(0.75, 0.79, 0.82, 1.0)
            },
        );
        draw_rectangle_lines(
            p.x,
            p.y,
            wall.w * z,
            wall.h * z,
            2.0 * z,
            if dark {
                Color::new(0.2, 0.29, 0.37, 1.0)
            } else {
                Color::new(0.44, 0.49, 0.54, 1.0)
            },
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
        draw_poly_lines(p.x, p.y, 6, 24.0 * z, get_time() as f32 * 8.0, 2.0 * z, c);
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
        polygon(p, sides, radius, angle, color);
        if let Some(flash) = effects.flashes.get(&target.id) {
            polygon(p, sides, radius, angle, alpha(WHITE, flash / 0.12 * 0.5));
        }
        if target.health < target.max_health {
            health_bar(p, radius, target.health / target.max_health, z, AMBER);
        }
    }
    // Ghosts are short-lived render-only echoes of expired / impacted bullets.
    // They never modify simulation state or require network protocol changes.
    for ghost in &effects.projectile_fades {
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
        let c = team_color(
            projectile.team,
            projectile.owner == local,
            settings.colorblind,
        );
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
                polygon(p, 3, r + 2.0 * z, angle, alpha(OUTLINE, opacity));
                polygon(p, 3, r, angle, alpha(c, opacity));
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
        let (sides, mut color) = match boss.kind {
            BossKind::Sentinel => (6, AMBER),
            BossKind::Prism => (3, VIOLET),
            BossKind::Foundry => (5, RED),
            BossKind::Tempest => (8, CYAN),
            BossKind::Bastion => (4, Color::new(0.6, 0.7, 0.8, 1.0)),
            BossKind::Eclipse => (7, VIOLET),
        };
        if boss.mothership {
            color = team_color(boss.team, false, settings.colorblind);
        }
        if boss.timer < 0.9 {
            draw_circle_lines(
                p.x,
                p.y,
                r * (1.5 + (0.9 - boss.timer) * 0.5),
                2.0,
                alpha(RED, 0.32),
            );
        }
        for i in 0..sides {
            let a = boss.aim + i as f32 * std::f32::consts::TAU / sides as f32;
            cannon(
                p + vec2(a.cos(), a.sin()) * r * 0.5,
                a,
                r * 0.8,
                r * 0.23,
                color,
                0.0,
            );
        }
        polygon(p, sides, r * 1.07, boss.aim * 0.2, bg_color());
        polygon(p, sides, r, boss.aim * 0.2, mix(color, bg_color(), 0.35));
        polygon(p, sides, r * 0.76, -boss.aim * 0.4, color);
        polygon(p, 3, r * 0.4, boss.aim, line_color());
        draw_circle(p.x, p.y, r * 0.18, AMBER);
        health_bar(p, r, boss.health / boss.max_health, z, RED);
        center_label(
            if boss.mothership {
                "MOTHERSHIP"
            } else {
                match boss.kind {
                    BossKind::Sentinel => "SENTINEL",
                    BossKind::Prism => "PRISM",
                    BossKind::Foundry => "FOUNDRY",
                    BossKind::Tempest => "TEMPEST",
                    BossKind::Bastion => "BASTION",
                    BossKind::Eclipse => "ECLIPSE",
                }
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
        let parrying = class.ability == tankyard_content::Ability::Parry && tank.shield > 0.0;
        if tank.protection > 0.0 || tank.shield > 0.0 {
            let aura = if parrying { AMBER } else { color };
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
            *effects.flashes.get(&tank.id).unwrap_or(&0.0),
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
            arcade_center(&tank.name, p.x, p.y - r - 14.0 * z, 15.0 * z);
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
        if !view.visible(particle.pos, 40.0) {
            continue;
        }
        let p = view.screen(particle.pos);
        let c = Effects::particle_color(particle);
        if particle.ring {
            draw_circle_lines(
                p.x,
                p.y,
                (particle.size + (1.0 - particle.life / particle.total) * 60.0) * z,
                2.0 * z,
                c,
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
        let fill = if is_dark() {
            mix(c, bg_color(), 0.54)
        } else {
            mix(c, bg_color(), 0.22)
        };
        polygon(vec2(x, y), sides, r + 3.0, angle, border);
        polygon(vec2(x, y), sides, r, angle, fill);
    }
}
