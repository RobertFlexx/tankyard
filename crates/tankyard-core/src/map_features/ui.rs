//! Map widgets: what Show UI and Hide UI triggers put on players' screens,
//! and the choices players pick from them.
use crate::*;
use std::collections::BTreeMap;
use tankyard_content::{
    HUD_TEXT, HUD_TITLE, HudPin, HudWidget, MAP_UI_SLOTS, MapUi, UiFill, UiPin, fill_ui_text,
};

/// The pack name map widgets file under in [`World::hud`]; packs always
/// have a name.
pub const MAP_HUD: &str = "";
/// Ticks between refreshes of widgets whose text or bar changes.
const REFRESH_TICKS: u64 = 6;
/// Ticks a player waits between picks.
const PICK_TICKS: u64 = 6;

/// A Show UI trigger's widget on screen.
#[derive(Clone, Debug)]
pub struct ShownUi {
    pub slot: u8,
    /// The player it shows to, or 0 for every player but `except`.
    pub viewer: EntityId,
    /// The trigger that showed it, from 0.
    pub trigger: usize,
    pub since: f32,
    /// When it goes; infinite while it stays.
    pub until: f32,
    /// Players an everyone widget no longer shows to: they picked from it,
    /// or were shown their own.
    pub except: Vec<EntityId>,
    /// The menu number its choices answer.
    pub menu: u32,
    /// The tank that set it off, which a widget pinned to it floats over;
    /// 0 for none.
    pub cause: EntityId,
}

/// A player's pick from a script's widget, for the pack's `choice` event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HudPick {
    pub pack: String,
    pub tank: EntityId,
    pub widget: String,
    /// The choice picked, from 0.
    pub option: u8,
}

/// The widget id of map slot `slot`.
pub fn ui_slot_id(slot: u8) -> String {
    format!("map_{slot}")
}

/// A menu number for a pack's widget, steady while its id stays. Map
/// menus count up below the top bit; packs' have it set.
pub fn menu_number(pack: &str, id: &str) -> u32 {
    let hash = tankyard_mods::digest(format!("{pack}\0{id}").as_bytes());
    (hash as u32 ^ (hash >> 32) as u32) | 1 << 31
}

impl World {
    /// Show trigger `trigger`'s widget in `slot` to `viewers`, or to every
    /// player without a list. `cause` is the tank that set it off.
    pub(crate) fn show_ui(
        &mut self,
        trigger: usize,
        slot: u8,
        viewers: Option<Vec<EntityId>>,
        cause: EntityId,
    ) {
        let Some(found) = self.map.triggers.get(trigger) else {
            return;
        };
        let Some(ui) = found.ui.as_deref() else {
            return;
        };
        // A widget over the tank that set it off needs that tank.
        if !(1..=MAP_UI_SLOTS).contains(&slot) || (ui.pin == UiPin::Cause && cause == 0) {
            return;
        }
        let now = self.match_state.elapsed;
        let until = if found.duration > 0. {
            now + found.duration
        } else {
            f32::INFINITY
        };
        self.next_menu = self.next_menu % ((1 << 31) - 1) + 1;
        let menu = self.next_menu;
        let shown = |viewer| ShownUi {
            slot,
            viewer,
            trigger,
            since: now,
            until,
            except: Vec::new(),
            menu,
            cause,
        };
        match viewers {
            None => {
                self.map_ui.retain(|ui| ui.slot != slot);
                self.map_ui.push(shown(0));
            }
            Some(viewers) => {
                for viewer in viewers {
                    self.map_ui
                        .retain(|ui| !(ui.slot == slot && ui.viewer == viewer));
                    for ui in &mut self.map_ui {
                        if ui.slot == slot && ui.viewer == 0 && !ui.except.contains(&viewer) {
                            ui.except.push(viewer);
                        }
                    }
                    self.map_ui.push(shown(viewer));
                }
            }
        }
        self.map_ui_dirty = true;
    }

