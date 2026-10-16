use prost::Message as _;
use wa_proto::jaw::engine::{Frame, Hello, frame};
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e::{ContextInfo, ExtendedTextMessage, Message};

#[test]
fn a_whatsapp_message_survives_encoding() {
    let message = Message {
        extended_text_message: Some(Box::new(ExtendedTextMessage {
            text: Some("on my way".into()),
            context_info: Some(Box::new(ContextInfo {
                stanza_id: Some("3EB0A1B2C3D4E5F6".into()),
                ..Default::default()
            })),
            ..Default::default()
        })),
        ..Default::default()
    };
    let decoded = Message::decode(message.encode_to_vec().as_slice()).unwrap();
    assert_eq!(decoded, message);
    let reply = decoded.extended_text_message.unwrap();
    assert_eq!(reply.text.as_deref(), Some("on my way"));
    assert_eq!(reply.context_info.unwrap().stanza_id.as_deref(), Some("3EB0A1B2C3D4E5F6"));
}

#[test]
fn a_message_key_from_another_package_is_shared() {
    let key = MessageKey {
        remote_jid: Some("351912345678@s.whatsapp.net".into()),
        from_me: Some(true),
        ..Default::default()
    };
    assert_eq!(MessageKey::decode(key.encode_to_vec().as_slice()).unwrap(), key);
}

#[test]
fn the_engine_hello_frame() {
    let frame = Frame {
        id: 0,
        body: Some(frame::Body::Hello(Hello {
            protocol: 1,
            program: "jaw-engine 0.1.0".into(),
            whatsmeow: "v0.0.0-20261015105041-df33a485cc46".into(),
        })),
    };
    assert_eq!(Frame::decode(frame.encode_to_vec().as_slice()).unwrap(), frame);
}
