//! The Links view: how a map's logic connects, as wires on the canvas.
//!
//! A busy net runs through a hub, and wires whose ends lie near each other
//! share a trunk most of the way, so a map of a thousand triggers still reads
//! as a handful of routes. Pointing at a trigger or a hub lifts its wires out
//! of the rest and shows which way they run.
use super::ink::{Ink, bowed, on_curve};
use super::{Editor, Layout, Mode};
use crate::ui::{AMBER, CYAN, RED, VIOLET, *};
use macroquad::prelude::*;
use std::collections::HashMap;
use tankyard_client::editor_document::{Key, Link};
use tankyard_client::editor_wiring::{
    Carrier, LINK_KIND_NAMES, LINK_KINDS, Net, bundles, link_kind,
};
use tankyard_content::*;
use tankyard_core::V2;

/// The most watched triggers whose links are drawn one by one; past it the
/// nets they touch are drawn whole, through hubs where busy.
const LINK_FOCUS: usize = 48;
/// The least distance, in pixels, between two hubs' middles.
const HUB_GAP: f32 = 24.;
/// A hub's radius on screen, and how much farther the pointer still finds it.
const HUB_RADIUS: f32 = 9.5;
const HUB_REACH: f32 = 3.;
/// Ends this close together on screen share a trunk.
const BUNDLE: f32 = 58.;
/// How strongly everything else shows while some wires are lifted.
const DIMMED: f32 = 0.14;
/// How many of a net's ends can show at full strength before the wires fade
/// back, so a dense map builds up gently instead of to a solid block.
const CLEAR_ENDS: f32 = 110.;
/// The faintest a net fades to on a dense map.
const FAINTEST: f32 = 0.3;
/// Seconds a net stays lit after one of its triggers acts in the preview.
const LIT_SECONDS: f32 = 0.7;
/// How many times wider than a trunk's bundle one part of the map is.
const REGION: f32 = 4.4;
/// Past this many hubs on screen they rest as dots of this radius, opening
/// up within this many pixels of the pointer.
const MOST_OPEN_HUBS: usize = 26;
const HUB_DOT: f32 = 3.2;
const HUB_OPENS: f32 = 150.;
/// How quickly the rest of the wires ease back and return.
const LIFT_RATE: f32 = 16.;
/// The size of the words on the wire key.
const LEGEND_TEXT: f32 = 10.5;

/// Where a wire meets something, and how far out from there to stop.
#[derive(Clone, Copy)]
struct End {
    at: Vec2,
    radius: f32,
}

/// Some of a net's ends, near each other, sharing a trunk to its hub.
struct Bundle {
    ends: Vec<u32>,
    /// The part of the map it's in. Trunks bound for the same part bend
    /// through the same point, so they run together as one cable.
    region: u32,
}

/// One net's ends, in the bundles that share a trunk.
#[derive(Default)]
struct Route {
    from: Vec<Bundle>,
    to: Vec<Bundle>,
}

/// Which ends share a trunk at the current zoom. Found again only when the
/// map or the zoom step changes; the wires themselves follow objects live.
#[derive(Default)]
pub struct Routes {
    stamp: Option<(Option<(u64, usize)>, i32)>,
    nets: Vec<Route>,
    /// The middle of each part of the map, in world units.
    regions: Vec<[f32; 2]>,
}

struct HubMark {
    at: Vec2,
    net: u32,
    color: Color,
    strength: f32,
    lifted: bool,
}

/// The hubs placed this frame, kept apart from each other.
#[derive(Default)]
struct Hubs {
    placed: Vec<HubMark>,
    /// Placed hubs by `HUB_GAP` cell, as indexes into `placed`.
    cells: HashMap<(i32, i32), Vec<u32>>,
}

