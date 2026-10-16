//! WhatsApp's formatting, parsed into the protocol's spans: `*bold*`, `_italic_`, `~strike~`,
//! ```` ```mono``` ```` and `` `mono` ``, lines starting `> ` as quotes and `* `, `- ` or `1. ` as
//! list items, links, and mentions, which WhatsApp writes as `@` and the user part of a JID the
//! message lists as mentioned. Spans are flat: formatting inside formatting stays as written.

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Span {
    pub t: &'static str,
    pub s: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub person: Option<String>,
}

impl Span {
    fn new(t: &'static str, s: impl Into<String>) -> Span {
        Span { t, s: s.into(), url: None, person: None }
    }
}

/// `text` as spans, with `mentioned` the JIDs the message says it mentions.
pub fn parse(text: &str, mentioned: &[String]) -> Vec<Span> {
    let mut out = Spans::default();
    let mut rest = text;
    let mut at_line_start = true;
    while !rest.is_empty() {
        match mono_block(rest) {
            Some((start, inner, end)) => {
                lines(&rest[..start], mentioned, &mut at_line_start, &mut out);
                out.push(Span::new("mono", inner));
                rest = &rest[end..];
                at_line_start = false;
            }
            None => {
                lines(rest, mentioned, &mut at_line_start, &mut out);
                break;
            }
        }
    }
    out.0
}

/// Where the first ```` ``` ```` block in `text` starts, what it holds, and where it ends.
fn mono_block(text: &str) -> Option<(usize, &str, usize)> {
    let mut from = 0;
    while let Some(found) = text[from..].find("```") {
        let start = from + found;
        let inner_start = start + 3;
        if opens(text[..start].chars().next_back())
            && let Some(len) = text[inner_start..].find("```").filter(|&len| len > 0)
        {
            return Some((start, &text[inner_start..inner_start + len], inner_start + len + 3));
        }
        from = inner_start;
    }
    None
}

/// Text without mono blocks: quotes and list items where a line starts, inline formatting elsewhere.
fn lines(mut rest: &str, mentioned: &[String], at_line_start: &mut bool, out: &mut Spans) {
    while !rest.is_empty() {
        let line_end = rest.find('\n').unwrap_or(rest.len());
        let line = &rest[..line_end];
        if *at_line_start {
            if let Some(quoted) = line.strip_prefix("> ") {
                out.push(Span::new("quote", quoted));
                rest = &rest[line_end..];
                *at_line_start = false;
                continue;
            }
            if is_list_item(line) {
                out.push(Span::new("list", line));
                rest = &rest[line_end..];
                *at_line_start = false;
                continue;
            }
        }
        let (line, after) = rest.split_at((line_end + 1).min(rest.len()));
        inline(line, mentioned, out);
        rest = after;
        *at_line_start = line.ends_with('\n');
    }
}

/// The spans as the store keeps them: JSON, or `None` when they're one plain span or none.
pub fn json(spans: &[Span]) -> Option<String> {
    match spans {
        [] => None,
        [only] if only.t == "text" => None,
        _ => Some(serde_json::to_string(spans).expect("spans serialize")),
    }
}

/// The text the spans show, on one line, at most `most` characters, for a summary.
pub fn line(spans: &[Span], most: usize) -> String {
    let mut text = String::new();
    for span in spans {
        if !text.is_empty() && matches!(span.t, "quote" | "list") && !text.ends_with(' ') {
            text.push(' ');
        }
        text.push_str(&span.s);
    }
    one_line(&text, most)
}

/// `text` with its runs of whitespace as single spaces, cut at `most` characters with an
/// ellipsis.
pub fn one_line(text: &str, most: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let joined = words.join(" ");
    if joined.chars().count() <= most {
        return joined;
    }
    let mut cut: String = joined.chars().take(most.saturating_sub(1)).collect();
    if let Some(space) = cut.rfind(' ').filter(|&space| space > 0) {
        cut.truncate(space);
    }
    cut.push('…');
    cut
}

