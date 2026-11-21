//! What a map's logic connects, for the workshop's Links view.
//!
//! A net is one signal channel, counter or group with the triggers that act
//! on it and everything those reach. Every trigger on one side reaches
//! everything on the other, so a busy net can be drawn through a hub, with a
//! line in from each trigger and an arrow out to each thing it reaches,
//! where a line for every pair would bury the map.
use crate::editor_document::{Document, Key, Link, counters_read, groups_watched};
use tankyard_content::{
    MAP_CHANNELS, MAP_COUNTERS, MAP_GROUPS, TriggerAction, TriggerWhen, counters_named,
};

/// What a net carries, strongest link first: where two triggers are joined
/// more than one way, the first carrier names the link.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Carrier {
    /// Signal channel `slot`, from the triggers that send on it to the
    /// triggers listening.
    Signal,
    /// Counter `slot`, from the triggers that change it to the triggers
    /// that watch, check or use it and the widgets and labels showing it.
    Counter,
    /// Group `slot` shown or hidden, to the triggers that watch or check it
    /// and the board cells it lights.
    Visibility,
    /// Group `slot`'s motions, to the triggers waiting for it to arrive.
    Motion,
    /// Group `slot`'s triggers reset; slot 0 resets every trigger.
    Reset,
    /// Channel `slot` sent as gated triggers' otherwise signal.
    Otherwise,
    /// Group `slot`'s objects, which triggers act on.
    Members,
}
impl Carrier {
    /// The link it makes from one trigger to another.
    pub fn link(self) -> Link {
        match self {
            Self::Signal => Link::Signal,
            Self::Counter => Link::Counter,
            Self::Visibility | Self::Motion | Self::Members => Link::Group,
            Self::Reset => Link::Reset,
            Self::Otherwise => Link::Otherwise,
        }
    }
    /// A hub's label: `#3` for a counter, `G3` for a group, the number for
    /// a channel, as trigger tags name them.
    pub fn label(self, slot: u8) -> String {
        match self {
            Self::Signal | Self::Otherwise => slot.to_string(),
            Self::Counter => format!("#{slot}"),
            Self::Reset if slot == 0 => "All".into(),
            Self::Visibility | Self::Motion | Self::Reset | Self::Members => format!("G{slot}"),
        }
    }
}

/// One channel, counter or group, and what it joins.
#[derive(Clone, Debug, PartialEq)]
pub struct Net {
    pub carrier: Carrier,
    /// The channel, counter or group number.
    pub slot: u8,
    /// The triggers that act on it, by index, each once and in order.
    pub from: Vec<usize>,
    /// What it reaches, each once: triggers, and for counters and groups
    /// the labels and objects too.
    pub to: Vec<Key>,
}
impl Net {
    /// Whether to draw it through a hub: five ends or more. One line from
    /// each trigger to the hub and short arrows on from there cross the map
    /// far less than a line for every pair, even from a single sender.
    pub fn hub(&self) -> bool {
        self.from.len() + self.to.len() >= 5
    }
}

/// Every net in a map, with each trigger's nets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wiring {
    /// Weakest carrier first, so drawing in order leaves the strongest link
    /// on top where two join the same pair.
    pub nets: Vec<Net>,
    /// By trigger: the nets it acts on, as indexes into `nets`.
    acts: Vec<Vec<u32>>,
    /// By trigger: the nets that reach it.
    reached: Vec<Vec<u32>>,
    /// By group, from 0: its Members net, when triggers act on its objects.
    members: Vec<Option<u32>>,
}

/// Lists by slot, each item once.
struct Slots<T>(Vec<Vec<T>>);
impl<T: Copy + PartialEq> Slots<T> {
    fn new(slots: u8) -> Self {
        Self((0..=slots).map(|_| Vec::new()).collect())
    }
    /// Adds `item` to `slot`'s list. One item's additions come together, so
    /// checking the last is enough to keep each once.
    fn add(&mut self, slot: u8, item: T) {
        if let Some(list) = self.0.get_mut(usize::from(slot))
            && list.last() != Some(&item)
        {
            list.push(item);
        }
    }
}