    /// Take `slot`'s widget (every slot for 0) off `viewers`' screens, or
    /// everyone's without a list.
    pub(crate) fn hide_ui(&mut self, slot: u8, viewers: Option<Vec<EntityId>>) {
        let chosen = |ui: &ShownUi| slot == 0 || ui.slot == slot;
        match viewers {
            None => self.map_ui.retain(|ui| !chosen(ui)),
            Some(viewers) => {
                self.map_ui
                    .retain(|ui| !(chosen(ui) && viewers.contains(&ui.viewer)));
                for ui in self.map_ui.iter_mut().filter(|ui| ui.viewer == 0) {
                    if chosen(ui) {
                        for &viewer in &viewers {
                            if !ui.except.contains(&viewer) {
                                ui.except.push(viewer);
                            }
                        }
                    }
                }
            }
        }
        self.map_ui_dirty = true;
    }

    /// Bring the map's widgets in [`World::hud`] up to date: drop those
    /// that ran out or whose player left, and fill in changing text.
    pub(crate) fn sync_map_ui(&mut self) {
        if self.map_ui.is_empty() && !self.map_ui_dirty {
            return;
        }
        let now = self.match_state.elapsed;
        let before = self.map_ui.len();
        let triggers = self.map.triggers.len();
        self.map_ui
            .retain(|ui| ui.until > now && ui.trigger < triggers);
        let refresh = self.tick.is_multiple_of(REFRESH_TICKS);
        if refresh {
            // Players who left take their widgets, and their places in
            // everyone widgets' exceptions, with them.
            let mut ids: Vec<EntityId> = self.tanks.iter().map(|tank| tank.id).collect();
            ids.sort_unstable();
            let here = |id: &EntityId| ids.binary_search(id).is_ok();
            let triggers = &self.map.triggers;
            self.map_ui.retain_mut(|ui| {
                ui.except.retain(here);
                // A widget over a tank that left goes with it.
                let over_cause = triggers[ui.trigger]
                    .ui
                    .as_deref()
                    .is_some_and(|ui| ui.pin == UiPin::Cause);
                (ui.viewer == 0 || here(&ui.viewer)) && !(over_cause && !here(&ui.cause))
            });
        }
        if self.map_ui.len() == before && !self.map_ui_dirty && !refresh {
            return;
        }
        self.map_ui_dirty = false;
        let mut wanted: BTreeMap<(String, EntityId, String), HudWidget> = BTreeMap::new();
        for shown in &self.map_ui {
            let trigger = &self.map.triggers[shown.trigger];
            let Some(ui) = trigger.ui.as_deref() else {
                continue;
            };
            let duration = trigger.duration;
            let cause = self
                .tanks
                .iter()
                .find(|tank| tank.id == shown.cause)
                .map_or("", |tank| tank.name.as_str());
            let [x, y, w, h] = trigger.rect;
            let pin = match ui.pin {
                UiPin::Screen => HudPin::Screen,
                UiPin::Area if trigger.member != 0 => HudPin::Group {
                    id: trigger.member,
                    at: [
                        (x + w * 0.5) * self.config.width,
                        (y + h * 0.5) * self.config.height,
                    ],
                },
                UiPin::Area => {
                    let at = crate::map_point(&self.config, [x + w * 0.5, y + h * 0.5]);
                    HudPin::World([at.x, at.y])
                }
                UiPin::Cause => HudPin::Tank(shown.cause),
            };
            // A widget some players no longer see goes to each of the others.
            let personal = ui.personal() || shown.viewer != 0 || !shown.except.is_empty();
            let viewers = self.tanks.iter().filter(|tank| {
                !tank.bot
                    && if shown.viewer == 0 {
                        !shown.except.contains(&tank.id)
                    } else {
                        tank.id == shown.viewer
                    }
            });
            if personal {
                for tank in viewers {
                    let fill = UiFill {
                        counters: &self.match_state.counters,
                        left: shown.until - now,
                        time: now,
                        name: &tank.name,
                        cause,
                        score: u64::from(tank.score),
                    };
                    if let Some(widget) = fill_widget(ui, &fill, duration, shown.menu, pin) {
                        wanted.insert((MAP_HUD.into(), tank.id, ui_slot_id(shown.slot)), widget);
                    }
                }
            } else {
                let fill = UiFill {
                    counters: &self.match_state.counters,
                    left: shown.until - now,
                    time: now,
                    name: "",
                    cause,
                    score: 0,
                };
                if let Some(widget) = fill_widget(ui, &fill, duration, shown.menu, pin) {
                    wanted.insert((MAP_HUD.into(), 0, ui_slot_id(shown.slot)), widget);
                }
            }
        }
        // Change only what differs, so unchanged widgets cost nothing.
        self.hud
            .retain(|key, _| key.0 != MAP_HUD || wanted.contains_key(key));
        for (key, widget) in wanted {
            if self.hud.get(&key) != Some(&widget) {
                self.hud.insert(key, widget);
            }
        }
    }

