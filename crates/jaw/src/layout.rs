//! A message as lines at a width: others' under their name on the left, and yours in a column from
//! a third of the way across or, in the IRC layout, under "You" as others' are under their names,
//! each with its time at the right edge when the minute or the sender changes. Laying one out is
//! the costly part of a frame, so the view keeps what this returns per message, version, width and
//! layout, and lays out only what's on screen.

use crate::data::{Kind, Message, SpanKind, Tick, Voice};
use crate::fmt;
use crate::theme::Theme;
use chrono::FixedOffset;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// The column names take, others' and "You" in the IRC layout, and where the text under them
/// starts.
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
    /// Whether its pictures are drawn: not once none of their previews could be read.
    pub pictured: bool,
    /// For the voice message playing or paused: where.
    pub play: Option<Play>,
    /// Yours on the left under "You", as everyone's are, as IRC clients put them, rather than in
    /// a column from a third of the way across.
    pub irc: bool,
}

/// Where a voice message's playhead is, in steps its line can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Play {
    /// Tenths of a second in.
    pub at: u32,
    pub paused: bool,
    /// Tenths: 10, 15 or 20.
    pub speed: u8,
}

/// A waveform's heights, the lowest a bar of silence.
const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
/// The most cells a voice message's waveform takes: two of its 64 points to a cell.
pub const WAVE: usize = 32;

/// One level, 0 to 100, as a bar.
pub fn bar(level: u8) -> char {
    BARS[(level.min(100) as usize * 7 + 50) / 100]
}

/// A waveform in `cells` cells, each the loudest of the points it covers.
pub fn waveform(points: &[u8], cells: usize) -> String {
    if points.is_empty() {
        return BARS[0].to_string().repeat(cells);
    }
    let n = points.len();
    (0..cells)
        .map(|i| {
            let from = (i * n / cells).min(n - 1);
            let to = ((i + 1) * n / cells).clamp(from + 1, n);
            bar(points[from..to].iter().copied().max().unwrap_or(0))
        })
        .collect()
}

/// A voice message's line: ▶ or ⏸, its waveform with the part played bright, its length and
/// where it's got, the speed when it isn't 1×, and a dot until you've played it.
fn voice(m: &Message, v: &Voice, play: Option<Play>, room: usize, ctx: &Ctx) -> Vec<(String, Style)> {
    let t = ctx.theme;
    let playing = play.is_some_and(|p| !p.paused);
    let mut tail = match play {
        Some(p) => format!("  {} / {}", fmt::length(p.at as f64 / 10.0), fmt::length(v.seconds)),
        None => format!("  {}", fmt::length(v.seconds)),
    };
    if let Some(p) = play.filter(|p| p.speed != 10) {
        tail.push_str(&format!(" · {}×", fmt::speed(p.speed as f64 / 10.0)));
    }
    let dot = !m.mine && !v.played;
    let cells = room.saturating_sub(2 + tail.width() + if dot { 2 } else { 0 }).clamp(4, WAVE);
    let bars = waveform(&v.waveform, cells);
    let mut out = vec![(if playing { "⏸ " } else { "▶ " }.to_string(), Style::new().fg(t.focus))];
    match play {
        Some(p) => {
            let done = p.at as f64 / 10.0 / v.seconds.max(0.1);
            let played = ((done * cells as f64).round() as usize).min(cells);
            let split = bars.char_indices().nth(played).map_or(bars.len(), |(i, _)| i);
            out.push((bars[..split].to_string(), Style::new().fg(t.progress)));
            out.push((bars[split..].to_string(), Style::new().fg(t.comment)));
        }
        None => out.push((bars, Style::new().fg(t.muted))),
    }
    out.push((tail, Style::new().fg(t.muted)));
    if dot {
        out.push((" ●".into(), Style::new().fg(t.unread)));
    }
    out.retain(|(s, _)| !s.is_empty());
    out
}

pub struct Ctx<'a> {
    pub theme: &'a Theme,
    pub tz: FixedOffset,
    /// The account's own person, so a mention of you stands out.
    pub me: &'a [String],
    /// Whether pictures are drawn, and a cell's size in pixels for their shape.
    pub pictures: bool,
    pub cell: (u16, u16),
    /// Whether the terminal draws a picture's own pixels, which want the larger preview, rather
    /// than half-blocks, which the smaller has plenty for.
    pub sharp: bool,
}

