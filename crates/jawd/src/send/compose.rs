//! Messages with a file, as WhatsApp's phones send them: a photo, a video, a GIF (a video that loops
//! without sound), an audio file or a document, with its caption, the message it answers and the
//! chat's disappearing timer; several photos and videos as an album, a message of its own that each
//! of them names; and an HD copy as a message of its own that names its SD one, as WhatsApp's dual
//! upload sends it. Each is made as `send.media` is asked, from what the file is and how it's
//! planned, and filled in as its copy is made (`made`) and uploaded (`uploaded`).

use jaw_media::{Plan, Probe};
use jaw_proto::media::AttachmentKind;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_e2e::message_association::AssociationType;

/// What a copy is once it's made: its type, hash and size, its size in pixels and its length as
/// shown, and the thumbnail the message carries.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Made {
    pub mime: String,
    pub sha256: Vec<u8>,
    pub length: u64,
    pub dimensions: Option<(u32, u32)>,
    pub seconds: Option<u32>,
    pub thumbnail: Option<Vec<u8>>,
}

/// Where a copy is on WhatsApp's media server, and the key it's encrypted with, made at `at`, in
/// seconds since the epoch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Up {
    pub url: String,
    pub direct_path: String,
    pub media_key: Vec<u8>,
    pub enc_sha256: Vec<u8>,
    pub at: i64,
}

/// A whole number of seconds, as WhatsApp gives a length, rounded up so a short clip isn't 0.
pub(crate) fn seconds(probe: &Probe) -> Option<u32> {
    probe.duration.map(|d| d.as_secs_f64().ceil() as u32).filter(|&s| s > 0)
}

/// The message a file goes as, before its copy is made: what `plan` says the copy will be, with its
/// caption and `context` (the message it answers, the chat's timer), and, for one of an album's
/// photos and videos, the album it's in.
pub(crate) fn file_message(
    kind: AttachmentKind,
    plan: &Plan,
    probe: &Probe,
    name: &str,
    caption: Option<&str>,
    context: Option<e2e::ContextInfo>,
    album: Option<&MessageKey>,
) -> e2e::Message {
    let caption = caption.map(str::trim).filter(|c| !c.is_empty()).map(str::to_string);
    let context = context.map(Box::new);
    let (width, height) = plan.dimensions.map_or((None, None), |(w, h)| (Some(w), Some(h)));
    let mime = Some(plan.mime.clone());
    let mut message = match kind {
        AttachmentKind::Image => e2e::Message {
            image_message: Some(Box::new(e2e::ImageMessage {
                mimetype: mime,
                caption,
                width,
                height,
                context_info: context,
                ..Default::default()
            })),
            ..Default::default()
        },
        AttachmentKind::Video | AttachmentKind::Gif => e2e::Message {
            video_message: Some(Box::new(e2e::VideoMessage {
                mimetype: mime,
                caption,
                width,
                height,
                seconds: seconds(probe),
                gif_playback: (kind == AttachmentKind::Gif || plan.gif_playback).then_some(true),
                context_info: context,
                ..Default::default()
            })),
            ..Default::default()
        },
        AttachmentKind::Audio => e2e::Message {
            audio_message: Some(Box::new(e2e::AudioMessage {
                mimetype: mime,
                seconds: seconds(probe),
                ptt: Some(false),
                context_info: context,
                ..Default::default()
            })),
            ..Default::default()
        },
        AttachmentKind::Document => {
            let document = e2e::DocumentMessage {
                mimetype: mime,
                file_name: Some(name.to_string()),
                title: Some(name.to_string()),
                caption: caption.clone(),
                context_info: context,
                ..Default::default()
            };
            let document = e2e::Message { document_message: Some(Box::new(document)), ..Default::default() };
            // A document with a caption goes inside a wrapper of its own, as the phones send one.
            match caption {
                Some(_) => e2e::Message {
                    document_with_caption_message: Some(Box::new(e2e::FutureProofMessage {
                        message: Some(Box::new(document)),
                    })),
                    ..Default::default()
                },
                None => document,
            }
        }
    };
    if let Some(album) = album {
        message.message_context_info = Some(associated(AssociationType::MediaAlbum, album));
    }
    message
}

