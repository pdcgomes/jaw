//! Contact cards as the protocol carries them: vCards. A contact row's `vcard` holds its card, or
//! its cards one after another, as several contacts export to one file, and each of
//! `send.contact`'s cards is one. `read` gives each card's name and numbers, as an interface shows
//! them, and `write` makes a card for someone from their name and numbers. A card is what someone
//! else's phone wrote, so only what's shown is read, and only so much of it, in one pass whose cost
//! grows with the card's length and no faster, however its lines are folded.

use std::borrow::Cow;

/// The most cards read from one vCard, and numbers from one card.
const CARDS_MOST: usize = 256;
const NUMBERS_MOST: usize = 16;
/// The longest name and number kept, in characters.
const NAME_MOST: usize = 256;
const NUMBER_MOST: usize = 64;
/// How far into a line its colon is looked for, in bytes. The colon ends its property's name and
/// parameters, and a line without one that soon is no property, however long it goes on.
const HEAD_MOST: usize = 1024;

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
    cards(text, CARDS_MOST)
}

/// The first card `text` holds, as `read` gives it, read no further than its end.
pub fn first(text: &str) -> Option<Card> {
    cards(text, 1).pop()
}

/// The first `most` cards `text` holds, read no further than the last one's end.
fn cards(text: &str, most: usize) -> Vec<Card> {
    let mut cards = Vec::new();
    let mut open: Option<Reading> = None;
    for line in lines(text) {
        let Some(property) = Property::of(&line) else { continue };
        let begins = property.marks("BEGIN");
        if begins || property.marks("END") {
            cards.extend(open.take().map(Reading::card));
            if cards.len() >= most {
                return cards;
            }
            open = begins.then(Reading::default);
        } else if let Some(card) = open.as_mut() {
            card.take(&property);
        }
    }
    cards.extend(open.map(Reading::card));
    cards
}

/// Whether `text` is one card and nothing else: BEGIN:VCARD first, END:VCARD last, and no other
/// card's beginning or end between them, whitespace around it apart. It reads no further than the
/// line that says it isn't.
pub fn is_one(text: &str) -> bool {
    let mut lines = lines(text).filter(|line| !line.trim().is_empty());
    let marks = |line: &str, what: &str| Property::of(line).is_some_and(|p| p.marks(what));
    if !lines.next().is_some_and(|line| marks(&line, "BEGIN")) {
        return false;
    }
    while let Some(line) = lines.next() {
        if marks(&line, "END") {
            return lines.next().is_none();
        }
        if marks(&line, "BEGIN") {
            return false;
        }
    }
    false
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

/// A card being read: its formatted name and its structured one as written, the last of each it
/// gives, read once it ends; and its numbers so far.
#[derive(Default)]
struct Reading {
    formatted: Written,
    structured: Written,
    phones: Vec<Phone>,
}

/// A property's value as written, and whether it's quoted-printable, kept until the card ends in a
/// buffer the next such property's value takes over.
#[derive(Default)]
struct Written {
    value: Option<String>,
    quoted_printable: bool,
}

impl Written {
    /// Keeps `property`'s value in place of the last one's.
    fn keep(&mut self, property: &Property) {
        let value = self.value.get_or_insert_default();
        value.clear();
        value.push_str(property.value);
        self.quoted_printable = property.quoted_printable();
    }

    /// Its value with any quoted-printable encoding undone, vCard's escapes still in it.
    fn text(&self) -> Option<Cow<'_, str>> {
        let value = self.value.as_deref()?;
        Some(if self.quoted_printable { Cow::Owned(quoted_printable(value)) } else { Cow::Borrowed(value) })
    }
}

impl Reading {
    fn take(&mut self, property: &Property) {
        if property.is("FN") {
            self.formatted.keep(property);
        } else if property.is("N") {
            self.structured.keep(property);
        } else if property.is("TEL") && self.phones.len() < NUMBERS_MOST {
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
    }

    /// The card read: named by its formatted name, or failing that its structured one.
    fn card(self) -> Card {
        let formatted = self.formatted.text().and_then(|text| said(&unescaped(&text)));
        let name = formatted.or_else(|| structured(&self.structured.text()?));
        Card { name: name.map(|name| name.chars().take(NAME_MOST).collect()), phones: self.phones }
    }
}

/// A line's property, read from the line where it is: its name without its group (`item1.TEL` is
/// `TEL`), in any case; its head, the name and its parameters; and its value as written.
struct Property<'a> {
    name: &'a str,
    head: &'a str,
    value: &'a str,
}

