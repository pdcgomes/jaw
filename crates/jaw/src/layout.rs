//! A message as lines at a width: others' under their name on the left, yours in a column from a
//! third of the way across, each with its time at the right edge when the minute or the sender
//! changes. Laying one out is the costly part of a frame, so the view keeps what this returns per
//! message, version and width, and lays out only what's on screen.

use crate::data::{Kind, Message, SpanKind, Tick};
use crate::fmt;
use crate::theme::Theme;
use chrono::FixedOffset;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// The column others' names take, and where their text starts.
pub const NAME: usize = 6;
/// The time at the right edge: two spaces and HH:MM.
const TIME: usize = 7;
/// Your ticks before the time.
const TICKS: usize = 3;
/// A long message folds after this many lines, until it's opened.
pub const FOLD: usize = 40;

/// What a message's lines depend on beyond the message itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shape {
    pub width: usize,
    /// The sender changed: their name shows.
    pub name: bool,
    /// The minute or the sender changed: the time shows.
    pub time: bool,
    pub unfolded: bool,
}

pub struct Ctx<'a> {
    pub theme: &'a Theme,
    pub tz: FixedOffset,
    /// The account's own person, so a mention of you stands out.
    pub me: &'a [String],
}

/// Where your own messages start: a third of the way across.
pub fn indent(width: usize) -> usize {
    width / 3
}

/// Styled text cut into lines: the first `first` columns wide, the rest `rest`, broken at spaces
/// where it can be and at newlines always.
pub fn wrap(segments: &[(String, Style)], first: usize, rest: usize) -> Vec<Vec<(String, Style)>> {
    let mut lines: Vec<Vec<(String, Style)>> = vec![vec![]];
    let mut used = 0;
    let room = |n: usize| if n == 0 { first.max(1) } else { rest.max(1) };
    let push = |line: &mut Vec<(String, Style)>, s: &str, style: Style| match line.last_mut() {
        Some((text, st)) if *st == style => text.push_str(s),
        _ => line.push((s.to_string(), style)),
    };
    for (text, style) in segments {
        for (i, hard) in text.split('\n').enumerate() {
            if i > 0 {
                lines.push(vec![]);
                used = 0;
            }
            for word in runs(hard) {
                let w = word.width();
                let n = lines.len() - 1;
                if word.trim().is_empty() {
                    if used + w <= room(n) {
                        push(&mut lines[n], word, *style);
                        used += w;
                    } else if used > 0 {
                        lines.push(vec![]);
                        used = 0;
                    }
                    continue;
                }
                if used + w > room(n) && used > 0 {
                    if let Some((t, _)) = lines[n].last_mut() {
                        let kept = t.trim_end().len();
                        t.truncate(kept);
                    }
                    lines.push(vec![]);
                    used = 0;
                }
                if w <= room(lines.len() - 1) {
                    let n = lines.len() - 1;
                    push(&mut lines[n], word, *style);
                    used += w;
                    continue;
                }
                for g in word.graphemes(true) {
                    let gw = g.width();
                    if used + gw > room(lines.len() - 1) && used > 0 {
                        lines.push(vec![]);
                        used = 0;
                    }
                    let n = lines.len() - 1;
                    push(&mut lines[n], g, *style);
                    used += gw;
                }
            }
        }
    }
    lines
}

/// Words and the spaces between them, in turn.
fn runs(s: &str) -> impl Iterator<Item = &str> {
    let mut rest = s;
    std::iter::from_fn(move || {
        let first = rest.chars().next()?;
        let space = first.is_whitespace();
        let end = rest.find(|c: char| c.is_whitespace() != space).unwrap_or(rest.len());
        let (run, after) = rest.split_at(end);
        rest = after;
        Some(run)
    })
}

fn width_of(line: &[(String, Style)]) -> usize {
    line.iter().map(|(s, _)| s.width()).sum()
}