impl Hubs {
    fn clear(&mut self) {
        self.placed.clear();
        self.cells.clear();
    }
    /// Puts a hub as near `want` as it can without crowding another, and
    /// returns where.
    fn place(&mut self, want: Vec2, net: u32, color: Color, strength: f32, lifted: bool) -> Vec2 {
        let cell = |p: Vec2| {
            (
                (p.x / HUB_GAP).floor() as i32,
                (p.y / HUB_GAP).floor() as i32,
            )
        };
        let crowded = |p: Vec2| {
            let (cx, cy) = cell(p);
            (cx - 1..=cx + 1).any(|x| {
                (cy - 1..=cy + 1).any(|y| {
                    self.cells.get(&(x, y)).is_some_and(|list| {
                        list.iter()
                            .any(|&i| self.placed[i as usize].at.distance(p) < HUB_GAP)
                    })
                })
            })
        };
        // Out along a spiral, which fills the space around the middle
        // evenly; past its last turn the hub stays crowded rather than wander.
        let spot = (0..48)
            .map(|step| {
                let turn = step as f32 * 2.399_963;
                want + vec2(turn.cos(), turn.sin()) * HUB_GAP * 0.75 * (step as f32).sqrt()
            })
            .find(|&p| !crowded(p))
            .unwrap_or(want);
        self.cells
            .entry(cell(spot))
            .or_default()
            .push(self.placed.len() as u32);
        self.placed.push(HubMark {
            at: spot,
            net,
            color,
            strength,
            lifted,
        });
        spot
    }
    /// Every hub on screen: the discs in one batch, then the numbers. When
    /// there are many, they rest as small dots and open up near the pointer.
    fn draw(&self, ink: &mut Ink, nets: &[Net], pointer: Vec2) {
        let screen = Rect::new(-40., -40., screen_width() + 80., screen_height() + 80.);
        let shown = || self.placed.iter().filter(|hub| screen.contains(hub.at));
        let many = shown().count() > MOST_OPEN_HUBS;
        let open = |hub: &HubMark| {
            if !many || hub.lifted {
                1.
            } else {
                ((HUB_OPENS - hub.at.distance(pointer)) / (HUB_OPENS * 0.5)).clamp(0., 1.)
            }
        };
        for hub in shown() {
            let fade = (hub.strength * 1.5 + 0.2).min(1.);
            let radius = if hub.lifted {
                HUB_RADIUS + 1.5
            } else {
                HUB_DOT + (HUB_RADIUS - HUB_DOT) * open(hub)
            };
            if hub.lifted {
                ink.disc(hub.at, radius + 6., alpha(hub.color, 0.16));
            }
            ink.disc(
                hub.at,
                radius,
                alpha(Color::new(0.1, 0.09, 0.08, 1.), 0.94 * fade),
            );
            ink.ring(hub.at, radius, 1.8, alpha(hub.color, fade));
        }
        ink.flush();
        for hub in shown() {
            let open = open(hub);
            let Some(net) = nets.get(hub.net as usize).filter(|_| open > 0.3) else {
                continue;
            };
            let text = net.carrier.label(net.slot);
            let size = if text.len() > 3 { 9. } else { 11. };
            let fade = (hub.strength * 1.5 + 0.2).min(1.) * open;
            center_label(
                &text,
                hub.at.x,
                hub.at.y + size * 0.36,
                size,
                alpha(hub.color, fade),
            );
        }
    }
}

/// What the Links view reuses from frame to frame.
#[derive(Default)]
pub struct LinkDrawing {
    ink: Ink,
    hubs: Hubs,
    from: Vec<Option<End>>,
    to: Vec<Option<End>>,
    found: Vec<(usize, Link)>,
}

/// How one wire looks.
#[derive(Clone, Copy)]
struct Wire {
    width: f32,
    color: Color,
    /// Its arrowhead's size; 0 for none.
    head: f32,
    /// Sideways bend, as a share of its length.
    bow: f32,
    /// A point to bend toward instead, shared with the trunks running the
    /// same way.
    via: Option<Vec2>,
    /// When set, dots run along it, at this time.
    flow: Option<f32>,
    /// When set, one bright pulse is this far along it.
    pulse: Option<f32>,
}

fn on_screen(a: Vec2, b: Vec2) -> bool {
    a.x.max(b.x) >= 0.
        && a.y.max(b.y) >= 0.
        && a.x.min(b.x) <= screen_width()
        && a.y.min(b.y) <= screen_height()
}

