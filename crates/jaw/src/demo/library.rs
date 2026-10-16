//! The made-up account's library, as jawd would keep it: every photo (an album's one by one),
//! video, voice message, file and link in its messages, newest first, filtered and windowed as
//! `library.watch` asks, with counts of each kind for the strip, and what arrives pushed to the
//! window watched.

use super::Account;
use crate::data::{
    FileInfo, Kind, LibraryAsk, LibraryKind, LibraryRow, LibraryWindow, LinkInfo, Mark, MarkKind, Message, SpanKind,
    Update,
};
use crate::find::fold;
use std::collections::HashMap;

/// Titles and descriptions of the links the made-up account's people share.
const PREVIEWS: &[(&str, &str, &str)] = &[
    ("casadapraia.pt", "Casa da Praia · Ericeira", "Moradia T3 a 200 m da praia, até 8 pessoas. Julho e agosto."),
    (
        "thecrag.com",
        "Climbing in Sintra: the complete guide",
        "Every crag around Sintra, with topos, grades and approaches.",
    ),
    ("cp.pt", "CP · Horários Lisboa–Porto", "Comboios Alfa Pendular e Intercidades, horários e preços."),
    ("idealista.pt", "Casas à venda em Sintra | idealista", "1.204 casas à venda em Sintra."),
    ("ipma.pt", "IPMA · Previsão para Sintra", "Céu pouco nublado, vento fraco de noroeste."),
    ("8a.nu", "Peneda: route database", "Sport routes in Peneda-Gerês, with grades and ascents."),
];

/// The host of a URL, without its scheme or `www.`.
fn host(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    host.trim_start_matches("www.").to_string()
}

/// A document's name, type, size and pages, from the line the made-up account sums it up in.
fn file_of(m: &Message) -> FileInfo {
    let summary = m.summary.clone().unwrap_or_default();
    let mut parts = summary.trim_start_matches("▣ ").split(" · ");
    let name = parts.next().unwrap_or("file").to_string();
    let mut info = FileInfo {
        path: super::pictures::file(&name),
        kind: name.rsplit_once('.').map(|(_, ext)| ext.to_uppercase()),
        name,
        ..FileInfo::default()
    };
    for part in parts {
        if let Some(n) = part.strip_suffix(" pages").and_then(|n| n.parse().ok()) {
            info.pages = Some(n);
        } else if let Some((n, unit)) = part.split_once(' ') {
            let scale = match unit {
                "KB" => 1e3,
                "MB" => 1e6,
                "GB" => 1e9,
                _ => continue,
            };
            info.bytes = n.parse::<f64>().ok().map(|n| (n * scale) as u64);
        }
    }
    info
}

/// The words of `text` that `query` asks for, marked, as `find` marks a message.
fn marked(text: &str, words: &[Vec<char>]) -> Vec<Mark> {
    if text.is_empty() {
        return vec![];
    }
    let folded = fold(text);
    let chars: Vec<char> = text.chars().collect();
    let mut marks = vec![false; chars.len()];
    for w in words.iter().filter(|w| !w.is_empty()) {
        let mut i = 0;
        while i + w.len() <= folded.len() {
            let starts = i == 0 || !folded[i - 1].is_alphanumeric();
            if starts && folded[i..i + w.len()] == w[..] {
                marks[i..i + w.len()].iter_mut().for_each(|m| *m = true);
            }
            i += 1;
        }
    }
    let mut out: Vec<Mark> = vec![];
    for (c, m) in chars.into_iter().zip(marks) {
        let t = if m { MarkKind::Mark } else { MarkKind::Text };
        match out.last_mut() {
            Some(last) if last.t == t => last.s.push(c),
            _ => out.push(Mark { t, s: c.to_string() }),
        }
    }
    out
}

