//! Matching as jawd's `find` matches: names fuzzily, by initials, word starts and parts of words,
//! accent-blind on either side, with letters scattered inside words not matching; and messages by
//! their words, a word of three letters or more matching the words it starts and a shorter one
//! only itself. The palette matches its own actions this way, and the made-up account answers
//! `find` with it.

use crate::data::{Mark, MarkKind};

/// A letter folded for matching: lower case, without its accent.
pub fn fold_char(c: char) -> char {
    let c = c.to_lowercase().next().unwrap_or(c);
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ī' => 'i',
        'ñ' | 'ń' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' => 'u',
        'ý' | 'ÿ' => 'y',
        'š' | 'ś' => 's',
        'ž' | 'ź' | 'ż' => 'z',
        'ł' => 'l',
        _ => c,
    }
}

/// A text folded a character to a character, so positions in it are positions in the original.
pub fn fold(s: &str) -> Vec<char> {
    s.chars().map(fold_char).collect()
}

fn starts_word(name: &[char], i: usize) -> bool {
    i == 0 || !name[i - 1].is_alphanumeric()
}

/// How well `query` matches `name`, and which of its characters matched, as ranges; `None` if it
/// doesn't. Higher is better: the whole start of the name, then a word's start, then initials and
/// runs at word starts, then a part of a word.
pub fn name(query: &str, name: &str) -> Option<(i64, Vec<(usize, usize)>)> {
    let q: Vec<char> = fold(query).into_iter().filter(|c| !c.is_whitespace()).collect();
    if q.is_empty() {
        return None;
    }
    let n = fold(name);
    let spaced = fold(query.trim());
    if n.starts_with(&spaced) {
        return Some((100, vec![(0, spaced.len())]));
    }
    for i in (1..n.len()).filter(|&i| starts_word(&n, i)) {
        if n[i..].starts_with(&spaced) {
            return Some((80 - i.min(20) as i64, vec![(i, i + spaced.len())]));
        }
    }
    if let Some(runs) = runs(&q, &n, 0) {
        let longest = runs.iter().map(|(a, b)| b - a).max().unwrap_or(0) as i64;
        return Some((60 + longest.min(10) - runs.len() as i64, runs));
    }
    if q.len() >= 2 {
        for part in [&spaced, &q] {
            for i in 0..n.len().saturating_sub(part.len().saturating_sub(1)) {
                if n[i..].starts_with(part) {
                    return Some((40 - i.min(20) as i64, vec![(i, i + part.len())]));
                }
            }
        }
    }
    None
}

/// The query as runs of letters, each beginning at a word's start, in order: "ar" in Ana Ribeiro.
fn runs(q: &[char], n: &[char], from: usize) -> Option<Vec<(usize, usize)>> {
    if q.is_empty() {
        return Some(vec![]);
    }
    for i in (from..n.len()).filter(|&i| starts_word(n, i) && n[i] == q[0]) {
        let mut len = 1;
        while len < q.len() && i + len < n.len() && n[i + len] == q[len] {
            len += 1;
        }
        // The longest run first, then shorter ones, so "anri" finds Ana Ribeiro.
        for take in (1..=len).rev() {
            if let Some(mut rest) = runs(&q[take..], n, i + take) {
                rest.insert(0, (i, i + take));
                return Some(rest);
            }
        }
    }
    None
}

/// The words of a query that messages are searched with, folded; none if the query has fewer
/// than two letters, since one letter finds names and actions only.
pub fn words(query: &str) -> Vec<Vec<char>> {
    let letters = query.chars().filter(|c| c.is_alphanumeric()).count();
    if letters < 2 {
        return vec![];
    }
    query.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(fold).collect()
}

/// Where each of `words` matches in `text`, as ranges, or `None` if one of them doesn't.
pub fn message(words: &[Vec<char>], text: &[char]) -> Option<Vec<(usize, usize)>> {
    let mut found = vec![];
    for w in words {
        let mut at = None;
        let mut i = 0;
        while i < text.len() {
            if starts_word(text, i) && text[i].is_alphanumeric() {
                let end = (i..text.len()).find(|&j| !text[j].is_alphanumeric()).unwrap_or(text.len());
                let word = &text[i..end];
                let hit = if w.len() >= 3 { word.starts_with(w) } else { word == w.as_slice() };
                if hit {
                    at = Some((i, i + w.len()));
                    break;
                }
                i = end;
            } else {
                i += 1;
            }
        }
        found.push(at?);
    }
    found.sort();
    Some(found)
}