fn draw_wire(ink: &mut Ink, from: End, to: End, wire: Wire) {
    let length = from.at.distance(to.at);
    let room = from.radius + to.radius + 2.;
    if length <= room || !on_screen(from.at, to.at) {
        return;
    }
    let along = (to.at - from.at) / length;
    let (a, b) = (from.at + along * from.radius, to.at - along * to.radius);
    let via = wire.via.unwrap_or_else(|| bowed(a, b, wire.bow));
    let end = if wire.head > 0. {
        // The line stops inside the head, so its tip stays sharp.
        b - (b - via).normalize_or_zero() * wire.head * 0.7
    } else {
        b
    };
    ink.curve(
        (a, via, end),
        (wire.width, wire.width),
        (wire.color, wire.color),
    );
    if wire.head > 0. {
        ink.head(b, (b - via).normalize_or_zero(), wire.head, wire.color);
    }
    if let Some(time) = wire.flow {
        let bright = Color {
            a: 1.,
            ..mix(wire.color, WHITE, 0.62)
        };
        let dots = (length / 62.).ceil().clamp(1., 12.);
        for step in 0..dots as usize {
            let t = (time * 0.8 + step as f32 / dots).fract();
            // Fading in and out at the ends, so none pops.
            let edge = (t.min(1. - t) * 8.).min(1.);
            ink.disc(
                on_curve(a, via, b, t),
                wire.width * 0.5 + 1.15,
                alpha(bright, edge),
            );
        }
    }
    if let Some(t) = wire.pulse {
        let bright = mix(wire.color, WHITE, 0.75);
        let at = on_curve(a, via, b, t.clamp(0., 1.));
        ink.disc(at, wire.width + 3.4, alpha(bright, 0.22));
        ink.disc(at, wire.width * 0.5 + 1.5, alpha(bright, 0.95));
    }
}

/// A trunk's width for how many wires it carries.
fn trunk_width(count: usize) -> f32 {
    (1.05 + 0.42 * (count as f32).sqrt()).min(4.8)
}

/// How strongly a map's wires show when none are lifted: fully on a simple
/// map, and fainter the more there are.
fn rest_strength(ends: usize) -> f32 {
    (CLEAR_ENDS / ends.max(1) as f32).sqrt().clamp(FAINTEST, 1.)
}

fn mean(points: impl Iterator<Item = [f32; 2]>) -> [f32; 2] {
    let (mut x, mut y, mut count) = (0., 0., 0_u32);
    for point in points {
        x += point[0];
        y += point[1];
        count += 1;
    }
    let count = count.max(1) as f32;
    [x / count, y / count]
}

fn middle(ends: &[Option<End>], bundle: &[u32]) -> Option<Vec2> {
    let (sum, count) = bundle
        .iter()
        .filter_map(|&i| ends.get(i as usize).copied().flatten())
        .fold((Vec2::ZERO, 0u32), |(sum, count), end| {
            (sum + end.at, count + 1)
        });
    (count > 0).then(|| sum / count as f32)
}