    /// Player `viewer` picks choice `option` of the widget numbered `menu`.
    /// A map menu signals its choice's channel with the player as the tank
    /// that set it off; a script's raises its pack's `choice` event. Returns
    /// whether the pick counted.
    pub fn choose(&mut self, viewer: EntityId, menu: u32, option: u8) -> bool {
        if menu == 0 {
            return false;
        }
        if self
            .pick_ticks
            .get(&viewer)
            .is_some_and(|&tick| self.tick < tick + PICK_TICKS)
        {
            return false;
        }
        let Some((key, choices)) = self
            .hud
            .iter()
            .find(|((_, shown_to, _), widget)| {
                (*shown_to == 0 || *shown_to == viewer) && widget.menu == menu
            })
            .map(|(key, widget)| (key.clone(), widget.choices.len()))
        else {
            return false;
        };
        if usize::from(option) >= choices {
            return false;
        }
        if self.pick_ticks.len() > 4096 {
            self.pick_ticks.clear();
        }
        self.pick_ticks.insert(viewer, self.tick);
        if key.0 != MAP_HUD {
            if self.picks_in.len() < 256 {
                self.picks_in.push(HudPick {
                    pack: key.0.clone(),
                    tank: viewer,
                    widget: key.2.clone(),
                    option,
                });
            }
            // A menu shown to one player closes once they pick.
            if key.1 == viewer {
                self.hud.remove(&key);
            }
            return true;
        }
        let Some(index) = self
            .map_ui
            .iter()
            .position(|ui| ui.menu == menu && (ui.viewer == viewer || ui.viewer == 0))
        else {
            return false;
        };
        let shown = &self.map_ui[index];
        let Some(ui) = self
            .map
            .triggers
            .get(shown.trigger)
            .and_then(|trigger| trigger.ui.as_deref())
        else {
            return false;
        };
        let channel = ui.channels.get(usize::from(option)).copied().unwrap_or(0);
        if channel > 0 {
            self.logic.input(crate::map_features::LogicInput::Pick {
                channel,
                tank: viewer,
            });
        }
        if !ui.keep {
            if shown.viewer == viewer {
                self.map_ui.remove(index);
            } else {
                self.map_ui[index].except.push(viewer);
            }
            self.hud.remove(&key);
            self.map_ui_dirty = true;
        }
        true
    }

    /// The map widgets on screen.
    pub fn shown_ui(&self) -> &[ShownUi] {
        &self.map_ui
    }
}