/// What a kind jaw doesn't draw yet says, when jawd hasn't summed it up itself.
pub fn summary(m: &Message) -> String {
    if let Some(s) = &m.summary {
        return s.clone();
    }
    match m.kind {
        Kind::Image => "▣ photo",
        Kind::Video => "▶ video",
        Kind::Gif => "▶ GIF",
        Kind::VideoNote => "▶ video message",
        Kind::Voice => "▶ voice message",
        Kind::Audio => "♪ audio",
        Kind::Document => "▣ document",
        Kind::Sticker => "◇ sticker",
        Kind::Location => "⌖ location",
        Kind::LiveLocation => "⌖ live location",
        Kind::Contact => "◉ contact",
        Kind::Poll => "☰ poll",
        Kind::Event => "◷ event",
        Kind::Text | Kind::System | Kind::Call => "",
        Kind::Unknown => "jaw can't show this yet",
    }
    .to_string()
}

pub fn ticks(tick: Tick, t: &Theme) -> Span<'static> {
    match tick {
        Tick::Pending => Span::styled("◷", Style::new().fg(t.unseen)),
        Tick::Sent => Span::styled("✓", Style::new().fg(t.unseen)),
        Tick::Delivered => Span::styled("✓✓", Style::new().fg(t.unseen)),
        Tick::Read | Tick::Played => Span::styled("✓✓", Style::new().fg(t.read)),
        Tick::Failed => Span::styled("!", Style::new().fg(t.warn).add_modifier(Modifier::BOLD)),
    }
}

/// The text of a message as styled segments, from jawd's spans.
fn body(m: &Message, ctx: &Ctx) -> Vec<(String, Style)> {
    let t = ctx.theme;
    let plain = Style::new().fg(t.fg);
    let dim = Style::new().fg(t.comment).add_modifier(Modifier::ITALIC);
    if m.deleted {
        let said = if m.mine { "⊘ You deleted this message" } else { "⊘ This message was deleted" };
        return vec![(said.into(), dim)];
    }
    let mut out = vec![];
    if m.kind != Kind::Text {
        let s = summary(m);
        let style = if m.kind == Kind::Unknown { dim } else { Style::new().fg(t.muted) };
        out.push((s, style));
        if !m.spans.is_empty() {
            out.push(("\n".into(), plain));
        }
    }
    for span in &m.spans {
        let style = match span.t {
            SpanKind::Bold => plain.add_modifier(Modifier::BOLD),
            SpanKind::Italic => plain.add_modifier(Modifier::ITALIC),
            SpanKind::Strike => plain.add_modifier(Modifier::CROSSED_OUT),
            SpanKind::Mono => Style::new().fg(t.text).bg(t.surface),
            SpanKind::Quote => Style::new().fg(t.muted).add_modifier(Modifier::ITALIC),
            SpanKind::Link => Style::new().fg(t.link).add_modifier(Modifier::UNDERLINED),
            SpanKind::Mention if span.person.as_ref().is_some_and(|p| ctx.me.contains(p)) => {
                Style::new().fg(t.mention).add_modifier(Modifier::BOLD)
            }
            SpanKind::Mention => plain.add_modifier(Modifier::BOLD),
            SpanKind::Text | SpanKind::List | SpanKind::Other => plain,
        };
        out.push((span.s.clone(), style));
    }
    out
}

