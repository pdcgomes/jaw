//! Converters from WhatsApp's raw protobufs into the store's rows. Every message is a
//! `WebMessageInfo`, as history keeps it, and a live one is made into one before it's converted,
//! so both go through the same converter. Wrappers (device-sent, ephemeral, view-once, edits) are
//! unwrapped, each kind has a converter of its own (see `kinds`), and what jaw doesn't know yet
//! becomes a row of kind `unknown` with its raw protobuf, so nothing is lost before jaw learns it.
//! Reactions, edits, deletes and votes become amendments to the message they name, which the store
//! keeps in `pending` until that message arrives.

pub mod app_state;
pub mod history;
pub(crate) mod kinds;
pub mod people;
pub mod spans;

use prost::Message as _;
use serde_json::json;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::{PollUpdate, WebMessageInfo, web_message_info};

use crate::engine::pb;
use crate::store::{Amend, Amendment, Change, Chat, Message, Packed, Person};

/// The longest summary, in characters.
pub const SUMMARY: usize = 120;

/// The account's own JIDs, without devices: its phone number's and its LID.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Me {
    pub jids: Vec<String>,
}

impl Me {
    pub fn new(jids: impl IntoIterator<Item = String>) -> Me {
        let mut jids: Vec<String> = jids.into_iter().filter(|jid| !jid.is_empty()).map(|jid| user_jid(&jid)).collect();
        jids.sort();
        jids.dedup();
        Me { jids }
    }

    pub fn is(&self, jid: &str) -> bool {
        let jid = user_jid(jid);
        self.jids.contains(&jid)
    }

    /// The JID to name the account by in rows that need one, such as a reaction's.
    pub fn jid(&self) -> Option<&str> {
        self.jids.iter().find(|jid| jid.ends_with("@s.whatsapp.net")).or(self.jids.first()).map(String::as_str)
    }
}

/// A JID without its device.
pub fn user_jid(jid: &str) -> String {
    match jid.split_once('@') {
        Some((user, server @ ("s.whatsapp.net" | "lid" | "hosted" | "hosted.lid"))) => {
            let user = user.split([':', '.']).next().unwrap_or(user);
            format!("{user}@{server}")
        }
        _ => jid.to_string(),
    }
}

pub fn is_person(jid: &str) -> bool {
    jid.ends_with("@s.whatsapp.net")
        || jid.ends_with("@lid")
        || jid.ends_with("@hosted")
        || jid.ends_with("@hosted.lid")
}

/// A live message from the engine as WhatsApp's `WebMessageInfo`, the form history keeps, and the
/// chat it belongs to. A one-to-one chat WhatsApp addressed by LID is kept under the phone
/// number's JID when the message says what it is, so the person has one chat.
pub fn web_message(message: &pb::Message) -> Result<WebMessageInfo, prost::DecodeError> {
    let info = message.info.clone().unwrap_or_default();
    let mut chat = info.chat.clone();
    if !info.group && chat.ends_with("@lid") {
        let alt = if info.from_me { &info.recipient_alt } else { &info.sender_alt };
        if alt.ends_with("@s.whatsapp.net") {
            chat = user_jid(alt);
        }
    }
    let participant = (info.group || chat.ends_with("@broadcast")).then(|| info.sender.clone());
    Ok(WebMessageInfo {
        key: MessageKey {
            remote_jid: Some(chat),
            from_me: Some(info.from_me),
            id: Some(info.id.clone()),
            participant: participant.clone(),
        },
        message: Some(e2e::Message::decode(message.raw.as_slice())?),
        message_timestamp: Some((info.timestamp_ms / 1000).max(0) as u64),
        participant,
        push_name: (!info.push_name.is_empty()).then(|| info.push_name.clone()),
        verified_biz_name: (!info.verified_name.is_empty()).then(|| info.verified_name.clone()),
        status: info.from_me.then_some(web_message_info::Status::ServerAck as i32),
        ..Default::default()
    })
}

/// Who sent a message and how they're known, for the people it adds or joins.
#[derive(Clone, Debug, Default)]
struct From {
    jid: Option<String>,
    mine: bool,
}

/// What a message's envelope says, whatever its kind.
struct Envelope<'a> {
    chat: String,
    id: String,
    at: i64,
    from: From,
    info: &'a WebMessageInfo,
    live: bool,
}

