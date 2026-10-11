//! Terminal-independent input editing and navigation. No persistence of prompt history.
use hud_agent_protocol::Provider;
use std::collections::VecDeque;
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ViewTab {
    #[default]
    Notes,
    Codex,
    Claude,
    OpenCode,
}
impl ViewTab {
    pub fn provider(self) -> Option<Provider> {
        match self {
            Self::Notes => None,
            Self::Codex => Some(Provider::Codex),
            Self::Claude => Some(Provider::Claude),
            Self::OpenCode => Some(Provider::OpenCode),
        }
    }
    pub fn index(self) -> usize {
        match self {
            Self::Notes => 0,
            Self::Codex => 1,
            Self::Claude => 2,
            Self::OpenCode => 3,
        }
    }
    pub fn from_digit(digit: char) -> Option<Self> {
        match digit {
            '1' => Some(Self::Notes),
            '2' => Some(Self::Claude),
            '3' => Some(Self::Codex),
            '4' => Some(Self::OpenCode),
            _ => None,
        }
    }
}
#[derive(Default)]
pub struct InputNavigation {
    pub cursor: usize,
    history: VecDeque<String>,
    selected: Option<usize>,
    draft: String,
}
impl InputNavigation {
    pub fn begin(&mut self) {
        self.cursor = 0;
        self.selected = None;
        self.draft.clear();
    }
    fn clamp(&mut self, text: &str) {
        self.cursor = self.cursor.min(text.len());
        while !text.is_char_boundary(self.cursor) {
            self.cursor -= 1;
        }
    }
    pub fn insert(&mut self, text: &mut String, c: char) {
        self.clamp(text);
        if !c.is_control() && text.len() + c.len_utf8() <= 16384 {
            text.insert(self.cursor, c);
            self.cursor += c.len_utf8();
            self.selected = None;
        }
    }
    pub fn left(&mut self, text: &str) {
        self.clamp(text);
        self.cursor = text[..self.cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i);
    }
    pub fn right(&mut self, text: &str) {
        self.clamp(text);
        self.cursor += text[self.cursor..].chars().next().map_or(0, char::len_utf8);
    }
    pub fn backspace(&mut self, text: &mut String) {
        let end = self.cursor;
        self.left(text);
        if end <= text.len() {
            text.drain(self.cursor..end);
        }
        self.selected = None;
    }
    pub fn delete(&mut self, text: &mut String) {
        self.clamp(text);
        let start = self.cursor;
        self.right(text);
        text.drain(start..self.cursor);
        self.cursor = start;
        self.selected = None;
    }
    pub fn remember(&mut self, text: &str) {
        if !text.trim().is_empty()
            && text.len() <= 16384
            && self.history.back().map(String::as_str) != Some(text)
        {
            self.history.push_back(text.into());
            while self.history.len() > 64 {
                self.history.pop_front();
            }
        }
        self.begin();
    }
    pub fn history(&mut self, text: &mut String, older: bool) {
        if self.history.is_empty() {
            return;
        }
        if older {
            let index = match self.selected {
                None => {
                    self.draft = text.clone();
                    self.history.len() - 1
                }
                Some(i) => i.saturating_sub(1),
            };
            self.selected = Some(index);
            *text = self.history[index].clone();
        } else if let Some(i) = self.selected {
            if i + 1 < self.history.len() {
                self.selected = Some(i + 1);
                *text = self.history[i + 1].clone();
            } else {
                self.selected = None;
                *text = self.draft.clone();
            }
        }
        self.cursor = text.len();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_edits_never_split_codepoints() {
        let mut n = InputNavigation::default();
        let mut s = String::new();
        for c in "ação日本".chars() {
            n.insert(&mut s, c);
        }
        n.left(&s);
        n.backspace(&mut s);
        assert_eq!(s, "ação本");
        n.insert(&mut s, '語');
        assert_eq!(s, "ação語本");
        n.delete(&mut s);
        assert_eq!(s, "ação語");
        n.cursor = 1;
        n.backspace(&mut s);
        assert_eq!(s, "ção語");
    }
    #[test]
    fn history_restores_unsent_draft_and_is_bounded() {
        let mut n = InputNavigation::default();
        n.remember("one");
        n.remember("two");
        let mut s = "draft".into();
        n.history(&mut s, true);
        assert_eq!(s, "two");
        n.history(&mut s, true);
        assert_eq!(s, "one");
        n.history(&mut s, false);
        n.history(&mut s, false);
        assert_eq!(s, "draft");
        for i in 0..90 {
            n.remember(&format!("{i}"));
        }
        assert_eq!(n.history.len(), 64);
    }
}

/// Horizontal viewport follows the cursor by terminal cells, never byte slicing mid-character.
pub fn input_view(text: &str, cursor: usize, width: usize) -> (String, usize) {
    let mut cursor = cursor.min(text.len());
    while !text.is_char_boundary(cursor) {
        cursor -= 1;
    }
    let safe = hud_resources::sanitize(text);
    let prefix = hud_resources::sanitize(&text[..cursor]);
    let cursor = prefix.len().min(safe.len());
    let budget = width.saturating_sub(2).max(1);
    let mut start = 0;
    while ratatui::text::Line::raw(&safe[start..cursor]).width() > budget {
        start += safe[start..cursor].chars().next().map_or(0, char::len_utf8);
    }
    let mut display = if start > 0 {
        "‹".into()
    } else {
        String::new()
    };
    let cursor_cell = ratatui::text::Line::raw(display.clone()).width()
        + ratatui::text::Line::raw(&safe[start..cursor]).width();
    for c in safe[start..].chars() {
        let mut next = display.clone();
        next.push(c);
        if ratatui::text::Line::raw(&next).width() > width.max(1) {
            break;
        }
        display = next;
    }
    (display, cursor_cell.min(width.saturating_sub(1)))
}
#[cfg(test)]
mod viewport_tests {
    use super::*;
    #[test]
    fn long_unicode_cursor_is_visible_and_boundaries_are_safe() {
        let s = "ação日本語".repeat(20);
        let (view, cursor) = input_view(&s, s.len(), 12);
        assert!(view.starts_with('‹'));
        assert!(ratatui::text::Line::raw(&view).width() <= 12);
        assert!(cursor < 12);
        let (view, cursor) = input_view(&s, 1, 12);
        assert!(view.starts_with('a'));
        assert_eq!(cursor, 1);
        let (view, _) = input_view("x\u{202e}y", 4, 8);
        assert!(!view.contains('\u{202e}'));
    }
}