/// How a voice message's file goes: Ogg Opus, as WhatsApp's phones record one and jaw-voice does.
pub(crate) const VOICE_MIME: &str = "audio/ogg; codecs=opus";

/// A voice message, as WhatsApp's phones send one, before its copy is made: an audio message
/// that's a voice note (PTT), Ogg Opus, `seconds` long rounded up as WhatsApp gives a length, with
/// the recording's 64-point `waveform` and `context` (the chat's timer).
pub(crate) fn voice_message(seconds: f64, waveform: &[u8], context: Option<e2e::ContextInfo>) -> e2e::Message {
    e2e::Message {
        audio_message: Some(Box::new(e2e::AudioMessage {
            mimetype: Some(VOICE_MIME.into()),
            seconds: Some(seconds.ceil().max(1.0) as u32),
            ptt: Some(true),
            waveform: Some(waveform.to_vec()),
            context_info: context.map(Box::new),
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Whether `caption` goes with a file of `kind`, as `file_message` sends it: trimmed, it's
/// something, and the kind's message has a caption, which an audio file's hasn't.
pub(crate) fn caption_goes(kind: AttachmentKind, caption: Option<&str>) -> bool {
    let carries = match kind {
        AttachmentKind::Image | AttachmentKind::Video | AttachmentKind::Gif | AttachmentKind::Document => true,
        AttachmentKind::Audio => false,
    };
    carries && caption.is_some_and(|c| !c.trim().is_empty())
}

/// The album several photos and videos go in: a message of its own, sent first, saying how many of
/// each are coming, which each then names.
pub(crate) fn album_message(photos: u32, videos: u32, context: Option<e2e::ContextInfo>) -> e2e::Message {
    e2e::Message {
        album_message: Some(Box::new(e2e::AlbumMessage {
            expected_image_count: Some(photos),
            expected_video_count: Some(videos),
            context_info: context.map(Box::new),
        })),
        ..Default::default()
    }
}

/// The HD copy of the photo or video `sd` is, as a message of its own: what `plan` says the copy
/// will be, naming its SD message, `parent`.
pub(crate) fn hd_message(sd: &e2e::Message, plan: &Plan, parent: &MessageKey) -> Option<e2e::Message> {
    let (width, height) = plan.dimensions.map_or((None, None), |(w, h)| (Some(w), Some(h)));
    let mut hd = e2e::Message::default();
    let association = if let Some(image) = sd.image_message.as_deref() {
        hd.image_message = Some(Box::new(e2e::ImageMessage {
            mimetype: Some(plan.mime.clone()),
            caption: image.caption.clone(),
            width,
            height,
            ..Default::default()
        }));
        AssociationType::HdImageDualUpload
    } else {
        let video = sd.video_message.as_deref()?;
        hd.video_message = Some(Box::new(e2e::VideoMessage {
            mimetype: Some(plan.mime.clone()),
            caption: video.caption.clone(),
            width,
            height,
            seconds: video.seconds,
            ..Default::default()
        }));
        AssociationType::HdVideoDualUpload
    };
    hd.message_context_info = Some(associated(association, parent));
    Some(e2e::Message {
        associated_child_message: Some(Box::new(e2e::FutureProofMessage { message: Some(Box::new(hd)) })),
        ..Default::default()
    })
}

fn associated(association: AssociationType, parent: &MessageKey) -> e2e::MessageContextInfo {
    e2e::MessageContextInfo {
        message_association: Some(e2e::MessageAssociation {
            association_type: Some(association as i32),
            parent_message_key: Some(parent.clone()),
            message_index: None,
        }),
        ..Default::default()
    }
}

/// The file a message carries, out of the wrappers a document with a caption and an HD copy go in.
enum File<'a> {
    Image(&'a mut e2e::ImageMessage),
    Video(&'a mut e2e::VideoMessage),
    Audio(&'a mut e2e::AudioMessage),
    Document(&'a mut e2e::DocumentMessage),
}

fn file_of(message: &mut e2e::Message) -> Option<File<'_>> {
    if message.document_with_caption_message.is_some() || message.associated_child_message.is_some() {
        let wrapper =
            message.document_with_caption_message.as_deref_mut().or(message.associated_child_message.as_deref_mut())?;
        return file_of(wrapper.message.as_deref_mut()?);
    }
    if let Some(x) = message.image_message.as_deref_mut() {
        return Some(File::Image(x));
    }
    if let Some(x) = message.video_message.as_deref_mut() {
        return Some(File::Video(x));
    }
    if let Some(x) = message.audio_message.as_deref_mut() {
        return Some(File::Audio(x));
    }
    message.document_message.as_deref_mut().map(File::Document)
}

/// Fills in what a message says of its file once its copy is made. False if it carries no file.
pub(crate) fn made(message: &mut e2e::Message, made: &Made) -> bool {
    let (width, height) = made.dimensions.map_or((None, None), |(w, h)| (Some(w), Some(h)));
    let (mime, sha256, length) = (Some(made.mime.clone()), Some(made.sha256.clone()), Some(made.length));
    let thumbnail = made.thumbnail.clone().filter(|t| !t.is_empty());
    match file_of(message) {
        Some(File::Image(x)) => {
            (x.mimetype, x.file_sha256, x.file_length) = (mime, sha256, length);
            (x.width, x.height) = (width.or(x.width), height.or(x.height));
            x.jpeg_thumbnail = thumbnail;
        }
        Some(File::Video(x)) => {
            (x.mimetype, x.file_sha256, x.file_length) = (mime, sha256, length);
            (x.width, x.height) = (width.or(x.width), height.or(x.height));
            x.seconds = made.seconds.or(x.seconds);
            x.jpeg_thumbnail = thumbnail;
        }
        Some(File::Audio(x)) => {
            (x.mimetype, x.file_sha256, x.file_length) = (mime, sha256, length);
            x.seconds = made.seconds.or(x.seconds);
        }
        Some(File::Document(x)) => {
            (x.mimetype, x.file_sha256, x.file_length) = (mime, sha256, length);
            x.jpeg_thumbnail = thumbnail;
        }
        None => return false,
    }
    true
}

/// Fills in where a message's file is on WhatsApp's media server, once it's uploaded. False if it
/// carries no file.
pub(crate) fn uploaded(message: &mut e2e::Message, up: &Up) -> bool {
    let (url, path, key, enc, at) = (
        Some(up.url.clone()),
        Some(up.direct_path.clone()),
        Some(up.media_key.clone()),
        Some(up.enc_sha256.clone()),
        Some(up.at),
    );
    match file_of(message) {
        Some(File::Image(x)) => {
            (x.url, x.direct_path, x.media_key, x.file_enc_sha256, x.media_key_timestamp) = (url, path, key, enc, at);
        }
        Some(File::Video(x)) => {
            (x.url, x.direct_path, x.media_key, x.file_enc_sha256, x.media_key_timestamp) = (url, path, key, enc, at);
        }
        Some(File::Audio(x)) => {
            (x.url, x.direct_path, x.media_key, x.file_enc_sha256, x.media_key_timestamp) = (url, path, key, enc, at);
        }
        Some(File::Document(x)) => {
            (x.url, x.direct_path, x.media_key, x.file_enc_sha256, x.media_key_timestamp) = (url, path, key, enc, at);
        }
        None => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert;
    use crate::store::Change;
    use jaw_media::{Action, Kind, Quality};
    use wa_proto::wa_web_protobufs_web::{WebMessageInfo, web_message_info};

    const ANA: &str = "351912345678@s.whatsapp.net";

    fn plan(kind: Kind, mime: &str, dimensions: Option<(u32, u32)>) -> Plan {
        Plan {
            quality: Quality::Sd,
            kind,
            action: Action::Untouched,
            mime: mime.into(),
            extension: "jpg",
            dimensions,
            gif_playback: false,
        }
    }

    fn probe(kind: Kind, seconds: Option<f64>) -> Probe {
        let mut probe = Probe::new(kind, jaw_media::Format::Other, "application/octet-stream", 0);
        probe.duration = seconds.map(std::time::Duration::from_secs_f64);
        probe
    }

    /// The row a message of yours would be, as the converter reads it.
    fn row(message: &e2e::Message) -> crate::store::Message {
        let web = WebMessageInfo {
            key: MessageKey {
                remote_jid: Some(ANA.into()),
                from_me: Some(true),
                id: Some("3EB0AA".into()),
                participant: None,
            },
            message: Some(message.clone()),
            message_timestamp: Some(1_792_000_000),
            status: Some(web_message_info::Status::Pending as i32),
            ..Default::default()
        };
        match convert::message(&web, &convert::Me::default(), true)
            .into_iter()
            .find(|c| matches!(c, Change::Message(_)))
        {
            Some(Change::Message(message)) => message,
            other => panic!("no row: {other:?}"),
        }
    }

    #[test]
    fn each_kind_goes_as_the_phones_send_it_and_reads_back_as_its_kind() {
        let photo = file_message(
            AttachmentKind::Image,
            &plan(Kind::Photo, "image/jpeg", Some((1600, 1200))),
            &probe(Kind::Photo, None),
            "lake.jpg",
            Some(" the lake "),
            None,
            None,
        );
        let row_of = row(&photo);
        assert_eq!((row_of.kind, row_of.text.as_deref()), ("image", Some("the lake")));
        let gif = file_message(
            AttachmentKind::Gif,
            &plan(Kind::Gif, "video/mp4", Some((480, 270))),
            &probe(Kind::Gif, Some(2.2)),
            "dance.gif",
            None,
            None,
            None,
        );
        assert_eq!(gif.video_message.as_ref().map(|v| (v.gif_playback, v.seconds)), Some((Some(true), Some(3))));
        assert_eq!(row(&gif).kind, "gif");
        let video = file_message(
            AttachmentKind::Video,
            &plan(Kind::Video, "video/mp4", Some((848, 480))),
            &probe(Kind::Video, Some(12.0)),
            "clip.mov",
            None,
            None,
            None,
        );
        assert_eq!(row(&video).kind, "video");
        let audio = file_message(
            AttachmentKind::Audio,
            &plan(Kind::Audio, "audio/mp4", None),
            &probe(Kind::Audio, Some(61.5)),
            "song.mp3",
            None,
            None,
            None,
        );
        assert_eq!(audio.audio_message.as_ref().map(|a| (a.ptt, a.seconds)), Some((Some(false), Some(62))));
        assert_eq!(row(&audio).kind, "audio");
        let captioned = file_message(
            AttachmentKind::Document,
            &plan(Kind::Document, "application/pdf", None),
            &probe(Kind::Document, None),
            "rent.pdf",
            Some("for March"),
            None,
            None,
        );
        assert!(captioned.document_with_caption_message.is_some(), "a captioned document goes in its wrapper");
        let document = row(&captioned);
        assert_eq!((document.kind, document.text.as_deref()), ("document", Some("for March\nrent.pdf")));
        let plain = file_message(
            AttachmentKind::Document,
            &plan(Kind::Document, "application/pdf", None),
            &probe(Kind::Document, None),
            "rent.pdf",
            None,
            None,
            None,
        );
        assert!(plain.document_message.is_some());
    }

    #[test]
    fn a_caption_goes_once_trimmed_with_each_kind_whose_message_has_one_and_never_with_audio() {
        let kinds = [
            (AttachmentKind::Image, Kind::Photo, "image/jpeg"),
            (AttachmentKind::Video, Kind::Video, "video/mp4"),
            (AttachmentKind::Gif, Kind::Gif, "video/mp4"),
            (AttachmentKind::Audio, Kind::Audio, "audio/mp4"),
            (AttachmentKind::Document, Kind::Document, "application/pdf"),
        ];
        for (kind, of, mime) in kinds {
            for caption in [None, Some(""), Some(" \n "), Some(" the lake ")] {
                let message = file_message(kind, &plan(of, mime, None), &probe(of, None), "lake", caption, None, None);
                let wrapped = message.document_with_caption_message.as_ref().and_then(|w| w.message.as_deref());
                let carried = message.image_message.as_ref().is_some_and(|m| m.caption.is_some())
                    || message.video_message.as_ref().is_some_and(|m| m.caption.is_some())
                    || message.document_message.as_ref().is_some_and(|m| m.caption.is_some())
                    || wrapped.and_then(|m| m.document_message.as_ref()).is_some_and(|m| m.caption.is_some());
                assert_eq!(caption_goes(kind, caption), carried, "{kind:?} with {caption:?}");
            }
        }
    }

    #[test]
    fn a_copy_made_and_uploaded_fills_in_what_its_message_says_of_its_file() {
        let album = MessageKey {
            remote_jid: Some(ANA.into()),
            from_me: Some(true),
            id: Some("3EB0AL".into()),
            participant: None,
        };
        let mut photo = file_message(
            AttachmentKind::Image,
            &plan(Kind::Photo, "image/jpeg", Some((1600, 1200))),
            &probe(Kind::Photo, None),
            "lake.jpg",
            None,
            None,
            Some(&album),
        );
        let made_ = Made {
            mime: "image/jpeg".into(),
            sha256: vec![7; 32],
            length: 123_456,
            dimensions: Some((1600, 1199)),
            seconds: None,
            thumbnail: Some(vec![0xFF, 0xD8]),
        };
        assert!(made(&mut photo, &made_));
        let up = Up {
            url: "https://mmg.whatsapp.net/x".into(),
            direct_path: "/v/x".into(),
            media_key: vec![1; 32],
            enc_sha256: vec![2; 32],
            at: 1_792_000_000,
        };
        assert!(uploaded(&mut photo, &up));
        let image = photo.image_message.as_deref().unwrap();
        assert_eq!(
            (image.file_sha256.as_deref(), image.file_length, image.height, image.jpeg_thumbnail.as_deref()),
            (Some(&[7; 32][..]), Some(123_456), Some(1199), Some(&[0xFF, 0xD8][..]))
        );
        assert_eq!((image.direct_path.as_deref(), image.media_key_timestamp), (Some("/v/x"), Some(1_792_000_000)));
        let association = photo.message_context_info.unwrap().message_association.unwrap();
        assert_eq!(association.association_type, Some(AssociationType::MediaAlbum as i32));
        assert_eq!(association.parent_message_key.and_then(|k| k.id).as_deref(), Some("3EB0AL"));
        let mut text = e2e::Message { conversation: Some("no file".into()), ..Default::default() };
        assert!(!made(&mut text, &made_) && !uploaded(&mut text, &up));
    }

    #[test]
    fn an_hd_copy_names_its_sd_message_as_the_converter_reads_one() {
        let sd = file_message(
            AttachmentKind::Image,
            &plan(Kind::Photo, "image/jpeg", Some((1600, 1200))),
            &probe(Kind::Photo, None),
            "lake.jpg",
            None,
            None,
            None,
        );
        let parent = MessageKey {
            remote_jid: Some(ANA.into()),
            from_me: Some(true),
            id: Some("3EB0SD".into()),
            participant: None,
        };
        let mut hd = hd_message(&sd, &plan(Kind::Photo, "image/jpeg", Some((4032, 3024))), &parent).unwrap();
        let made_ = Made { mime: "image/jpeg".into(), sha256: vec![9; 32], length: 3_000_000, ..Made::default() };
        assert!(made(&mut hd, &made_), "a copy inside its wrapper is filled in too");
        let web = WebMessageInfo {
            key: MessageKey {
                remote_jid: Some(ANA.into()),
                from_me: Some(true),
                id: Some("3EB0HD".into()),
                participant: None,
            },
            message: Some(hd),
            message_timestamp: Some(1_792_000_000),
            ..Default::default()
        };
        let changes = convert::message(&web, &convert::Me::default(), true);
        let amend = changes.iter().find_map(|c| match c {
            Change::Amend(a) => Some(a),
            _ => None,
        });
        let amend = amend.expect("an HD copy amends its message");
        assert_eq!(amend.target, "3EB0SD");
        assert!(
            matches!(&amend.what, crate::store::Amendment::Hd { copy } if copy.w == Some(4032) && copy.sha256.as_deref() == Some(&[9; 32][..]))
        );
        let audio = file_message(
            AttachmentKind::Audio,
            &plan(Kind::Audio, "audio/mp4", None),
            &probe(Kind::Audio, None),
            "a.m4a",
            None,
            None,
            None,
        );
        assert!(hd_message(&audio, &plan(Kind::Audio, "audio/mp4", None), &parent).is_none(), "only photos and videos");
    }
}