/// The most cells a photo, a GIF or a video's poster fills, and a sticker.
const PHOTO: (u16, u16) = (40, 16);
const STICKER: (u16, u16) = (18, 8);
/// Album pictures shown, two a row; the rest are counted.
const ALBUM_SHOWN: usize = 4;

/// A picture in a message's lines: the line it starts on, the column, the cells it fills, and
/// its previews' paths under jawd's folder, the best first.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub line: usize,
    pub col: u16,
    pub cols: u16,
    pub rows: u16,
    pub sources: Vec<String>,
}

/// A message's lines and the pictures drawn over them.
#[derive(Clone, Debug, Default)]
pub struct Laidout {
    pub lines: Vec<Line<'static>>,
    pub pictures: Vec<Picture>,
}

/// Where a picture is drawn from: the preview its cells need, the other, then the thumbnail.
fn sources(media: &crate::data::Media, cols: u16, ctx: &Ctx) -> Vec<String> {
    let big = ctx.sharp && cols as u32 * ctx.cell.0 as u32 > 256;
    let (first, second) = if big {
        (&media.previews.large, &media.previews.small)
    } else {
        (&media.previews.small, &media.previews.large)
    };
    [first, second, &media.thumbnail].into_iter().flatten().cloned().collect()
}

/// The pictures a message shows, each sized and placed in rows of their own from `col`, at most
/// `room` columns wide, and how many rows they take.
fn plan(m: &Message, col: usize, room: usize, ctx: &Ctx) -> (Vec<Picture>, usize) {
    let room = room.min(u16::MAX as usize) as u16;
    match (&m.kind, &m.media, &m.album) {
        (Kind::Album, _, Some(album)) => {
            let shown: Vec<_> =
                album.items.iter().filter(|i| !sources(&i.media, 1, ctx).is_empty()).take(ALBUM_SHOWN).collect();
            if shown.is_empty() {
                return (vec![], 0);
            }
            let tile = (room.saturating_sub(1) / 2).clamp(4, PHOTO.0 / 2);
            let (_, tile_rows) = crate::graphics::fit(1, 1, tile, PHOTO.1 / 2, ctx.cell);
            let mut pictures = vec![];
            for (i, item) in shown.iter().enumerate() {
                let (r, c) = (i / 2, i % 2);
                pictures.push(Picture {
                    line: r * (tile_rows as usize + 1),
                    col: (col + c * (tile as usize + 1)) as u16,
                    cols: tile,
                    rows: tile_rows,
                    sources: sources(&item.media, tile, ctx),
                });
            }
            let bands = shown.len().div_ceil(2);
            (pictures, bands * (tile_rows as usize + 1) - 1)
        }
        (Kind::Image | Kind::Sticker | Kind::Video | Kind::Gif | Kind::VideoNote, Some(media), _) => {
            let most = if m.kind == Kind::Sticker { STICKER } else { PHOTO };
            let (cols, rows) = crate::graphics::fit(media.w, media.h, most.0.min(room), most.1, ctx.cell);
            let sources = sources(media, cols, ctx);
            if sources.is_empty() {
                return (vec![], 0);
            }
            (vec![Picture { line: 0, col: col as u16, cols, rows, sources }], rows as usize)
        }
        _ => (vec![], 0),
    }
}

/// Where your own messages start outside the IRC layout: a third of the way across.
fn indent(width: usize) -> usize {
    width / 3
}

/// What labels your messages in the IRC layout.
pub const YOU: &str = "You";