/// One message's changes: its row or the amendment it makes, the people it names, and what it
/// says of its chat. `live` is for messages as they arrive rather than in history.
pub fn message(info: &WebMessageInfo, me: &Me, live: bool) -> Vec<Change> {
    let chat = info.key.remote_jid.clone().unwrap_or_default();
    let id = info.key.id.clone().unwrap_or_default();
    if chat.is_empty() || id.is_empty() || chat == "status@broadcast" {
        return Vec::new();
    }
    let mine = info.key.from_me.unwrap_or(false);
    let group = !is_person(&chat);
    let sender = if mine {
        None
    } else if group {
        info.key.participant.clone().or_else(|| info.participant.clone()).filter(|jid| !jid.is_empty())
    } else {
        Some(chat.clone())
    };
    let envelope = Envelope {
        at: info.message_timestamp.unwrap_or_default() as i64 * 1000,
        from: From { jid: sender.map(|jid| user_jid(&jid)), mine },
        chat,
        id,
        info,
        live,
    };
    let mut changes = Vec::new();
    if let (Some(jid), Some(name)) = (&envelope.from.jid, info.push_name.as_ref().filter(|n| !n.is_empty()))
        && is_person(jid)
    {
        changes.push(Change::Person(Person {
            jids: vec![jid.clone()],
            push_name: Some(name.clone()),
            verified_name: info.verified_biz_name.clone().filter(|n| !n.is_empty()),
            ..Person::default()
        }));
    }
    match info.message.as_ref() {
        Some(message) => convert(&envelope, message, me, &mut changes),
        None => match info.message_stub_type {
            Some(stub) if stub == web_message_info::StubType::Revoke as i32 => changes.extend(revoked(&envelope)),
            Some(stub) if stub != 0 => changes.push(Change::Message(stub_line(&envelope, stub))),
            _ => {}
        },
    }
    // An edit that stands in for its message carries that message's reactions.
    let target = changes.iter().find_map(|c| match c {
        Change::Edited { amend, .. } => Some(amend.target.clone()),
        _ => None,
    });
    for reaction in &info.reactions {
        if let Some(mut amend) = history_reaction(&envelope, reaction, me) {
            if let Some(target) = &target {
                amend.target = target.clone();
            }
            changes.push(Change::Amend(amend));
        }
    }
    changes
        .extend(info.poll_updates.iter().filter_map(|update| history_vote(&envelope, update, me)).map(Change::Amend));
    changes
}

/// Each option a vote chose, as the hex SHA-256 of its name it names it by.
fn chosen(vote: &e2e::PollVoteMessage) -> Vec<String> {
    vote.selected_options.iter().map(|hash| hash.iter().map(|b| format!("{b:02x}")).collect()).collect()
}

/// A vote history keeps on the poll it's in, decrypted already.
fn history_vote(envelope: &Envelope, update: &PollUpdate, me: &Me) -> Option<Amend> {
    let key = update.poll_update_message_key.as_ref()?;
    let mine = key.from_me.unwrap_or(false);
    let sender = if mine {
        me.jid().map(str::to_string)
    } else {
        key.participant
            .clone()
            .or_else(|| key.remote_jid.clone())
            .filter(|jid| !jid.is_empty())
            .map(|jid| user_jid(&jid))
    };
    Some(Amend {
        chat: envelope.chat.clone(),
        target: envelope.id.clone(),
        sender,
        mine,
        at: update.sender_timestamp_ms.filter(|&at| at > 0).unwrap_or(envelope.at),
        what: Amendment::Vote { options: chosen(update.vote.as_ref()?), id: key.id.clone().unwrap_or_default() },
        raw: None,
    })
}

/// The vote a live poll update's decrypted payload says (`decrypted`, a PollVoteMessage), in place
/// of the vote `changes` would keep for a later jawd, which couldn't read it; says whether there was
/// one to read.
pub fn decrypted_vote(changes: &mut [Change], id: &str, decrypted: &[u8]) -> bool {
    let Ok(vote) = e2e::PollVoteMessage::decode(decrypted) else { return false };
    for change in changes {
        if let Change::Amend(Amend { what, .. }) = change
            && matches!(what, Amendment::Keep { kind } if kind == "vote")
        {
            *what = Amendment::Vote { options: chosen(&vote), id: id.to_string() };
            return true;
        }
    }
    false
}

/// What a message adds besides its kind's body: a reply, mentions, forwarding, expiry.
#[derive(Default)]
struct Context {
    reply: Option<String>,
    mentioned: Vec<String>,
    forwarded: u32,
    expires_in: Option<u32>,
}