impl<'a> Property<'a> {
    fn of(line: &'a str) -> Option<Property<'a>> {
        let mut head = Head::UNREAD;
        head.read(line).map(|colon| Property::ended(line, colon))
    }

    /// `line`'s property, its head ended by the colon at `colon`.
    fn ended(line: &'a str, colon: usize) -> Property<'a> {
        let head = &line[..colon];
        let name = head.split_once(';').map_or(head, |(name, _)| name);
        let name = name.rsplit_once('.').map_or(name, |(_, name)| name).trim().trim_start_matches('\u{feff}');
        Property { name, head, value: &line[colon + 1..] }
    }

    /// Whether it's a `name`, in any case.
    fn is(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
    }

    /// Whether it begins or ends a card, as `what` says.
    fn marks(&self, what: &str) -> bool {
        self.is(what) && self.value.trim().eq_ignore_ascii_case("VCARD")
    }

    /// Its parameters, each a name and a value; a bare one, as vCard 2.1 writes `CELL`, has no name.
    fn params(&self) -> impl Iterator<Item = (&'a str, &'a str)> {
        self.head.split(';').skip(1).map(|param| match param.split_once('=') {
            Some((key, value)) => (key.trim(), value.trim().trim_matches('"')),
            None => ("", param.trim()),
        })
    }

    fn param(&self, key: &str) -> Option<&'a str> {
        self.params().find(|(name, _)| name.eq_ignore_ascii_case(key)).map(|(_, value)| value)
    }

    fn quoted_printable(&self) -> bool {
        self.params().any(|(name, value)| {
            (name.is_empty() || name.eq_ignore_ascii_case("ENCODING")) && value.eq_ignore_ascii_case("QUOTED-PRINTABLE")
        })
    }

    /// Its value with any quoted-printable encoding undone, vCard's escapes still in it.
    fn text(&self) -> Cow<'a, str> {
        if self.quoted_printable() { Cow::Owned(quoted_printable(self.value)) } else { Cow::Borrowed(self.value) }
    }
}

/// How far a line's head, its property's name and parameters, has been read. It ends at the line's
/// first colon outside quotes, which is looked for in what the line has gained since the last look,
/// and only as far as HEAD_MOST bytes in.
#[derive(Clone, Copy)]
enum Head {
    /// Read as far as `at`, inside a quote or not.
    Reading { at: usize, quoted: bool },
    /// Ended by a colon.
    Ended,
    /// No colon came in its first HEAD_MOST bytes, so the line is no property.
    Endless,
}

impl Head {
    const UNREAD: Head = Head::Reading { at: 0, quoted: false };

    /// Reads on in `line`, which goes on from what was read before, and gives where its colon is if
    /// this read found it.
    fn read(&mut self, line: &str) -> Option<usize> {
        let Head::Reading { at, mut quoted } = *self else { return None };
        let end = line.len().min(HEAD_MOST);
        let from = at.min(end);
        let colon = line.as_bytes()[from..end].iter().position(|&byte| {
            quoted ^= byte == b'"';
            byte == b':' && !quoted
        });
        *self = match colon {
            Some(_) => Head::Ended,
            None if end == HEAD_MOST => Head::Endless,
            None => Head::Reading { at: end, quoted },
        };
        colon.map(|colon| from + colon)
    }
}