impl Editor {
    /// Finds which of each net's ends share a trunk, when the map or the
    /// zoom has moved on far enough to change it.
    pub(super) fn refresh_routes(&mut self) {
        let step = (self.view.zoom.max(1e-3).log2() * 2.).round() as i32;
        let stamp = (self.wiring_stamp, step);
        if self.routes.stamp == Some(stamp) {
            return;
        }
        let reach = BUNDLE / 2_f32.powf(step as f32 / 2.);
        let (w, h) = (self.doc.map.width, self.doc.map.height);
        let spot = |key: Key| {
            self.doc.rect(key).map_or([0., 0.], |r| {
                [(r[0] + r[2] * 0.5) * w, (r[1] + r[3] * 0.5) * h]
            })
        };
        // Each bundle's middle, in the order they're found; a bundle holds
        // its place in this list until the regions are known.
        let mut middles: Vec<[f32; 2]> = Vec::new();
        let mut split = |points: &[[f32; 2]]| -> Vec<Bundle> {
            bundles(points, reach)
                .into_iter()
                .map(|ends| {
                    let region = middles.len() as u32;
                    middles.push(mean(ends.iter().map(|&i| points[i as usize])));
                    Bundle { ends, region }
                })
                .collect()
        };
        let mut nets: Vec<Route> = self
            .wiring
            .nets
            .iter()
            .map(|net| {
                if !net.hub() {
                    return Route::default();
                }
                let from: Vec<[f32; 2]> =
                    net.from.iter().map(|&at| spot(Key::Trigger(at))).collect();
                let to: Vec<[f32; 2]> = net.to.iter().map(|&key| spot(key)).collect();
                Route {
                    from: split(&from),
                    to: split(&to),
                }
            })
            .collect();
        let groups = bundles(&middles, reach * REGION);
        let mut region_of = vec![0; middles.len()];
        let mut regions = Vec::with_capacity(groups.len());
        for (region, group) in groups.iter().enumerate() {
            regions.push(mean(group.iter().map(|&i| middles[i as usize])));
            for &member in group {
                region_of[member as usize] = region as u32;
            }
        }
        for route in &mut nets {
            for bundle in route.from.iter_mut().chain(route.to.iter_mut()) {
                bundle.region = region_of[bundle.region as usize];
            }
        }
        self.routes = Routes {
            stamp: Some(stamp),
            nets,
            regions,
        };
    }

    /// Eases everything else back while some wires are lifted out of it.
    pub(super) fn ease_links(&mut self, dt: f32) {
        let triggers = self.doc.map.triggers.len();
        let watched = |key: &Key| matches!(*key, Key::Trigger(index) if index < triggers);
        let count = self.selection.iter().filter(|key| watched(key)).count()
            + usize::from(self.hover.as_ref().is_some_and(watched));
        let lifted = self.hub_hover.is_some() || (count > 0 && count <= LINK_FOCUS);
        let goal = if lifted { 1. } else { 0. };
        self.link_lift = if reduced_motion() {
            goal
        } else {
            self.link_lift + (goal - self.link_lift) * (1. - (-dt.max(0.) * LIFT_RATE).exp())
        };
    }

    pub(super) fn link_color(link: Link) -> Color {
        match link {
            Link::Signal | Link::Otherwise => VIOLET,
            Link::Counter => AMBER,
            Link::Group => mix(accent_color(), CYAN, 0.4),
            Link::Reset => RED,
        }
    }

    /// Where a wire meets `key`: a trigger's badge or the middle of anything
    /// else, or `None` for a key a gesture just removed.
    fn link_end(&self, key: Key) -> Option<End> {
        match key {
            Key::Trigger(index) if index < self.doc.map.triggers.len() => {
                let trigger = &self.doc.map.triggers[index];
                let badge = self.badge_rect(index, trigger, self.on_screen(trigger.rect));
                Some(End {
                    at: badge.center(),
                    radius: badge.w * 0.5,
                })
            }
            Key::Trigger(_) => None,
            _ => self.doc.rect(key).map(|r| End {
                at: self.on_screen(r).center(),
                radius: 0.,
            }),
        }
    }

    /// How brightly a net shows that one of its triggers just acted in the
    /// running preview, from 0 to 1.
    fn net_lit(&self, net: &Net) -> f32 {
        if !self.preview.running {
            return 0.;
        }
        net.from
            .iter()
            .filter_map(|&at| self.preview.flashes.get(at))
            .map(|&at| (1. - (self.preview.time - at) / LIT_SECONDS).clamp(0., 1.))
            .fold(0., f32::max)
    }

