use std::path::Path;
use std::time::Duration;

use jaw_proto::code;
use rusqlite::Connection;
use serde_json::json;
use tokio::sync::{broadcast, mpsc};

use super::*;
use crate::engine::demo::{self, DemoEngine, Replies, World};
use crate::engine::pb;
use crate::engine::supervisor::{Backoff, Supervisor};
use crate::send::{Sending, Timing, link};
use crate::store::{Chat, Lane, Store, read};
use crate::work::Priority;

const CLIMBING: &str = "120363012345678901@g.us";

/// jawd's sending on a jaw.db in `home`, through the demo engine, connected, with the climbing
/// group's chat.
async fn sending(home: &Path) -> (Store, Sending, Arc<World>) {
    let store = Store::open(home).unwrap();
    let live: crate::sync::Shared = Arc::new(std::sync::Mutex::new(crate::sync::starting(&store, true)));
    let (pushes, _) = broadcast::channel(256);
    let (events, incoming) = mpsc::channel(1024);
    let world: Arc<World> = Arc::default();
    world.replying(Replies { delivered: Duration::from_millis(5), read: Duration::from_millis(5) });
    let engine = DemoEngine::sharing(vec![demo::connected()], world.clone());
    let supervisor = Supervisor::spawn(Arc::new(engine), events, Backoff::default());
    let mut connected = pushes.subscribe();
    let sync = crate::sync::run(
        incoming,
        store.clone(),
        live.clone(),
        pushes.clone(),
        Some(supervisor.clone()),
        home.into(),
        Default::default(),
        crate::feed::Typing::default(),
    );
    tokio::spawn(sync);
    let attention = Arc::new(crate::attention::Attention::default());
    let timing =
        Timing { retry_first: Duration::from_millis(5), retry_most: Duration::from_millis(20), ..Timing::default() };
    let sending = Sending::start(
        store.clone(),
        Some(supervisor),
        live,
        &pushes,
        attention.focus(),
        timing,
        link::Previews::off(),
    )
    .await
    .unwrap();
    let online = async {
        while let Ok(push) = connected.recv().await {
            if push.method == "connection" && push.params["state"] == "online" {
                return;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(10), online).await.expect("the demo engine never connected");
    let changes = vec![
        Change::Chat(Chat { id: CLIMBING.into(), ..Chat::default() }),
        Change::Remember { key: "me".into(), value: demo::ME.into() },
    ];
    store.apply(Lane::Live, changes).await.unwrap();
    (store, sending, world)
}

async fn query<T: Send + 'static>(
    store: &Store,
    query: impl FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
) -> T {
    store.reads().query(Priority::Interactive, query).await.unwrap()
}

/// What the demo engine was asked to send, once `n` sends have gone.
async fn sent(world: &World, n: usize) -> Vec<e2e::Message> {
    world.until(|asked| asked.iter().filter(|kind| matches!(kind, pb::command::Kind::Send(_))).count() >= n).await;
    world.sends().into_iter().map(|send| e2e::Message::decode(send.raw.as_slice()).unwrap()).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_location_goes_as_a_pin_with_its_name_and_address_and_shows_at_once() {
    let home = tempfile::tempdir().unwrap();
    let (store, sending, world) = sending(home.path()).await;
    let params = json!({
        "chat": CLIMBING, "lat": 38.7118, "lon": -9.1301,
        "name": " Miradouro de Santa Luzia ", "address": "Largo de Santa Luzia, Lisboa",
    });
    let answer = sending.call("send.location", params).await.unwrap();
    let row = &answer["row"];
    assert_eq!((&row["kind"], &row["status"], &row["spans"]), (&json!("location"), &json!("pending"), &json!([])));
    assert_eq!(
        row["location"],
        json!({
            "lat": 38.7118, "lon": -9.1301, "name": "Miradouro de Santa Luzia",
            "address": "Largo de Santa Luzia, Lisboa", "url": null,
        }),
        "shown at once, trimmed"
    );
    let pin = sending.call("send.location", json!({ "chat": CLIMBING, "lat": -33.9, "lon": 151.2 })).await.unwrap();
    assert_eq!(pin["row"]["location"]["name"], json!(null), "a pin alone");
    let sent = sent(&world, 2).await;
    let place = sent[0].location_message.as_ref().expect("a location goes as one");
    assert_eq!((place.degrees_latitude, place.degrees_longitude), (Some(38.7118), Some(-9.1301)));
    assert_eq!(place.name.as_deref(), Some("Miradouro de Santa Luzia"));
    assert_eq!(place.address.as_deref(), Some("Largo de Santa Luzia, Lisboa"));
    assert!(place.context_info.is_none(), "with no timer in a chat without one");
    assert_eq!(sent[1].location_message.as_ref().unwrap().name, None);
    store.indexed().await.unwrap();
    let found = query(&store, |db| read::search(db, "largo luzia", None, None, 10)).await;
    assert_eq!(found.hits.len(), 1, "found by its address");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_location_that_cant_be_one_is_refused_and_nothing_of_it_kept() {
    let home = tempfile::tempdir().unwrap();
    let (store, sending, world) = sending(home.path()).await;
    let at = |lat: f64, lon: f64| json!({ "chat": CLIMBING, "lat": lat, "lon": lon });
    let named = |name: String| json!({ "chat": CLIMBING, "lat": 1.0, "lon": 1.0, "name": name });
    let refused = [
        (at(90.5, 0.0), code::INVALID_PARAMS, "past the pole"),
        (at(0.0, -180.5), code::INVALID_PARAMS, "past the date line"),
        (json!({ "chat": CLIMBING, "lat": 1.0 }), code::INVALID_PARAMS, "no longitude"),
        (named("a".repeat(1_025)), code::LIMIT, "a name too long"),
        (
            json!({ "chat": CLIMBING, "lat": 1.0, "lon": 1.0, "address": "a".repeat(1_025) }),
            code::LIMIT,
            "an address too long",
        ),
        (
            json!({ "chat": "351919999999@s.whatsapp.net", "lat": 1.0, "lon": 1.0 }),
            code::NOT_FOUND,
            "a chat jaw hasn't",
        ),
    ];
    for (params, code, why) in refused {
        let error = sending.call("send.location", params).await.expect_err(why);
        assert_eq!(error.code, code, "{why}: {}", error.message);
    }
    assert!(sending.call("send.location", at(90.0, -180.0)).await.is_ok(), "the pole, on the date line");
    assert!(sending.call("send.location", named("a".repeat(1_024))).await.is_ok(), "1,024 characters");
    assert_eq!(store.count("messages").unwrap(), 2, "only those kept");
    assert_eq!(sent(&world, 2).await.len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_location_asked_again_with_its_key_goes_once_and_in_a_chat_with_disappearing_messages_with_its_timer() {
    let home = tempfile::tempdir().unwrap();
    let (store, sending, world) = sending(home.path()).await;
    store
        .apply(Lane::Live, vec![Change::Chat(Chat { id: CLIMBING.into(), ephemeral: Some(86_400), ..Chat::default() })])
        .await
        .unwrap();
    let pin = json!({ "chat": CLIMBING, "lat": 38.7, "lon": -9.1, "key": "jaw-pin-1" });
    let (a, b) = tokio::join!(sending.call("send.location", pin.clone()), sending.call("send.location", pin.clone()));
    let first = a.unwrap();
    assert_eq!(first["message"], b.unwrap()["message"]);
    let again = sending.call("send.location", pin).await.unwrap();
    assert_eq!((&again["message"], &again["row"]["kind"]), (&first["message"], &json!("location")));
    let sent = sent(&world, 1).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!((world.sends().len(), store.count("messages").unwrap()), (1, 1), "it went once");
    let timer = sent[0].location_message.as_ref().unwrap().context_info.as_ref().and_then(|c| c.expiration);
    assert_eq!(timer, Some(86_400), "with the chat's timer");
}