/// `text`'s lines, each folded one joined again: a line beginning with a space or a tab goes on
/// from the one before, and so does the line after one of quoted-printable text ending in `=`.
/// Each comes as it's asked for and costs what its length does, however many lines it was folded
/// into: its head is read once, and of each line it goes on with, only its own end is looked at.
fn lines(text: &str) -> impl Iterator<Item = Cow<'_, str>> {
    let mut physical =
        text.trim_start_matches('\u{feff}').split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line)).peekable();
    std::iter::from_fn(move || {
        let mut piece = physical.next()?;
        let mut line = Cow::Borrowed(piece);
        let (mut head, mut quoted_printable) = (Head::UNREAD, false);
        loop {
            if let Some(colon) = head.read(&line) {
                quoted_printable = Property::ended(&line, colon).quoted_printable();
            }
            let soft = quoted_printable && piece.ends_with('=');
            match physical.peek().copied() {
                Some(next) if soft => {
                    let joined = line.to_mut();
                    joined.pop();
                    joined.push_str(next);
                    piece = next;
                }
                Some(next) if next.starts_with([' ', '\t']) => {
                    piece = &next[1..];
                    line.to_mut().push_str(piece);
                }
                _ => return Some(line),
            }
            physical.next();
        }
    })
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

    #[test]
    fn a_line_whose_colon_comes_a_kib_in_or_later_is_no_property_and_only_its_own_end_breaks_it_softly() {
        let padded = |pad: usize| {
            let pad = "a".repeat(pad);
            format!("BEGIN:VCARD\nTEL;X-PAD={pad};waid=351912345678:+351 912 345 678\nFN:Ana\nEND:VCARD")
        };
        assert_eq!(one(&padded(HEAD_MOST - 100)).phones, [phone("+351 912 345 678", Some("+351912345678"))]);
        let card = one(&padded(HEAD_MOST));
        assert_eq!((card.name.as_deref(), card.phones), (Some("Ana"), vec![]), "the line skipped, the rest read");
        let open = open_quoted(64 * 1024).replace("END:VCARD", "FN:Ana\r\nEND:VCARD");
        assert_eq!(one(&open).name.as_deref(), Some("Ana"), "a quote left open ends with its line");
        let after = "BEGIN:VCARD\r\nNOTE;ENCODING=QUOTED-PRINTABLE:a==\r\n\r\nFN:Rui\r\nEND:VCARD\r\n";
        assert_eq!(one(after).name.as_deref(), Some("Rui"), "the = an empty line goes on from isn't its own");
    }

    /// The fastest of three runs of `run` on `card`.
    fn fastest(card: &str, run: impl Fn(&str)) -> std::time::Duration {
        let timed = || {
            let began = std::time::Instant::now();
            run(card);
            began.elapsed()
        };
        (0..3).map(|_| timed()).min().expect("three runs")
    }

    #[test]
    fn a_card_takes_as_long_as_its_length_however_its_lines_are_folded_and_the_first_no_longer_than_its_own() {
        // Each line read once, these take about 8 ms per 256 KiB in a debug build. A reader that
        // looks at the whole of a line again for each line folded into it took 179 ms and 1.8 s
        // broken softly and 45 s at 256 KiB left open (60 ms, 720 ms, 2.5 s and 42 s in release).
        for size in [256 * 1024, 1024 * 1024] {
            let most = std::time::Duration::from_millis(50) * (size / (256 * 1024)) as u32;
            for (shape, card) in [("broken softly", soft_broken(size)), ("left open", open_quoted(size))] {
                let reading = fastest(&card, |card| assert_eq!(read(card).len(), 1));
                let checking = fastest(&card, |card| assert!(is_one(card)));
                for (what, took) in [("read", reading), ("is_one", checking)] {
                    assert!(took < most, "{what} took {took:?} on a card {shape} of {} KiB", size / 1024);
                }
            }
        }
        let first_of_many = "BEGIN:VCARD\r\nFN:Ana\r\nEND:VCARD\r\n".to_string() + &open_quoted(4 * 1024 * 1024);
        let took = fastest(&first_of_many, |text| assert_eq!(first(text).and_then(|c| c.name).as_deref(), Some("Ana")));
        assert!(took < std::time::Duration::from_millis(5), "the first card of many read in {took:?}");
    }

    /// A nameless card of about `size` bytes whose note is quoted-printable broken softly after
    /// each character, its lines `x=`.
    fn soft_broken(size: usize) -> String {
        let mut card = "BEGIN:VCARD\r\nVERSION:2.1\r\nNOTE;ENCODING=QUOTED-PRINTABLE:".to_string();
        while card.len() < size {
            card.push_str("x=\r\n");
        }
        card + "x\r\nEND:VCARD\r\n"
    }

    /// A nameless card of about `size` bytes whose note leaves a parameter's quote open and goes on
    /// in folded lines, each ending in `=`.
    fn open_quoted(size: usize) -> String {
        let mut card = "BEGIN:VCARD\r\nVERSION:3.0\r\nNOTE;X-OPEN=\"".to_string();
        while card.len() < size {
            card.push_str("\r\n x=");
        }
        card + "\r\nEND:VCARD\r\n"
    }

    /// A card of about `size` bytes with a photo, its base64 folded at 75 columns.
    fn with_photo(size: usize) -> String {
        let mut card = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Marta Lopes\r\nPHOTO;ENCODING=b;TYPE=JPEG:".to_string();
        while card.len() < size {
            card.push_str(&"A".repeat(75));
            card.push_str("\r\n ");
        }
        card + "A==\r\nEND:VCARD\r\n"
    }

    /// About `size` bytes of `piece` again and again inside one card.
    fn repeated(piece: &str, size: usize) -> String {
        "BEGIN:VCARD\r\n".to_string() + &piece.repeat(size / piece.len()) + "END:VCARD\r\n"
    }

    /// Measures what `read` and `is_one` cost on cards of each shape:
    /// `cargo test --release -p jaw-proto measure_what_cards_cost -- --ignored --nocapture`.
    #[test]
    #[ignore = "a measurement: cargo test --release -p jaw-proto measure_what_cards_cost -- --ignored --nocapture"]
    fn measure_what_cards_cost() {
        for size in [256 * 1024, 1024 * 1024] {
            let folded = format!("BEGIN:VCARD\r\nNOTE:{}\r\nEND:VCARD\r\n", "\r\n x".repeat(size / 4));
            let shapes = [
                ("a photo, folded at 75 columns", with_photo(size)),
                ("quoted-printable broken softly, x= lines", soft_broken(size)),
                ("a quote left open, folded lines ending in =", open_quoted(size)),
                ("a note folded after each character", folded),
                ("blank lines", repeated("\n", size)),
                ("lines of a property each, A:", repeated("A:\r\n", size)),
                ("formatted names, FN:a", repeated("FN:a\r\n", size)),
                ("numbers, TEL:1", repeated("TEL:1\r\n", size)),
                ("cards, one after another", "BEGIN:VCARD\r\nEND:VCARD\r\n".repeat(size / 24)),
            ];
            for (what, card) in shapes {
                let began = std::time::Instant::now();
                let cards = read(&card);
                let reading = began.elapsed();
                let began = std::time::Instant::now();
                let one = is_one(&card);
                let checking = began.elapsed();
                println!(
                    "{what:<45} {:>5} KiB: read {reading:>10.2?} ({} cards), is_one {checking:>10.2?} ({one})",
                    card.len() / 1024,
                    cards.len()
                );
            }
        }
    }
}
