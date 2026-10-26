//! One converter per kind of message, each turning WhatsApp's protobuf into a body: the kind as the
//! protocol names it, the text search finds it by, its spans, a one-line summary, and what its kind
//! needs to be drawn. M1 reads text fully and gives every other kind its row and summary; later
//! steps deepen each kind from the raw protobuf the row keeps.

use serde_json::{Value, json};
use wa_proto::wa_web_protobufs_e2e as e2e;

use super::spans::{self, Span};
use super::{SUMMARY, user_jid};
use crate::store::media::{Keys, MediaCopy};
use crate::store::thumbnails;

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
    /// A poll's options, by name, in their order.
    pub poll: Vec<String>,
}

impl Body {
    fn new(kind: &'static str, summary: impl Into<String>) -> Body {
        Body { kind, text: None, spans: Vec::new(), summary: summary.into(), detail: None, poll: Vec::new() }
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
    if let Some(x) = m.template_message.as_deref() {
        return template(x, mentioned);
    }
    if let Some(x) = m.interactive_message.as_deref() {
        return interactive(x, mentioned);
    }
    if let Some(x) = m.buttons_message.as_deref() {
        return buttons(x, mentioned);
    }
    if let Some(x) = m.list_message.as_deref() {
        return list(x, mentioned);
    }
    if let Some(chosen) = answer(m) {
        return Body::with_text("text", Some(chosen), mentioned, "");
    }
    if let Some(x) = m.rich_response_message.as_deref() {
        return rich_response(x, mentioned);
    }
    if let Some(x) = m.newsletter_follower_invite_message_v2.as_deref() {
        let channel = Channel { jid: x.newsletter_jid.as_deref(), name: x.newsletter_name.as_deref(), expires: None };
        return channel_invite(&channel, false, x.caption.as_deref(), mentioned);
    }
    if let Some(x) = m.newsletter_admin_invite_message.as_deref() {
        let channel = Channel {
            jid: x.newsletter_jid.as_deref(),
            name: x.newsletter_name.as_deref(),
            expires: x.invite_expiration,
        };
        return channel_invite(&channel, true, x.caption.as_deref(), mentioned);
    }
    if let Some(x) = m.message_history_notice.as_deref() {
        return shared_history(x.message_history_metadata.as_ref());
    }
    if let Some(x) = m.message_history_bundle.as_deref() {
        return shared_history(x.message_history_metadata.as_ref());
    }
    if m.placeholder_message.is_some() {
        return line("Open WhatsApp on your phone to see this message");
    }
    if let Some(x) = m.keep_in_chat_message.as_ref() {
        let kept = x.keep_type == Some(e2e::KeepType::KeepForAll as i32);
        let sentence = if kept { "A message was kept in the chat" } else { "A message is no longer kept" };
        return Body::new("system", sentence).detail(json!({ "system": sentence }));
    }
    if m.pin_in_chat_message.is_some() {
        let sentence = "A message was pinned or unpinned";
        return Body::new("system", sentence).detail(json!({ "system": sentence }));
    }
    if is_only_plumbing(m) {
        return Body::new(NOTHING, "");
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

/// The file a message carries, as WhatsApp describes it, with the keys to download it: a photo's,
/// a video's or video message's, a voice message's or an audio file's, a document's or a sticker's.
pub fn copy(m: &e2e::Message) -> Option<MediaCopy> {
    let present = |bytes: &Option<Vec<u8>>| bytes.as_ref().is_some_and(|b| !b.is_empty());
    let copy =
        |keys, mime: &Option<String>, sha: &Option<Vec<u8>>, enc: &Option<Vec<u8>>, key: &Option<Vec<u8>>| MediaCopy {
            keys,
            mime: mime.clone().filter(|m| !m.is_empty()),
            sha256: sha.clone().filter(|s| s.len() == 32),
            enc_sha256: enc.clone().filter(|s| s.len() == 32),
            media_key: key.clone().filter(|k| k.len() == 32),
            ..MediaCopy::default()
        };
    let positive = |n: Option<u32>| n.filter(|&n| n > 0);
    if let Some(x) = m.image_message.as_deref().or(m.instant_image_message.as_deref()) {
        return Some(MediaCopy {
            bytes: x.file_length,
            w: positive(x.width),
            h: positive(x.height),
            direct_path: x.direct_path.clone(),
            thumbnail: present(&x.jpeg_thumbnail),
            ..copy(Keys::Image, &x.mimetype, &x.file_sha256, &x.file_enc_sha256, &x.media_key)
        });
    }
    if let Some(x) = m.video_message.as_deref().or(m.ptv_message.as_deref()) {
        return Some(MediaCopy {
            bytes: x.file_length,
            w: positive(x.width),
            h: positive(x.height),
            seconds: positive(x.seconds).map(f64::from),
            direct_path: x.direct_path.clone(),
            thumbnail: present(&x.jpeg_thumbnail),
            ..copy(Keys::Video, &x.mimetype, &x.file_sha256, &x.file_enc_sha256, &x.media_key)
        });
    }
    if let Some(x) = m.audio_message.as_deref() {
        return Some(MediaCopy {
            bytes: x.file_length,
            seconds: positive(x.seconds).map(f64::from),
            direct_path: x.direct_path.clone(),
            ..copy(Keys::Audio, &x.mimetype, &x.file_sha256, &x.file_enc_sha256, &x.media_key)
        });
    }
    if let Some(x) = m.document_message.as_deref() {
        return Some(MediaCopy {
            bytes: x.file_length,
            name: x.file_name.clone().or(x.title.clone()).filter(|n| !n.is_empty()),
            direct_path: x.direct_path.clone(),
            thumbnail: present(&x.jpeg_thumbnail),
            ..copy(Keys::Document, &x.mimetype, &x.file_sha256, &x.file_enc_sha256, &x.media_key)
        });
    }
    if let Some(x) = m.sticker_message.as_deref() {
        return Some(MediaCopy {
            bytes: x.file_length,
            w: positive(x.width),
            h: positive(x.height),
            direct_path: x.direct_path.clone(),
            thumbnail: present(&x.png_thumbnail),
            ..copy(Keys::Image, &x.mimetype, &x.file_sha256, &x.file_enc_sha256, &x.media_key)
        });
    }
    None
}

/// The small picture a message carries to draw until its file comes, if it has one.
pub fn thumbnail(m: &e2e::Message) -> Option<&[u8]> {
    let bytes = m
        .image_message
        .as_deref()
        .or(m.instant_image_message.as_deref())
        .and_then(|x| x.jpeg_thumbnail.as_deref())
        .or_else(|| m.video_message.as_deref().or(m.ptv_message.as_deref()).and_then(|x| x.jpeg_thumbnail.as_deref()))
        .or_else(|| m.document_message.as_deref().and_then(|x| x.jpeg_thumbnail.as_deref()))
        .or_else(|| m.sticker_message.as_deref().and_then(|x| x.png_thumbnail.as_deref()))?;
    (!bytes.is_empty()).then_some(bytes)
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

/// A URL as a link span makes one: `https://` before one that doesn't say how it's reached.
pub fn absolute(url: &str) -> String {
    let starts = |scheme: &str| url.get(..scheme.len()).is_some_and(|start| start.eq_ignore_ascii_case(scheme));
    if starts("http://") || starts("https://") { url.to_string() } else { format!("https://{url}") }
}

/// Text with its link preview, if it has one: the URL it's for, its title and description, and the
/// SHA-256 of the thumbnail that came with it, which names its file once jawd has kept it.
fn extended_text(x: &e2e::ExtendedTextMessage, mentioned: &[String]) -> Body {
    let body = Body::with_text("text", x.text.as_deref(), mentioned, "");
    let url = x.matched_text.as_deref().map(str::trim).filter(|u| !u.is_empty());
    let said = |s: &Option<String>| s.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let (title, description) = (said(&x.title), said(&x.description));
    let thumbnail = x.jpeg_thumbnail.as_deref().filter(|t| !t.is_empty());
    match url {
        Some(url) if title.is_some() || description.is_some() || thumbnail.is_some() => {
            let mut link = json!({ "url": absolute(url), "title": title, "description": description });
            if let Some(thumbnail) = thumbnail {
                link["thumbnail"] = json!(thumbnails::sha256(thumbnail));
            }
            body.detail(json!({ "link": link }))
        }
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

/// How many of a poll's options one vote may choose: what it says, or every one when it says none,
/// as WhatsApp's "allow multiple answers" sends it.
pub fn poll_max(x: &e2e::PollCreationMessage) -> u32 {
    let options = (x.options.len() as u32).max(1);
    match x.selectable_options_count {
        Some(n) if (1..=options).contains(&n) => n,
        _ => options,
    }
}

/// A poll: its question, its options with their votes, which windows count as they read it, and
/// how many one vote may choose.
fn poll_body(x: &e2e::PollCreationMessage) -> Body {
    let question = x.name.as_deref().unwrap_or_default();
    let options: Vec<String> = x.options.iter().map(|o| o.option_name.clone().unwrap_or_default()).collect();
    let mut body = Body::new("poll", spans::one_line(&format!("Poll: {question}"), SUMMARY));
    body.text =
        Some(std::iter::once(question).chain(options.iter().map(String::as_str)).collect::<Vec<_>>().join("\n"));
    body.spans = vec![Span { t: "text", s: question.to_string(), url: None, person: None }];
    let mut body = body.detail(json!({
        "poll": {
            "question": question,
            "options": options.iter().map(|name| json!({ "name": name, "votes": 0, "mine": false })).collect::<Vec<_>>(),
            "max": poll_max(x),
            "voters": 0,
        }
    }));
    body.poll = options;
    body
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
    Body::new("call", sentence.clone()).detail(json!({ "system": sentence, "video": video }))
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

/// A line in the chat that isn't anyone's message.
fn line(sentence: &str) -> Body {
    Body::new("system", sentence).detail(json!({ "system": sentence }))
}

/// A business's message as text: its parts that say something, a paragraph each, its first line
/// its summary; one with nothing to say is still one jaw can't show.
fn paragraphs(parts: &[Option<&str>], mentioned: &[String]) -> Body {
    let said: Vec<&str> = parts.iter().flatten().map(|part| part.trim()).filter(|part| !part.is_empty()).collect();
    if said.is_empty() {
        return Body::new("unknown", UNKNOWN);
    }
    Body::with_text("text", Some(&said.join("\n\n")), mentioned, "")
}

/// A business's template: its title, its text and its footer, whichever form it came in.
fn template(x: &e2e::TemplateMessage, mentioned: &[String]) -> Body {
    use e2e::template_message::{Format, hydrated_four_row_template::Title};
    let hydrated = match &x.format {
        Some(Format::HydratedFourRowTemplate(t)) => Some(t.as_ref()),
        Some(Format::InteractiveMessageTemplate(t)) => return interactive(t, mentioned),
        _ => x.hydrated_template.as_deref(),
    };
    let Some(t) = hydrated else { return Body::new("unknown", UNKNOWN) };
    let title = match &t.title {
        Some(Title::HydratedTitleText(title)) => Some(title.as_str()),
        _ => None,
    };
    paragraphs(&[title, t.hydrated_content_text.as_deref(), t.hydrated_footer_text.as_deref()], mentioned)
}

fn interactive(x: &e2e::InteractiveMessage, mentioned: &[String]) -> Body {
    let title = x.header.as_deref().and_then(|header| header.title.as_deref());
    let body = x.body.as_ref().and_then(|body| body.text.as_deref());
    let footer = x.footer.as_deref().and_then(|footer| footer.text.as_deref());
    paragraphs(&[title, body, footer], mentioned)
}

fn buttons(x: &e2e::ButtonsMessage, mentioned: &[String]) -> Body {
    use e2e::buttons_message::Header;
    let header = match &x.header {
        Some(Header::Text(text)) => Some(text.as_str()),
        _ => None,
    };
    paragraphs(&[header, x.content_text.as_deref(), x.footer_text.as_deref()], mentioned)
}

/// A business's list: what it says, then its options a line each, since they're chosen on the phone.
fn list(x: &e2e::ListMessage, mentioned: &[String]) -> Body {
    let options: Vec<String> = x
        .sections
        .iter()
        .flat_map(|section| &section.rows)
        .filter_map(|row| row.title.as_deref().map(str::trim).filter(|title| !title.is_empty()))
        .map(|title| format!("- {title}"))
        .collect();
    let options = (!options.is_empty()).then(|| options.join("\n"));
    paragraphs(&[x.title.as_deref(), x.description.as_deref(), x.footer_text.as_deref(), options.as_deref()], mentioned)
}

/// What an answer to a business's list, buttons or template says: the option chosen.
fn answer(m: &e2e::Message) -> Option<&str> {
    use e2e::buttons_response_message::Response;
    m.list_response_message
        .as_deref()
        .and_then(|x| x.title.as_deref().or(x.description.as_deref()))
        .or_else(|| m.template_button_reply_message.as_deref().and_then(|x| x.selected_display_text.as_deref()))
        .or_else(|| {
            m.buttons_response_message
                .as_deref()
                .and_then(|x| x.response.as_ref().map(|Response::SelectedDisplayText(text)| text.as_str()))
        })
        .or_else(|| m.interactive_response_message.as_deref().and_then(|x| x.body.as_ref()?.text.as_deref()))
        .filter(|chosen| !chosen.trim().is_empty())
}

/// Meta AI's answer: the text of each part of it that's text.
fn rich_response(x: &e2e::AiRichResponseMessage, mentioned: &[String]) -> Body {
    use wa_proto::waai_common_deprecated::AiRichResponseSubMessageType as Part;
    let text: Vec<Option<&str>> = x
        .submessages
        .iter()
        .filter(|part| part.message_type.is_none_or(|t| t == Part::AiRichResponseText as i32))
        .map(|part| part.message_text.as_deref())
        .collect();
    paragraphs(&text, mentioned)
}

/// A channel an invitation names, and when the invitation runs out, in seconds.
struct Channel<'a> {
    jid: Option<&'a str>,
    name: Option<&'a str>,
    expires: Option<i64>,
}

/// An invitation to follow a channel, or to be one of its admins.
fn channel_invite(channel: &Channel, admin: bool, caption: Option<&str>, mentioned: &[String]) -> Body {
    let name = channel.name.filter(|name| !name.is_empty());
    let fallback = match (admin, name) {
        (false, Some(name)) => format!("Invitation to follow {name}"),
        (false, None) => "Invitation to follow a channel".to_string(),
        (true, Some(name)) => format!("Invitation to be an admin of {name}"),
        (true, None) => "Invitation to be a channel's admin".to_string(),
    };
    let mut invite = serde_json::Map::new();
    if let Some(jid) = channel.jid.filter(|jid| !jid.is_empty()) {
        invite.insert("channel".into(), json!(jid));
    }
    if let Some(name) = name {
        invite.insert("name".into(), json!(name));
    }
    if let Some(expires) = channel.expires.filter(|&seconds| seconds > 0) {
        invite.insert("expires".into(), json!(expires.saturating_mul(1000)));
    }
    if admin {
        invite.insert("admin".into(), json!(true));
    }
    Body::with_text("invite", caption, mentioned, &fallback).detail(json!({ "invite": invite }))
}

/// Recent messages a member shared with those who joined after them.
fn shared_history(metadata: Option<&e2e::MessageHistoryMetadata>) -> Body {
    let sentence = match metadata.and_then(|metadata| metadata.message_count).filter(|&n| n > 0) {
        Some(1) => "1 recent message shared with new members".to_string(),
        Some(n) => format!("{n} recent messages shared with new members"),
        None => "Recent messages shared with new members".to_string(),
    };
    line(&sentence)
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
