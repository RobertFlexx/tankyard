//! Ctrl+F: find objects by what they are, the group they're in or what they
//! do, and step through them on the canvas.
use super::{Editor, Layout, Mode, objects, shift};
use crate::ui::*;
use macroquad::prelude::*;
use tankyard_client::editor_document::Key;

/// The most letters worth searching for.
const LONGEST_QUERY: usize = 60;

#[derive(Default)]
pub(super) struct Find {
    pub(super) open: bool,
    /// Whether the box has the keyboard.
    pub(super) typing: bool,
    query: String,
    hits: Vec<Key>,
    /// The hit the view is on, once one has been gone to.
    at: Option<usize>,
    /// The revision and words `hits` was found for.
    stamp: Option<(u64, String)>,
}

impl Find {
    /// What the bar is showing, to be outlined on the canvas.
    pub(super) fn hits(&self) -> &[Key] {
        if self.open { &self.hits } else { &[] }
    }
    /// The next hit `step` away, wrapping around at the ends.
    fn stepped(at: Option<usize>, step: isize, count: usize) -> Option<usize> {
        if count == 0 {
            return None;
        }
        let count = count as isize;
        Some(match at {
            None if step < 0 => (count - 1) as usize,
            None => 0,
            Some(at) => (at as isize + step).rem_euclid(count) as usize,
        })
    }
    /// How far through the hits the bar is, in a few words.
    fn tally(&self) -> String {
        match (self.at, self.hits.len()) {
            _ if self.query.trim().is_empty() => String::new(),
            (_, 0) => "No match".to_owned(),
            (Some(at), count) => format!("{} of {count}", at + 1),
            (None, 1) => "1 found".to_owned(),
            (None, count) => format!("{count} found"),
        }
    }
}