impl Account {
    /// The items a message gives the library.
    fn items_of(&self, chat_name: &str, m: &Message) -> Vec<LibraryRow> {
        if m.deleted {
            return vec![];
        }
        let caption: String = m.spans.iter().map(|s| s.s.as_str()).collect();
        let row = |id: String, kind: LibraryKind| LibraryRow {
            sort: format!("{:013}~{id}", m.at),
            id,
            kind,
            message: m.id.clone(),
            chat: m.chat.clone(),
            chat_name: chat_name.to_string(),
            from: m.from.clone().filter(|_| !m.mine),
            mine: m.mine,
            at: m.at,
            media: None,
            voice: None,
            file: None,
            link: None,
            caption: vec![Mark { t: MarkKind::Text, s: caption.clone() }]
                .into_iter()
                .filter(|c| !c.s.is_empty())
                .collect(),
        };
        let mut out = vec![];
        match m.kind {
            Kind::Image if m.media.is_some() => {
                out.push(LibraryRow { media: m.media.clone(), ..row(m.id.clone(), LibraryKind::Photo) })
            }
            Kind::Video | Kind::Gif | Kind::VideoNote => {
                out.push(LibraryRow { media: m.media.clone(), ..row(m.id.clone(), LibraryKind::Video) })
            }
            Kind::Voice => out.push(LibraryRow { voice: m.voice.clone(), ..row(m.id.clone(), LibraryKind::Voice) }),
            Kind::Document => out.push(LibraryRow { file: Some(file_of(m)), ..row(m.id.clone(), LibraryKind::File) }),
            Kind::Album => {
                for (i, item) in m.album.iter().flat_map(|a| a.items.iter()).enumerate() {
                    let kind = if item.kind == Kind::Video { LibraryKind::Video } else { LibraryKind::Photo };
                    out.push(LibraryRow {
                        media: Some(Box::new(item.media.clone())),
                        ..row(format!("{}:{i}", m.id), kind)
                    });
                }
            }
            _ => {}
        }
        for (i, s) in m.spans.iter().filter(|s| s.t == SpanKind::Link).enumerate() {
            let url = s.url.clone().unwrap_or_else(|| s.s.clone());
            let host = host(&url);
            let preview = PREVIEWS.iter().find(|p| p.0 == host);
            let link = LinkInfo {
                title: preview.map(|p| p.1.to_string()),
                description: preview.map(|p| p.2.to_string()),
                url,
                host,
            };
            out.push(LibraryRow { link: Some(link), ..row(format!("{}~{i}", m.id), LibraryKind::Link) });
        }
        out
    }

    /// Every item in the made-up account, newest first.
    fn items(&self) -> Vec<LibraryRow> {
        let mut out: Vec<LibraryRow> =
            self.threads.iter().flat_map(|t| t.messages.iter().flat_map(|m| self.items_of(&t.chat.name, m))).collect();
        out.sort_by(|a, b| b.sort.cmp(&a.sort));
        out
    }

    /// Whether an item passes the ask's filters but its kind, and with its caption marked.
    fn passes(ask: &LibraryAsk, words: &[Vec<char>], item: &mut LibraryRow) -> bool {
        if ask.chat.as_ref().is_some_and(|c| *c != item.chat)
            || ask
                .person
                .as_ref()
                .is_some_and(|p| if p == "me" { !item.mine } else { item.from.as_ref().is_none_or(|f| f.id != *p) })
            || ask.since.is_some_and(|s| item.at < s)
            || ask.until.is_some_and(|u| item.at > u)
        {
            return false;
        }
        if words.is_empty() {
            return true;
        }
        let caption: String = item.caption.iter().map(|m| m.s.as_str()).collect();
        let mut text = caption.clone();
        if let Some(f) = &item.file {
            text.push(' ');
            text.push_str(&f.name);
        }
        if let Some(l) = &item.link {
            for part in [l.title.as_deref().unwrap_or(""), &l.host, l.description.as_deref().unwrap_or("")] {
                text.push(' ');
                text.push_str(part);
            }
        }
        let has = |w: &[char]| marked(&text, &[w.to_vec()]).iter().any(|m| m.t == MarkKind::Mark);
        if !words.iter().all(|w| has(w)) {
            return false;
        }
        item.caption = marked(&caption, words);
        true
    }

    fn words(text: &str) -> Vec<Vec<char>> {
        text.split_whitespace().map(fold).collect()
    }

