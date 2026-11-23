//! Up and Down in a chat box, to say something again without retyping it.
use macroquad::prelude::{KeyCode, is_key_pressed};
use std::collections::VecDeque;

/// How many sent lines are kept, for this session only.
const MOST: usize = 24;

#[derive(Default)]
pub struct ChatRecall {
    sent: VecDeque<String>,
    /// How far back Up has gone, from the newest line.
    at: Option<usize>,
    /// What was being typed when Up took over.
    kept: String,
}

impl ChatRecall {
    /// A line went out. Saying the same thing twice is kept once.
    pub fn note(&mut self, text: &str) {
        self.stop();
        if self.sent.back().is_some_and(|last| last == text) {
            return;
        }
        if self.sent.len() == MOST {
            self.sent.pop_front();
        }
        self.sent.push_back(text.to_owned());
    }

    /// The box has been opened or closed: back to typing fresh.
    pub fn stop(&mut self) {
        self.at = None;
        self.kept.clear();
    }

    fn line(&self, back: usize) -> Option<&str> {
        let index = self.sent.len().checked_sub(back + 1)?;
        self.sent.get(index).map(String::as_str)
    }

    fn up(&mut self, draft: &mut String) -> bool {
        let next = self.at.map_or(0, |at| at + 1);
        let Some(line) = self.line(next).map(str::to_owned) else {
            return false;
        };
        if self.at.is_none() {
            self.kept.clone_from(draft);
        }
        self.at = Some(next);
        *draft = line;
        true
    }

    fn down(&mut self, draft: &mut String) -> bool {
        match self.at {
            None => false,
            Some(0) => {
                self.at = None;
                *draft = std::mem::take(&mut self.kept);
                true
            }
            Some(at) => {
                let Some(line) = self.line(at - 1).map(str::to_owned) else {
                    return false;
                };
                self.at = Some(at - 1);
                *draft = line;
                true
            }
        }
    }

    /// Up and Down, while this box has the keyboard.
    pub fn keys(&mut self, draft: &mut String) {
        let moved = if is_key_pressed(KeyCode::Up) {
            self.up(draft)
        } else if is_key_pressed(KeyCode::Down) {
            self.down(draft)
        } else {
            false
        };
        if moved {
            crate::ui::caret_to_end(draft);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_walks_back_and_down_returns_what_you_were_typing() {
        let mut recall = ChatRecall::default();
        recall.note("first");
        recall.note("second");
        let mut draft = "half a tho".to_owned();
        assert!(recall.up(&mut draft));
        assert_eq!(draft, "second");
        assert!(recall.up(&mut draft));
        assert_eq!(draft, "first");
        assert!(!recall.up(&mut draft), "nothing older");
        assert_eq!(draft, "first");
        assert!(recall.down(&mut draft));
        assert_eq!(draft, "second");
        assert!(recall.down(&mut draft));
        assert_eq!(draft, "half a tho");
        assert!(!recall.down(&mut draft), "already typing fresh");
        assert_eq!(draft, "half a tho");
    }

    #[test]
    fn nothing_sent_means_up_leaves_the_box_alone() {
        let mut recall = ChatRecall::default();
        let mut draft = "hello".to_owned();
        assert!(!recall.up(&mut draft));
        assert!(!recall.down(&mut draft));
        assert_eq!(draft, "hello");
    }

    #[test]
    fn repeats_collapse_and_the_history_stays_bounded() {
        let mut recall = ChatRecall::default();
        recall.note("gg");
        recall.note("gg");
        assert_eq!(recall.sent.len(), 1);
        for line in 0..MOST * 3 {
            recall.note(&line.to_string());
        }
        assert_eq!(recall.sent.len(), MOST);
        assert_eq!(recall.line(0), Some((MOST * 3 - 1).to_string().as_str()));
        assert_eq!(recall.line(MOST), None);
    }

    #[test]
    fn sending_ends_the_walk_back() {
        let mut recall = ChatRecall::default();
        recall.note("old");
        let mut draft = String::new();
        recall.up(&mut draft);
        recall.note("old");
        assert_eq!(recall.at, None);
        let mut draft = "new".to_owned();
        assert!(!recall.down(&mut draft));
        assert_eq!(draft, "new");
    }
}
