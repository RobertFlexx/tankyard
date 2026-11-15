//! Shared inline number fields. Only committed, validated values leave this module.
use super::*;
use std::{
    cell::RefCell,
    hash::{Hash, Hasher},
};
use tankyard_client::number_input::{self, Error};

#[derive(Clone, Copy)]
pub(super) struct Units {
    pub scale: f32,
    pub offset: f32,
}
impl Units {
    pub const RAW: Self = Self {
        scale: 1.,
        offset: 0.,
    };
    pub const PERCENT: Self = Self {
        scale: 100.,
        offset: 0.,
    };
    fn display(self, raw: f32) -> f32 {
        raw * self.scale + self.offset
    }
    fn text(self, raw: f32) -> String {
        if self.scale == 1. && self.offset == 0. {
            return raw.to_string();
        }
        format!("{:.4}", self.display(raw))
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
    fn raw(self, shown: f32) -> f32 {
        (shown - self.offset) / self.scale
    }
}
#[derive(Clone)]
struct Target {
    id: u64,
    scope: u64,
    original: f32,
    limits: [f32; 2],
    integer: bool,
    units: Units,
    rect: Rect,
}
impl Target {
    fn parse(&self, text: &str) -> Result<f32, Error> {
        // Relative edits start from the displayed value: 117% must not carry
        // the hidden binary rounding of 1.17 * 100 into the calculation.
        let original = self.entry().parse().map_err(|_| Error::Invalid)?;
        number_input::evaluate(
            text,
            original,
            self.limits.map(|v| self.units.display(v)),
            self.integer,
        )
        .map(|v| self.units.raw(v).clamp(self.limits[0], self.limits[1]))
    }
    fn entry(&self) -> String {
        self.units.text(self.original)
    }
}
struct Editing {
    target: Target,
    text: String,
    fresh: bool,
    seen: bool,
    error: Option<Error>,
}
impl Editing {
    fn new(target: Target) -> Box<Self> {
        Box::new(Self {
            text: target.entry(),
            target,
            fresh: true,
            seen: true,
            error: None,
        })
    }
}
#[derive(Default)]
struct State {
    active: Option<Box<Editing>>,
    previous: Vec<Target>,
    current: Vec<Target>,
    commits: Vec<(u64, f32)>,
    guard: bool,
    handled: bool,
    committed: bool,
    reveal: bool,
}
thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
    static SALT: Cell<u64> = const { Cell::new(0) };
}
pub fn numeric_scope(salt: u64) -> u64 {
    SALT.with(|s| s.replace(salt))
}
pub fn numeric_committed() -> bool {
    STATE.with(|s| s.borrow().committed)
}
pub fn numeric_active() -> bool {
    STATE.with(|s| {
        let s = s.borrow();
        s.guard || s.active.is_some()
    })
}
/// At frame start the old field still owns keyboard shortcuts, including the
/// Enter/Escape used to finish it. Pointer blur applies valid input only.
pub fn begin_frame() {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let State {
            previous, current, ..
        } = &mut *s;
        std::mem::swap(previous, current);
        current.clear();
        s.guard = s.active.is_some();
        s.handled = false;
        s.committed = false;
        if let Some(edit) = &mut s.active {
            edit.seen = false;
        }
        if is_mouse_button_pressed(MouseButton::Left)
            && s.active.as_ref().is_some_and(|e| {
                !e.target
                    .rect
                    .contains(vec2(mouse_position().0, mouse_position().1))
            })
            && let Some(edit) = s.active.take()
            && let Ok(value) = edit.target.parse(&edit.text)
        {
            s.commits.push((edit.target.id, value));
        }
    });
}
pub fn end_frame() {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.active.as_ref().is_some_and(|e| !e.seen) {
            s.active = None;
        }
        s.commits.clear();
    });
}
pub fn cancel_numeric() {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.active = None;
        s.commits.clear();
    });
}
/// A newly focused field asks its containing panel to bring it into view.
pub fn numeric_focus_rect() -> Option<Rect> {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if !s.reveal
            || s.active
                .as_ref()
                .is_none_or(|e| e.target.scope != POINTER_SCOPE.with(Cell::get))
        {
            return None;
        }
        s.reveal = false;
        s.active.as_ref().map(|e| Rect {
            h: e.target.rect.h * 2.,
            ..e.target.rect
        })
    })
}
pub(super) fn id(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    POINTER_SCOPE.with(Cell::get).hash(&mut h);
    SALT.with(Cell::get).hash(&mut h);
    text.hash(&mut h);
    h.finish()
}