impl Editor {
    pub(super) fn open_find(&mut self) {
        self.find.open = true;
        self.find.typing = true;
        self.text_focus = None;
        cancel_numeric();
        // The F of Ctrl+F shouldn't land in the box.
        while get_char_pressed().is_some() {}
    }
    /// Where the bar sits, under the chips along the canvas's top edge.
    pub(super) fn find_rect(&self, layout: &Layout) -> Option<Rect> {
        self.find.open.then(|| {
            let s = layout.s;
            let w = (430. * s).min(layout.canvas.w - 20. * s);
            Rect::new(
                layout.canvas.right() - w - 10. * s,
                layout.canvas.y + 42. * s,
                w,
                36. * s,
            )
        })
    }
    /// Searches again when the words or the map have changed.
    pub(super) fn refresh_find(&mut self) {
        if !self.find.open {
            return;
        }
        let current = self.find.stamp.as_ref().is_some_and(|(revision, query)| {
            *revision == self.revision && *query == self.find.query
        });
        if current {
            return;
        }
        let words_changed = self
            .find
            .stamp
            .as_ref()
            .is_none_or(|(_, query)| *query != self.find.query);
        self.find.hits = self.doc.find(&self.find.query);
        self.find.at = if words_changed {
            None
        } else {
            self.find.at.filter(|&at| at < self.find.hits.len())
        };
        self.find.stamp = Some((self.revision, self.find.query.clone()));
    }
    fn step_find(&mut self, step: isize) {
        self.refresh_find();
        let Some(at) = Find::stepped(self.find.at, step, self.find.hits.len()) else {
            return;
        };
        self.find.at = Some(at);
        self.locate(self.find.hits[at]);
        self.mode = Mode::Edit;
    }
    /// Selects every hit, for moving or changing them together.
    fn select_hits(&mut self) {
        self.refresh_find();
        let keys = self.find.hits.clone();
        if keys.is_empty() {
            return;
        }
        if keys.iter().any(|&key| !self.on_layer(key)) {
            self.set_layer(None);
        }
        let count = keys.len();
        self.set_selection(keys);
        self.mode = Mode::Edit;
        self.say(format!("Selected {}.", objects(count)));
    }
    pub(super) fn find_bar(&mut self, layout: &Layout) {
        let Some(r) = self.find_rect(layout) else {
            return;
        };
        let s = layout.s;
        draw_rectangle(r.x, r.y, r.w, r.h, alpha(surface_color(), 0.96));
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1., line_color());
        let (inner_y, inner_h) = (r.y + 4. * s, r.h - 8. * s);
        let cell = |right: f32, width: f32| Rect::new(right - width, inner_y, width, inner_h);
        let close = cell(r.right() - 4. * s, 26. * s);
        let all = cell(close.x - 4. * s, 38. * s);
        let next = cell(all.x - 4. * s, 26. * s);
        let previous = cell(next.x - 4. * s, 26. * s);
        let tally = cell(previous.x - 4. * s, 70. * s);
        let field = Rect::new(
            r.x + 4. * s,
            inner_y,
            (tally.x - r.x - 8. * s).max(40. * s),
            inner_h,
        );
        let mut active = self.find.typing;
        text_field(
            field,
            &mut self.find.query,
            &mut active,
            LONGEST_QUERY,
            s * 0.62,
            false,
        );
        self.find.typing = active;
        if self.find.query.is_empty() && !active {
            label(
                &fitted_label("Name, group or job", 12. * s, field.w - 14. * s),
                field.x + 8. * s,
                field.y + field.h * 0.5 + 4. * s,
                12. * s,
                muted_color(),
            );
        }
        let words = self.find.tally();
        if !words.is_empty() {
            center_label(
                &words,
                tally.center().x,
                tally.center().y + 4. * s,
                11.5 * s,
                if self.find.hits.is_empty() {
                    AMBER
                } else {
                    muted_color()
                },
            );
        }
        let any = !self.find.hits.is_empty();
        if button(previous, "‹", false, any, s * 0.8) {
            self.step_find(-1);
        }
        if button(next, "›", false, any, s * 0.8) {
            self.step_find(1);
        }
        if button(all, "All", false, any, s * 0.62) {
            self.select_hits();
        }
        if button(close, "×", false, true, s * 0.8) {
            self.close_find();
            return;
        }
        if !self.find.typing {
            return;
        }
        if is_key_pressed(KeyCode::Escape) {
            self.close_find();
        } else if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
            self.step_find(if shift() { -1 } else { 1 });
        } else if is_key_pressed(KeyCode::Down) {
            self.step_find(1);
        } else if is_key_pressed(KeyCode::Up) {
            self.step_find(-1);
        }
    }
    fn close_find(&mut self) {
        self.find.open = false;
        self.find.typing = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stepping_wraps_at_both_ends_and_nothing_found_goes_nowhere() {
        assert_eq!(Find::stepped(None, 1, 3), Some(0));
        assert_eq!(Find::stepped(None, -1, 3), Some(2));
        assert_eq!(Find::stepped(Some(2), 1, 3), Some(0));
        assert_eq!(Find::stepped(Some(0), -1, 3), Some(2));
        assert_eq!(Find::stepped(Some(1), 1, 3), Some(2));
        assert_eq!(Find::stepped(None, 1, 0), None);
        assert_eq!(Find::stepped(Some(4), 1, 0), None);
    }

    #[test]
    fn the_tally_says_where_you_are() {
        let mut find = Find::default();
        assert_eq!(find.tally(), "");
        find.query = "portal".into();
        assert_eq!(find.tally(), "No match");
        find.hits = vec![Key::Zone(0)];
        assert_eq!(find.tally(), "1 found");
        find.hits = vec![Key::Zone(0), Key::Zone(1), Key::Zone(2)];
        assert_eq!(find.tally(), "3 found");
        find.at = Some(1);
        assert_eq!(find.tally(), "2 of 3");
        find.query = "   ".into();
        assert_eq!(find.tally(), "", "blank isn't a search");
    }
}
