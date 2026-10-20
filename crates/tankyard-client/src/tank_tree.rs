use crate::{render, ui::*, ui_layout};
use macroquad::prelude::*;
use std::hash::{Hash, Hasher};
use tankyard_client::tank_tree_layout::Layout;
use tankyard_content::{Ability, Content};

#[derive(Default)]
pub struct Tree {
    layout: Layout,
    signature: u64,
    zoom: f32,
    pan: Vec2,
    previous: Option<Vec2>,
}
impl Tree {
    pub fn draw(&mut self, content: &Content, selected: &mut usize, scale: f32) {
        let s = scale
            .min(screen_width() / 1060.)
            .min(screen_height() / 740.);
        let top = 144. * scale;
        let bottom = screen_height() - 82. * scale;
        let width = screen_width() - 64. * s;
        let detail_w = 282. * s;
        let viewport = Rect::new(
            32. * s,
            top + 47. * s,
            width - detail_w - 20. * s,
            (bottom - top - 47. * s).max(80. * s),
        );
        let detail = Rect::new(viewport.right() + 20. * s, top, detail_w, bottom - top);
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for class in &content.classes {
            class.id.hash(&mut hash);
            class.parents.hash(&mut hash);
            class.tier.hash(&mut hash);
        }
        let signature = hash.finish();
        let changed = self.signature != signature;
        if changed {
            self.signature = signature;
            self.layout = Layout::new(content);
        }
        *selected = (*selected).min(content.classes.len().saturating_sub(1));
        label(
            "Tank tree",
            viewport.x,
            top + 22. * s,
            26. * s,
            text_color(),
        );
        let fit = small_button(
            Rect::new(viewport.right() - 151. * s, top, 68. * s, 30. * s),
            "Fit all",
            s * 0.72,
        );
        let focus = small_button(
            Rect::new(viewport.right() - 74. * s, top, 74. * s, 30. * s),
            "Focus",
            s * 0.72,
        );
        let vertical_floor = viewport.h / (self.layout.height + 100.);
        let vertical = |zoom: f32| zoom.max(vertical_floor);
        if fit || changed || self.zoom == 0. {
            self.zoom = (viewport.w / (self.layout.width + 100.))
                .min(viewport.h / (self.layout.height + 80.))
                .clamp(0.055, 1.2 * s);
            self.pan = vec2(
                0.,
                -self.layout.height * vertical(self.zoom) * 0.5 + 40. * vertical(self.zoom),
            );
        }
        if focus && let Some(node) = self.layout.nodes.get(*selected) {
            self.zoom = s.clamp(0.2, 2.);
            self.pan = -vec2(node.x * self.zoom, node.y * vertical(self.zoom));
        }
        panel(viewport);
        let pointer = ui_layout::pointer();
        let hovered = viewport.contains(pointer);
        if hovered && !pointer_captured() {
            let wheel = mouse_wheel().1;
            if wheel != 0. {
                let old = self.zoom;
                self.zoom = (self.zoom * (wheel * 0.12).exp()).clamp(0.05 * s, 2.4 * s);
                let anchor = pointer - viewport.center();
                self.pan = anchor
                    - (anchor - self.pan)
                        * vec2(self.zoom / old, vertical(self.zoom) / vertical(old));
            }
        }
        if !is_mouse_button_down(MouseButton::Left)
            && !is_mouse_button_down(MouseButton::Middle)
            && !is_mouse_button_down(MouseButton::Right)
        {
            self.previous = None;
        }
        if let Some(previous) = &mut self.previous {
            self.pan += pointer - *previous;
            *previous = pointer;
        }
        // Keep at least part of the graph reachable after an aggressive drag.
        let range = vec2(
            self.layout.width * self.zoom * 0.5 + viewport.w * 0.4,
            self.layout.height * vertical(self.zoom) + viewport.h * 0.4,
        );
        self.pan.x = self.pan.x.clamp(-range.x, range.x);
        self.pan.y = self.pan.y.clamp(-range.y, viewport.h * 0.4);
        let origin = viewport.center() + self.pan;
        let point = |i: usize| {
            origin
                + vec2(
                    self.layout.nodes[i].x * self.zoom,
                    self.layout.nodes[i].y * vertical(self.zoom),
                )
        };
        let clip = crate::ui_layout::Scissor::new(viewport);
        for &(a, b) in &self.layout.edges {
            let (a_pos, b_pos) = (point(a), point(b));
            let highlighted = a == *selected || b == *selected;
            let color = if highlighted {
                CYAN
            } else {
                alpha(line_color(), 0.7)
            };
            let middle = (a_pos.y + b_pos.y) * 0.5;
            let edge = if highlighted { 2. * s } else { s.max(0.65) };
            // Route through card gutters, so wrapped siblings never appear
            // to be upgrades of one another when a link crosses their row.
            let points = [
                vec2(a_pos.x + 54. * self.zoom, a_pos.y),
                vec2(a_pos.x + 62. * self.zoom, a_pos.y),
                vec2(a_pos.x + 62. * self.zoom, middle),
                vec2(b_pos.x - 62. * self.zoom, middle),
                vec2(b_pos.x - 62. * self.zoom, b_pos.y),
                vec2(b_pos.x - 54. * self.zoom, b_pos.y),
            ];
            for (from, to) in points.iter().zip(points.iter().skip(1)) {
                crate::ui::ui_line(from.x, from.y, to.x, to.y, edge, color);
            }
        }
        let mut on_card = false;
        for (i, class) in content.classes.iter().enumerate() {
            let center = point(i);
            let r = Rect::new(
                center.x - 54. * self.zoom,
                center.y - 37. * self.zoom,
                108. * self.zoom,
                74. * self.zoom,
            );
            if !r.overlaps(&viewport) {
                continue;
            }
            let hover = hovered && r.contains(pointer);
            on_card |= hover;
            let color = card_color(i);
            crate::ui::ui_rectangle(
                r.x,
                r.y,
                r.w,
                r.h,
                mix(color, WHITE, if hover { 0.3 } else { 0.12 }),
            );
            crate::ui::ui_rectangle_lines(
                r.x,
                r.y,
                r.w,
                r.h,
                if *selected == i {
                    3. * s
                } else {
                    self.zoom.max(0.7)
                },
                if *selected == i { CYAN } else { line_color() },
            );
            let size = 13. * self.zoom;
            render::class_preview(
                class,
                Rect::new(
                    r.x + 8. * self.zoom,
                    r.y + 5. * self.zoom,
                    r.w - 16. * self.zoom,
                    r.h - if size >= 6.5 {
                        23. * self.zoom
                    } else {
                        10. * self.zoom
                    },
                ),
                18. * self.zoom,
                -0.45,
                player_color(),
            );
            if size >= 6.5 {
                center_label(
                    &fitted_label(&class.name, size, r.w - 6. * self.zoom),
                    center.x,
                    r.bottom() - 8. * self.zoom,
                    size,
                    Color::new(0.12, 0.15, 0.18, 1.),
                );
            }
            if hover && is_mouse_button_pressed(MouseButton::Left) {
                *selected = i;
            }
        }
        drop(clip);
        if hovered
            && (is_mouse_button_pressed(MouseButton::Middle)
                || is_mouse_button_pressed(MouseButton::Right)
                || (!on_card && is_mouse_button_pressed(MouseButton::Left)))
        {
            self.previous = Some(pointer);
        }
        label(
            "Scroll to zoom · drag to explore · select a tank",
            viewport.x,
            bottom + 19. * s,
            12. * s,
            muted_color(),
        );
        panel(detail);
        if let Some(class) = content.classes.get(*selected) {
            let x = detail.x + 18. * s;
            let w = detail.w - 36. * s;
            label(
                &fitted_label(&class.name, 24. * s, w),
                x,
                detail.y + 31. * s,
                24. * s,
                text_color(),
            );
            label(
                &format!("TIER {} · LEVEL {}", class.tier + 1, class.min_level),
                x,
                detail.y + 53. * s,
                12. * s,
                AMBER,
            );
            render::class_preview(
                class,
                Rect::new(x, detail.y + 66. * s, w, 116. * s),
                39. * s,
                -0.4,
                player_color(),
            );
            let mut y = wrap(
                &class.description,
                x,
                detail.y + 206. * s,
                w,
                15. * s,
                text_color(),
            ) + 20. * s;
            for (title, text, color) in [
                ("STRENGTHS", class.strengths.as_str(), CYAN),
                ("TRADEOFFS", class.weaknesses.as_str(), AMBER),
            ] {
                if y + 70. * s > detail.bottom() - 55. * s {
                    break;
                }
                label(title, x, y, 11. * s, color);
                y = wrap(text, x, y + 21. * s, w, 13. * s, muted_color()) + 22. * s;
            }
            if class.ability != Ability::None {
                label(
                    &fitted_label(
                        &format!("{} · {:.0}s", class.ability.name(), class.ability_cooldown),
                        12. * s,
                        w,
                    ),
                    x,
                    detail.bottom() - 18. * s,
                    12. * s,
                    CYAN,
                );
            }
        }
    }
}