fn context(info: Option<&e2e::ContextInfo>) -> Context {
    let Some(info) = info else { return Context::default() };
    let reply = info.stanza_id.as_ref().filter(|id| !id.is_empty()).map(|id| {
        let summary = info.quoted_message.as_deref().map(|quoted| kinds::body(unwrap(quoted).0, &[]).summary);
        json!({
            "id": id,
            "from": info.participant.as_deref().map(user_jid),
            "summary": summary.unwrap_or_default(),
        })
        .to_string()
    });
    Context {
        reply,
        mentioned: info.mentioned_jid.clone(),
        forwarded: if info.is_forwarded.unwrap_or(false) { info.forwarding_score.unwrap_or(1).max(1) } else { 0 },
        expires_in: info.expiration.filter(|&seconds| seconds > 0),
    }
}

/// What wrapped the message, which the row records.
#[derive(Default)]
struct Wrapped {
    view_once: bool,
    /// For a comment on a community's announcement, the announcement it's on.
    comment_on: Option<MessageKey>,
}

/// The message inside its wrappers.
fn unwrap(mut message: &e2e::Message) -> (&e2e::Message, Wrapped) {
    let mut wrapped = Wrapped::default();
    for _ in 0..8 {
        let inner = if let Some(sent) = message.device_sent_message.as_deref() {
            sent.message.as_deref()
        } else if let Some(m) = message.ephemeral_message.as_deref() {
            m.message.as_deref()
        } else if let Some(m) = message
            .view_once_message
            .as_deref()
            .or(message.view_once_message_v2.as_deref())
            .or(message.view_once_message_v2_extension.as_deref())
        {
            wrapped.view_once = true;
            m.message.as_deref()
        } else if let Some(m) = message.comment_message.as_deref() {
            wrapped.comment_on = m.target_message_key.clone();
            m.message.as_deref()
        } else if let Some(m) = message
            .document_with_caption_message
            .as_deref()
            .or(message.edited_message.as_deref())
            .or(message.group_mentioned_message.as_deref())
            .or(message.lottie_sticker_message.as_deref())
            .or(message.associated_child_message.as_deref())
            .or(message.bot_invoke_message.as_deref())
            .or(message.group_status_message.as_deref())
            .or(message.group_status_message_v2.as_deref())
        {
            m.message.as_deref()
        } else {
            None
        };
        match inner {
            Some(inner) => message = inner,
            None => break,
        }
    }
    (message, wrapped)
}

fn convert(envelope: &Envelope, message: &e2e::Message, me: &Me, changes: &mut Vec<Change>) {
    let (message, wrapped) = unwrap(message);
    if let Some(protocol) = message.protocol_message.as_deref() {
        protocol_message(envelope, protocol, me, changes);
        return;
    }
    if let Some(reaction) = message.reaction_message.as_ref() {
        if let Some(mut amend) = reaction_amend(envelope, reaction.key.as_ref(), reaction.text.clone(), me) {
            // Its sender's own time, to the millisecond, as history keeps it too.
            amend.at = reaction.sender_timestamp_ms.filter(|&at| at > 0).unwrap_or(amend.at);
            changes.push(Change::Amend(amend));
            changes.extend(own_action(envelope));
        }
        return;
    }
    if let Some(update) = message.poll_update_message.as_ref() {
        // Its sender's own time, as history keeps it too; the engine's decryption says what it chose.
        if let Some(mut amend) =
            update.poll_creation_message_key.as_ref().and_then(|key| keep(envelope, key, "vote", me))
        {
            amend.at = update.sender_timestamp_ms.filter(|&at| at > 0).unwrap_or(amend.at);
            changes.push(Change::Amend(amend));
            changes.extend(own_action(envelope));
        }
        return;
    }
    if let Some(encrypted) = message.enc_reaction_message.as_ref() {
        changes.extend(
            encrypted
                .target_message_key
                .as_ref()
                .and_then(|key| keep(envelope, key, "reaction", me))
                .map(Change::Amend),
        );
        return;
    }
    if let Some(kept) = encrypted_amendment(envelope, message, me) {
        changes.push(Change::Amend(kept));
        changes.extend(own_action(envelope));
        return;
    }
    if let Some(pin) = message.pin_in_chat_message.as_ref() {
        changes.extend(pin.key.as_ref().and_then(|key| keep(envelope, key, "pin", me)).map(Change::Amend));
    }
    if let Some(sentence) = status_sentence(message, envelope.from.mine) {
        changes.push(Change::Message(system(envelope, "system", sentence)));
        return;
    }
    let body = kinds::body(message, &context_of(message).map(|c| c.mentioned_jid.clone()).unwrap_or_default());
    if body.kind == kinds::NOTHING {
        return;
    }
    let mut context = context(context_of(message));
    if context.reply.is_none() {
        context.reply = wrapped.comment_on.as_ref().and_then(commented);
    }
    let mentions_me = context.mentioned.iter().any(|jid| me.is(jid));
    let info = envelope.info;
    let status = envelope.from.mine.then(|| status(info.status));
    let expires_at = match (info.ephemeral_start_timestamp, info.ephemeral_duration.or(context.expires_in)) {
        (Some(start), Some(seconds)) if start > 0 => Some(start as i64 * 1000 + i64::from(seconds) * 1000),
        (_, Some(seconds)) => Some(envelope.at + i64::from(seconds) * 1000),
        _ => None,
    };
    changes.push(Change::Message(Message {
        chat: envelope.chat.clone(),
        id: envelope.id.clone(),
        at: envelope.at,
        sender: envelope.from.jid.clone(),
        mine: envelope.from.mine,
        kind: body.kind,
        status,
        spans: body.text.as_deref().and_then(|_| spans::json(&body.spans)),
        text: body.text,
        summary: body.summary,
        reply: context.reply,
        detail: body.detail,
        forwarded: context.forwarded,
        expires_at,
        starred: info.starred.unwrap_or(false),
        view_once: wrapped.view_once,
        live: envelope.live,
        mentions_me,
        raw: Some(Packed::new(&info.encode_to_vec())),
        poll: body.poll,
        ..Message::default()
    }));
}