    /// Links between triggers and what they act on. With Links on, every
    /// net; otherwise, and on top of that, the links of the selected or
    /// hovered triggers, and the triggers acting on a selected or hovered
    /// object's group.
    pub(super) fn draw_links(&self) {
        let mut drawing = self.link_drawing.borrow_mut();
        let drawing = &mut *drawing;
        drawing.hubs.clear();
        let wiring = &self.wiring;
        if wiring.nets.is_empty() {
            return;
        }
        let triggers = self.doc.map.triggers.len();
        let mut watched: Vec<usize> = self
            .selection
            .iter()
            .chain(self.hover.iter())
            .filter_map(|&key| match key {
                Key::Trigger(index) if index < triggers => Some(index),
                _ => None,
            })
            .collect();
        watched.sort_unstable();
        watched.dedup();
        let mut watched_groups = 0u32;
        for &key in self.selection.iter().chain(self.hover.iter()) {
            if !matches!(key, Key::Trigger(_))
                && let Some(object) = self.doc.object(key)
            {
                watched_groups |= MapDef::group_bit(object.group());
            }
        }
        let focused = watched.len() <= LINK_FOCUS;
        let flow = (!reduced_motion()).then_some(self.time);
        let hidden = if self.show_links { self.link_hidden } else { 0 };
        let shown = |kind: usize| hidden & (1 << kind) == 0;
        let hub = self.hub_hover.map(|net| net as usize);
        if self.show_links {
            let ends = wiring
                .nets
                .iter()
                .filter(|net| shown(net.carrier.kind()))
                .map(|net| net.from.len() + net.to.len())
                .sum();
            let rest = rest_strength(ends) * (1. + (DIMMED - 1.) * self.link_lift);
            for (index, net) in wiring.nets.iter().enumerate() {
                if shown(net.carrier.kind()) && hub != Some(index) {
                    self.draw_net(drawing, index, net, rest, None);
                }
            }
            if let Some(index) = hub
                && let Some(net) = wiring.nets.get(index)
            {
                self.draw_net(drawing, index, net, 1., flow);
            }
        } else {
            let mut nets: Vec<u32> = Vec::new();
            if !focused {
                for &at in &watched {
                    nets.extend_from_slice(wiring.acts(at));
                    nets.extend_from_slice(wiring.reached(at));
                }
            }
            for group in 1..=MAP_GROUPS {
                if MapDef::group_bit(group) & watched_groups != 0
                    && let Some(index) = wiring.members_net(group)
                {
                    let index = index as usize;
                    self.draw_net(drawing, index, &wiring.nets[index], 1., flow);
                }
            }
            nets.sort_unstable();
            nets.dedup();
            for index in nets {
                let index = index as usize;
                self.draw_net(drawing, index, &wiring.nets[index], 1., flow);
            }
        }
        if focused && hub.is_none() {
            for &at in &watched {
                self.draw_trigger_links(drawing, at, flow, &shown);
            }
        }
        drawing.ink.flush();
        drawing
            .hubs
            .draw(&mut drawing.ink, &wiring.nets, crate::ui_layout::pointer());
        drawing.ink.flush();
    }

