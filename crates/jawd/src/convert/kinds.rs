//! One converter per kind of message, each turning WhatsApp's protobuf into a body: the kind as the
//! protocol names it, the text search finds it by, its spans, a one-line summary, and what its kind
//! needs to be drawn. M1 reads text fully and gives every other kind its row and summary; later
//! steps deepen each kind from the raw protobuf the row keeps.

use serde_json::{Value, json};
use wa_proto::wa_web_protobufs_e2e as e2e;

use super::spans::{self, Span};
use super::{SUMMARY, user_jid};

/// What a message that isn't a row converts to.
pub const NOTHING: &str = "";
/// The summary of a kind jaw can't show yet.
pub const UNKNOWN: &str = "jaw can't show this yet";

pub struct Body {
    pub kind: &'static str,
    pub text: Option<String>,
    pub spans: Vec<Span>,
    pub summary: String,
    pub detail: Option<String>,
    /// Whether it counts towards its chat's unread messages.
    pub counts: bool,
}

impl Body {
    fn new(kind: &'static str, summary: impl Into<String>) -> Body {
        Body { kind, text: None, spans: Vec::new(), summary: summary.into(), detail: None, counts: true }
    }

    /// A body whose text is `text`, parsed, and whose summary is its first line, or `fallback`
    /// when there's no text.
    fn with_text(kind: &'static str, text: Option<&str>, mentioned: &[String], fallback: &str) -> Body {
        match text.filter(|t| !t.trim().is_empty()) {
            Some(text) => {
                let spans = spans::parse(text, mentioned);
                Body {
                    text: Some(text.to_string()),
                    summary: spans::line(&spans, SUMMARY),
                    spans,
                    ..Body::new(kind, "")
                }
            }
            None => Body::new(kind, fallback),
        }
    }

    fn detail(mut self, detail: Value) -> Body {
        self.detail = Some(detail.to_string());
        self
    }
}

/// The body of a message already out of its wrappers.
pub fn body(m: &e2e::Message, mentioned: &[String]) -> Body {
    if let Some(text) = m.conversation.as_deref() {
        return Body::with_text("text", Some(text), mentioned, "");
    }
    if let Some(x) = m.extended_text_message.as_deref() {
        return extended_text(x, mentioned);
    }
    if let Some(x) = m.image_message.as_deref().or(m.instant_image_message.as_deref()) {
        return image(x, mentioned);
    }
    if let Some(x) = m.video_message.as_deref() {
        return video(x, mentioned);
    }
    if let Some(x) = m.ptv_message.as_deref() {
        return video_note(x);
    }
    if let Some(x) = m.audio_message.as_deref() {
        return audio(x);
    }
    if let Some(x) = m.document_message.as_deref() {
        return document(x, mentioned);
    }
    if let Some(x) = m.sticker_message.as_deref() {
        return sticker(x);
    }
    if let Some(x) = m.location_message.as_deref() {
        return location(x);
    }
    if let Some(x) = m.live_location_message.as_deref() {
        return live_location(x);
    }
    if let Some(x) = m.contact_message.as_deref() {
        return contact(x);
    }
    if let Some(x) = m.contacts_array_message.as_deref() {
        return contacts(x);
    }
    if let Some(x) = poll(m) {
        return poll_body(x);
    }
    if let Some(x) = m.event_message.as_deref() {
        return event(x);
    }
    if let Some(x) = m.group_invite_message.as_deref() {
        return group_invite(x, mentioned);
    }
    if let Some(x) = m.call_log_messsage.as_ref() {
        return call(x);
    }
    if let Some(x) = m.album_message.as_deref() {
        return album(x);
    }
    if let Some(x) = m.keep_in_chat_message.as_ref() {
        let kept = x.keep_type == Some(e2e::KeepType::KeepForAll as i32);
        let sentence = if kept { "A message was kept in the chat" } else { "A message is no longer kept" };
        return Body { counts: false, ..Body::new("system", sentence) }.detail(json!({ "system": sentence }));
    }
    if m.pin_in_chat_message.is_some() {
        let sentence = "A message was pinned or unpinned";
        return Body { counts: false, ..Body::new("system", sentence) }.detail(json!({ "system": sentence }));
    }
    if is_only_plumbing(m) {
        return Body { counts: false, ..Body::new(NOTHING, "") };
    }
    Body::new("unknown", UNKNOWN)
}