/// An edit or an answer to an event encrypted with its message's secret, which jawd can't read yet:
/// it waits in `pending` with its protobuf, for a later jawd, rather than showing as a row.
fn encrypted_amendment(envelope: &Envelope, message: &e2e::Message, me: &Me) -> Option<Amend> {
    if let Some(x) = message.secret_encrypted_message.as_ref() {
        return keep(envelope, x.target_message_key.as_ref()?, "secret", me);
    }
    let x = message.enc_event_response_message.as_ref()?;
    keep(envelope, x.event_creation_message_key.as_ref()?, "rsvp", me)
}

/// What a mention in a status, or an answer to one, says in the chat it reaches.
fn status_sentence(message: &e2e::Message, mine: bool) -> Option<&'static str> {
    if message.status_mention_message.is_some() {
        return Some(if mine { "You mentioned them in your status" } else { "Mentioned you in their status" });
    }
    message
        .status_notification_message
        .as_ref()
        .map(|_| if mine { "You responded to their status" } else { "Responded to your status" })
}

/// The quote of a comment: the announcement it's on, which the comment doesn't carry the words of.
fn commented(on: &MessageKey) -> Option<String> {
    let id = on.id.as_deref().filter(|id| !id.is_empty())?;
    Some(json!({ "id": id, "from": on.participant.as_deref().map(user_jid), "summary": "" }).to_string())
}

/// The poll a message creates, out of its wrappers, whichever version it was sent as.
pub fn poll_of(message: &e2e::Message) -> Option<&e2e::PollCreationMessage> {
    kinds::poll(unwrap(message).0)
}

/// The thumbnail a message's link preview came with, out of its wrappers.
pub fn link_thumbnail(message: &e2e::Message) -> Option<&[u8]> {
    let (message, _) = unwrap(message);
    let preview = message.extended_text_message.as_deref()?;
    preview.matched_text.as_deref().filter(|url| !url.trim().is_empty())?;
    preview.jpeg_thumbnail.as_deref().filter(|thumbnail| !thumbnail.is_empty())
}

/// What a message says, out of its wrappers and without the quote, mentions, forwarding and timer
/// it carried, as a reply quotes it.
pub fn bare(message: &e2e::Message) -> e2e::Message {
    let mut bare = unwrap(message).0.clone();
    if let Some(context) = context_mut(&mut bare) {
        *context = None;
    }
    bare
}

/// `message` with its caption, a photo's, a video's or a document's, as `caption`, for one whose
/// caption an edit changed: the row keeps the edit, and the raw protobuf what came first.
pub fn captioned(message: &e2e::Message, caption: &str) -> e2e::Message {
    let mut captioned = message.clone();
    if let Some(x) = captioned.image_message.as_deref_mut() {
        x.caption = Some(caption.into());
    } else if let Some(x) = captioned.video_message.as_deref_mut() {
        x.caption = Some(caption.into());
    } else if let Some(x) = captioned.document_message.as_deref_mut() {
        x.caption = Some(caption.into());
    }
    captioned
}

/// A message sent again as a forward, as WhatsApp's phones send one: what it says, marked forwarded
/// `score` times, with the chat's disappearing timer when it has one; `None` for a kind that can't
/// carry that.
pub fn forwarded(message: &e2e::Message, score: u32, expiration: u32) -> Option<e2e::Message> {
    let mut forwarded = bare(message);
    if let Some(text) = forwarded.conversation.take() {
        let extended = e2e::ExtendedTextMessage { text: Some(text), ..Default::default() };
        forwarded.extended_text_message = Some(Box::new(extended));
    }
    let context = context_mut(&mut forwarded)?;
    *context = Some(Box::new(e2e::ContextInfo {
        is_forwarded: Some(true),
        forwarding_score: Some(score),
        expiration: (expiration > 0).then_some(expiration),
        ..Default::default()
    }));
    Some(forwarded)
}

