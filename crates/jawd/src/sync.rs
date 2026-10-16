//! What jawd does with the engine's events. Each message is kept in the inbox, and acked once it's
//! committed; history is fetched a blob at a time when the phone offers it; linking and the
//! connection's state go out to interfaces. Converting messages into what interfaces show comes
//! with the store, in M1.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use jaw_proto::{Account, AccountState, CatchUp, Connection, ConnectionState, HistoryProgress, LinkStep, Me, Push};
use prost::Message as _;
use tokio::sync::{broadcast, mpsc};
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_history_sync as history_sync;

use crate::engine::supervisor::Supervisor;
use crate::engine::{Incoming, pb};
use crate::store::{HistoryRow, Store};

/// The account's and the connection's state, as interfaces see them.
pub struct Live {
    pub account: Account,
    pub connection: Connection,
    pub catch_up: Option<CatchUp>,
    catching_up: Option<Instant>,
    pub history: Option<HistoryProgress>,
}

impl Default for Live {
    fn default() -> Self {
        Live {
            account: Account { state: AccountState::Unlinked, me: None },
            connection: Connection::new(ConnectionState::Offline),
            catch_up: None,
            catching_up: None,
            history: None,
        }
    }
}

pub type Shared = Arc<Mutex<Live>>;

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}

fn push(pushes: &broadcast::Sender<Push>, method: &str, params: impl serde::Serialize) {
    let params = serde_json::to_value(params).expect("jawd's own types serialize");
    let _ = pushes.send(Push { method: method.into(), params });
}

/// Handles the engine's events until it stops sending them.
pub async fn run(
    mut incoming: mpsc::Receiver<Incoming>,
    store: Store,
    live: Shared,
    pushes: broadcast::Sender<Push>,
    engine: Option<Supervisor>,
    spool: PathBuf,
) {
    let (offered, offers) = mpsc::unbounded_channel();
    if let Some(engine) = engine {
        tokio::spawn(fetch_history(offers, store.clone(), engine, live.clone(), spool));
    }
    while let Some(Incoming { event, ack }) = incoming.recv().await {
        match event.kind {
            Some(pb::event::Kind::Message(message)) => {
                let info = message.info.clone().unwrap_or_default();
                if let Some(notification) = history_notification(&message.raw) {
                    let _ = offered.send((info.id.clone(), notification));
                }
                if let Some(catch_up) = live.lock().expect("live state").catch_up.as_mut()
                    && catch_up.seconds.is_none()
                {
                    catch_up.stored += 1;
                }
                store.inbox("message", info.timestamp_ms, message.encode_to_vec(), ack);
            }
            Some(pb::event::Kind::Bench(bench)) => store.inbox("bench", now_ms(), bench.payload, ack),
            Some(pb::event::Kind::Connection(connection)) => {
                let connection = {
                    let mut live = live.lock().expect("live state");
                    apply(&mut live, &connection);
                    live.connection.clone()
                };
                push(&pushes, "connection", connection);
            }
            Some(pb::event::Kind::Link(link)) => {
                if let Some(step) = link_step(link) {
                    push(&pushes, "link", step);
                }
            }
            Some(pb::event::Kind::OfflinePreview(preview)) => {
                let connection = {
                    let mut live = live.lock().expect("live state");
                    live.catch_up = Some(CatchUp {
                        total: preview.total.into(),
                        messages: preview.messages.into(),
                        receipts: preview.receipts.into(),
                        stored: 0,
                        seconds: None,
                    });
                    live.catching_up = Some(Instant::now());
                    live.connection.state = ConnectionState::CatchingUp;
                    live.connection.total = Some(preview.messages.into());
                    live.connection.done = Some(0);
                    live.connection.clone()
                };
                push(&pushes, "connection", connection);
            }
            Some(pb::event::Kind::OfflineDone(done)) => {
                let connection = {
                    let mut live = live.lock().expect("live state");
                    let took = live.catching_up.take().map(|started| started.elapsed().as_secs_f64());
                    if let (Some(catch_up), Some(seconds)) = (live.catch_up.as_mut(), took) {
                        catch_up.seconds = Some(seconds);
                        eprintln!(
                            "jawd: caught up: {} messages stored of {} waiting ({} things in all, {} said done) in {seconds:.2} s, {:.0} messages a second",
                            catch_up.stored,
                            catch_up.messages,
                            catch_up.total,
                            done.count,
                            catch_up.stored as f64 / seconds.max(1e-6)
                        );
                    }
                    live.connection = Connection::new(ConnectionState::Online);
                    live.connection.clone()
                };
                push(&pushes, "connection", connection);
            }
            Some(pb::event::Kind::Receipt(_)) | None => {}
        }
    }
}