    pub(super) fn on_watch_library(&mut self, ask: LibraryAsk) {
        let words = Self::words(&ask.text);
        let mut counts: HashMap<LibraryKind, u32> = LibraryKind::ALL.iter().map(|k| (*k, 0)).collect();
        let mut matching = vec![];
        for mut item in self.items() {
            if Self::passes(&ask, &words, &mut item) {
                *counts.entry(item.kind).or_default() += 1;
                matching.push(item);
            }
        }
        let (rows, total, older) = match ask.kind {
            None => {
                let mut taken: HashMap<LibraryKind, u32> = HashMap::new();
                let rows: Vec<LibraryRow> = matching
                    .into_iter()
                    .filter(|i| {
                        let n = taken.entry(i.kind).or_default();
                        *n += 1;
                        *n <= ask.after
                    })
                    .collect();
                (rows, counts.values().sum(), false)
            }
            Some(kind) => {
                let all: Vec<LibraryRow> = matching.into_iter().filter(|i| i.kind == kind).collect();
                let at = if ask.around == "top" { 0 } else { all.iter().position(|i| i.id == ask.around).unwrap_or(0) };
                let from = at.saturating_sub(ask.before as usize);
                let to = (at + ask.after as usize + usize::from(ask.around != "top")).min(all.len());
                let older = to < all.len();
                (all[from..to].to_vec(), all.len() as u32, older)
            }
        };
        let asked = ask.id;
        self.library = Some((ask, counts.clone()));
        let window = LibraryWindow {
            upsert: rows,
            whole: true,
            total: Some(total),
            older: Some(older),
            counts: Some(counts),
            ..LibraryWindow::default()
        };
        self.send(Update::Library { asked, window });
    }

    pub(super) fn on_unwatch_library(&mut self) {
        self.library = None;
    }

    /// A message arrived: its items into the library watched, if they pass its filters.
    pub(super) fn library_arrived(&mut self, chat_name: &str, m: &Message) {
        if self.library.is_none() {
            return;
        }
        let mut items = self.items_of(chat_name, m);
        let Some((ask, counts)) = self.library.as_mut() else { return };
        let words = Self::words(&ask.text);
        items.retain_mut(|i| Self::passes(ask, &words, i));
        if items.is_empty() {
            return;
        }
        for i in &items {
            *counts.entry(i.kind).or_default() += 1;
        }
        items.retain(|i| ask.kind.is_none_or(|k| k == i.kind));
        let (asked, counts) = (ask.id, counts.clone());
        let total = ask.kind.map(|k| counts.get(&k).copied().unwrap_or(0));
        let window = LibraryWindow { upsert: items, total, counts: Some(counts), ..LibraryWindow::default() };
        self.send(Update::Library { asked, window });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_documents_line_gives_its_name_type_size_and_pages() {
        let mut m: Message = serde_json::from_str(
            r#"{"id":"m","v":1,"sort":"1","chat":"c","at":0,"mine":true,"kind":"document","spans":[]}"#,
        )
        .unwrap();
        m.summary = Some("▣ orcamento-obras-cozinha.pdf · 2.4 MB · 3 pages".into());
        let f = file_of(&m);
        assert_eq!(
            (f.name.as_str(), f.kind.as_deref(), f.bytes, f.pages),
            ("orcamento-obras-cozinha.pdf", Some("PDF"), Some(2_400_000), Some(3))
        );
    }

    #[test]
    fn words_asked_for_are_marked_where_they_start_a_word_accents_aside() {
        let marks = marked("já na praia, Praiá", &[fold("praia")]);
        let shown: Vec<(bool, &str)> = marks.iter().map(|m| (m.t == MarkKind::Mark, m.s.as_str())).collect();
        assert_eq!(shown, [(false, "já na "), (true, "praia"), (false, ", "), (true, "Praiá")]);
        assert!(marked("apraia", &[fold("praia")]).iter().all(|m| m.t == MarkKind::Text));
    }

    #[test]
    fn a_links_host_drops_its_scheme_path_and_www() {
        assert_eq!(host("https://www.casadapraia.pt/ericeira?x=1"), "casadapraia.pt");
    }
}