    /// One net: a wire from each trigger to each thing it reaches, or, when
    /// busy, through a hub with nearby ends sharing a trunk.
    fn draw_net(
        &self,
        drawing: &mut LinkDrawing,
        index: usize,
        net: &Net,
        strength: f32,
        flow: Option<f32>,
    ) {
        let LinkDrawing {
            ink,
            hubs,
            from,
            to,
            ..
        } = drawing;
        let base = Self::link_color(net.carrier.link());
        let lit = self.net_lit(net);
        let color = mix(base, WHITE, lit * 0.45);
        let strength = (strength + lit * 0.85).min(1.);
        let lifted = flow.is_some();
        let pulse = (lit > 0.).then_some(1. - lit);
        from.clear();
        from.extend(net.from.iter().map(|&at| self.link_end(Key::Trigger(at))));
        to.clear();
        to.extend(net.to.iter().map(|&key| self.link_end(key)));
        // Neighbors bow opposite ways, so wires between the same places part.
        let bow = if index % 2 == 0 { 0.07 } else { -0.07 };
        let thin = net.carrier == Carrier::Otherwise;
        if !net.hub() {
            let wire = Wire {
                width: if thin { 1.2 } else { 1.8 },
                color: alpha(color, 0.92 * strength),
                head: if strength > 0.5 { 8. } else { 6.5 },
                bow,
                via: None,
                flow,
                pulse,
            };
            for (i, a) in from.iter().enumerate() {
                let Some(a) = *a else { continue };
                for (j, b) in to.iter().enumerate() {
                    let Some(b) = *b else { continue };
                    if net.to[j] != Key::Trigger(net.from[i]) {
                        draw_wire(ink, a, b, wire);
                    }
                }
            }
            return;
        }
        let Some(route) = self.routes.nets.get(index) else {
            return;
        };
        // Halfway between the two sides, whatever their sizes, so one sender
        // to many listeners reads as a line that fans out.
        let side = |ends: &[Option<End>]| {
            let (sum, count) = ends
                .iter()
                .flatten()
                .fold((Vec2::ZERO, 0u32), |(sum, count), end| {
                    (sum + end.at, count + 1)
                });
            (count > 0).then(|| sum / count as f32)
        };
        let (Some(senders), Some(receivers)) = (side(from), side(to)) else {
            return;
        };
        let spot = hubs.place(
            senders.lerp(receivers, 0.5),
            index as u32,
            base,
            strength,
            lifted,
        );
        let hub = End {
            at: spot,
            radius: HUB_RADIUS,
        };
        let twig = Wire {
            width: 1.,
            color: alpha(color, 0.62 * strength),
            head: 0.,
            bow: 0.,
            via: None,
            flow: None,
            pulse: None,
        };
        // Trunks bound for one part of the map bend through one point, each
        // net a little to the side so a cable keeps some grain.
        let grain = (index % 7) as f32 * 1.7 - 5.1;
        let port = |bundle: &Bundle| {
            let [x, y] = *self.routes.regions.get(bundle.region as usize)?;
            let at = self.view.screen(V2::new(x, y));
            Some(spot.lerp(at, 0.86) + vec2(grain, -grain))
        };
        // In: twigs gather at each bundle's middle and a trunk runs to the hub.
        for bundle in &route.from {
            let Some(at) = middle(from, &bundle.ends) else {
                continue;
            };
            let gather = End { at, radius: 0. };
            if bundle.ends.len() > 1 {
                for &i in &bundle.ends {
                    if let Some(end) = from[i as usize] {
                        draw_wire(ink, end, gather, twig);
                    }
                }
            }
            let start = match (bundle.ends.len(), from[bundle.ends[0] as usize]) {
                (1, Some(end)) => end,
                _ => gather,
            };
            draw_wire(
                ink,
                start,
                hub,
                Wire {
                    width: trunk_width(bundle.ends.len()),
                    color: alpha(color, 0.72 * strength),
                    head: 0.,
                    bow,
                    via: port(bundle),
                    flow,
                    pulse,
                },
            );
        }
        // Out: a trunk from the hub to each bundle, then a twig to each end.
        for bundle in &route.to {
            let Some(at) = middle(to, &bundle.ends) else {
                continue;
            };
            let gather = End { at, radius: 0. };
            let finish = match (bundle.ends.len(), to[bundle.ends[0] as usize]) {
                (1, Some(end)) => end,
                _ => gather,
            };
            draw_wire(
                ink,
                hub,
                finish,
                Wire {
                    width: trunk_width(bundle.ends.len()),
                    color: alpha(color, 0.86 * strength),
                    head: 5.5 + 1.7 * (bundle.ends.len() as f32).sqrt().min(3.),
                    bow,
                    via: port(bundle),
                    flow,
                    pulse,
                },
            );
            if bundle.ends.len() > 1 {
                for &i in &bundle.ends {
                    if let Some(end) = to[i as usize] {
                        draw_wire(ink, gather, end, twig);
                    }
                }
            }
        }
    }

