//! Contact cards as the protocol carries them: vCards. A contact row's `vcard` holds its card, or
//! its cards one after another, as several contacts export to one file, and each of
//! `send.contact`'s cards is one. `read` gives each card's name and numbers, as an interface shows
//! them, and `write` makes a card for someone from their name and numbers. A card is what someone
//! else's phone wrote, so only what's shown is read, and only so much of it.

/// The most cards read from one vCard, and numbers from one card.
const CARDS_MOST: usize = 256;
const NUMBERS_MOST: usize = 16;
/// The longest name and number kept, in characters.
const NAME_MOST: usize = 256;
const NUMBER_MOST: usize = 64;

/// A card as an interface shows it: the name it gives, and its numbers in the order they come.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Card {
    pub name: Option<String>,
    pub phones: Vec<Phone>,
}

/// One of a card's numbers as the card writes it, and, when the card says it's on WhatsApp (the
/// phones mark it `waid`), the number of that account in international form: `+` and its digits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Phone {
    pub number: String,
    pub whatsapp: Option<String>,
}

impl Phone {
    /// The number to start a chat with: its WhatsApp account's, or the one written.
    pub fn dialled(&self) -> &str {
        self.whatsapp.as_deref().unwrap_or(&self.number)
    }
}

/// The cards `text` holds, each from BEGIN:VCARD to END:VCARD, in order. What's outside them is
/// left out, and a card the text ends inside still counts.
pub fn read(text: &str) -> Vec<Card> {
    let mut cards = Vec::new();
    let mut open: Option<Reading> = None;
    for line in lines(text) {
        let Some(property) = Property::of(&line) else { continue };
        if property.marks("BEGIN") {
            if let Some(card) = open.take() {
                cards.push(card.card());
            }
            if cards.len() >= CARDS_MOST {
                return cards;
            }
            open = Some(Reading::default());
        } else if property.marks("END") {
            if let Some(card) = open.take() {
                cards.push(card.card());
            }
        } else if let Some(card) = open.as_mut() {
            card.take(&property);
        }
    }
    if let Some(card) = open.filter(|_| cards.len() < CARDS_MOST) {
        cards.push(card.card());
    }
    cards
}

/// Whether `text` is one card and nothing else: BEGIN:VCARD first, END:VCARD last, and no other
/// card's beginning or end between them, whitespace around it apart.
pub fn is_one(text: &str) -> bool {
    let lines: Vec<String> = lines(text).into_iter().filter(|line| !line.trim().is_empty()).collect();
    let marks = |line: &String, what: &str| Property::of(line).is_some_and(|p| p.marks(what));
    let (begins, ends) =
        (lines.iter().filter(|l| marks(l, "BEGIN")).count(), lines.iter().filter(|l| marks(l, "END")).count());
    lines.first().is_some_and(|l| marks(l, "BEGIN"))
        && lines.last().is_some_and(|l| marks(l, "END"))
        && begins == 1
        && ends == 1
}

/// A vCard 3.0 for `name`, whose numbers are `numbers`, each in international form with or without
/// its `+`, as WhatsApp's phones write a card for someone on it: each number a mobile one, marked
/// with the WhatsApp account it is. Its lines end in CRLF, as vCard 3.0's do.
pub fn write(name: &str, numbers: &[&str]) -> String {
    let name = escaped(name.trim());
    let mut card = format!("BEGIN:VCARD\r\nVERSION:3.0\r\nN:;{name};;;\r\nFN:{name}\r\n");
    for number in numbers {
        let digits: String = number.chars().filter(char::is_ascii_digit).collect();
        if !digits.is_empty() {
            card.push_str(&format!("TEL;TYPE=CELL;waid={digits}:+{digits}\r\n"));
        }
    }
    card.push_str("END:VCARD\r\n");
    card
}

/// A card being read: its formatted name, its structured one, and its numbers so far.
#[derive(Default)]
struct Reading {
    formatted: Option<String>,
    structured: Option<String>,
    phones: Vec<Phone>,
}