/// The ContextInfo of whichever kind the message is, to change; `None` for a kind that has none,
/// plain text among them.
fn context_mut(m: &mut e2e::Message) -> Option<&mut Option<Box<e2e::ContextInfo>>> {
    if let Some(x) = m.extended_text_message.as_deref_mut() {
        return Some(&mut x.context_info);
    }
    if let Some(x) = m.image_message.as_deref_mut() {
        return Some(&mut x.context_info);
    }
    if let Some(x) = m.video_message.as_deref_mut() {
        return Some(&mut x.context_info);
    }
    if let Some(x) = m.ptv_message.as_deref_mut() {
        return Some(&mut x.context_info);
    }
    if let Some(x) = m.audio_message.as_deref_mut() {
        return Some(&mut x.context_info);
    }
    if let Some(x) = m.document_message.as_deref_mut() {
        return Some(&mut x.context_info);
    }
    if let Some(x) = m.sticker_message.as_deref_mut() {
        return Some(&mut x.context_info);
    }
    if let Some(x) = m.location_message.as_deref_mut() {
        return Some(&mut x.context_info);
    }
    if let Some(x) = m.contact_message.as_deref_mut() {
        return Some(&mut x.context_info);
    }
    if let Some(x) = m.contacts_array_message.as_deref_mut() {
        return Some(&mut x.context_info);
    }
    None
}

/// The ContextInfo of whichever kind the message is.
fn context_of(m: &e2e::Message) -> Option<&e2e::ContextInfo> {
    m.extended_text_message
        .as_deref()
        .and_then(|x| x.context_info.as_deref())
        .or_else(|| m.image_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.video_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.ptv_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.audio_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.document_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.sticker_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.location_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.live_location_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.contact_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.contacts_array_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| kinds::poll(m).and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.event_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.group_invite_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.template_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.interactive_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.buttons_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.list_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.list_response_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.buttons_response_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.template_button_reply_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.interactive_response_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.rich_response_message.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.newsletter_follower_invite_message_v2.as_deref().and_then(|x| x.context_info.as_deref()))
        .or_else(|| m.newsletter_admin_invite_message.as_deref().and_then(|x| x.context_info.as_deref()))
}

fn status(status: Option<i32>) -> &'static str {
    use web_message_info::Status;
    match status.and_then(|s| Status::try_from(s).ok()) {
        Some(Status::Error) => "failed",
        Some(Status::Pending) => "pending",
        Some(Status::DeliveryAck) => "delivered",
        Some(Status::Read) => "read",
        Some(Status::Played) => "played",
        _ => "sent",
    }
}

/// What a message does to the one `key` names, in the same chat. The account's own amendments
/// name it by its JID, since reactions need someone.
fn amend(envelope: &Envelope, key: &MessageKey, what: Amendment, me: &Me) -> Option<Amend> {
    let target = key.id.clone().filter(|id| !id.is_empty())?;
    let mine = envelope.from.mine;
    Some(Amend {
        chat: envelope.chat.clone(),
        target,
        sender: if mine { me.jid().map(str::to_string) } else { envelope.from.jid.clone() },
        mine,
        at: envelope.at,
        what,
        raw: Some(Packed::new(&envelope.info.encode_to_vec())),
    })
}

/// An amendment jaw keeps in `pending` without applying it yet, so a later jawd can.
fn keep(envelope: &Envelope, key: &MessageKey, kind: &str, me: &Me) -> Option<Amend> {
    amend(envelope, key, Amendment::Keep { kind: kind.into() }, me)
}

fn reaction_amend(envelope: &Envelope, key: Option<&MessageKey>, emoji: Option<String>, me: &Me) -> Option<Amend> {
    amend(envelope, key?, Amendment::React { emoji: emoji.unwrap_or_default() }, me)
}

/// A reaction history keeps on the message it's for.
fn history_reaction(
    envelope: &Envelope,
    reaction: &wa_proto::wa_web_protobufs_web::Reaction,
    me: &Me,
) -> Option<Amend> {
    let key = reaction.key.as_ref()?;
    let mine = key.from_me.unwrap_or(false);
    let sender = if mine {
        me.jid().map(str::to_string)
    } else {
        key.participant
            .clone()
            .or_else(|| key.remote_jid.clone())
            .filter(|jid| !jid.is_empty())
            .map(|jid| user_jid(&jid))
    };
    Some(Amend {
        chat: envelope.chat.clone(),
        target: envelope.id.clone(),
        sender,
        mine,
        at: reaction.sender_timestamp_ms.unwrap_or(envelope.at),
        what: Amendment::React { emoji: reaction.text.clone().unwrap_or_default() },
        raw: None,
    })
}