impl Wiring {
    pub fn new(doc: &Document) -> Self {
        let map = &doc.map;
        let triggers = &map.triggers;
        let mut send = Slots::new(MAP_CHANNELS);
        let mut otherwise = Slots::new(MAP_CHANNELS);
        let mut listen = Slots::new(MAP_CHANNELS);
        let mut change = Slots::new(MAP_COUNTERS);
        let mut read = Slots::new(MAP_COUNTERS);
        let mut switch = Slots::new(MAP_GROUPS);
        let mut watch = Slots::new(MAP_GROUPS);
        let mut animate = Slots::new(MAP_GROUPS);
        let mut wait = Slots::new(MAP_GROUPS);
        let mut reset = Slots::new(MAP_GROUPS);
        let mut belong = Slots::new(MAP_GROUPS);
        let mut act = Slots::new(MAP_GROUPS);
        let mut objects = Slots::new(MAP_GROUPS);
        let mut found = Vec::new();
        for (at, t) in triggers.iter().enumerate() {
            let me = Key::Trigger(at);
            let mut channels: Vec<u8> = t.sends().collect();
            channels.sort_unstable();
            channels.dedup();
            for channel in channels {
                send.add(channel, at);
            }
            if t.otherwise > 0 && t.only_if.is_some() {
                otherwise.add(t.otherwise, at);
            }
            if t.when == TriggerWhen::Signal {
                listen.add(t.source, me);
            }
            // Slot 0 is no counter or group; Slots has room for it, so each
            // carrier leaves it out where it means nothing.
            if t.action.targets_counter() && t.group > 0 {
                change.add(t.group, at);
            }
            counters_read(t, &mut found);
            for &counter in &found {
                read.add(counter, me);
            }
            if t.action.switches_group() && t.group > 0 {
                switch.add(t.group, at);
            }
            groups_watched(t, &mut found);
            for &group in &found {
                watch.add(group, me);
            }
            if t.action.animates() && t.group > 0 {
                animate.add(t.group, at);
            }
            if t.when == TriggerWhen::Arrived && t.source > 0 {
                wait.add(t.source, me);
            }
            if t.action == TriggerAction::Reset {
                reset.add(t.group, at);
            }
            belong.add(t.member, me);
            if t.action.targets_group() && t.group > 0 {
                act.add(t.group, at);
            }
        }
        for (at, decor) in map.decor.iter().enumerate() {
            counters_named(&decor.text, &mut found);
            found.sort_unstable();
            found.dedup();
            for &counter in &found {
                read.add(counter, Key::Decor(at));
            }
            found.clear();
        }
        for (key, _, group) in doc.items() {
            if group > 0 {
                objects.add(group, key);
            }
        }
        // Resetting group 0 resets every trigger.
        let everyone: Vec<Key> = (0..triggers.len()).map(Key::Trigger).collect();

        let mut wiring = Self {
            acts: vec![Vec::new(); triggers.len()],
            reached: vec![Vec::new(); triggers.len()],
            members: vec![None; usize::from(MAP_GROUPS) + 1],
            ..Default::default()
        };
        let mut net = |carrier: Carrier, slot: usize, from: &[usize], to: &[Key]| {
            if from.is_empty() || to.is_empty() {
                return;
            }
            let index = wiring.nets.len() as u32;
            for &at in from {
                wiring.acts[at].push(index);
            }
            for &key in to {
                if let Key::Trigger(at) = key {
                    wiring.reached[at].push(index);
                }
            }
            if carrier == Carrier::Members {
                wiring.members[slot] = Some(index);
            }
            wiring.nets.push(Net {
                carrier,
                slot: slot as u8,
                from: from.to_vec(),
                to: to.to_vec(),
            });
        };
        for (slot, from) in act.0.iter().enumerate() {
            net(Carrier::Members, slot, from, &objects.0[slot]);
        }
        for (slot, from) in otherwise.0.iter().enumerate() {
            net(Carrier::Otherwise, slot, from, &listen.0[slot]);
        }
        for (slot, from) in reset.0.iter().enumerate() {
            let to = if slot == 0 {
                &everyone
            } else {
                &belong.0[slot]
            };
            net(Carrier::Reset, slot, from, to);
        }
        for (slot, from) in animate.0.iter().enumerate() {
            net(Carrier::Motion, slot, from, &wait.0[slot]);
        }
        for (slot, from) in switch.0.iter().enumerate() {
            net(Carrier::Visibility, slot, from, &watch.0[slot]);
        }
        for (slot, from) in change.0.iter().enumerate() {
            net(Carrier::Counter, slot, from, &read.0[slot]);
        }
        for (slot, from) in send.0.iter().enumerate() {
            net(Carrier::Signal, slot, from, &listen.0[slot]);
        }
        wiring
    }

    /// The nets trigger `at` acts on, by index into `nets`.
    pub fn acts(&self, at: usize) -> &[u32] {
        self.acts.get(at).map_or(&[], Vec::as_slice)
    }
    /// The nets that reach trigger `at`.
    pub fn reached(&self, at: usize) -> &[u32] {
        self.reached.get(at).map_or(&[], Vec::as_slice)
    }
    /// The net of triggers acting on group `group`'s objects, if any.
    pub fn members(&self, group: u8) -> Option<&Net> {
        let index = (*self.members.get(usize::from(group))?)?;
        self.nets.get(index as usize)
    }

    /// The other triggers trigger `at` reaches, each once with its
    /// strongest link, in order: what [`crate::editor_document::link`]
    /// finds, without searching every trigger.
    pub fn links_from(&self, at: usize, out: &mut Vec<(usize, Link)>) {
        self.gather(self.acts(at), at, out, |net| {
            net.to.iter().filter_map(|key| match key {
                Key::Trigger(other) => Some(*other),
                _ => None,
            })
        });
    }
    /// The other triggers that reach trigger `at`, each once with its
    /// strongest link, in order.
    pub fn links_into(&self, at: usize, out: &mut Vec<(usize, Link)>) {
        self.gather(self.reached(at), at, out, |net| net.from.iter().copied());
    }
    fn gather<'a, I: Iterator<Item = usize>>(
        &'a self,
        nets: &[u32],
        at: usize,
        out: &mut Vec<(usize, Link)>,
        side: impl Fn(&'a Net) -> I,
    ) {
        let mut found: Vec<(usize, Carrier)> = Vec::new();
        for &index in nets {
            let net = &self.nets[index as usize];
            if net.carrier == Carrier::Members {
                continue;
            }
            found.extend(
                side(net)
                    .filter(|&other| other != at)
                    .map(|other| (other, net.carrier)),
            );
        }
        found.sort_unstable();
        found.dedup_by_key(|&mut (other, _)| other);
        out.clear();
        out.extend(
            found
                .into_iter()
                .map(|(other, carrier)| (other, carrier.link())),
        );
    }
}