/// The account and connection as a Connection event says they are now.
fn apply(live: &mut Live, connection: &pb::Connection) {
    use pb::connection::State;
    let reason = (!connection.reason.is_empty()).then(|| connection.reason.clone());
    let state = match connection.state() {
        State::Unspecified | State::Disconnected | State::StreamReplaced | State::ConnectFailed => {
            ConnectionState::Offline
        }
        State::Unlinked => ConnectionState::Offline,
        State::Connecting | State::KeepaliveTimeout => ConnectionState::Connecting,
        State::Connected => ConnectionState::Online,
        State::LoggedOut => ConnectionState::LoggedOut,
        State::ClientOutdated => ConnectionState::UpdateNeeded,
        State::TemporaryBan => ConnectionState::Banned,
    };
    if matches!(connection.state(), State::Unlinked | State::LoggedOut) {
        live.account = Account { state: AccountState::Unlinked, me: None };
    } else if !connection.jid.is_empty() {
        let phone = connection.jid.split(['@', ':', '.']).next().unwrap_or_default();
        live.account = Account {
            state: AccountState::Linked,
            me: Some(Me { id: connection.jid.clone(), name: connection.push_name.clone(), phone: format!("+{phone}") }),
        };
    }
    if live.catching_up.is_some() && state == ConnectionState::Online {
        return;
    }
    live.connection = Connection::new(state);
    live.connection.reason = reason;
    if state == ConnectionState::Banned && connection.until_ms > 0 {
        live.connection.until = Some(connection.until_ms);
    }
}

fn link_step(link: pb::Link) -> Option<LinkStep> {
    Some(match link.step? {
        pb::link::Step::Qr(qr) => LinkStep::Qr { code: qr.code, expires: qr.expires_ms },
        pb::link::Step::Code(code) => LinkStep::Code { code },
        pb::link::Step::Done(done) => {
            let phone = done.jid.split(['@', ':', '.']).next().unwrap_or_default();
            LinkStep::Done { phone: format!("+{phone}") }
        }
        pb::link::Step::Failed(reason) => LinkStep::Failed { reason },
    })
}

/// The HistorySyncNotification in a message from the phone, encoded, if it carries one.
fn history_notification(raw: &[u8]) -> Option<Vec<u8>> {
    let message = e2e::Message::decode(raw).ok()?;
    let protocol = message
        .protocol_message
        .as_ref()
        .or_else(|| message.device_sent_message.as_ref()?.message.as_ref()?.protocol_message.as_ref())?;
    Some(protocol.history_sync_notification.as_ref()?.encode_to_vec())
}