    /// Trigger `at`'s own links, both ways, each once: what it sets off and
    /// acts on, and, fainter, what sets it off. A ring marks each one.
    fn draw_trigger_links(
        &self,
        drawing: &mut LinkDrawing,
        at: usize,
        flow: Option<f32>,
        shown: &impl Fn(usize) -> bool,
    ) {
        let wiring = &self.wiring;
        let Some(origin) = self.link_end(Key::Trigger(at)) else {
            return;
        };
        let LinkDrawing { ink, found, .. } = drawing;
        let mark = |ink: &mut Ink, end: End, color: Color| {
            if end.radius > 0. {
                ink.ring(end.at, end.radius * 1.2 + 1.2, 1.5, alpha(color, 0.9));
            }
        };
        let wire = |width: f32, color: Color, opacity: f32, bow: f32| Wire {
            width,
            color: alpha(color, opacity),
            head: 8.5,
            bow,
            via: None,
            flow,
            pulse: None,
        };
        if shown(link_kind(Link::Group)) {
            let color = Self::link_color(Link::Group);
            for &index in wiring.acts(at) {
                let net = &wiring.nets[index as usize];
                if net.carrier != Carrier::Members {
                    continue;
                }
                for &key in &net.to {
                    if key != Key::Trigger(at)
                        && let Some(to) = self.link_end(key)
                    {
                        draw_wire(ink, origin, to, wire(1.9, color, 0.88, 0.05));
                    }
                }
            }
        }
        wiring.links_from(at, found);
        for &(other, kind) in found.iter() {
            if !shown(link_kind(kind)) {
                continue;
            }
            let width = if kind == Link::Otherwise { 1.3 } else { 2. };
            let color = Self::link_color(kind);
            if let Some(to) = self.link_end(Key::Trigger(other)) {
                draw_wire(ink, origin, to, wire(width, color, 0.95, 0.09));
                mark(ink, to, color);
            }
        }
        wiring.links_into(at, found);
        for &(other, kind) in found.iter() {
            if !shown(link_kind(kind)) {
                continue;
            }
            let color = Self::link_color(kind);
            if let Some(source) = self.link_end(Key::Trigger(other)) {
                draw_wire(ink, source, origin, wire(1.5, color, 0.62, 0.09));
                mark(ink, source, alpha(color, 0.6));
            }
        }
    }

    /// The net whose hub is under `pointer`, as last drawn.
    pub(super) fn hub_at(&self, pointer: Vec2) -> Option<u32> {
        let drawing = self.link_drawing.borrow();
        drawing
            .hubs
            .placed
            .iter()
            .filter(|hub| hub.at.distance(pointer) <= HUB_RADIUS + HUB_REACH)
            .min_by(|a, b| {
                a.at.distance_squared(pointer)
                    .total_cmp(&b.at.distance_squared(pointer))
            })
            .map(|hub| hub.net)
    }

    /// Selects everything on net `index`; `add` keeps the current selection.
    pub(super) fn select_net(&mut self, index: usize, add: bool) {
        let Some(net) = self.wiring.nets.get(index) else {
            return;
        };
        let (carrier, slot) = (net.carrier, net.slot);
        let mut keys: Vec<Key> = net
            .from
            .iter()
            .map(|&at| Key::Trigger(at))
            .chain(net.to.iter().copied())
            .filter(|&key| {
                self.doc
                    .object(key)
                    .is_some_and(|object| self.pickable(key, object.group()))
            })
            .collect();
        let count = keys.len();
        if add {
            keys.extend_from_slice(&self.selection);
        }
        self.set_selection(keys);
        self.tab = super::Tab::Object;
        self.say(format!(
            "{}: selected {}.",
            carrier.title(slot),
            super::objects(count)
        ));
    }

    /// What a hub's tooltip says: which net it is and who's on it.
    pub(super) fn hub_words(&self, index: usize) -> Option<(String, String)> {
        let net = self.wiring.nets.get(index)?;
        let (from, to) = net.carrier.roles();
        let hint = if self.mode == Mode::Edit {
            " Click to select them."
        } else {
            ""
        };
        Some((
            net.carrier.title(net.slot),
            format!("{} {from}, {} {to}.{hint}", net.from.len(), net.to.len()),
        ))
    }