/// An action of yours from another of your devices: its own ID never becomes a row, and noting it
/// as deleted for you keeps the receipts for it, which reach each of your devices, from waiting in
/// `pending` for a message that won't come.
fn own_action(envelope: &Envelope) -> Option<Change> {
    envelope.from.mine.then(|| {
        Change::Amend(Amend {
            chat: envelope.chat.clone(),
            target: envelope.id.clone(),
            sender: None,
            mine: true,
            at: envelope.at,
            what: Amendment::DeleteForMe,
            raw: None,
        })
    })
}

/// A message deleted for everyone, as history keeps it, with nothing of what it said: a copy jaw
/// has is deleted as a live deletion deletes it, and the row says who deleted it.
fn revoked(envelope: &Envelope) -> Vec<Change> {
    let summary = if envelope.from.mine { "You deleted this message" } else { "This message was deleted" };
    let revoke = Amend {
        chat: envelope.chat.clone(),
        target: envelope.id.clone(),
        sender: envelope.from.jid.clone(),
        mine: envelope.from.mine,
        at: envelope.at,
        what: Amendment::Revoke,
        raw: None,
    };
    let row = Message {
        chat: envelope.chat.clone(),
        id: envelope.id.clone(),
        at: envelope.at,
        sender: envelope.from.jid.clone(),
        mine: envelope.from.mine,
        kind: "text",
        deleted: true,
        summary: summary.into(),
        live: envelope.live,
        raw: Some(Packed::new(&envelope.info.encode_to_vec())),
        ..Message::default()
    };
    vec![Change::Amend(revoke), Change::Message(row)]
}

fn protocol_message(envelope: &Envelope, protocol: &e2e::ProtocolMessage, me: &Me, changes: &mut Vec<Change>) {
    use e2e::protocol_message::Type;
    let Some(kind) = protocol.r#type.and_then(|t| Type::try_from(t).ok()) else { return };
    match kind {
        Type::Revoke => {
            if let Some(amend) = protocol.key.as_ref().and_then(|key| amend(envelope, key, Amendment::Revoke, me)) {
                changes.push(Change::Amend(amend));
                changes.extend(own_action(envelope));
            }
        }
        Type::MessageEdit => {
            let Some(edited) = protocol.edited_message.as_deref() else { return };
            let (inner, wrapped) = unwrap(edited);
            let body = kinds::body(inner, &context_of(inner).map(|c| c.mentioned_jid.clone()).unwrap_or_default());
            let spans = body.text.as_deref().and_then(|_| spans::json(&body.spans));
            let edit = Amendment::Edit { text: body.text.clone(), spans: spans.clone(), summary: body.summary.clone() };
            let Some(mut amended) = protocol.key.as_ref().and_then(|key| amend(envelope, key, edit, me)) else {
                return;
            };
            let edited_at = protocol.timestamp_ms.unwrap_or(envelope.at);
            amended.at = edited_at;
            let shown = Message {
                chat: envelope.chat.clone(),
                id: amended.target.clone(),
                at: envelope.at,
                sender: envelope.from.jid.clone(),
                mine: envelope.from.mine,
                kind: body.kind,
                status: envelope.from.mine.then(|| status(envelope.info.status)),
                text: body.text,
                spans,
                summary: body.summary,
                detail: body.detail,
                edited_at: Some(edited_at),
                view_once: wrapped.view_once,
                live: envelope.live,
                raw: Some(Packed::new(&envelope.info.encode_to_vec())),
                ..Message::default()
            };
            changes.push(Change::Edited { amend: amended, shown: Box::new(shown) });
            changes.extend(own_action(envelope));
        }
        Type::EphemeralSetting => {
            let seconds = protocol.ephemeral_expiration.unwrap_or(0);
            changes.push(Change::Chat(Chat { id: envelope.chat.clone(), ephemeral: Some(seconds), ..Chat::default() }));
            changes.push(Change::Message(system(envelope, "system", &kinds::ephemeral_sentence(seconds))));
        }
        _ => {}
    }
}