pub(super) fn take_commit(id: u64, value: &mut f32) -> bool {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if let Some(at) = s.commits.iter().position(|&(key, _)| key == id) {
            let (_, next) = s.commits.remove(at);
            let changed = *value != next;
            *value = next;
            s.committed |= changed;
            changed
        } else {
            false
        }
    })
}
pub(super) fn field(
    rect: Rect,
    text: &str,
    value: &mut f32,
    range: SliderRange,
    s: f32,
    display: &str,
    field: Rect,
) -> bool {
    let SliderRange {
        limits,
        step,
        units,
        integer,
    } = range;
    let id = id(text);
    let target = Target {
        id,
        scope: POINTER_SCOPE.with(Cell::get),
        original: *value,
        limits,
        integer,
        units,
        rect: {
            let p = crate::ui_layout::position(field.point());
            Rect::new(p.x, p.y, field.w, field.h)
        },
    };
    let allowed = interactive();
    let mut changed = false;
    STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        if allowed {
            state.current.push(target.clone());
        }
        if allowed
            && contains(field)
            && is_mouse_button_pressed(MouseButton::Left)
            && state.active.as_ref().is_none_or(|e| e.target.id != id)
        {
            state.active = Some(Editing::new(target.clone()));
            state.guard = true;
        }
        let mine = allowed && state.active.as_ref().is_some_and(|e| e.target.id == id);
        if mine {
            let mut edit = state.active.take().unwrap();
            edit.seen = true;
            edit.target.rect = target.rect;
            if edit.fresh {
                edit.target = target.clone();
                edit.text = target.entry();
                reset_field(&mut edit.text);
            }
            let mut active = true;
            text_field(field, &mut edit.text, &mut active, 128, s * 0.76, false);
            if edit.fresh {
                select_field_all(&mut edit.text);
                edit.fresh = false;
            }
            let submit = !state.handled
                && (is_key_pressed(KeyCode::Enter)
                    || is_key_pressed(KeyCode::KpEnter)
                    || is_key_pressed(KeyCode::Tab));
            let cancel = !state.handled && is_key_pressed(KeyCode::Escape);
            if cancel {
                state.handled = true;
            } else if submit {
                state.handled = true;
                match edit.target.parse(&edit.text) {
                    Ok(next) => {
                        changed |= *value != next;
                        *value = next;
                        state.committed |= changed;
                        if is_key_pressed(KeyCode::Tab) {
                            let peers: Vec<_> = state
                                .previous
                                .iter()
                                .filter(|t| t.scope == target.scope)
                                .collect();
                            if let Some(at) = peers.iter().position(|t| t.id == id) {
                                let backwards = is_key_down(KeyCode::LeftShift)
                                    || is_key_down(KeyCode::RightShift);
                                let at = if backwards {
                                    (at + peers.len() - 1) % peers.len()
                                } else {
                                    (at + 1) % peers.len()
                                };
                                let mut next = peers[at].clone();
                                if next.id == id {
                                    next.original = *value;
                                }
                                state.active = Some(Editing::new(next));
                                state.reveal = true;
                            }
                        }
                    }
                    Err(error) => {
                        edit.error = Some(error);
                        state.active = Some(edit);
                    }
                }
            } else {
                let delta = i32::from(is_key_pressed(KeyCode::Up))
                    - i32::from(is_key_pressed(KeyCode::Down));
                if !state.handled && delta != 0 {
                    let base = edit
                        .target
                        .parse(&edit.text)
                        .unwrap_or(edit.target.original);
                    let mut step = if step > 0. {
                        step
                    } else {
                        (limits[1] - limits[0]) / 100.
                    };
                    if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) {
                        step *= 10.;
                    }
                    if is_key_down(KeyCode::LeftAlt) || is_key_down(KeyCode::RightAlt) {
                        step = (step * 0.1).max(if edit.target.integer { 1. } else { 0.000001 });
                    }
                    let next = (base + delta as f32 * step).clamp(limits[0], limits[1]);
                    edit.text = units.text(next);
                    edit.error = None;
                    select_field_all(&mut edit.text);
                } else if !edit.fresh && !submit {
                    edit.error = edit.target.parse(&edit.text).err();
                }
                if let Some(error) = edit.error {
                    ui_rectangle_lines(field.x, field.y, field.w, field.h, 1., RED);
                    let message = if error == Error::OutOfRange {
                        format!(
                            "Allowed: {} to {}",
                            units.text(limits[0]),
                            units.text(limits[1])
                        )
                    } else {
                        error.message().to_owned()
                    };
                    label(
                        &fitted_label(&message, 10. * s, rect.w),
                        rect.x,
                        rect.y + 47. * s,
                        10. * s,
                        RED,
                    );
                } else {
                    label(
                        "Enter: apply · Esc: cancel · Tab: next",
                        rect.x,
                        rect.y + 47. * s,
                        10. * s,
                        muted_color(),
                    );
                }
                state.active = Some(edit);
            }
        } else {
            ui_rectangle(
                field.x,
                field.y,
                field.w,
                field.h,
                if allowed && contains(field) {
                    hover_color()
                } else {
                    mix(surface_color(), bg_color(), 0.35)
                },
            );
            ui_rectangle_lines(field.x, field.y, field.w, field.h, 1., line_color());
            right_label(
                &fitted_label(display, 16. * s, (field.w - 12. * s).max(0.)),
                field.right() - 6. * s,
                rect.y + 18. * s,
                16. * s,
                if allowed { text_color() } else { muted_color() },
            );
            if allowed && contains(field) {
                label(
                    "Click value to type · calculations work too",
                    rect.x,
                    rect.y + 47. * s,
                    10. * s,
                    muted_color(),
                );
            }
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    fn target(units: Units, original: f32, limits: [f32; 2]) -> Target {
        Target {
            id: 42,
            scope: 0,
            original,
            limits,
            integer: false,
            units,
            rect: Rect::new(0., 0., 100., 24.),
        }
    }
    #[test]
    fn typed_percentages_use_displayed_units_without_drag_step_quantization() {
        let percent = target(Units::PERCENT, 1.17, [0.8, 1.4]);
        assert_eq!(percent.entry(), "117");
        assert_eq!(percent.parse("+=3%"), Ok(1.2));
        assert_eq!(percent.parse("95.5%"), Ok(0.955));
        assert_eq!(percent.parse("150%"), Err(Error::OutOfRange));
        let width = target(Units::RAW, 4000., [800., 20_000.]);
        assert_eq!(width.parse("1250"), Ok(1250.));
        assert_eq!(width.parse("1250.25"), Ok(1250.25));
        let mut whole = target(Units::RAW, 2., [0., 32.]);
        whole.integer = true;
        assert_eq!(whole.parse("3.6"), Ok(4.));
        let original = width.parse("/=2").unwrap();
        assert_eq!(original, 2000.);
        assert_eq!(width.parse("/=2"), Ok(original));
    }
    #[test]
    fn a_blurred_value_commits_once_and_cancel_drops_pending_edits() {
        STATE.with(|s| *s.borrow_mut() = State::default());
        STATE.with(|s| s.borrow_mut().commits.push((42, 1200.)));
        let mut value = 800.;
        assert!(take_commit(42, &mut value));
        assert_eq!(value, 1200.);
        assert!(numeric_committed());
        assert!(!take_commit(42, &mut value));
        STATE.with(|s| s.borrow_mut().commits.push((42, 1500.)));
        cancel_numeric();
        assert!(!take_commit(42, &mut value));
        assert_eq!(value, 1200.);
        STATE.with(|s| *s.borrow_mut() = State::default());
    }
}