/// `text` as runs of text and marks over `ranges` of its characters.
pub fn marked(text: &str, ranges: &[(usize, usize)]) -> Vec<Mark> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = vec![];
    let mut at = 0;
    for &(a, b) in ranges {
        if a > at {
            out.push(Mark { t: MarkKind::Text, s: chars[at..a].iter().collect() });
        }
        out.push(Mark { t: MarkKind::Mark, s: chars[a..b.min(chars.len())].iter().collect() });
        at = b;
    }
    if at < chars.len() {
        out.push(Mark { t: MarkKind::Text, s: chars[at..].iter().collect() });
    }
    out
}

/// A stretch of `text` around its first match, at most `most` characters on one line, with an
/// ellipsis where text is left out, and the ranges moved to it.
pub fn stretch(text: &str, ranges: &[(usize, usize)], most: usize) -> (String, Vec<(usize, usize)>) {
    let chars: Vec<char> = text.chars().map(|c| if c == '\n' { ' ' } else { c }).collect();
    if chars.len() <= most {
        return (chars.into_iter().collect(), ranges.to_vec());
    }
    let first = ranges.first().map_or(0, |r| r.0);
    let start = first.saturating_sub(most / 4);
    let end = (start + most).min(chars.len());
    let lead = usize::from(start > 0);
    let mut s: String = if start > 0 { "…".into() } else { String::new() };
    s.extend(&chars[start..end]);
    if end < chars.len() {
        s.push('…');
    }
    let moved = ranges
        .iter()
        .filter(|(a, b)| *a >= start && *b <= end)
        .map(|(a, b)| (a - start + lead, b - start + lead))
        .collect();
    (s, moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marks(q: &str, n: &str) -> Option<Vec<String>> {
        name(q, n).map(|(_, r)| {
            let c: Vec<char> = n.chars().collect();
            r.iter().map(|(a, b)| c[*a..*b].iter().collect()).collect()
        })
    }

    #[test]
    fn names_match_by_initials_word_starts_and_parts_of_words_accent_blind() {
        assert_eq!(marks("ar", "Ana Ribeiro"), Some(vec!["A".into(), "R".into()]), "initials");
        assert_eq!(marks("rib", "Ana Ribeiro"), Some(vec!["Rib".into()]), "a word's start");
        assert_eq!(marks("ines", "Inês Gomes"), Some(vec!["Inês".into()]), "accents ignored in the name");
        assert_eq!(marks("Inês", "Ines"), Some(vec!["Ines".into()]), "and in the query");
        assert_eq!(marks("ar", "Start a chat"), Some(vec!["ar".into()]), "a part of a word");
        assert_eq!(marks("pin this", "Unpin this chat"), Some(vec!["pin this".into()]), "and on from it");
        assert_eq!(marks("anri", "Ana Ribeiro"), Some(vec!["An".into(), "Ri".into()]));
        assert_eq!(marks("ao", "Ana Ribeiro"), None, "letters scattered inside words don't match");
        let (prefix, _) = name("ana", "Ana Ribeiro").unwrap();
        let (initials, _) = name("ar", "Ana Ribeiro").unwrap();
        let (part, _) = name("ib", "Ana Ribeiro").unwrap();
        assert!(prefix > initials && initials > part);
    }

    #[test]
    fn message_words_from_three_letters_match_what_they_start_and_shorter_ones_only_themselves() {
        let text = fold("Vens jantar já? Levo o vinho");
        assert!(message(&words("jan"), &text).is_some());
        assert!(message(&words("ja"), &text).is_some(), "já, accent-blind");
        assert!(message(&words("ve"), &text).is_none(), "two letters match only a word of two");
        assert!(message(&words("jantar vinho"), &text).is_some());
        assert!(message(&words("jantar praia"), &text).is_none(), "every word typed");
        assert!(words("j").is_empty(), "one letter finds names and actions only");
    }

    #[test]
    fn a_long_message_is_a_stretch_around_its_first_match() {
        let text = format!("{} jantar {}", "a ".repeat(100), "b ".repeat(100));
        let w = words("jantar");
        let r = message(&w, &fold(&text)).unwrap();
        let (s, moved) = stretch(&text, &r, 160);
        assert!(s.starts_with('…') && s.ends_with('…') && s.chars().count() <= 162);
        let chars: Vec<char> = s.chars().collect();
        assert_eq!(chars[moved[0].0..moved[0].1].iter().collect::<String>(), "jantar");
        let m = marked("vens jantar", &[(5, 11)]);
        assert_eq!(m[1], Mark { t: MarkKind::Mark, s: "jantar".into() });
    }
}