/// A line in a chat that isn't anyone's message.
fn system(envelope: &Envelope, kind: &'static str, sentence: &str) -> Message {
    Message {
        chat: envelope.chat.clone(),
        id: envelope.id.clone(),
        at: envelope.at,
        sender: envelope.from.jid.clone(),
        mine: envelope.from.mine,
        kind,
        summary: sentence.to_string(),
        detail: Some(json!({ "system": sentence }).to_string()),
        live: envelope.live,
        raw: Some(Packed::new(&envelope.info.encode_to_vec())),
        ..Message::default()
    }
}

/// A history entry with no message, only a stub type: a group's changes, calls, notices.
fn stub_line(envelope: &Envelope, stub: i32) -> Message {
    use web_message_info::StubType;
    let parameters = &envelope.info.message_stub_parameters;
    let first = parameters.first().map(String::as_str).unwrap_or_default();
    let nth = |i: usize| parameters.get(i).map(|p| p.trim()).filter(|p| !p.is_empty());
    let (kind, sentence) = match StubType::try_from(stub).ok() {
        Some(StubType::Revoke) => ("system", "This message was deleted".to_string()),
        Some(StubType::Ciphertext) => ("system", "Waiting for this message".to_string()),
        Some(StubType::GroupCreate) => ("system", format!("Group created: {first}")),
        Some(StubType::GroupChangeSubject) => ("system", format!("Group name changed to {first}")),
        Some(StubType::GroupChangeIcon) => ("system", "Group picture changed".to_string()),
        Some(StubType::GroupChangeDescription) => ("system", "Group description changed".to_string()),
        Some(StubType::GroupChangeInviteLink) => ("system", "Group invite link reset".to_string()),
        Some(StubType::GroupParticipantAdd) => ("system", members_sentence("added", parameters)),
        Some(StubType::GroupParticipantRemove) => ("system", members_sentence("removed", parameters)),
        Some(StubType::GroupParticipantLeave) => ("system", members_sentence("left", parameters)),
        Some(StubType::GroupParticipantPromote) => ("system", members_sentence("made admin", parameters)),
        Some(StubType::GroupParticipantDemote) => ("system", members_sentence("no longer admin", parameters)),
        Some(StubType::GroupParticipantInvite) => ("system", members_sentence("joined by invite", parameters)),
        Some(StubType::GroupParticipantChangeNumber) => ("system", "A member changed their number".to_string()),
        Some(StubType::E2eEncrypted | StubType::E2eEncryptedNow) => {
            ("system", "Messages and calls are end-to-end encrypted".to_string())
        }
        Some(StubType::CallMissedVoice | StubType::CallMissedGroupVoice) => ("call", "Missed voice call".to_string()),
        Some(StubType::CallMissedVideo | StubType::CallMissedGroupVideo) => ("call", "Missed video call".to_string()),
        Some(StubType::ChangeEphemeralSetting) => {
            ("system", kinds::ephemeral_sentence(first.parse().unwrap_or_default()))
        }
        Some(StubType::GroupParticipantLinkedGroupJoin) => {
            ("system", members_sentence("joined from the community", parameters))
        }
        // Joining the community's general chat, its first parameter saying how.
        Some(StubType::GeneralChatAdd) if first == "linked_group_join" => {
            ("system", format!("{} joined from the community", who(envelope)))
        }
        Some(StubType::CommunityLinkSubGroup | StubType::CommunityLinkSiblingGroup) => {
            ("system", named(nth(1), "Group added to the community: ", "A group was added to the community"))
        }
        Some(StubType::CommunityUnlinkSubGroup | StubType::CommunityUnlinkSiblingGroup) => {
            ("system", named(nth(1), "Group removed from the community: ", "A group was removed from the community"))
        }
        Some(StubType::CommunityLinkParentGroup | StubType::CommunityLinkParentGroupRich) => {
            ("system", named(nth(1), "This group was added to the community ", "This group was added to a community"))
        }
        Some(StubType::CommunityUnlinkParentGroup) => (
            "system",
            named(nth(1), "This group was removed from the community ", "This group was removed from its community"),
        ),
        Some(StubType::CommunityParentGroupDeleted) => (
            "system",
            nth(0).map_or_else(
                || "The community was deactivated".into(),
                |name| format!("The community {name} was deactivated"),
            ),
        ),
        Some(StubType::CommunityCreate) => ("system", named(nth(1), "Community created: ", "Community created")),
        Some(StubType::CommunityParticipantPromote) => ("system", members_sentence("made community admin", parameters)),
        Some(StubType::CommunityParticipantDemote) => {
            ("system", members_sentence("no longer community admin", parameters))
        }
        Some(StubType::CommunityParticipantAddRich) => {
            ("system", named(nth(1), "You were added to the community ", "You were added to the community"))
        }
        Some(StubType::SubGroupParticipantAddRich) => {
            ("system", named(nth(1), "You were added to the group ", "You were added to a group"))
        }
        Some(StubType::CagMaskedThreadCreated) => ("system", "A thread was started".to_string()),
        Some(StubType::PinnedMessageInChat) => ("system", members_sentence("pinned a message", parameters)),
        Some(StubType::ChangeUsername) => ("system", username_sentence(parameters)),
        Some(
            StubType::BizPrivacyModeInitFb
            | StubType::BizPrivacyModeToFb
            | StubType::BizCoexPrivacyInit
            | StubType::BizCoexPrivacyTransition,
        ) => ("system", "This business uses a secure service from Meta to manage this chat".to_string()),
        Some(StubType::BizPrivacyModeInitBsp | StubType::BizPrivacyModeToBsp) => {
            ("system", "This business works with other companies to manage this chat".to_string())
        }
        Some(StubType::BlockContact) if first == "true" => ("system", "You blocked this contact".to_string()),
        Some(StubType::BlockContact) if first == "false" => ("system", "You unblocked this contact".to_string()),
        Some(StubType::DisappearingMode) if first.parse::<u32>().is_ok() => {
            ("system", kinds::ephemeral_sentence(first.parse().unwrap_or_default()))
        }
        Some(StubType::ChangeLimitSharing) if first == "true" => {
            ("system", "Advanced chat privacy turned on".to_string())
        }
        Some(StubType::ChangeLimitSharing) if first == "false" => {
            ("system", "Advanced chat privacy turned off".to_string())
        }
        Some(StubType::GroupMembershipJoinApprovalMode) if nth(1) == Some("true") => {
            ("system", "New members need admin approval to join".to_string())
        }
        Some(StubType::GroupMembershipJoinApprovalMode) if nth(1) == Some("false") => {
            ("system", "New members don't need admin approval to join".to_string())
        }
        Some(StubType::EventUpdated) => ("system", named(nth(0), "Event updated: ", "Event updated")),
        Some(StubType::EventCanceled) => ("system", named(nth(0), "Event cancelled: ", "Event cancelled")),
        Some(StubType::ScheduledMessageCreated) => ("system", "Message scheduled".to_string()),
        Some(StubType::BroadcastCreate) => (
            "system",
            match first.parse::<u32>() {
                Ok(1) => "Broadcast list created with 1 recipient".to_string(),
                Ok(n) => format!("Broadcast list created with {n} recipients"),
                Err(_) => "Broadcast list created".to_string(),
            },
        ),
        Some(StubType::IndividualChangeNumber) => ("system", "This contact changed their number".to_string()),
        _ => ("system", kinds::UNKNOWN.to_string()),
    };
    system(envelope, kind, &sentence)
}