/// `ui`'s widget with its fields filled in and pinned to `pin`, or None
/// when nothing is left to show.
fn fill_widget(
    ui: &MapUi,
    fill: &UiFill,
    duration: f32,
    menu: u32,
    pin: HudPin,
) -> Option<HudWidget> {
    let mut widget = ui.widget.clone();
    widget.pin = pin;
    widget.text = fill_ui_text(&widget.text, fill, HUD_TEXT);
    widget.title = fill_ui_text(&widget.title, fill, HUD_TITLE);
    if ui.bar > 0 {
        let value = fill
            .counters
            .get(usize::from(ui.bar) - 1)
            .copied()
            .unwrap_or(0);
        widget.progress = Some((value as f32 / ui.bar_max.max(1) as f32).clamp(0., 1.));
    } else if ui.timer && duration > 0. {
        widget.progress = Some((fill.left / duration).clamp(0., 1.));
    }
    // Even a label needs a stable identity while its text or pin moves.
    // A pick is still rejected when the widget has no choices.
    widget.menu = menu;
    widget.valid().then_some(widget)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_content::{
        HudStyle, MapTrigger, MapUi, TriggerAction, TriggerWhen, TriggerWho, UiAudience,
    };

    fn world_with(triggers: Vec<MapTrigger>) -> World {
        let mut maps = tankyard_content::builtin_maps();
        maps[0].triggers = triggers;
        tankyard_content::validate_maps(&maps).unwrap();
        let config = WorldConfig {
            target_count: 0,
            ..WorldConfig::default()
        };
        World::with_maps_for_tests(config, maps)
    }
    fn trigger(when: TriggerWhen, action: TriggerAction) -> MapTrigger {
        MapTrigger {
            when,
            action,
            rect: [0.4, 0.4, 0.2, 0.2],
            seconds: 0.5,
            ..Default::default()
        }
    }
    fn show(slot: u8, ui: MapUi) -> MapTrigger {
        MapTrigger {
            group: slot,
            ui: Some(Box::new(ui)),
            ..trigger(TriggerWhen::Start, TriggerAction::Ui)
        }
    }
    fn widget(text: &str) -> HudWidget {
        HudWidget {
            text: text.into(),
            style: HudStyle::Card,
            ..HudWidget::default()
        }
    }
    fn run(world: &mut World, seconds: f32) {
        for _ in 0..(seconds * 60.).round() as usize {
            world.step();
        }
    }
    fn mine(world: &World, id: EntityId) -> Vec<HudWidget> {
        world.hud_for(id)
    }

    #[test]
    fn map_widgets_fill_in_counters_and_time_and_go_when_their_time_is_up() {
        let mut world = world_with(vec![
            MapTrigger {
                group: 1,
                amount: 3,
                seconds: 0.2,
                ..trigger(TriggerWhen::Start, TriggerAction::Set)
            },
            MapTrigger {
                duration: 2.,
                ..show(
                    2,
                    MapUi {
                        widget: widget("Cores {1} · {left}s · {name}"),
                        timer: true,
                        ..MapUi::default()
                    },
                )
            },
        ]);
        let player = world.add_player("Ada", false);
        run(&mut world, 1.);
        let shown = mine(&world, player);
        assert_eq!(shown.len(), 1, "{shown:?}");
        assert!(
            shown[0].text.starts_with("Cores 3 · 2s · Ada"),
            "{}",
            shown[0].text
        );
        let progress = shown[0].progress.expect("a timer bar");
        assert!((0.6..=1.).contains(&progress), "{progress}");
        run(&mut world, 2.);
        assert!(mine(&world, player).is_empty(), "gone after its duration");
        assert!(world.shown_ui().is_empty());
    }

    #[test]
    fn pinned_widgets_float_over_their_area_or_the_tank_that_set_them_off() {
        let mut world = world_with(vec![
            show(
                1,
                MapUi {
                    widget: widget("Shop"),
                    pin: UiPin::Area,
                    ..MapUi::default()
                },
            ),
            show(
                2,
                MapUi {
                    widget: widget("It"),
                    pin: UiPin::Cause,
                    ..MapUi::default()
                },
            ),
        ]);
        let watcher = world.add_player("Watcher", false);
        let it = world.add_player("It", false);
        run(&mut world, 1.);
        let shown = mine(&world, watcher);
        assert_eq!(
            shown.len(),
            1,
            "nothing to float over without a tank that set it off: {shown:?}"
        );
        let HudPin::World([x, y]) = shown[0].pin else {
            panic!("over the area: {:?}", shown[0].pin);
        };
        assert!((x - world.config.width * 0.5).abs() < 0.01);
        assert!((y - world.config.height * 0.5).abs() < 0.01);
        world.show_ui(1, 2, None, it);
        run(&mut world, 0.2);
        let over = mine(&world, watcher)
            .into_iter()
            .find(|widget| widget.text == "It")
            .expect("shown over the tank");
        assert_eq!(over.pin, HudPin::Tank(it));
        world.remove_player(it);
        run(&mut world, 0.2);
        assert!(
            mine(&world, watcher)
                .iter()
                .all(|widget| widget.text != "It"),
            "it goes with its tank"
        );
        assert_eq!(mine(&world, watcher).len(), 1, "the shop sign stays");
    }

    #[test]
    fn a_pick_signals_its_channel_with_the_picker_as_the_tank_that_set_it_off() {
        let mut world = world_with(vec![
            show(
                1,
                MapUi {
                    widget: HudWidget {
                        choices: vec!["Points".into(), "Nothing".into()],
                        ..widget("Pick one")
                    },
                    channels: vec![4, 0],
                    ..MapUi::default()
                },
            ),
            MapTrigger {
                source: 4,
                amount: 50,
                who: TriggerWho::Cause,
                ..trigger(TriggerWhen::Signal, TriggerAction::Award)
            },
        ]);
        let first = world.add_player("First", false);
        let second = world.add_player("Second", false);
        run(&mut world, 1.);
        let menu = mine(&world, first)[0].menu;
        assert_ne!(menu, 0, "a menu has a number");
        assert_eq!(mine(&world, second)[0].menu, menu, "everyone sees the menu");
        let score = |world: &World, id| world.tanks.iter().find(|t| t.id == id).unwrap().score;
        let (before_first, before_second) = (score(&world, first), score(&world, second));
        assert!(!world.choose(first, menu, 2), "no third choice");
        assert!(!world.choose(first, menu + 2, 0), "no such menu");
        assert!(world.choose(first, menu, 0));
        assert!(
            mine(&world, first).is_empty(),
            "the menu closes for the picker"
        );
        assert!(!world.choose(first, menu, 0), "and can't be picked again");
        run(&mut world, 0.2);
        assert_eq!(
            score(&world, first),
            before_first + 50,
            "the picker is rewarded"
        );
        assert_eq!(
            score(&world, second),
            before_second,
            "the other player is not"
        );
        assert_eq!(
            mine(&world, second).len(),
            1,
            "still open for the other player"
        );
        // A player who joins later still gets the menu.
        let late = world.add_player("Late", false);
        run(&mut world, 0.2);
        assert_eq!(mine(&world, late).len(), 1);
        // A player who leaves no longer holds a place in the exceptions.
        world.remove_player(first);
        run(&mut world, 0.2);
        assert!(world.shown_ui()[0].except.is_empty());
    }

    #[test]
    fn widgets_shown_to_one_player_replace_their_slot_and_hide_ui_clears_them() {
        let mut world = world_with(vec![
            show(1, MapUi::starter()),
            MapTrigger {
                seconds: 1.,
                ..show(
                    1,
                    MapUi {
                        widget: widget("Just for you"),
                        to: UiAudience::Inside,
                        ..MapUi::default()
                    },
                )
            },
            MapTrigger {
                source: 2,
                group: 0,
                ..trigger(TriggerWhen::Signal, TriggerAction::HideUi)
            },
        ]);
        let inside = world.add_player("Inside", false);
        let outside = world.add_player("Outside", false);
        let center = V2::new(world.config.width * 0.5, world.config.height * 0.5);
        // Hold both tanks in place while the triggers run.
        for _ in 0..72 {
            for tank in &mut world.tanks {
                tank.pos = if tank.id == inside {
                    center
                } else {
                    V2::new(60., 60.)
                };
            }
            world.step();
        }
        assert_eq!(mine(&world, inside).len(), 1, "one widget in the slot");
        assert_eq!(mine(&world, inside)[0].text, "Just for you");
        assert_eq!(mine(&world, outside)[0].title, "Notice");
        // The inside player's own widget leaves with them.
        world.remove_player(inside);
        run(&mut world, 0.2);
        assert!(world.shown_ui().iter().all(|ui| ui.viewer == 0));
        assert!(world.shown_ui().iter().all(|ui| ui.except.is_empty()));
        world.map_input(crate::map_features::LogicInput::Signal(2));
        run(&mut world, 0.1);
        assert!(mine(&world, outside).is_empty());
        assert!(world.shown_ui().is_empty());
    }

    #[test]
    fn widget_text_fills_known_fields_and_keeps_the_rest() {
        let counters = [5, -2, 0, 0, 0, 0, 0, 0];
        let fill = UiFill {
            counters: &counters,
            left: 3.2,
            time: 125.,
            name: "Bo",
            cause: "Zoë",
            score: 7,
        };
        assert_eq!(
            fill_ui_text("{1}/{2} {left} {time} {name} {score} {9} {x} {", &fill, 120),
            "5/-2 4 2:05 Bo 7 {9} {x} {"
        );
        assert_eq!(fill_ui_text("{name}", &fill, 1), "B");
        assert_eq!(fill_ui_text("{cause}", &fill, 120), "Zoë");
        assert_eq!(fill_ui_text("{cause}", &fill, 3), "Zo");
        let staying = UiFill {
            left: f32::INFINITY,
            ..fill
        };
        assert_eq!(fill_ui_text("[{left}]", &staying, 120), "[]");
    }

    #[test]
    fn all_thirty_two_slots_stay_distinct_and_hide_independently() {
        let mut world = world_with(
            (1..=32)
                .map(|slot| {
                    show(
                        slot,
                        MapUi {
                            widget: widget(&format!("Slot {slot}")),
                            ..Default::default()
                        },
                    )
                })
                .collect(),
        );
        let player = world.add_player("Pilot", false);
        run(&mut world, 1.);
        let shown = mine(&world, player);
        assert_eq!(shown.len(), 32);
        let ids: std::collections::BTreeSet<_> = shown.iter().map(|widget| widget.menu).collect();
        assert_eq!(
            ids.len(),
            32,
            "overlapping labels keep their own identities"
        );
        assert!(
            !world.choose(player, shown[0].menu, 0),
            "labels are not choices"
        );
        world.hide_ui(32, None);
        run(&mut world, 0.2);
        assert_eq!(mine(&world, player).len(), 31);
        assert!(
            mine(&world, player)
                .iter()
                .all(|widget| widget.text != "Slot 32")
        );
    }

    #[test]
    fn cause_names_survive_delays_without_expanding_fields_inside_names() {
        let mut world = world_with(vec![
            MapTrigger {
                group: 1,
                amount: 7,
                seconds: 0.,
                ..trigger(TriggerWhen::Start, TriggerAction::Set)
            },
            MapTrigger {
                group: 2,
                delay: 0.2,
                ..trigger(TriggerWhen::Enter, TriggerAction::Signal)
            },
            MapTrigger {
                source: 2,
                text: "{cause} switched {1}".into(),
                ..trigger(TriggerWhen::Signal, TriggerAction::Announce)
            },
            MapTrigger {
                source: 2,
                group: 1,
                ui: Some(Box::new(MapUi {
                    widget: widget("{cause}: {1}"),
                    ..Default::default()
                })),
                ..trigger(TriggerWhen::Signal, TriggerAction::Ui)
            },
        ]);
        let who = world.add_player("Zoë {2}", false);
        let spectator = world.add_player("Viewer", false);
        world
            .tanks
            .iter_mut()
            .find(|tank| tank.id == who)
            .unwrap()
            .pos = V2::new(1800., 1300.);
        world
            .tanks
            .iter_mut()
            .find(|tank| tank.id == spectator)
            .unwrap()
            .pos = V2::new(100., 100.);
        run(&mut world, 0.1);
        world
            .tanks
            .iter_mut()
            .find(|tank| tank.id == who)
            .unwrap()
            .pos = V2::new(200., 200.);
        run(&mut world, 0.4);
        assert_eq!(world.match_state.announcement, "Zoë {2} switched 7");
        assert_eq!(mine(&world, spectator)[0].text, "Zoë {2}: 7");
        world.remove_player(who);
        run(&mut world, 0.2);
        assert_eq!(mine(&world, spectator)[0].text, ": 7");
    }

    #[test]
    fn area_markers_follow_group_poses_without_changing_identity() {
        let mut world = world_with(vec![
            MapTrigger {
                member: 1,
                seconds: 0.,
                ..show(
                    1,
                    MapUi {
                        widget: widget("Platform"),
                        pin: UiPin::Area,
                        ..Default::default()
                    },
                )
            },
            MapTrigger {
                group: 1,
                seconds: 0.,
                travel: [0.1, 0.],
                tween: 1.,
                ..trigger(TriggerWhen::Start, TriggerAction::Move)
            },
        ]);
        let player = world.add_player("Pilot", false);
        run(&mut world, 0.2);
        let before = mine(&world, player).remove(0);
        assert_eq!(
            before.pin,
            HudPin::Group {
                id: 1,
                at: [1800., 1300.]
            }
        );
        run(&mut world, 1.);
        let after = mine(&world, player).remove(0);
        assert_eq!(before.menu, after.menu);
        let snapshot = world.snapshot();
        let poses = crate::snapshot_poses(&snapshot, snapshot.match_state.elapsed);
        assert!((poses.offset(1)[0] - 0.1).abs() < 0.0001);
    }

    #[test]
    fn pack_menus_and_map_menus_never_share_numbers() {
        assert_ne!(menu_number("pack", "menu") & 1 << 31, 0);
        assert_ne!(menu_number("pack", "menu"), menu_number("pack", "other"));
        assert_eq!(menu_number("pack", "menu"), menu_number("pack", "menu"));
        let mut world = world_with(vec![]);
        world.map.triggers = vec![show(1, MapUi::starter())];
        world.show_ui(0, 1, None, 0);
        assert_eq!(world.shown_ui()[0].menu & 1 << 31, 0);
    }
}
