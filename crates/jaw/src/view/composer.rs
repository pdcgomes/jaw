//! The message box along the whole bottom of the screen, under the sidebar and the chat list too,
//! as the Claude command line has its own, since sending is the main job. Its border names the
//! chat it sends to, so that's never in doubt, and the status line, when it's on, sits above it.
//! The text, its draft per chat and its keys are the box's as ever (`crate::composer`); only where
//! it's drawn changes.

use super::{Focus, View};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Padding, Paragraph};

/// The prompt before the text, and the columns it takes.
const PROMPT: &str = "› ";
const PROMPT_W: u16 = 2;
/// The rows the panels keep however tall the box grows.
const PANELS_KEEP: u16 = 3;

impl View {
    /// Whether the box is along the bottom: switched on, with a chat open and the keys typing into
    /// it rather than the library's or the chat list's.
    fn composer_below(&self) -> bool {
        self.experiments.composer && self.open.is_some() && self.library.is_none() && !(self.narrow() && self.picking)
    }

    /// The rows the box takes along the bottom, `width` wide: its lines and its border.
    fn composer_rows(&self, width: u16) -> u16 {
        if self.recorder.is_some() {
            return 3;
        }
        let text = width.saturating_sub(4 + PROMPT_W) as usize;
        self.composer.height(text) as u16 + 2
    }

    /// The main screen with what the switches put below its panels: the status line, then the box.
    pub(super) fn draw_main_and_below(&mut self, f: &mut Frame, body: Rect) {
        let line = u16::from(self.status_line_shown()).min(body.height.saturating_sub(1));
        let mut boxed = if self.composer_below() { self.composer_rows(body.width) } else { 0 };
        boxed = boxed.min(body.height.saturating_sub(line + PANELS_KEEP));
        if boxed < 3 {
            boxed = 0;
        }
        let rest = Rect { height: body.height - line - boxed, ..body };
        self.draw_main(f, rest);
        if line > 0 {
            self.draw_status_line(f.buffer_mut(), Rect { y: rest.bottom(), height: line, ..body });
        }
        if boxed > 0 {
            let area = Rect { y: rest.bottom() + line, height: boxed, ..body };
            self.draw_composer_below(f, area);
            self.draw_naming(f, area);
        }
    }

    /// The box along the bottom: "to" and the chat in its border before what it replies to or
    /// edits, a prompt, the text and the cursor.
    fn draw_composer_below(&mut self, f: &mut Frame, area: Rect) {
        if self.recorder.is_some() {
            return self.draw_recorder(f, area);
        }
        let t = self.theme;
        let typing = self.focus == Focus::Typing && self.overlay.is_none();
        let name = self.open.as_deref().and_then(|id| self.chat_known(id)).map_or_else(String::new, |c| c.name.clone());
        let to = Line::from(vec![
            Span::styled(" to ", Style::new().fg(t.comment)),
            Span::styled(name, Style::new().fg(t.fg).add_modifier(Modifier::BOLD)),
            Span::raw(" "),
        ]);
        let titles_w = area.width.saturating_sub(to.width() as u16 + 1);
        let block = self.panel(if typing { t.focus } else { t.gutter }).title(to).padding(Padding::horizontal(1));
        let block = self.composer_titles(block, titles_w);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let prompt = Style::new().fg(if typing { t.focus } else { t.gutter }).add_modifier(Modifier::BOLD);
        f.buffer_mut().set_line(inner.x, inner.y, &Line::from(Span::styled(PROMPT, prompt)), PROMPT_W);
        let text = Rect { x: inner.x + PROMPT_W, width: inner.width.saturating_sub(PROMPT_W), ..inner };
        self.areas.composer = text;
        self.areas.composer_width = text.width as usize;
        let (lines, (row, col)) = self.composer.view(text.width as usize);
        let style = Style::new().fg(if typing { t.fg } else { t.text });
        let lines: Vec<Line> = lines.into_iter().map(|l| Line::from(Span::styled(l, style))).collect();
        f.render_widget(Paragraph::new(lines), text);
        if typing && (row as u16) < text.height {
            f.set_cursor_position((text.x + col as u16, text.y + row as u16));
        }
    }
}