impl Reading {
    fn take(&mut self, property: &Property) {
        match property.name.as_str() {
            "FN" => self.formatted = said(&unescaped(&property.text())),
            "N" => self.structured = structured(&property.text()),
            "TEL" if self.phones.len() < NUMBERS_MOST => {
                let written = unescaped(&property.text());
                let written = written.trim();
                let written =
                    written.get(..4).filter(|s| s.eq_ignore_ascii_case("tel:")).map_or(written, |_| &written[4..]);
                let number: String = written.trim().chars().take(NUMBER_MOST).collect();
                if number.is_empty() {
                    return;
                }
                let whatsapp = property
                    .param("WAID")
                    .filter(|digits| (6..=20).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit()))
                    .map(|digits| format!("+{digits}"));
                self.phones.push(Phone { number, whatsapp });
            }
            _ => {}
        }
    }

    /// The card read: named by its formatted name, or failing that its structured one.
    fn card(self) -> Card {
        let name = self.formatted.or(self.structured).map(|name| name.chars().take(NAME_MOST).collect());
        Card { name, phones: self.phones }
    }
}

/// A line's property: its name in capitals without its group (`item1.TEL` is `TEL`), its
/// parameters, each a name in capitals and a value (a bare one, as vCard 2.1 writes `CELL`, has
/// no name), and its value as written.
struct Property {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

impl Property {
    fn of(line: &str) -> Option<Property> {
        let mut quoted = false;
        let (colon, _) = line.char_indices().find(|&(_, c)| {
            quoted ^= c == '"';
            c == ':' && !quoted
        })?;
        let (head, value) = (&line[..colon], &line[colon + 1..]);
        let mut parts = head.split(';');
        let name = parts.next()?.rsplit('.').next()?.trim().trim_start_matches('\u{feff}').to_ascii_uppercase();
        let params = parts
            .map(|param| match param.split_once('=') {
                Some((key, value)) => (key.trim().to_ascii_uppercase(), value.trim().trim_matches('"').to_string()),
                None => (String::new(), param.trim().to_string()),
            })
            .collect();
        Some(Property { name, params, value: value.to_string() })
    }

    /// Whether it begins or ends a card, as `what` says.
    fn marks(&self, what: &str) -> bool {
        self.name == what && self.value.trim().eq_ignore_ascii_case("VCARD")
    }

    fn param(&self, key: &str) -> Option<&str> {
        self.params.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str())
    }

    fn quoted_printable(&self) -> bool {
        self.params.iter().any(|(name, value)| {
            (name == "ENCODING" || name.is_empty()) && value.eq_ignore_ascii_case("QUOTED-PRINTABLE")
        })
    }

    /// Its value with any quoted-printable encoding undone, vCard's escapes still in it.
    fn text(&self) -> String {
        if self.quoted_printable() { quoted_printable(&self.value) } else { self.value.clone() }
    }
}

/// `text`'s lines, each folded one joined again: a line beginning with a space or a tab goes on
/// from the one before, and so does the line after one of quoted-printable text ending in `=`.
fn lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut soft = false;
    for line in text.trim_start_matches('\u{feff}').split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        match lines.last_mut() {
            Some(last) if soft => {
                last.pop();
                last.push_str(line);
            }
            Some(last) if line.starts_with([' ', '\t']) => last.push_str(&line[1..]),
            _ => lines.push(line.to_string()),
        }
        soft = lines
            .last()
            .is_some_and(|last| last.ends_with('=') && Property::of(last).is_some_and(|p| p.quoted_printable()));
    }
    lines
}

/// Quoted-printable text decoded: each `=` and two hex digits a byte, read as UTF-8.
fn quoted_printable(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
        match (bytes[at], bytes.get(at + 1).copied().and_then(hex), bytes.get(at + 2).copied().and_then(hex)) {
            (b'=', Some(high), Some(low)) => {
                out.push(high << 4 | low);
                at += 3;
            }
            (byte, _, _) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A value with vCard's escapes undone: `\n` a new line, and a backslash before anything else that
/// thing itself.
fn unescaped(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('n' | 'N') => out.push('\n'),
                Some(other) => out.push(other),
                None => {}
            },
            c => out.push(c),
        }
    }
    out
}