    /// The key to the wire colors: where it goes along the canvas's upper
    /// right, and each chip's words and width. None when Links is off or the
    /// canvas has no room for it, even with the names left out.
    fn legend(&self, layout: &Layout) -> Option<(Rect, [(String, f32); LINK_KINDS])> {
        if !self.show_links {
            return None;
        }
        let s = layout.s;
        let mut counts = [0usize; LINK_KINDS];
        for net in &self.wiring.nets {
            counts[net.carrier.kind()] += 1;
        }
        let (h, gap) = (26. * s, 4. * s);
        // Along the top, clear of the mode label and the notices below.
        let left = layout.canvas.x + 300. * s;
        let right = layout.canvas.right() - 10. * s;
        for named in [true, false] {
            let chips = std::array::from_fn(|kind| {
                let text = if named {
                    format!("{} {}", LINK_KIND_NAMES[kind], counts[kind])
                } else {
                    counts[kind].to_string()
                };
                let width = label_width(&text, LEGEND_TEXT * s) + 30. * s;
                (text, width)
            });
            let w =
                chips.iter().map(|(_, width)| width).sum::<f32>() + gap * (LINK_KINDS - 1) as f32;
            if right - w >= left {
                let y = layout.canvas.y + 10. * s;
                return Some((Rect::new(right - w, y, w, h), chips));
            }
        }
        None
    }

    pub(super) fn legend_rect(&self, layout: &Layout) -> Option<Rect> {
        self.legend(layout).map(|(r, _)| r)
    }

    /// The key to the wire colors, each kind a switch that shows or hides it.
    pub(super) fn link_legend(&mut self, layout: &Layout) {
        let Some((r, chips)) = self.legend(layout) else {
            return;
        };
        let s = layout.s;
        let colors = [Link::Signal, Link::Counter, Link::Group, Link::Reset].map(Self::link_color);
        draw_rectangle(
            r.x - 3. * s,
            r.y - 3. * s,
            r.w + 6. * s,
            r.h + 6. * s,
            alpha(surface_color(), 0.72),
        );
        let mut drawing = self.link_drawing.borrow_mut();
        let mut x = r.x;
        for (kind, (text, width)) in chips.iter().enumerate() {
            let at = Rect::new(x, r.y, *width, r.h);
            x += width + 4. * s;
            if small_button(at, "", s) {
                self.link_hidden ^= 1 << kind;
            }
            let off = self.link_hidden & (1 << kind) != 0;
            let strength = if off { 0.4 } else { 1. };
            let dot = vec2(at.x + 12. * s, at.center().y);
            if off {
                drawing
                    .ink
                    .ring(dot, 4. * s, 1.4 * s, alpha(colors[kind], 0.7));
            } else {
                drawing.ink.disc(dot, 4.6 * s, colors[kind]);
            }
            drawing.ink.flush();
            let size = LEGEND_TEXT * s;
            label(
                text,
                at.x + 22. * s,
                at.center().y + size * 0.36,
                size,
                alpha(text_color(), strength),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_simple_map_shows_its_wires_fully_and_a_dense_one_fades_them() {
        assert_eq!(rest_strength(0), 1.);
        assert_eq!(rest_strength(40), 1.);
        let busy = rest_strength(1000);
        assert!(busy < 0.5 && busy >= FAINTEST);
        assert_eq!(rest_strength(1_000_000), FAINTEST);
        assert!(rest_strength(300) > rest_strength(600));
    }

    #[test]
    fn a_trunk_widens_with_what_it_carries_up_to_a_limit() {
        assert!(trunk_width(1) < trunk_width(4));
        assert!(trunk_width(4) < trunk_width(25));
        assert_eq!(trunk_width(10_000), 4.8);
    }

    #[test]
    fn bundle_middles_skip_ends_that_are_gone() {
        let ends = [
            Some(End {
                at: vec2(10., 0.),
                radius: 0.,
            }),
            None,
            Some(End {
                at: vec2(30., 20.),
                radius: 0.,
            }),
        ];
        assert_eq!(middle(&ends, &[0, 1, 2]), Some(vec2(20., 10.)));
        assert_eq!(middle(&ends, &[1]), None);
    }
}