/// Fetches history blobs one at a time, as the phone offers them, and stores their messages.
async fn fetch_history(
    mut offers: mpsc::UnboundedReceiver<(String, Vec<u8>)>,
    store: Store,
    engine: Supervisor,
    live: Shared,
    spool: PathBuf,
) {
    while let Some((id, notification)) = offers.recv().await {
        let path = spool.join(format!("{id}.pb"));
        let started = Instant::now();
        let command =
            pb::command::Kind::HistoryDownload(pb::HistoryDownload { notification, path: path.display().to_string() });
        let file = match engine.call(command).await {
            Ok(pb::reply::Kind::HistoryFile(file)) => file,
            Ok(other) => {
                eprintln!("jawd: the engine answered a history download with {other:?}");
                continue;
            }
            Err(e) => {
                eprintln!("jawd: couldn't fetch history {id}: {e}");
                continue;
            }
        };
        let downloaded = started.elapsed();
        let rows = match tokio::fs::read(&path).await {
            Ok(data) => match tokio::task::spawn_blocking(move || rows(&data)).await {
                Ok(Ok(rows)) => rows,
                Ok(Err(e)) => {
                    eprintln!("jawd: history {id} doesn't decode: {e}");
                    continue;
                }
                Err(e) => {
                    eprintln!("jawd: decoding history {id} failed: {e}");
                    continue;
                }
            },
            Err(e) => {
                eprintln!("jawd: can't read history {id}: {e}");
                continue;
            }
        };
        let messages = rows.len() as u64;
        let written = match store.history(rows).await {
            Ok(written) => written,
            Err(e) => {
                eprintln!("jawd: can't store history {id}: {e:#}");
                continue;
            }
        };
        let _ = tokio::fs::remove_file(&path).await;
        eprintln!(
            "jawd: history {} chunk {}: {} chats, {messages} messages, {:.1} MB, fetched in {:.2} s and stored in {:.2} s ({:.0} a second, {} transactions); the phone says {}%",
            history_sync::history_sync::HistorySyncType::try_from(file.r#type).map(|t| t.as_str_name()).unwrap_or("?"),
            file.chunk_order,
            file.conversations,
            file.bytes as f64 / 1e6,
            downloaded.as_secs_f64(),
            written.took.as_secs_f64(),
            messages as f64 / written.took.as_secs_f64().max(1e-6),
            written.transactions,
            file.progress
        );
        let mut live = live.lock().expect("live state");
        let progress = live.history.get_or_insert_with(HistoryProgress::default);
        progress.blobs += 1;
        progress.conversations += u64::from(file.conversations);
        progress.messages += messages;
        progress.bytes += file.bytes;
        progress.download_seconds += downloaded.as_secs_f64();
        progress.store_seconds += written.took.as_secs_f64();
        progress.progress = progress.progress.max(file.progress);
    }
}

/// A history blob's messages, as they're kept until they're converted.
fn rows(data: &[u8]) -> Result<Vec<HistoryRow>, prost::DecodeError> {
    let history = history_sync::HistorySync::decode(data)?;
    let mut rows = Vec::new();
    for conversation in history.conversations {
        for entry in conversation.messages {
            let Some(message) = entry.message else { continue };
            let id = message.key.id.clone().unwrap_or_default();
            let at_ms = message.message_timestamp.unwrap_or_default() as i64 * 1000;
            rows.push(HistoryRow { chat: conversation.id.clone(), id, at_ms, raw: message.encode_to_vec() });
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_history_notification_is_found_where_the_phone_puts_it() {
        let notification = e2e::HistorySyncNotification { chunk_order: Some(3), ..Default::default() };
        let protocol =
            e2e::ProtocolMessage { history_sync_notification: Some(notification.clone()), ..Default::default() };
        let direct = e2e::Message { protocol_message: Some(Box::new(protocol.clone())), ..Default::default() };
        let sent = e2e::Message {
            device_sent_message: Some(Box::new(e2e::DeviceSentMessage {
                message: Some(Box::new(direct.clone())),
                ..Default::default()
            })),
            ..Default::default()
        };
        let text = e2e::Message { conversation: Some("hi".into()), ..Default::default() };
        assert_eq!(history_notification(&direct.encode_to_vec()), Some(notification.encode_to_vec()));
        assert_eq!(history_notification(&sent.encode_to_vec()), Some(notification.encode_to_vec()));
        assert_eq!(history_notification(&text.encode_to_vec()), None);
    }

    #[test]
    fn connection_events_set_the_account_and_connection() {
        let mut live = Live::default();
        let connected = pb::Connection {
            state: pb::connection::State::Connected as i32,
            jid: "351910000001:12@s.whatsapp.net".into(),
            push_name: "Pedro".into(),
            ..Default::default()
        };
        apply(&mut live, &connected);
        assert_eq!(live.account.state, AccountState::Linked);
        assert_eq!(live.account.me.as_ref().unwrap().phone, "+351910000001");
        assert_eq!(live.connection.state, ConnectionState::Online);
        let banned = pb::Connection {
            state: pb::connection::State::TemporaryBan as i32,
            reason: "sent to too many".into(),
            until_ms: 99,
            ..Default::default()
        };
        apply(&mut live, &banned);
        assert_eq!(live.connection.state, ConnectionState::Banned);
        assert_eq!((live.connection.until, live.connection.reason.as_deref()), (Some(99), Some("sent to too many")));
        apply(&mut live, &pb::Connection { state: pb::connection::State::LoggedOut as i32, ..Default::default() });
        assert_eq!(live.account.state, AccountState::Unlinked);
    }
}