/// A value with what vCard 3.0 escapes escaped.
fn escaped(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' | ',' | ';' => {
                out.push('\\');
                out.push(c);
            }
            '\n' => out.push_str("\\n"),
            '\r' => {}
            c => out.push(c),
        }
    }
    out
}

/// Text that says something, trimmed.
fn said(text: &str) -> Option<String> {
    Some(text.trim()).filter(|text| !text.is_empty()).map(str::to_string)
}

/// A structured name (family, given, additional, prefixes and suffixes, split at the semicolons
/// that aren't escaped) as a person reads it: prefixes, given, additional, family, suffixes.
fn structured(value: &str) -> Option<String> {
    let mut parts: Vec<String> = vec![String::new()];
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let part = parts.last_mut().expect("there's always a part");
                part.push('\\');
                part.extend(chars.next());
            }
            ';' => parts.push(String::new()),
            c => parts.last_mut().expect("there's always a part").push(c),
        }
    }
    let part = |i: usize| parts.get(i).map(|part| unescaped(part).trim().to_string()).unwrap_or_default();
    let name: Vec<String> =
        [part(3), part(1), part(2), part(0), part(4)].into_iter().filter(|p| !p.is_empty()).collect();
    (!name.is_empty()).then(|| name.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phone(number: &str, whatsapp: Option<&str>) -> Phone {
        Phone { number: number.into(), whatsapp: whatsapp.map(str::to_string) }
    }

    fn one(text: &str) -> Card {
        let mut cards = read(text);
        assert_eq!(cards.len(), 1, "{text:?} is one card: {cards:?}");
        cards.remove(0)
    }

    #[test]
    fn a_phones_card_gives_its_name_and_each_number_with_the_whatsapp_account_it_is() {
        let vcard = "BEGIN:VCARD\r\nVERSION:3.0\r\nN:Lopes;Marta;;Dr.ª;\r\nFN:Dr.ª Marta Lopes\r\n\
            item1.TEL;waid=351213456789:+351 21 345 6789\r\nitem1.X-ABLabel:Work\r\n\
            TEL;type=HOME:21 000 00 00\r\nEND:VCARD";
        let card = one(vcard);
        assert_eq!(card.name.as_deref(), Some("Dr.ª Marta Lopes"));
        assert_eq!(card.phones, [phone("+351 21 345 6789", Some("+351213456789")), phone("21 000 00 00", None)]);
        assert_eq!((card.phones[0].dialled(), card.phones[1].dialled()), ("+351213456789", "21 000 00 00"));
    }

    #[test]
    fn a_card_without_a_formatted_name_is_named_by_its_parts_escaped_semicolons_and_all() {
        let vcard = "BEGIN:VCARD\nN:Costa\\;Silva;Inês;Maria;;\nTEL;CELL;WAID=351914444444:+351 914 444 444\nEND:VCARD";
        let card = one(vcard);
        assert_eq!(card.name.as_deref(), Some("Inês Maria Costa;Silva"));
        assert_eq!(card.phones, [phone("+351 914 444 444", Some("+351914444444"))]);
        assert_eq!(one("BEGIN:VCARD\nFN:  \nN:;;;;\nEND:VCARD").name, None, "a name of nothing names nothing");
    }

    #[test]
    fn folded_lines_escapes_quoted_params_and_vcard_2_1s_quoted_printable_read_as_they_mean() {
        let vcard = "BEGIN:VCARD\nFN:Ana\\, a Ribeiro\\nde Lisboa\nTEL;TYPE=\"cell:voice\";waid=351912\n 345678:+351 912\n\t345 678\nEND:VCARD";
        let card = one(vcard);
        assert_eq!(card.name.as_deref(), Some("Ana, a Ribeiro\nde Lisboa"));
        assert_eq!(card.phones, [phone("+351 912345 678", Some("+351912345678"))]);
        let old = "BEGIN:VCARD\r\nVERSION:2.1\r\nFN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:=C3=89lia S=C3=A1=\r\nnchez\r\n\
            TEL;CELL;QUOTED-PRINTABLE:+34=20600=20000=20000\r\nTEL;VALUE=uri:tel:+351-21-000-0000\r\nEND:VCARD\r\n";
        let card = one(old);
        assert_eq!(card.name.as_deref(), Some("Élia Sánchez"));
        assert_eq!(card.phones, [phone("+34 600 000 000", None), phone("+351-21-000-0000", None)]);
    }

    #[test]
    fn several_cards_in_one_text_read_one_after_another_and_what_isnt_a_card_reads_as_none() {
        let both = "\u{feff}BEGIN:VCARD\r\nFN:Marta\r\nEND:VCARD\r\nBEGIN:VCARD\r\nFN:Rui\r\nTEL:+351913333333\r\nEND:VCARD\r\n";
        let names: Vec<_> = read(both).into_iter().map(|card| card.name).collect();
        assert_eq!(names, [Some("Marta".to_string()), Some("Rui".to_string())]);
        assert_eq!(read("FN:outside\nBEGIN:VCARD\nFN:Inês").len(), 1, "an open card at the end still counts");
        assert_eq!(read("not a card at all"), []);
        assert_eq!(read(""), []);
    }

    #[test]
    fn a_card_reads_so_many_numbers_of_so_much_and_no_waid_that_isnt_an_accounts_digits() {
        let mut vcard = "BEGIN:VCARD\nFN:Many\n".to_string();
        for n in 0..40 {
            vcard.push_str(&format!("TEL;waid=35191000{n:04}:+35191000{n:04}\n"));
        }
        vcard.push_str("END:VCARD");
        assert_eq!(one(&vcard).phones.len(), NUMBERS_MOST);
        let long = format!(
            "BEGIN:VCARD\nFN:{}\nTEL;waid=12x456789:{}\nTEL;waid=123:+123\nEND:VCARD",
            "n".repeat(1_000),
            "9".repeat(500)
        );
        let card = one(&long);
        assert_eq!(card.name.map(|n| n.chars().count()), Some(NAME_MOST));
        assert_eq!(card.phones, [phone(&"9".repeat(NUMBER_MOST), None), phone("+123", None)]);
        let lots = "BEGIN:VCARD\nFN:x\nEND:VCARD\n".repeat(CARDS_MOST + 10);
        assert_eq!(read(&lots).len(), CARDS_MOST);
    }

    #[test]
    fn one_card_is_one_and_two_or_a_card_with_more_after_it_or_none_arent() {
        assert!(is_one("  BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Ana\r\nEND:VCARD\r\n\r\n"));
        assert!(is_one("begin:vcard\nfn:ana\nend:vcard"), "in any case");
        assert!(!is_one("BEGIN:VCARD\nFN:Ana\nEND:VCARD\nBEGIN:VCARD\nFN:Rui\nEND:VCARD"), "two cards");
        assert!(!is_one("BEGIN:VCARD\nFN:Ana\nEND:VCARD\nFN:Rui"), "something after it");
        assert!(!is_one("BEGIN:VCARD\nFN:Ana"), "never ended");
        assert!(!is_one("FN:Ana\nEND:VCARD"), "never begun");
        assert!(!is_one("Ana Ribeiro, +351 912 345 678"));
        assert!(!is_one(""));
    }

    #[test]
    fn a_card_jaw_writes_is_one_card_of_its_name_and_numbers_each_on_whatsapp() {
        let written = write(" Ana; Ribeiro, de\\Lisboa ", &["+351912345678", "351 913 333 333", "no digits"]);
        assert!(
            written.starts_with("BEGIN:VCARD\r\nVERSION:3.0\r\n") && written.ends_with("END:VCARD\r\n"),
            "{written}"
        );
        assert!(
            written.lines().all(|line| !line.is_empty()) && !written.replace("\r\n", "").contains('\n'),
            "{written:?}"
        );
        assert!(is_one(&written));
        let card = one(&written);
        assert_eq!(card.name.as_deref(), Some("Ana; Ribeiro, de\\Lisboa"));
        assert_eq!(
            card.phones,
            [phone("+351912345678", Some("+351912345678")), phone("+351913333333", Some("+351913333333"))]
        );
    }
}