fn is_list_item(line: &str) -> bool {
    if line.starts_with("* ") || line.starts_with("- ") {
        return true;
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && digits <= 3 && line[digits..].starts_with(". ")
}

/// Spans being built, plain text runs merged.
#[derive(Default)]
struct Spans(Vec<Span>);

impl Spans {
    fn push(&mut self, span: Span) {
        if span.s.is_empty() {
            return;
        }
        if span.t == "text"
            && let Some(last) = self.0.last_mut()
            && last.t == "text"
        {
            last.s.push_str(&span.s);
            return;
        }
        self.0.push(span);
    }

    fn text(&mut self, s: &str) {
        self.push(Span::new("text", s));
    }
}

const MARKERS: [(char, &str); 4] = [('*', "bold"), ('_', "italic"), ('~', "strike"), ('`', "mono")];

/// One line's inline formatting, links and mentions.
fn inline(line: &str, mentioned: &[String], out: &mut Spans) {
    let mut plain = 0;
    let mut i = 0;
    while i < line.len() {
        let c = line[i..].chars().next().expect("a char at a boundary");
        let before = line[..i].chars().next_back();
        if let Some(&(marker, kind)) = MARKERS.iter().find(|(m, _)| *m == c)
            && opens(before)
            && let Some(len) = closes(&line[i + 1..], marker)
        {
            out.text(&line[plain..i]);
            out.push(Span::new(kind, &line[i + 1..i + 1 + len]));
            i += len + 2;
            plain = i;
            continue;
        }
        if opens(before)
            && let Some(len) = link(&line[i..])
        {
            out.text(&line[plain..i]);
            let shown = &line[i..i + len];
            let url = if shown.starts_with("www.") { format!("https://{shown}") } else { shown.to_string() };
            out.push(Span { url: Some(url), ..Span::new("link", shown) });
            i += len;
            plain = i;
            continue;
        }
        if c == '@'
            && opens(before)
            && let Some((len, person)) = mention(&line[i + 1..], mentioned)
        {
            out.text(&line[plain..i]);
            out.push(Span { person: Some(person), ..Span::new("mention", &line[i..i + 1 + len]) });
            i += len + 1;
            plain = i;
            continue;
        }
        i += c.len_utf8();
    }
    out.text(&line[plain..]);
}

/// Whether formatting can start after `before`: at the start, or after a space or punctuation.
fn opens(before: Option<char>) -> bool {
    before.is_none_or(|c| c.is_whitespace() || (c.is_ascii_punctuation() && !MARKERS.iter().any(|(m, _)| *m == c)))
}

/// The length of what `marker` encloses at the start of `after`, if it closes on this line with
/// something inside that neither starts nor ends with a space, and nothing alphanumeric after it.
fn closes(after: &str, marker: char) -> Option<usize> {
    let end = after.find(marker)?;
    let inner = &after[..end];
    if inner.is_empty() || inner.starts_with(char::is_whitespace) || inner.ends_with(char::is_whitespace) {
        return None;
    }
    if inner.contains('\n') {
        return None;
    }
    let next = after[end + marker.len_utf8()..].chars().next();
    next.is_none_or(|c| !c.is_alphanumeric()).then_some(end)
}

/// The length of a link at the start of `text`, without punctuation that ends a sentence after it.
fn link(text: &str) -> Option<usize> {
    let starts = |prefix: &str| text.get(..prefix.len()).is_some_and(|start| start.eq_ignore_ascii_case(prefix));
    if !(starts("https://") || starts("http://") || starts("www.")) {
        return None;
    }
    let mut len = text.find(char::is_whitespace).unwrap_or(text.len());
    let mut url = &text[..len];
    loop {
        let trimmed = url.trim_end_matches(['.', ',', '!', '?', ':', ';', '\'', '"', '*', '_', '~']);
        let trimmed = if trimmed.ends_with(')') && trimmed.matches('(').count() < trimmed.matches(')').count() {
            &trimmed[..trimmed.len() - 1]
        } else {
            trimmed
        };
        if trimmed.len() == url.len() {
            break;
        }
        url = trimmed;
    }
    len = url.len();
    let host = url.split_once("://").map_or(url, |(_, rest)| rest);
    (host.len() > 4 && host.contains('.')).then_some(len)
}

/// The length of a mention's digits at the start of `text`, and the JID they name, when the
/// message lists that JID as mentioned.
fn mention(text: &str, mentioned: &[String]) -> Option<(usize, String)> {
    let len = text.bytes().take_while(u8::is_ascii_digit).count();
    if len == 0 {
        return None;
    }
    let user = &text[..len];
    let person = mentioned.iter().find(|jid| jid.split(['@', ':']).next() == Some(user))?;
    Some((len, person.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(spans: &[Span]) -> Vec<(&str, &str)> {
        spans.iter().map(|span| (span.t, span.s.as_str())).collect()
    }

    #[test]
    fn plain_text_is_one_span_and_stored_as_none() {
        let spans = parse("did you get the photos from Sunday?", &[]);
        assert_eq!(kinds(&spans), [("text", "did you get the photos from Sunday?")]);
        assert_eq!(json(&spans), None);
    }

    #[test]
    fn bold_italic_strike_and_mono_are_marked_where_whatsapp_marks_them() {
        let spans = parse("did you get the *photos* from _Sunday_, ~Saturday~? `ok` ```let x = 1;```", &[]);
        assert_eq!(
            kinds(&spans),
            [
                ("text", "did you get the "),
                ("bold", "photos"),
                ("text", " from "),
                ("italic", "Sunday"),
                ("text", ", "),
                ("strike", "Saturday"),
                ("text", "? "),
                ("mono", "ok"),
                ("text", " "),
                ("mono", "let x = 1;"),
            ]
        );
    }

    #[test]
    fn markers_inside_words_or_with_spaces_inside_stay_as_written() {
        for text in ["2*3*4", "snake_case_name", "a * b * c", "*open", "a * not bold*", "x_y_"] {
            let spans = parse(text, &[]);
            assert_eq!(kinds(&spans), [("text", text)], "{text}");
        }
    }

    #[test]
    fn mono_blocks_can_span_lines_and_bold_cannot() {
        let spans = parse("```fn main() {\n}``` and *not\nbold*", &[]);
        assert_eq!(kinds(&spans), [("mono", "fn main() {\n}"), ("text", " and *not\nbold*")]);
    }

    #[test]
    fn quotes_and_list_items_are_lines_of_their_own() {
        let spans = parse("> she said yes\nso:\n- bread\n* milk\n12. eggs\nend", &[]);
        assert_eq!(
            kinds(&spans),
            [
                ("quote", "she said yes"),
                ("text", "\nso:\n"),
                ("list", "- bread"),
                ("text", "\n"),
                ("list", "* milk"),
                ("text", "\n"),
                ("list", "12. eggs"),
                ("text", "\nend"),
            ]
        );
    }

    #[test]
    fn links_are_marked_without_the_punctuation_after_them() {
        let spans = parse("see https://example.com/a_b?x=1. or (www.jaw.dev/x) and http://x", &[]);
        let links: Vec<(&str, Option<&str>)> =
            spans.iter().filter(|s| s.t == "link").map(|s| (s.s.as_str(), s.url.as_deref())).collect();
        assert_eq!(
            links,
            [
                ("https://example.com/a_b?x=1", Some("https://example.com/a_b?x=1")),
                ("www.jaw.dev/x", Some("https://www.jaw.dev/x"))
            ]
        );
        assert_eq!(spans.last().unwrap().s, ") and http://x");
    }

    #[test]
    fn mentions_are_marked_only_for_jids_the_message_lists() {
        let mentioned = vec!["351910000002@s.whatsapp.net".to_string(), "100000000001@lid".to_string()];
        let spans = parse("@351910000002 and @100000000001, not @123 or a@351910000002", &mentioned);
        let marked: Vec<(&str, Option<&str>)> =
            spans.iter().filter(|s| s.t == "mention").map(|s| (s.s.as_str(), s.person.as_deref())).collect();
        assert_eq!(
            marked,
            [("@351910000002", Some("351910000002@s.whatsapp.net")), ("@100000000001", Some("100000000001@lid"))]
        );
        assert!(spans.iter().any(|s| s.t == "text" && s.s.contains("@123 or a@351910000002")));
    }

    #[test]
    fn spans_serialize_as_the_protocol_has_them() {
        let spans = parse("*hi* https://jaw.dev", &[]);
        assert_eq!(
            json(&spans).unwrap(),
            r#"[{"t":"bold","s":"hi"},{"t":"text","s":" "},{"t":"link","s":"https://jaw.dev","url":"https://jaw.dev"}]"#
        );
    }

    #[test]
    fn a_summary_is_one_line_cut_with_an_ellipsis() {
        let spans = parse("> quoted\n*first*   line\nsecond", &[]);
        assert_eq!(line(&spans, 100), "quoted first line second");
        assert_eq!(one_line("não sei bem o que dizer", 10), "não sei…");
    }
}
