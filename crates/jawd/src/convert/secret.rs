//! What WhatsApp seals with a message's own secret: the answers to an event and its edits, as the
//! phones seal them (votes and reactions in announcements are sealed the same way, and the engine
//! opens those). A key is made from the secret the message carries, its ID, who sent it and who
//! made the change, and the change is sealed with it by AES-GCM. jawd opens answers and edits
//! itself, with the secret the event's raw protobuf keeps, so it reads them whichever way they came:
//! live, from history, or kept by an earlier jawd that couldn't.

use ring::{aead, hkdf};
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

/// What a change sealed with a message's secret is, as the key made for it says.
pub const EVENT_RESPONSE: &str = "Event Response";
pub const EVENT_EDIT: &str = "Event Edit";

/// Who a change is between, each a JID without its device: the message's `id` and its `author`, and
/// `who` changed it.
#[derive(Clone, Copy, Debug)]
pub struct Between<'a> {
    pub id: &'a str,
    pub author: &'a str,
    pub who: &'a str,
}

/// The key for `use_case` made from `secret`, as WhatsApp makes it: HKDF-SHA256 with no salt, its
/// info the message's ID, its author, who changed it and the use case.
fn key(secret: &[u8], use_case: &str, between: Between) -> aead::LessSafeKey {
    let info = [between.id.as_bytes(), between.author.as_bytes(), between.who.as_bytes(), use_case.as_bytes()].concat();
    let info = [info.as_slice()];
    let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, &[]).extract(secret);
    let okm = prk.expand(&info, &aead::AES_256_GCM).expect("HKDF-SHA256 gives an AES-256 key");
    aead::LessSafeKey::new(aead::UnboundKey::from(okm))
}

/// What's authenticated with a change beside it: for an answer, the message's ID and who answered.
fn additional(use_case: &str, between: Between) -> Vec<u8> {
    match use_case {
        EVENT_RESPONSE => format!("{}\0{}", between.id, between.who).into_bytes(),
        _ => Vec::new(),
    }
}

/// What `between.who` sealed for `use_case` with `secret`, `payload` with its tag opened with `iv`,
/// or None when it doesn't open with them, as it doesn't with any other key.
pub fn open(secret: &[u8], use_case: &str, between: Between, iv: &[u8], payload: &[u8]) -> Option<Vec<u8>> {
    let nonce = aead::Nonce::try_assume_unique_for_key(iv).ok()?;
    let mut sealed = payload.to_vec();
    let aad = aead::Aad::from(additional(use_case, between));
    let plain = key(secret, use_case, between).open_in_place(nonce, aad, &mut sealed).ok()?;
    Some(plain.to_vec())
}

/// `plain` sealed as `between.who` seals it for `use_case` with `secret` and `iv`, its tag after it:
/// the demo engine's people answer events with it.
pub fn seal(secret: &[u8], use_case: &str, between: Between, iv: [u8; 12], plain: &[u8]) -> Vec<u8> {
    let mut sealed = plain.to_vec();
    let aad = aead::Aad::from(additional(use_case, between));
    key(secret, use_case, between)
        .seal_in_place_append_tag(aead::Nonce::assume_unique_for_key(iv), aad, &mut sealed)
        .expect("AES-GCM seals what's this short");
    sealed
}

/// The secret message `info` carries: history keeps it beside the message, and a live one in the
/// context of the message as it came, or of the message inside its wrappers.
pub fn of(info: &WebMessageInfo) -> Option<&[u8]> {
    fn context(m: &e2e::Message) -> Option<&[u8]> {
        m.message_context_info.as_ref().and_then(|c| c.message_secret.as_deref()).filter(|s| !s.is_empty())
    }
    let outer = info.message.as_ref();
    info.message_secret
        .as_deref()
        .filter(|s| !s.is_empty())
        .or_else(|| outer.and_then(context))
        .or_else(|| outer.and_then(|m| context(super::unwrap(m).0)))
}

#[cfg(test)]
mod tests;
