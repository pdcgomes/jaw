use super::*;

/// 0 to 31, the secret the vectors below were sealed with.
fn secret() -> Vec<u8> {
    (0..32).collect()
}

const BETWEEN: Between = Between { id: "3EB0EVENT", author: "351912345678@s.whatsapp.net", who: "100000000002@lid" };
const IV: [u8; 12] = *b"jaw-event-iv";

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|at| u8::from_str_radix(&s[at..at + 2], 16).unwrap()).collect()
}

/// Sealed by whatsmeow's generateMsgSecretKey and gcmutil.Encrypt (msgsecret.go at df33a485cc46),
/// with the secret, the IV and those between them above: an answer, EventResponseMessage going
/// (08 01), authenticated with the event's ID and who answered, and an edit's nine bytes, with
/// nothing beside them.
const ANSWER: &str = "6bcd7b0f170a59d7d7e635cd7767af5a68cb";
const EDIT: &str = "4871b512a6ceb928c090cf27b86c69e0838cf7bad7b72d77f7";

#[test]
fn an_answer_and_an_edit_open_as_whatsmeow_seals_them_and_seal_as_it_does() {
    assert_eq!(open(&secret(), EVENT_RESPONSE, BETWEEN, &IV, &hex(ANSWER)), Some(vec![0x08, 0x01]));
    assert_eq!(seal(&secret(), EVENT_RESPONSE, BETWEEN, IV, &[0x08, 0x01]), hex(ANSWER));
    let edit = hex("a2010612044e6f766f");
    assert_eq!(open(&secret(), EVENT_EDIT, BETWEEN, &IV, &hex(EDIT)).as_ref(), Some(&edit));
    assert_eq!(seal(&secret(), EVENT_EDIT, BETWEEN, IV, &edit), hex(EDIT));
}

#[test]
fn a_change_opens_only_with_its_own_secret_message_people_and_use() {
    let sealed = hex(ANSWER);
    let opens = |secret: &[u8], use_case: &str, between: Between| open(secret, use_case, between, &IV, &sealed);
    assert!(opens(&[7; 32], EVENT_RESPONSE, BETWEEN).is_none(), "another secret");
    assert!(opens(&secret(), EVENT_EDIT, BETWEEN).is_none(), "another use");
    assert!(opens(&secret(), EVENT_RESPONSE, Between { id: "3EB0OTHER", ..BETWEEN }).is_none(), "another event");
    let author = Between { author: "100000000001@lid", ..BETWEEN };
    assert!(opens(&secret(), EVENT_RESPONSE, author).is_none(), "its author by another JID");
    let who = Between { who: "351913000000@s.whatsapp.net", ..BETWEEN };
    assert!(opens(&secret(), EVENT_RESPONSE, who).is_none(), "its answerer by another JID");
    assert!(open(&secret(), EVENT_RESPONSE, BETWEEN, &IV[..11], &sealed).is_none(), "an IV that isn't 12 bytes");
    let mut tampered = sealed.clone();
    tampered[0] ^= 1;
    assert!(open(&secret(), EVENT_RESPONSE, BETWEEN, &IV, &tampered).is_none(), "a change to what was sealed");
}

#[test]
fn a_messages_secret_is_found_beside_it_in_its_context_or_inside_its_wrappers() {
    let event = || e2e::Message { event_message: Some(Box::default()), ..Default::default() };
    let context = |secret: u8| e2e::MessageContextInfo { message_secret: Some(vec![secret; 32]), ..Default::default() };
    let history = WebMessageInfo { message_secret: Some(vec![1; 32]), message: Some(event()), ..Default::default() };
    assert_eq!(of(&history), Some(&[1; 32][..]));
    let live = WebMessageInfo {
        message: Some(e2e::Message { message_context_info: Some(context(2)), ..event() }),
        ..Default::default()
    };
    assert_eq!(of(&live), Some(&[2; 32][..]));
    let inner = e2e::Message { message_context_info: Some(context(3)), ..event() };
    let wrapped = WebMessageInfo {
        message: Some(e2e::Message {
            ephemeral_message: Some(Box::new(e2e::FutureProofMessage { message: Some(Box::new(inner)) })),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(of(&wrapped), Some(&[3; 32][..]));
    assert_eq!(of(&WebMessageInfo { message: Some(event()), ..Default::default() }), None, "none");
    let empty = WebMessageInfo { message_secret: Some(Vec::new()), message: Some(event()), ..Default::default() };
    assert_eq!(of(&empty), None, "an empty one is none");
}
