use crate::{render, ui::*, ui_layout};
use macroquad::prelude::*;
use std::hash::{Hash, Hasher};
use tankyard_client::tank_tree_layout::{COLUMN, Layout};
use tankyard_content::{Ability, Content};

/// Card size in layout units.
const CARD_W: f32 = 108.;
const CARD_H: f32 = 74.;

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
        label(
            &format!("{} tanks", content.classes.len()),
            viewport.x + label_width("Tank tree", 26. * s) + 14. * s,
            top + 22. * s,
            13. * s,
            muted_color(),
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
            let wheel = ui_layout::wheel_lines().y;
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
        let zoom = self.zoom;
        let vz = vertical(zoom);
        let origin = viewport.center() + self.pan;
        let layout = &self.layout;
        let point = |i: usize| origin + vec2(layout.nodes[i].x * zoom, layout.nodes[i].y * vz);
        // The selected tank, its full ancestry and its next upgrades.
        let count = content.classes.len();
        let mut lineage = vec![false; count];
        let mut pending = vec![*selected];
        while let Some(node) = pending.pop() {
            if node < count && !lineage[node] {
                lineage[node] = true;
                pending.extend(
                    layout
                        .edges
                        .iter()
                        .filter(|&&(_, child)| child == node)
                        .map(|&(parent, _)| parent),
                );
            }
        }
        let next: Vec<_> = layout
            .edges
            .iter()
            .filter(|&&(parent, _)| parent == *selected)
            .map(|&(_, child)| child)
            .collect();
        let clip = crate::ui_layout::Scissor::new(viewport);
        let (half_w, half_h, gutter) = (
            CARD_W * 0.5 * zoom,
            CARD_H * 0.5 * zoom,
            COLUMN * 0.5 * zoom,
        );
        // Faint wires first, highlighted ones on top.
        for pass in 0..2 {
            for &(a, b) in &layout.edges {
                let highlighted = (lineage[a] && lineage[b]) || a == *selected;
                if highlighted != (pass == 1) {
                    continue;
                }
                let (pa, pb) = (point(a), point(b));
                let parent_floor = origin.y + layout.tiers[layout.band[a]].1 * vz + half_h;
                let child_roof = origin.y + layout.tiers[layout.band[b]].0 * vz - half_h;
                // Each parent's bus sits at its own height, so neighbouring
                // branches no longer merge into one indistinct line.
                let share = [0.3, 0.5, 0.7, 0.4, 0.6][layout.lane[a] % 5];
                let bus = parent_floor + (child_roof - parent_floor) * share;
                let mut path = Vec::with_capacity(7);
                if layout.blocked[a] {
                    path.extend([
                        vec2(pa.x + half_w, pa.y),
                        vec2(pa.x + gutter, pa.y),
                        vec2(pa.x + gutter, bus),
                    ]);
                } else {
                    path.extend([vec2(pa.x, pa.y + half_h), vec2(pa.x, bus)]);
                }
                if layout.covered[b] {
                    path.extend([
                        vec2(pb.x - gutter, bus),
                        vec2(pb.x - gutter, pb.y),
                        vec2(pb.x - half_w, pb.y),
                    ]);
                } else {
                    path.extend([vec2(pb.x, bus), vec2(pb.x, pb.y - half_h)]);
                }
                let (color, edge) = if highlighted {
                    (accent(), 2.2 * s)
                } else {
                    (alpha(line_color(), 0.55), s.max(0.65))
                };
                for (from, to) in path.iter().zip(path.iter().skip(1)) {
                    crate::ui::ui_line(from.x, from.y, to.x, to.y, edge, color);
                }
            }
        }
        let mut on_card = false;
        for (i, class) in content.classes.iter().enumerate() {
            let center = point(i);
            let r = Rect::new(
                center.x - half_w,
                center.y - half_h,
                CARD_W * zoom,
                CARD_H * zoom,
            );
            if !r.overlaps(&viewport) {
                continue;
            }
            let hover = hovered && r.contains(pointer);
            on_card |= hover;
            let color = card_color(i);
            let related = lineage[i] || next.contains(&i);
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
                } else if related {
                    1.8 * s
                } else {
                    zoom.max(0.7)
                },
                if related { accent() } else { line_color() },
            );
            let size = 13. * zoom;
            render::class_preview(
                class,
                Rect::new(
                    r.x + 8. * zoom,
                    r.y + 5. * zoom,
                    r.w - 16. * zoom,
                    r.h - if size >= 6.5 { 23. * zoom } else { 10. * zoom },
                ),
                18. * zoom,
                -0.45,
                player_color(),
            );
            if size >= 6.5 {
                center_label(
                    &fitted_label(&class.name, size, r.w - 6. * zoom),
                    center.x,
                    r.bottom() - 8. * zoom,
                    size,
                    Color::new(0.12, 0.15, 0.18, 1.),
                );
            }
            if hover && is_mouse_button_pressed(MouseButton::Left) {
                *selected = i;
            }
        }
        // Level requirements stay pinned to the left edge while panning.
        for (tier, &(row, _, level)) in layout.tiers.iter().enumerate() {
            let y = origin.y + row * vz - half_h - 7. * s;
            if y < viewport.y + 12. * s || y > viewport.bottom() - 4. * s {
                continue;
            }
            let text = format!("TIER {} · LV {level}", tier + 1);
            let size = 10. * s;
            let pill = Rect::new(
                viewport.x + 8. * s,
                y - size,
                label_width(&text, size) + 12. * s,
                size + 6. * s,
            );
            crate::ui::ui_rectangle(pill.x, pill.y, pill.w, pill.h, alpha(surface_color(), 0.85));
            label(&text, pill.x + 6. * s, y + 1. * s, size, muted_color());
        }
        drop(clip);
        if hovered
            && (is_mouse_button_pressed(MouseButton::Middle)
                || is_mouse_button_pressed(MouseButton::Right)
                || (!on_card && is_mouse_button_pressed(MouseButton::Left)))
        {
            self.previous = Some(pointer);
        }
        right_label(
            "Scroll to zoom · drag to explore · select a tank",
            viewport.right(),
            bottom + 19. * s,
            12. * s,
            muted_color(),
        );
        panel(detail);
        let Some(class) = content.classes.get(*selected) else {
            return;
        };
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
            warm_text_color(),
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
        let footer = detail.bottom()
            - if class.ability != Ability::None {
                34. * s
            } else {
                12. * s
            };
        // Step through the tree from here: parents and next upgrades.
        for (title, links) in [
            ("UPGRADES TO", next.clone()),
            (
                "UPGRADES FROM",
                layout
                    .edges
                    .iter()
                    .filter(|&&(_, child)| child == *selected)
                    .map(|&(parent, _)| parent)
                    .collect(),
            ),
        ] {
            if links.is_empty() || y + 52. * s > footer {
                continue;
            }
            label(title, x, y, 11. * s, accent());
            let mut cx = x;
            let mut cy = y + 8. * s;
            for target in links {
                let name = &content.classes[target].name;
                let chip_w = (label_width(name, 11. * s) + 16. * s).min(w);
                if cx + chip_w > x + w {
                    cx = x;
                    cy += 28. * s;
                }
                if cy + 24. * s > footer {
                    break;
                }
                if small_button(Rect::new(cx, cy, chip_w, 24. * s), name, s * 0.66) {
                    *selected = target;
                }
                cx += chip_w + 6. * s;
            }
            y = cy + 42. * s;
        }
        for (title, text, color) in [
            ("STRENGTHS", class.strengths.as_str(), accent()),
            ("TRADEOFFS", class.weaknesses.as_str(), warm_text_color()),
        ] {
            if y + 70. * s > footer {
                break;
            }
            label(title, x, y, 11. * s, color);
            y = wrap(text, x, y + 21. * s, w, 13. * s, muted_color()) + 22. * s;
        }
        if class.ability != Ability::None {
            label(
                &fitted_label(
                    &format!(
                        "Ability: {} · {:.0}s",
                        class.ability.name(),
                        class.ability_cooldown
                    ),
                    12. * s,
                    w,
                ),
                x,
                detail.bottom() - 18. * s,
                12. * s,
                accent(),
            );
        }
    }
}