/// A message's own lines: its quote, its text, edited, its reactions and whether it's sent.
pub fn message(m: &Message, shape: Shape, ctx: &Ctx) -> Vec<Line<'static>> {
    let t = ctx.theme;
    let w = shape.width;
    if matches!(m.kind, Kind::System | Kind::Call) {
        let text = m.system.clone().unwrap_or_else(|| fmt::one_line(&summary(m)));
        let text = fmt::fit(&format!("{text} · {}", fmt::clock(m.at, ctx.tz)), w);
        let colour = if m.kind == Kind::Call { t.muted } else { t.comment };
        return vec![Line::from(Span::styled(text, Style::new().fg(colour))).centered()];
    }
    let (left, right) = if m.mine { (indent(w), TIME + TICKS) } else { (NAME, TIME) };
    let text_w = w.saturating_sub(left + right).max(8);
    let name = m.from.as_ref().filter(|_| shape.name && !m.mine).map(|f| fmt::fit(&f.name, 20));
    let name_w = name.as_ref().map_or(0, |n| n.width() + 1);
    let first_w = if name_w > NAME { text_w.saturating_sub(name_w - NAME) } else { text_w };

    let mut segments = vec![];
    if let Some(q) = &m.reply {
        let bar = Style::new().fg(t.sender(q.colour));
        segments.push(("┃ ".to_string(), bar));
        let who = q.who();
        segments.push((format!("{who} "), bar.add_modifier(Modifier::BOLD)));
        let room = first_w.saturating_sub(who.width() + 3);
        segments.push((fmt::fit(&fmt::one_line(&q.summary), room), Style::new().fg(t.comment)));
        segments.push(("\n".into(), Style::new()));
    }
    segments.extend(body(m, ctx));
    if m.edited && !m.deleted {
        segments.push(("  edited".into(), Style::new().fg(t.comment)));
    }
    let mut wrapped = wrap(&segments, first_w, text_w);
    let folded = !shape.unfolded && wrapped.len() > FOLD;
    let more = wrapped.len().saturating_sub(FOLD);
    if folded {
        wrapped.truncate(FOLD);
    }

    let mut lines = Vec::with_capacity(wrapped.len() + 2);
    for (i, text) in wrapped.into_iter().enumerate() {
        let mut spans: Vec<Span<'static>> = vec![];
        let mut used = 0;
        if m.mine {
            spans.push(Span::raw(" ".repeat(left)));
            used += left;
        } else if i == 0 && name_w > 0 {
            let colour = m.from.as_ref().map_or(t.sender(0), |f| t.sender(f.colour));
            let n = name.clone().unwrap_or_default();
            let pad = NAME.max(name_w) - n.width();
            spans.push(Span::styled(n, Style::new().fg(colour).add_modifier(Modifier::BOLD)));
            spans.push(Span::raw(" ".repeat(pad)));
            used += NAME.max(name_w);
        } else {
            spans.push(Span::raw(" ".repeat(NAME)));
            used += NAME;
        }
        used += width_of(&text);
        spans.extend(text.into_iter().map(|(s, st)| Span::styled(s, st)));
        if i == 0 {
            let mut tail: Vec<Span<'static>> = vec![];
            if m.mine
                && let Some(tick) = m.status.filter(|_| !m.deleted)
            {
                let tick = ticks(tick, t);
                tail.push(Span::raw(" ".repeat(TICKS - tick.width())));
                tail.push(tick);
            } else if m.mine {
                tail.push(Span::raw(" ".repeat(TICKS)));
            }
            if shape.time {
                tail.push(Span::styled(format!("  {}", fmt::clock(m.at, ctx.tz)), Style::new().fg(t.comment)));
            }
            let tail_w: usize = tail.iter().map(|s| s.width()).sum();
            let tail_w = if m.mine && !shape.time { tail_w + TIME } else { tail_w };
            spans.push(Span::raw(" ".repeat(w.saturating_sub(used + tail_w))));
            spans.extend(tail);
        }
        lines.push(Line::from(spans));
    }
    let lead = || Span::raw(" ".repeat(left));
    if folded {
        let said = format!("⋯ {more} more lines · ⏎ shows them all");
        lines.push(Line::from(vec![lead(), Span::styled(said, Style::new().fg(t.comment))]));
    }
    if m.mine && m.status == Some(Tick::Failed) {
        lines.push(Line::from(vec![lead(), Span::styled("not sent", Style::new().fg(t.warn))]));
    }
    if !m.reactions.is_empty() && !m.deleted {
        let mut spans = vec![lead()];
        for (i, r) in m.reactions.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            let chip = format!(" {} {} ", r.emoji, r.n);
            let style = if r.mine {
                Style::new().fg(t.fg).bg(t.selection).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(t.text).bg(t.surface)
            };
            spans.push(Span::styled(chip, style));
        }
        lines.push(Line::from(spans));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{Quote, Reaction, Sender, Span as Run};

    const UTC: FixedOffset = FixedOffset::east_opt(0).unwrap();
    const NOW: i64 = 1_792_159_320_000;

    fn text(s: &str) -> Vec<Run> {
        vec![Run { t: SpanKind::Text, s: s.into(), person: None }]
    }

    fn theirs(s: &str) -> Message {
        Message {
            id: "m".into(),
            v: 1,
            chat: String::new(),
            sort: "1".into(),
            at: NOW - 4 * fmt::MINUTE,
            from: Some(Sender { id: "person:1".into(), name: "Ana".into(), colour: 0 }),
            mine: false,
            kind: Kind::Text,
            spans: text(s),
            reply: None,
            reactions: vec![],
            status: None,
            edited: false,
            deleted: false,
            summary: None,
            system: None,
        }
    }

    fn shown(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>().trim_end().to_string())
            .collect()
    }

    fn lay(m: &Message, width: usize) -> Vec<String> {
        let theme = Theme::with(true);
        let me = ["person:0".to_string()];
        let ctx = Ctx { theme: &theme, tz: UTC, me: &me };
        shown(&message(m, Shape { width, name: true, time: true, unfolded: false }, &ctx))
    }

    #[test]
    fn wrapping_breaks_at_spaces_and_newlines_and_inside_words_too_long_for_a_line() {
        let s = |t: &str| vec![(t.to_string(), Style::new())];
        let plain = |lines: Vec<Vec<(String, Style)>>| -> Vec<String> {
            lines.into_iter().map(|l| l.into_iter().map(|(s, _)| s).collect()).collect()
        };
        assert_eq!(plain(wrap(&s("did you get the photos"), 12, 12)), ["did you get", "the photos"]);
        assert_eq!(plain(wrap(&s("one\ntwo"), 12, 12)), ["one", "two"]);
        assert_eq!(plain(wrap(&s("https://casadapraia.pt"), 10, 10)), ["https://ca", "sadapraia.", "pt"]);
        assert_eq!(plain(wrap(&s("a b c"), 1, 3)), ["a", "b c"], "the first line can be narrower");
    }

    #[test]
    fn theirs_sit_under_their_name_with_the_time_at_the_edge() {
        let lines = lay(&theirs("did you get the photos from Sunday?"), 50);
        assert_eq!(lines, ["Ana   did you get the photos from Sunday?    13:58"]);
        let long = lay(&theirs("vens jantar sexta? a tua mãe pergunta se trazes o vinho e o pão"), 40);
        assert_eq!(
            long,
            ["Ana   vens jantar sexta? a tua     13:58", "      mãe pergunta se trazes o", "      vinho e o pão"]
        );
    }

    #[test]
    fn yours_start_a_third_of_the_way_across_with_their_ticks() {
        let mut m = theirs("yes, the one at the lake");
        m.mine = true;
        m.from = None;
        m.status = Some(Tick::Read);
        let lines = lay(&m, 60);
        assert_eq!(lines, [format!("{}yes, the one at the lake       ✓✓  13:58", " ".repeat(20))]);
        m.status = Some(Tick::Failed);
        let lines = lay(&m, 60);
        assert_eq!(lines[1].trim(), "not sent");
    }

    #[test]
    fn a_reply_quotes_its_message_on_a_line_and_reactions_follow() {
        let mut m = theirs("eu levo o vinho");
        m.reply = Some(Quote { from: Some("Mãe".into()), colour: 0, summary: "trazes o vinho?".into() });
        m.reactions = vec![Reaction { emoji: "❤".into(), n: 2, mine: true }];
        m.edited = true;
        let lines = lay(&m, 60);
        assert!(lines[0].starts_with("Ana   ┃ Mãe trazes o vinho?   ") && lines[0].ends_with("  13:58"), "{lines:?}");
        assert_eq!(fmt::width(&lines[0]), 60);
        assert_eq!(lines[1], "      eu levo o vinho  edited");
        assert_eq!(lines[2], "       ❤ 2");
    }

    #[test]
    fn other_kinds_are_their_one_line_summary_and_deleted_ones_say_so() {
        let mut m = theirs("a vista do quarto");
        m.kind = Kind::Image;
        m.summary = Some("▣ 2 photos".into());
        let lines = lay(&m, 50);
        assert!(lines[0].starts_with("Ana   ▣ 2 photos   ") && lines[0].ends_with("13:58"), "{lines:?}");
        assert_eq!(lines[1], "      a vista do quarto");
        m.kind = Kind::Unknown;
        m.summary = None;
        m.spans.clear();
        assert!(lay(&m, 50)[0].contains("jaw can't show this yet"));
        m.deleted = true;
        assert!(lay(&m, 50)[0].contains("⊘ This message was deleted"));
    }

    #[test]
    fn a_long_message_folds_after_forty_lines() {
        let m = theirs(&"line\n".repeat(60));
        let lines = lay(&m, 50);
        assert_eq!(lines.len(), FOLD + 1);
        assert!(lines[FOLD].contains("21 more lines"), "{}", lines[FOLD]);
    }
}