/// `prefix` and the name a stub gives, or `without` when it gives none.
fn named(name: Option<&str>, prefix: &str, without: &str) -> String {
    name.map_or_else(|| without.to_string(), |name| format!("{prefix}{name}"))
}

/// Who a line's own event is by, until names are resolved: you, or their number.
fn who(envelope: &Envelope) -> String {
    if envelope.from.mine {
        return "You".into();
    }
    envelope.from.jid.as_deref().map_or_else(|| "Someone".into(), shown_jid)
}

/// A username set, changed or removed: the stub gives the old and the new username, the person's
/// JID, and their name as the phone showed it, which reads better than a LID.
fn username_sentence(parameters: &[String]) -> String {
    let nth = |i: usize| parameters.get(i).map(|p| p.trim()).filter(|p| !p.is_empty());
    let who = nth(3)
        .map(|name| name.trim_start_matches('~').trim())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| nth(2).map(shown_jid))
        .unwrap_or_else(|| "Someone".into());
    let handle = |name: &str| if name.starts_with('@') { name.to_string() } else { format!("@{name}") };
    match (nth(0), nth(1)) {
        (None, Some(new)) => format!("{who} set their username to {}", handle(new)),
        (Some(_), Some(new)) => format!("{who} changed their username to {}", handle(new)),
        (_, None) => format!("{who} removed their username"),
    }
}

fn members_sentence(what: &str, jids: &[String]) -> String {
    let who: Vec<String> = jids.iter().map(|jid| shown_jid(jid)).collect();
    match who.len() {
        0 => format!("Someone {what}"),
        _ => format!("{} {what}", who.join(", ")),
    }
}

/// A JID as a person reads it, until names are resolved: the number with a plus.
fn shown_jid(jid: &str) -> String {
    match jid.split_once('@') {
        Some((user, "s.whatsapp.net")) => format!("+{}", user.split(':').next().unwrap_or(user)),
        Some((user, _)) => user.to_string(),
        None => jid.to_string(),
    }
}

#[cfg(test)]
mod tests;