/// Where a message's text starts and what its right edge keeps: others' at the name column, and
/// yours there too in the IRC layout or a third of the way across otherwise, with room for their
/// ticks.
pub fn margins(mine: bool, irc: bool, width: usize) -> (usize, usize) {
    match (mine, irc) {
        (true, false) => (indent(width), TIME + TICKS),
        (true, true) => (NAME, TIME + TICKS),
        (false, _) => (NAME, TIME),
    }
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
        Kind::Album => "▣ album",
        Kind::Invite => "✉ invitation",
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

/// The text of a message as styled segments, from jawd's spans; a voice message's line is drawn
/// to fit `room`. A deleted one of yours says you deleted it, but under "You" in the IRC layout it
/// reads as theirs do.
fn body(m: &Message, shape: Shape, room: usize, ctx: &Ctx) -> Vec<(String, Style)> {
    let t = ctx.theme;
    let plain = Style::new().fg(t.fg);
    let dim = Style::new().fg(t.comment).add_modifier(Modifier::ITALIC);
    if m.deleted {
        let said = if m.mine && !shape.irc { "⊘ You deleted this message" } else { "⊘ This message was deleted" };
        return vec![(said.into(), dim)];
    }
    let mut out = vec![];
    if m.kind == Kind::Voice
        && let Some(v) = &m.voice
    {
        out.extend(voice(m, v, shape.play, room, ctx));
        if !m.spans.is_empty() {
            out.push(("\n".into(), plain));
        }
    } else if m.kind != Kind::Text {
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

/// A message's own lines, its quote, its text, edited, its reactions and whether it's sent, and,
/// for a photo, a sticker, a video or an album, the pictures over them.
pub fn laid(m: &Message, shape: Shape, ctx: &Ctx) -> Laidout {
    let mut out = Laidout { lines: text(m, shape, ctx), pictures: vec![] };
    if !ctx.pictures || !shape.pictured || m.deleted {
        return out;
    }
    let w = shape.width;
    let (left, right) = margins(m.mine, shape.irc, w);
    let (mut pictures, rows) = plan(m, left, w.saturating_sub(left + right).max(8), ctx);
    if pictures.is_empty() {
        return out;
    }
    // Under the line that says what it is, and the quote's line if it's a reply.
    let at = (usize::from(m.reply.is_some()) + 1).min(out.lines.len());
    for p in &mut pictures {
        p.line += at;
    }
    let blank = || Line::from(Span::raw(" ".repeat(left)));
    out.lines.splice(at..at, std::iter::repeat_with(blank).take(rows));
    out.pictures = pictures;
    out
}

/// A message's text as lines, where its pictures go in under the first.
fn text(m: &Message, shape: Shape, ctx: &Ctx) -> Vec<Line<'static>> {
    let t = ctx.theme;
    let w = shape.width;
    if matches!(m.kind, Kind::System | Kind::Call) {
        let text = m.system.clone().unwrap_or_else(|| fmt::one_line(&summary(m)));
        let text = fmt::fit(&format!("{text} · {}", fmt::clock(m.at, ctx.tz)), w);
        let colour = if m.kind == Kind::Call { t.muted } else { t.comment };
        return vec![Line::from(Span::styled(text, Style::new().fg(colour))).centered()];
    }
    let (left, right) = margins(m.mine, shape.irc, w);
    let text_w = w.saturating_sub(left + right).max(8);
    let name = if m.mine {
        (shape.irc && shape.name).then(|| YOU.to_string())
    } else {
        m.from.as_ref().filter(|_| shape.name).map(|f| fmt::fit(&f.name, 20))
    };
    let name_w = name.as_ref().map_or(0, |n| n.width() + 1);
    let first_w = if name_w > NAME { text_w.saturating_sub(name_w - NAME) } else { text_w };

    let mut segments = vec![];
    if m.forwarded > 0 && !m.deleted {
        let said = if m.forwarded >= 5 { "↪ forwarded many times" } else { "↪ forwarded" };
        segments.push((said.to_string(), Style::new().fg(t.comment).add_modifier(Modifier::ITALIC)));
        segments.push(("\n".into(), Style::new()));
    }
    if let Some(q) = &m.reply {
        let bar = Style::new().fg(t.sender(q.colour));
        segments.push(("┃ ".to_string(), bar));
        let who = q.who();
        segments.push((format!("{who} "), bar.add_modifier(Modifier::BOLD)));
        let room = first_w.saturating_sub(who.width() + 3);
        segments.push((fmt::fit(&fmt::one_line(&q.summary), room), Style::new().fg(t.comment)));
        segments.push(("\n".into(), Style::new()));
    }
    let room = if m.reply.is_some() { text_w } else { first_w };
    segments.extend(body(m, shape, room, ctx));
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
        if i == 0 && name_w > 0 {
            let colour = if m.mine { t.fg } else { m.from.as_ref().map_or(t.sender(0), |f| t.sender(f.colour)) };
            let n = name.clone().unwrap_or_default();
            let pad = NAME.max(name_w) - n.width();
            spans.push(Span::styled(n, Style::new().fg(colour).add_modifier(Modifier::BOLD)));
            spans.push(Span::raw(" ".repeat(pad)));
            used += NAME.max(name_w);
        } else {
            spans.push(Span::raw(" ".repeat(left)));
            used += left;
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
        lines.push(Line::from(vec![
            lead(),
            Span::styled("! not sent", Style::new().fg(t.warn)),
            Span::styled(" · ⏎ tries again", Style::new().fg(t.comment)),
        ]));
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
    use crate::data::{Media, Previews, Quote, Reaction, Sender, Span as Run};

    const UTC: FixedOffset = FixedOffset::east_opt(0).unwrap();
    const NOW: i64 = 1_792_159_320_000;

    fn text(s: &str) -> Vec<Run> {
        vec![Run { t: SpanKind::Text, s: s.into(), person: None, url: None }]
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
            forwarded: 0,
            reactions: vec![],
            status: None,
            edited: false,
            deleted: false,
            summary: None,
            system: None,
            media: None,
            album: None,
            voice: None,
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
        let ctx = Ctx { theme: &theme, tz: UTC, me: &me, pictures: false, cell: (8, 16), sharp: false };
        let shape = Shape { width, name: true, time: true, unfolded: false, pictured: true, play: None, irc: false };
        shown(&laid(m, shape, &ctx).lines)
    }

    fn lay_irc(m: &Message, width: usize, name: bool) -> Laidout {
        let theme = Theme::with(true);
        let me = ["person:0".to_string()];
        let ctx = Ctx { theme: &theme, tz: UTC, me: &me, pictures: true, cell: (8, 16), sharp: false };
        laid(m, Shape { width, name, time: true, unfolded: false, pictured: true, play: None, irc: true }, &ctx)
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
        assert_eq!(lines[1].trim(), "! not sent · ⏎ tries again");
    }

    #[test]
    fn in_the_irc_layout_yours_sit_under_you_at_the_name_column_with_their_ticks_at_the_edge() {
        let mut m = theirs("yes, the one at the lake");
        m.mine = true;
        m.from = None;
        m.status = Some(Tick::Read);
        let first = lay_irc(&m, 60, true);
        assert_eq!(shown(&first.lines), [format!("You   yes, the one at the lake{}✓✓  13:58", " ".repeat(21))]);
        let you = &first.lines[0].spans[0];
        assert_eq!(you.content, "You");
        assert_eq!(you.style, Style::new().fg(Theme::with(true).fg).add_modifier(Modifier::BOLD));
        let next = shown(&lay_irc(&m, 60, false).lines);
        assert!(next[0].starts_with("      yes, the one at the lake") && next[0].contains("✓✓"), "{next:?}");
        let mut long = m.clone();
        long.spans = text("vens jantar sexta? a tua mãe pergunta se trazes o vinho e o pão");
        assert_eq!(
            shown(&lay_irc(&long, 40, true).lines),
            ["You   vens jantar sexta? a tua ✓✓  13:58", "      mãe pergunta se trazes o", "      vinho e o pão"]
        );
        m.reply = Some(Quote { id: None, from: Some("Ana".into()), colour: 0, summary: "trazes o vinho?".into() });
        m.reactions = vec![Reaction { emoji: "❤".into(), n: 2, mine: false }];
        let lines = shown(&lay_irc(&m, 60, true).lines);
        assert!(lines[0].starts_with("You   ┃ Ana trazes o vinho?"), "{lines:?}");
        assert!(lines.last().unwrap().starts_with("       ❤ 2"), "reactions start at the name column: {lines:?}");
        assert_eq!(margins(true, true, 60), (NAME, 10));
        assert_eq!(margins(true, false, 60), (20, 10));
        assert_eq!(margins(false, true, 60), (NAME, 7));
    }

    #[test]
    fn in_the_irc_layout_yours_more_lines_not_sent_and_pictures_start_at_the_name_column_too() {
        let mut m = theirs(&"line\n".repeat(60));
        m.mine = true;
        m.from = None;
        m.status = Some(Tick::Failed);
        let lines = shown(&lay_irc(&m, 60, true).lines);
        assert!(lines[FOLD].starts_with("      ⋯ 21 more lines"), "{lines:?}");
        assert_eq!(lines[FOLD + 1], "      ! not sent · ⏎ tries again");
        m.kind = Kind::Image;
        m.spans = text("a vista do topo");
        m.media = Some(Box::new(Media {
            w: 800,
            h: 600,
            previews: Previews { small: Some("topo-256.jpg".into()), large: None },
            ..Media::default()
        }));
        let photo = lay_irc(&m, 60, true);
        let at: Vec<(usize, u16, u16)> = photo.pictures.iter().map(|p| (p.line, p.col, p.cols)).collect();
        assert_eq!(at, [(1, NAME as u16, 40)], "under the line that says what it is, at the name column");
    }

    #[test]
    fn a_reply_quotes_its_message_on_a_line_and_reactions_follow() {
        let mut m = theirs("eu levo o vinho");
        m.reply = Some(Quote { id: None, from: Some("Mãe".into()), colour: 0, summary: "trazes o vinho?".into() });
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
        m.kind = Kind::Invite;
        m.summary = Some("Invitation to follow Jaw news".into());
        m.spans.clear();
        assert!(lay(&m, 50)[0].contains("Invitation to follow Jaw news"), "an invitation is one jaw shows");
        assert_eq!(serde_json::from_str::<Kind>(r#""invite""#).unwrap(), Kind::Invite);
        m.kind = Kind::Unknown;
        m.summary = None;
        assert!(lay(&m, 50)[0].contains("jaw can't show this yet"));
        m.deleted = true;
        assert!(lay(&m, 50)[0].contains("⊘ This message was deleted"));
    }

    #[test]
    fn a_deleted_message_of_yours_reads_as_theirs_do_under_you_in_the_irc_layout() {
        let mut m = theirs("vou já");
        m.mine = true;
        m.from = None;
        m.deleted = true;
        let bubbles = lay(&m, 60);
        assert!(bubbles[0].trim_start().starts_with("⊘ You deleted this message"), "{bubbles:?}");
        let irc = shown(&lay_irc(&m, 60, true).lines);
        assert!(irc[0].starts_with("You   ⊘ This message was deleted"), "{irc:?}");
    }

    #[test]
    fn a_long_message_folds_after_forty_lines() {
        let m = theirs(&"line\n".repeat(60));
        let lines = lay(&m, 50);
        assert_eq!(lines.len(), FOLD + 1);
        assert!(lines[FOLD].contains("21 more lines"), "{}", lines[FOLD]);
    }

    #[test]
    fn a_waveform_takes_the_loudest_of_the_points_each_cell_covers() {
        let points: Vec<u8> = (0..64).map(|i| if i % 2 == 0 { 0 } else { 100 }).collect();
        assert_eq!(waveform(&points, 32), "█".repeat(32));
        assert_eq!(waveform(&points[..2], 4), "▁▁██");
        assert_eq!(waveform(&[], 3), "▁▁▁");
        assert_eq!(waveform(&[0, 50, 100], 3), "▁▅█");
    }

    #[test]
    fn a_voice_messages_played_part_is_bright_and_the_rest_quiet() {
        let theme = Theme::with(true);
        let me: Vec<String> = vec![];
        let ctx = Ctx { theme: &theme, tz: UTC, me: &me, pictures: false, cell: (8, 16), sharp: false };
        let mut m = theirs("");
        m.spans = vec![];
        m.kind = Kind::Voice;
        m.voice = Some(Voice { seconds: 10.0, waveform: vec![50; 64], played: false });
        let play = Some(Play { at: 50, paused: false, speed: 15 });
        let segments = voice(&m, m.voice.as_ref().unwrap(), play, 60, &ctx);
        let text: String = segments.iter().map(|(s, _)| s.as_str()).collect();
        assert!(text.starts_with("⏸ ") && text.ends_with("  0:05 / 0:10 · 1.5× ●"), "{text}");
        let bars = |colour| -> usize {
            let shown = segments.iter().filter(|(_, st)| st.fg == Some(colour));
            shown.map(|(s, _)| s.chars().filter(|c| BARS.contains(c)).count()).sum()
        };
        let (progress, rest) = (bars(theme.progress), bars(theme.comment));
        assert_eq!((progress, rest), (16, 16));
    }
}
