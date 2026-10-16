//! The message box: text with a cursor, edited as readline edits a line, wrapped at words, five
//! lines tall at most and scrolling inside itself past that.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// The box grows to this many lines, then scrolls.
pub const MOST_LINES: usize = 5;

#[derive(Clone, Debug, Default)]
pub struct Composer {
    text: String,
    /// A byte offset, always on a grapheme's boundary.
    cursor: usize,
    /// What the last kill took, for Ctrl-Y.
    killed: String,
    /// The first line shown, once there are more than five.
    scroll: usize,
    /// The column Up and Down keep to, from where they started.
    goal: Option<usize>,
}

/// A line as drawn: where it starts and ends in the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Row {
    start: usize,
    end: usize,
    /// Whether the line was wrapped rather than broken by a newline.
    soft: bool,
}

impl Composer {
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Where the cursor is, as a byte offset into the text.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Puts `with` in place of the text from `start` to the cursor, the cursor after it.
    pub fn replace_back_to(&mut self, start: usize, with: &str) {
        let start = start.min(self.cursor);
        self.text.replace_range(start..self.cursor, with);
        self.cursor = start + with.len();
        self.goal = None;
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Replaces the text, the cursor at its end, as a chat's draft comes back.
    pub fn set(&mut self, text: &str) {
        self.text = text.to_string();
        self.cursor = self.text.len();
        self.scroll = 0;
        self.goal = None;
    }

    /// The text, leaving the box empty.
    pub fn take(&mut self) -> String {
        self.cursor = 0;
        self.scroll = 0;
        self.goal = None;
        std::mem::take(&mut self.text)
    }

    pub fn insert(&mut self, s: &str) {
        let s = s.replace("\r\n", "\n").replace('\r', "\n").replace('\t', "    ");
        self.text.insert_str(self.cursor, &s);
        self.cursor += s.len();
        self.goal = None;
    }

    fn prev(&self, at: usize) -> usize {
        self.text[..at].grapheme_indices(true).next_back().map_or(0, |(i, _)| i)
    }

    fn next(&self, at: usize) -> usize {
        self.text[at..].graphemes(true).next().map_or(at, |g| at + g.len())
    }

    fn cut(&mut self, from: usize, to: usize) {
        if from < to {
            self.killed = self.text[from..to].to_string();
            self.text.replace_range(from..to, "");
            self.cursor = from;
        }
        self.goal = None;
    }

    pub fn backspace(&mut self) {
        let from = self.prev(self.cursor);
        self.text.replace_range(from..self.cursor, "");
        self.cursor = from;
        self.goal = None;
    }

    pub fn delete(&mut self) {
        let to = self.next(self.cursor);
        self.text.replace_range(self.cursor..to, "");
        self.goal = None;
    }

    pub fn left(&mut self) {
        self.cursor = self.prev(self.cursor);
        self.goal = None;
    }

    pub fn right(&mut self) {
        self.cursor = self.next(self.cursor);
        self.goal = None;
    }

    /// Back to the start of the word, as readline's Alt-B.
    fn word_start(&self) -> usize {
        let mut at = self.cursor;
        let mut seen = false;
        for (i, g) in self.text[..self.cursor].grapheme_indices(true).rev() {
            let word = g.chars().any(char::is_alphanumeric);
            if seen && !word {
                break;
            }
            seen |= word;
            at = i;
        }
        at
    }

    /// On to the end of the word, as readline's Alt-F.
    fn word_end(&self) -> usize {
        let mut at = self.cursor;
        let mut seen = false;
        for (i, g) in self.text[self.cursor..].grapheme_indices(true) {
            let word = g.chars().any(char::is_alphanumeric);
            if seen && !word {
                break;
            }
            seen |= word;
            at = self.cursor + i + g.len();
        }
        at
    }

    pub fn word_left(&mut self) {
        self.cursor = self.word_start();
        self.goal = None;
    }

    pub fn word_right(&mut self) {
        self.cursor = self.word_end();
        self.goal = None;
    }

    fn line_start(&self) -> usize {
        self.text[..self.cursor].rfind('\n').map_or(0, |i| i + 1)
    }

    fn line_end(&self) -> usize {
        self.text[self.cursor..].find('\n').map_or(self.text.len(), |i| self.cursor + i)
    }

    pub fn home(&mut self) {
        self.cursor = self.line_start();
        self.goal = None;
    }

    pub fn end(&mut self) {
        self.cursor = self.line_end();
        self.goal = None;
    }

    /// Ctrl-U: the line up to the cursor.
    pub fn kill_line_back(&mut self) {
        self.cut(self.line_start(), self.cursor);
    }

    /// Ctrl-W: back to the last space, as readline's unix-word-rubout.
    pub fn kill_to_space(&mut self) {
        let before = &self.text[..self.cursor];
        let trimmed = before.trim_end_matches(' ');
        let from = trimmed.rfind([' ', '\n']).map_or(0, |i| i + 1);
        self.cut(from, self.cursor);
    }

    /// Alt-Backspace: back to the start of the word.
    pub fn kill_word_back(&mut self) {
        self.cut(self.word_start(), self.cursor);
    }

    /// Alt-D: on to the end of the word.
    pub fn kill_word_forward(&mut self) {
        let to = self.word_end();
        let from = self.cursor;
        self.cut(from, to);
    }

    /// Ctrl-Y: puts back what the last kill took.
    pub fn yank(&mut self) {
        let killed = self.killed.clone();
        self.insert(&killed);
    }

    /// The text as lines `width` columns wide, a column kept free for the cursor at a line's end.
    fn rows(&self, width: usize) -> Vec<Row> {
        let room = width.saturating_sub(1).max(1);
        let mut rows = Vec::new();
        let mut start = 0;
        for line in self.text.split('\n') {
            let end = start + line.len();
            let mut row_start = start;
            let mut used = 0;
            let mut space: Option<usize> = None;
            for (i, g) in line.grapheme_indices(true) {
                let at = start + i;
                let gw = g.width();
                if used + gw > room && at > row_start {
                    let cut = match space {
                        Some(s) if s > row_start => s,
                        _ => at,
                    };
                    rows.push(Row { start: row_start, end: cut, soft: true });
                    used = self.text[cut..at].width();
                    row_start = cut;
                    space = None;
                }
                used += gw;
                if g == " " {
                    space = Some(at + 1);
                }
            }
            rows.push(Row { start: row_start, end, soft: false });
            start = end + 1;
        }
        rows
    }

    fn cursor_row(&self, rows: &[Row]) -> usize {
        rows.iter()
            .position(|r| self.cursor >= r.start && (self.cursor < r.end || (self.cursor == r.end && !r.soft)))
            .unwrap_or(rows.len().saturating_sub(1))
    }

    /// How many lines the box shows at `width`: one to five.
    pub fn height(&self, width: usize) -> usize {
        self.rows(width).len().clamp(1, MOST_LINES)
    }

    /// Moves a line up or down at `width`, keeping to the column it started from; false at the
    /// first or last line, where the cursor stays.
    pub fn vertical(&mut self, by: isize, width: usize) -> bool {
        let rows = self.rows(width);
        let row = self.cursor_row(&rows);
        let target = row as isize + by;
        if target < 0 || target as usize >= rows.len() {
            return false;
        }
        let goal = self.goal.unwrap_or_else(|| self.text[rows[row].start..self.cursor].width());
        let r = rows[target as usize];
        let mut at = r.start;
        let mut used = 0;
        for (i, g) in self.text[r.start..r.end].grapheme_indices(true) {
            if used + g.width() > goal {
                break;
            }
            used += g.width();
            at = r.start + i + g.len();
        }
        if r.soft && at == r.end {
            at = self.prev(at).max(r.start);
        }
        self.cursor = at;
        self.goal = Some(goal);
        true
    }

    /// The lines to draw at `width`, at most five, scrolled to keep the cursor in sight, and where
    /// the cursor is among them.
    pub fn view(&mut self, width: usize) -> (Vec<String>, (usize, usize)) {
        let rows = self.rows(width);
        let row = self.cursor_row(&rows);
        if row < self.scroll {
            self.scroll = row;
        } else if row >= self.scroll + MOST_LINES {
            self.scroll = row + 1 - MOST_LINES;
        }
        self.scroll = self.scroll.min(rows.len().saturating_sub(MOST_LINES));
        let shown =
            rows.iter().skip(self.scroll).take(MOST_LINES).map(|r| self.text[r.start..r.end].to_string()).collect();
        let col = self.text[rows[row].start..self.cursor].width();
        (shown, (row - self.scroll, col))
    }

    /// Puts the cursor at a row and column of what `view` drew.
    pub fn click(&mut self, row: usize, col: usize, width: usize) {
        let rows = self.rows(width);
        let Some(r) = rows.get(self.scroll + row).copied() else {
            self.cursor = self.text.len();
            return;
        };
        let mut at = r.start;
        let mut used = 0;
        for (i, g) in self.text[r.start..r.end].grapheme_indices(true) {
            if used + g.width() > col {
                break;
            }
            used += g.width();
            at = r.start + i + g.len();
        }
        self.cursor = at;
        self.goal = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(s: &str) -> Composer {
        let mut c = Composer::default();
        c.insert(s);
        c
    }

    #[test]
    fn readline_keys_move_and_cut_by_words_and_lines() {
        let mut c = typed("vens jantar sexta");
        c.word_left();
        c.insert("na ");
        assert_eq!(c.text(), "vens jantar na sexta");
        c.home();
        c.kill_word_forward();
        assert_eq!(c.text(), " jantar na sexta");
        c.end();
        c.kill_to_space();
        assert_eq!(c.text(), " jantar na ");
        c.yank();
        assert_eq!(c.text(), " jantar na sexta", "Ctrl-Y puts the last kill back");
        c.kill_line_back();
        assert_eq!(c.text(), "");
        c.insert("ok, até já");
        c.kill_word_back();
        assert_eq!(c.text(), "ok, até ");
        c.backspace();
        c.left();
        c.delete();
        assert_eq!(c.text(), "ok, at", "accents are one grapheme");
    }

    #[test]
    fn it_wraps_at_words_and_keeps_the_cursor_in_view_past_five_lines() {
        let mut c = typed("did you get the photos from Sunday?");
        let (lines, cursor) = c.view(16);
        assert_eq!(lines, ["did you get ", "the photos ", "from Sunday?"]);
        assert_eq!(cursor, (2, 12));
        assert_eq!(c.height(16), 3);
        for _ in 0..4 {
            c.insert("\nmore");
        }
        assert_eq!(c.height(16), MOST_LINES, "five lines, then it scrolls");
        let (lines, cursor) = c.view(16);
        assert_eq!(lines.first().map(String::as_str), Some("from Sunday?"));
        assert_eq!(cursor, (4, 4));
    }

    #[test]
    fn up_and_down_move_between_lines_and_stop_at_the_ends() {
        let mut c = typed("first line\nsecond");
        assert!(!c.vertical(1, 40), "already on the last line");
        assert!(c.vertical(-1, 40));
        assert_eq!(&c.text()[..c.cursor], "first ");
        assert!(!c.vertical(-1, 40));
        assert!(c.vertical(1, 40));
        assert_eq!(c.cursor, c.text().len(), "the column it started from, as far as the line goes");
    }

    #[test]
    fn a_click_puts_the_cursor_where_it_lands() {
        let mut c = typed("hello there\nworld");
        c.view(40);
        c.click(0, 6, 40);
        c.insert("out ");
        assert_eq!(c.text(), "hello out there\nworld");
        c.click(1, 99, 40);
        assert_eq!(c.cursor, c.text().len());
    }
}