/// Whether the message carries only what keeps encryption going, with nothing to show.
fn is_only_plumbing(m: &e2e::Message) -> bool {
    let mut bare = m.clone();
    bare.sender_key_distribution_message = None;
    bare.fast_ratchet_key_sender_key_distribution_message = None;
    bare.message_context_info = None;
    bare == e2e::Message::default()
}

fn media(mime: Option<&str>, bytes: Option<u64>, w: Option<u32>, h: Option<u32>, seconds: Option<u32>) -> Value {
    let mut media = serde_json::Map::new();
    let mut put = |key: &str, value: Value| {
        if !value.is_null() {
            media.insert(key.into(), value);
        }
    };
    put("mime", json!(mime));
    put("bytes", json!(bytes));
    put("w", json!(w.filter(|&w| w > 0)));
    put("h", json!(h.filter(|&h| h > 0)));
    put("seconds", json!(seconds.filter(|&s| s > 0)));
    Value::Object(media)
}

/// A length as a person reads it: 0:14, 3:02, 1:04:09.
pub fn length(seconds: u32) -> String {
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

fn with_length(what: &str, seconds: Option<u32>) -> String {
    match seconds.filter(|&s| s > 0) {
        Some(s) => format!("{what}, {}", length(s)),
        None => what.to_string(),
    }
}

fn extended_text(x: &e2e::ExtendedTextMessage, mentioned: &[String]) -> Body {
    let body = Body::with_text("text", x.text.as_deref(), mentioned, "");
    let url = x.matched_text.as_deref().filter(|u| !u.is_empty());
    match url {
        Some(url) if x.title.is_some() || x.description.is_some() => body.detail(json!({
            "link": { "url": url, "title": x.title, "description": x.description }
        })),
        _ => body,
    }
}

fn image(x: &e2e::ImageMessage, mentioned: &[String]) -> Body {
    Body::with_text("image", x.caption.as_deref(), mentioned, "Photo").detail(json!({
        "media": media(x.mimetype.as_deref(), x.file_length, x.width, x.height, None)
    }))
}

fn video(x: &e2e::VideoMessage, mentioned: &[String]) -> Body {
    let gif = x.gif_playback.unwrap_or(false);
    let (kind, fallback) = if gif { ("gif", "GIF".to_string()) } else { ("video", with_length("Video", x.seconds)) };
    Body::with_text(kind, x.caption.as_deref(), mentioned, &fallback).detail(json!({
        "media": media(x.mimetype.as_deref(), x.file_length, x.width, x.height, x.seconds)
    }))
}

fn video_note(x: &e2e::VideoMessage) -> Body {
    Body::new("video_note", with_length("Video message", x.seconds)).detail(json!({
        "media": media(x.mimetype.as_deref(), x.file_length, x.width, x.height, x.seconds)
    }))
}

fn audio(x: &e2e::AudioMessage) -> Body {
    let voice = x.ptt.unwrap_or(false);
    let (kind, what) = if voice { ("voice", "Voice message") } else { ("audio", "Audio") };
    Body::new(kind, with_length(what, x.seconds)).detail(json!({
        "media": media(x.mimetype.as_deref(), x.file_length, None, None, x.seconds)
    }))
}

fn document(x: &e2e::DocumentMessage, mentioned: &[String]) -> Body {
    let name = x.file_name.as_deref().or(x.title.as_deref()).filter(|n| !n.is_empty());
    let caption = x.caption.as_deref().filter(|c| !c.trim().is_empty());
    let mut body = match caption {
        Some(caption) => Body::with_text("document", Some(caption), mentioned, ""),
        None => Body::new("document", name.unwrap_or("Document")),
    };
    // A document is found by its name as well as its caption.
    if let Some(name) = name {
        body.text = Some(match body.text.take() {
            Some(caption) => format!("{caption}\n{name}"),
            None => name.to_string(),
        });
        if caption.is_some() {
            body.spans.push(Span { t: "text", s: format!("\n{name}"), url: None, person: None });
        } else {
            body.spans = vec![Span { t: "text", s: name.to_string(), url: None, person: None }];
        }
    }
    let mut media = media(x.mimetype.as_deref(), x.file_length, None, None, None);
    if let (Some(name), Value::Object(map)) = (name, &mut media) {
        map.insert("name".into(), json!(name));
        if let Some(pages) = x.page_count.filter(|&p| p > 0) {
            map.insert("pages".into(), json!(pages));
        }
    }
    body.detail(json!({ "media": media }))
}

fn sticker(x: &e2e::StickerMessage) -> Body {
    let animated = x.is_animated.unwrap_or(false);
    Body::new("sticker", "Sticker").detail(json!({
        "media": media(x.mimetype.as_deref(), x.file_length, x.width, x.height, None),
        "animated": animated,
    }))
}

fn place(lat: Option<f64>, lon: Option<f64>) -> Value {
    json!({ "lat": lat, "lon": lon })
}

fn location(x: &e2e::LocationMessage) -> Body {
    let name = x.name.as_deref().or(x.address.as_deref()).filter(|n| !n.is_empty());
    let summary = name.map_or("Location".to_string(), |name| format!("Location: {name}"));
    let mut body = Body::new("location", spans::one_line(&summary, SUMMARY));
    body.text = name.map(str::to_string);
    let mut where_ = place(x.degrees_latitude, x.degrees_longitude);
    if let Value::Object(map) = &mut where_ {
        map.insert("name".into(), json!(x.name));
        map.insert("address".into(), json!(x.address));
        map.insert("url".into(), json!(x.url));
    }
    body.detail(json!({ "location": where_ }))
}

fn live_location(x: &e2e::LiveLocationMessage) -> Body {
    let mut body = Body::new("live_location", "Live location");
    body.text = x.caption.clone().filter(|c| !c.is_empty());
    body.detail(json!({ "location": place(x.degrees_latitude, x.degrees_longitude) }))
}

fn contact(x: &e2e::ContactMessage) -> Body {
    let name = x.display_name.as_deref().filter(|n| !n.is_empty());
    let mut body = Body::new("contact", name.map_or("Contact".to_string(), |name| format!("Contact: {name}")));
    body.text = name.map(str::to_string);
    body.detail(json!({ "contact": { "name": name, "vcard": x.vcard } }))
}

fn contacts(x: &e2e::ContactsArrayMessage) -> Body {
    let names: Vec<&str> = x.contacts.iter().filter_map(|c| c.display_name.as_deref()).collect();
    let mut body = Body::new("contact", format!("{} contacts", x.contacts.len()));
    body.text = (!names.is_empty()).then(|| names.join(", "));
    let cards: Vec<Value> = x.contacts.iter().map(|c| json!({ "name": c.display_name, "vcard": c.vcard })).collect();
    body.detail(json!({ "contacts": cards }))
}

/// A poll's creation, whichever version WhatsApp sent it as.
pub fn poll(m: &e2e::Message) -> Option<&e2e::PollCreationMessage> {
    m.poll_creation_message
        .as_deref()
        .or(m.poll_creation_message_v2.as_deref())
        .or(m.poll_creation_message_v3.as_deref())
        .or(m.poll_creation_message_v5.as_deref())
        .or(m.poll_creation_message_v6.as_deref())
        .or_else(|| m.poll_creation_message_v4.as_deref()?.message.as_deref().and_then(poll))
}

fn poll_body(x: &e2e::PollCreationMessage) -> Body {
    let question = x.name.as_deref().unwrap_or_default();
    let options: Vec<&str> = x.options.iter().filter_map(|o| o.option_name.as_deref()).collect();
    let mut body = Body::new("poll", spans::one_line(&format!("Poll: {question}"), SUMMARY));
    body.text = Some(std::iter::once(question).chain(options.iter().copied()).collect::<Vec<_>>().join("\n"));
    body.spans = vec![Span { t: "text", s: question.to_string(), url: None, person: None }];
    body.detail(json!({
        "poll": {
            "question": question,
            "options": options.iter().map(|name| json!({ "name": name, "votes": 0, "mine": false })).collect::<Vec<_>>(),
            "max": x.selectable_options_count.filter(|&n| n > 0).unwrap_or(1),
        }
    }))
}

fn event(x: &e2e::EventMessage) -> Body {
    let name = x.name.as_deref().unwrap_or_default();
    let cancelled = x.is_canceled.unwrap_or(false);
    let what = if cancelled { "Event cancelled" } else { "Event" };
    let mut body = Body::new("event", spans::one_line(&format!("{what}: {name}"), SUMMARY));
    body.text = Some([name, x.description.as_deref().unwrap_or_default()].join("\n").trim().to_string());
    body.detail(json!({
        "event": {
            "name": name,
            "description": x.description,
            "start": x.start_time.map(|s| s * 1000),
            "end": x.end_time.map(|s| s * 1000),
            "cancelled": cancelled,
            "link": x.join_link,
        }
    }))
}

fn group_invite(x: &e2e::GroupInviteMessage, mentioned: &[String]) -> Body {
    let name = x.group_name.as_deref().unwrap_or("a group");
    let fallback = format!("Invitation to join {name}");
    Body::with_text("invite", x.caption.as_deref(), mentioned, &fallback).detail(json!({
        "invite": { "group": x.group_jid.as_deref().map(user_jid), "name": x.group_name, "expires": x.invite_expiration.map(|s| s * 1000) }
    }))
}

fn call(x: &e2e::CallLogMessage) -> Body {
    use e2e::call_log_message::CallOutcome;
    let video = x.is_video.unwrap_or(false);
    let what = if video { "video call" } else { "voice call" };
    let seconds = x.duration_secs.and_then(|s| u32::try_from(s).ok());
    let sentence = match x.call_outcome.and_then(|o| CallOutcome::try_from(o).ok()) {
        Some(CallOutcome::Missed) => format!("Missed {what}"),
        Some(CallOutcome::Rejected) => format!("Declined {what}"),
        Some(CallOutcome::Failed) => format!("Failed {what}"),
        _ => with_length(&capitalised(what), seconds),
    };
    Body { counts: false, ..Body::new("call", sentence.clone()) }.detail(json!({ "system": sentence, "video": video }))
}

fn album(x: &e2e::AlbumMessage) -> Body {
    let (photos, videos) = (x.expected_image_count.unwrap_or(0), x.expected_video_count.unwrap_or(0));
    let mut parts = Vec::new();
    if photos > 0 {
        parts.push(format!("{photos} photo{}", if photos == 1 { "" } else { "s" }));
    }
    if videos > 0 {
        parts.push(format!("{videos} video{}", if videos == 1 { "" } else { "s" }));
    }
    let summary = if parts.is_empty() { "Album".to_string() } else { format!("Album: {}", parts.join(" and ")) };
    Body::new("album", summary).detail(json!({ "album": { "photos": photos, "videos": videos } }))
}

fn capitalised(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// What a chat's disappearing-message timer being set to `seconds` reads as.
pub fn ephemeral_sentence(seconds: u32) -> String {
    match seconds {
        0 => "Disappearing messages turned off".to_string(),
        s if s % 86_400 == 0 => {
            let days = s / 86_400;
            format!("Disappearing messages turned on: {days} day{}", if days == 1 { "" } else { "s" })
        }
        s if s % 3600 == 0 => format!("Disappearing messages turned on: {} hours", s / 3600),
        s => format!("Disappearing messages turned on: {s} seconds"),
    }
}
