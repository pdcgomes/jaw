
/// The jaw.dbs `sync::merge::tests` opens from `crates/jawd/tests/fixtures/merge-chains/`, made by
/// jawd's own store at 4126db2, before a chat that starts merging away took the merges going into
/// it with it: each scene's changes as they came, a step of a merge where it says, then
/// `VACUUM INTO`. This module was added to `crates/jawd/src/sync/merge/tests.rs` at 4126db2, run
/// with `JAW_FIXTURES=<folder> cargo test -p jawd made_at_4126db2 -- --ignored --nocapture`, and
/// taken out again.
mod made_at_4126db2 {
    use std::path::Path;

    use super::*;
    use crate::store::{Amend, Amendment, Chat, Opened, Read};

    /// One person's three one-to-one chats: their new number's, their old number's and their LID's.
    const NEW: &str = "351910000002@s.whatsapp.net";
    const OLD: &str = "351910000001@s.whatsapp.net";
    const LID: &str = "100000000009@lid";
    const AT: i64 = 1_792_000_000_000;

    fn from_them(chat: &str, id: &str, at: i64) -> Change {
        Change::Message(Message {
            chat: chat.into(),
            id: id.into(),
            at,
            sender: Some(chat.into()),
            kind: "text",
            text: Some(id.into()),
            summary: id.into(),
            live: true,
            ..Message::default()
        })
    }

    fn mine(chat: &str, id: &str, at: i64) -> Change {
        let Change::Message(message) = from_them(chat, id, at) else { unreachable!() };
        Change::Message(Message { sender: None, mine: true, status: Some("sent"), ..message })
    }

    fn join(a: &str, b: &str) -> Change {
        Change::Person(Person { jids: vec![a.into(), b.into()], ..Person::default() })
    }

    fn amend(chat: &str, target: &str, at: i64, what: Amendment) -> Change {
        Change::Amend(Amend {
            chat: chat.into(),
            target: target.into(),
            sender: Some(chat.into()),
            mine: false,
            at,
            what,
            raw: None,
        })
    }

    fn on_the_phone(chat: &str) -> Change {
        Change::Read(Read { chat: chat.into(), at: AT + 5_000, ..Read::default() })
    }

    /// A step of chat `gone`'s merge, as the merging task asks for one.
    async fn step(store: &Store, gone: &str) {
        let merging = waiting(store)
            .await
            .unwrap()
            .into_iter()
            .find(|m| m.reference.starts_with(&format!("{gone} into ")))
            .unwrap_or_else(|| panic!("{gone}'s merge waits"));
        let applied = store.apply(Lane::Background, vec![Change::MergeStep(Some(merging))]).await.unwrap();
        assert_eq!(applied.quarantined, 0, "a step of {gone}'s merge failed");
    }

    /// Writes jaw.db as it stands to JAW_FIXTURES/<name>, and prints what it holds of the merges.
    fn keep(home: &Path, name: &str) {
        let folder = std::env::var("JAW_FIXTURES").expect("JAW_FIXTURES names the folder to write to");
        let path = Path::new(&folder).join(name);
        let _ = std::fs::remove_file(&path);
        let db = Connection::open(home.join("jaw.db")).unwrap();
        db.busy_timeout(Duration::from_secs(5)).unwrap();
        db.execute("VACUUM INTO ?1", [path.to_str().unwrap()]).unwrap();
        let kept = Connection::open(&path).unwrap();
        println!("{name}:");
        for sql in [
            "SELECT 'chat', n, id, pinned, muted_until, archived, ephemeral, draft, frecency, unread, message_count,
                read_at, read_n, read_known, told_at, told_n
            FROM chats ORDER BY n",
            "SELECT 'merging', g.id, k.id FROM chat_merges m JOIN chats g ON g.n = m.gone JOIN chats k ON k.n = m.keep",
            "SELECT 'pending', chat, target, kind FROM pending ORDER BY n",
            "SELECT 'tombstone', c.id, t.id FROM tombstones t JOIN chats c ON c.n = t.chat",
            "SELECT 'messages', c.id, count(*), min(m.id), max(m.id) FROM messages m JOIN chats c ON c.n = m.chat
            GROUP BY c.id",
            "SELECT 'quarantine', source, ref FROM quarantine",
            "SELECT 'version', (SELECT user_version FROM pragma_user_version)",
        ] {
            let mut statement = kept.prepare(sql).unwrap();
            let columns = statement.column_count();
            let mut rows = statement.query([]).unwrap();
            while let Some(row) = rows.next().unwrap() {
                let values: Vec<String> = (0..columns)
                    .map(|i| format!("{:?}", row.get::<_, rusqlite::types::Value>(i).unwrap()))
                    .collect();
                println!("  {}", values.join(" "));
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn make_merge_chain_fixtures() {
        // X1's scene: the new number's chat is the oldest, then the old number's, then the LID's,
        // with a step of each merge taken while both stand; then what's sent to the LID lands on
        // the old number's chat, the one in the middle.
        {
            let home = tempfile::tempdir().unwrap();
            let store = Store::open(home.path()).unwrap();
            let mut changes = vec![from_them(NEW, "n0", AT), mine(NEW, "n1", AT + 1)];
            changes.extend((0..5).map(|i| from_them(OLD, &format!("o{i}"), AT + 10 + i)));
            changes.extend((0..210).map(|i| from_them(LID, &format!("l{i}"), AT + 1_000 + i)));
            // The old number's chat opened once, which the new number's frecency takes as it merges.
            changes.push(Change::Opened(Opened { chat: OLD.into(), at: AT + 500 }));
            store.apply(Lane::Live, changes).await.unwrap();
            store.apply(Lane::Live, vec![join(OLD, LID)]).await.unwrap();
            // Moves l0 to l199 into the old number's chat.
            step(&store, LID).await;
            store.apply(Lane::Live, vec![join(LID, NEW)]).await.unwrap();
            // Moves o0 to o4 and l0 to l194 into the new number's chat.
            step(&store, OLD).await;
            // A draft typed in the LID's chat, kept as draft.set keeps it, and a pin from the phone.
            let draft = Change::Chat(Chat { id: LID.into(), draft: Some("typed under the LID".into()), ..Chat::default() });
            store.apply_durable(vec![draft]).await.unwrap();
            let pin = Change::Chat(Chat { id: LID.into(), pinned: Some(AT + 3_000), ..Chat::default() });
            store.apply(Lane::Live, vec![pin]).await.unwrap();
            let edit = Amendment::Edit { text: Some("edited".into()), spans: None, summary: "edited".into() };
            let changes = vec![
                amend(LID, "l10", AT + 3_010, Amendment::React { emoji: "👍".into() }),
                amend(LID, "l30", AT + 3_011, edit),
                amend(LID, "l25", AT + 3_012, Amendment::Revoke),
                amend(LID, "l20", AT + 3_013, Amendment::DeleteForMe),
            ];
            store.apply(Lane::Live, changes).await.unwrap();
            keep(home.path(), "x1-chain.db");
        }
        // The receipts' scene, with the LID's chat older than the old number's: the numbers' chats
        // read on the phone and the LID's read here up to l9; then the two joins, a draft typed in
        // the LID's chat, which lands on the old number's, and a step of each merge, the LID's
        // first, as the merging task takes them: the LID's ends, leaving its messages in the old
        // number's chat, and the old number's moves 200 of its own.
        {
            let home = tempfile::tempdir().unwrap();
            let store = Store::open(home.path()).unwrap();
            let mut changes = vec![from_them(NEW, "n0", AT)];
            changes.extend((0..10).map(|i| from_them(LID, &format!("l{i}"), AT + 1_000 + i)));
            changes.extend((0..205).map(|i| from_them(OLD, &format!("o{i}"), AT + 10 + i)));
            store.apply(Lane::Live, changes).await.unwrap();
            let here = Change::Read(Read { chat: LID.into(), ids: vec!["l9".into()], here: true, ..Read::default() });
            store.apply(Lane::Live, vec![on_the_phone(NEW), on_the_phone(OLD), here]).await.unwrap();
            store.apply(Lane::Live, vec![join(OLD, LID)]).await.unwrap();
            store.apply(Lane::Live, vec![join(LID, NEW)]).await.unwrap();
            let draft = Change::Chat(Chat { id: LID.into(), draft: Some("typed under the LID".into()), ..Chat::default() });
            store.apply_durable(vec![draft]).await.unwrap();
            step(&store, LID).await;
            assert!(!waiting(&store).await.unwrap().iter().any(|m| m.reference.starts_with(LID)), "the LID's ended");
            step(&store, OLD).await;
            keep(home.path(), "ended-chain.db");
        }
        // The same, with one message in the old number's chat and no step taken: both merges stand.
        {
            let home = tempfile::tempdir().unwrap();
            let store = Store::open(home.path()).unwrap();
            let mut changes = vec![from_them(NEW, "n0", AT)];
            changes.extend((0..10).map(|i| from_them(LID, &format!("l{i}"), AT + 1_000 + i)));
            changes.push(from_them(OLD, "o0", AT + 10));
            store.apply(Lane::Live, changes).await.unwrap();
            let here = Change::Read(Read { chat: LID.into(), ids: vec!["l9".into()], here: true, ..Read::default() });
            store.apply(Lane::Live, vec![on_the_phone(NEW), on_the_phone(OLD), here]).await.unwrap();
            store.apply(Lane::Live, vec![join(OLD, LID)]).await.unwrap();
            store.apply(Lane::Live, vec![join(LID, NEW)]).await.unwrap();
            keep(home.path(), "standing-chain.db");
        }
        // A person's two chats, which can't make a chain: the LID's, pinned, muted, with a draft
        // and a 24-hour timer, starts merging into the number's, which takes all four, and a step
        // is taken; then, while the merge runs, the number's chat is unpinned, unmuted, archived,
        // its draft sent and its timer turned off.
        {
            let home = tempfile::tempdir().unwrap();
            let store = Store::open(home.path()).unwrap();
            let mut changes = vec![from_them(NEW, "n0", AT)];
            changes.extend((0..210).map(|i| from_them(LID, &format!("l{i}"), AT + 1_000 + i)));
            let set = Chat {
                id: LID.into(),
                pinned: Some(AT + 60),
                muted_until: Some(AT + 9_000_000),
                ephemeral: Some(86_400),
                ..Chat::default()
            };
            changes.push(Change::Chat(set));
            store.apply(Lane::Live, changes).await.unwrap();
            let draft = Change::Chat(Chat { id: LID.into(), draft: Some("typed under the LID".into()), ..Chat::default() });
            store.apply_durable(vec![draft]).await.unwrap();
            store.apply(Lane::Live, vec![join(NEW, LID)]).await.unwrap();
            // Moves l0 to l199 into the number's chat.
            step(&store, LID).await;
            let undone = Chat {
                id: NEW.into(),
                pinned: Some(0),
                muted_until: Some(0),
                archived: Some(true),
                ephemeral: Some(0),
                draft: Some(String::new()),
                ..Chat::default()
            };
            store.apply(Lane::Live, vec![Change::Chat(undone)]).await.unwrap();
            keep(home.path(), "two-chats.db");
        }
        // X1's scene with the new number's chat read on the phone while the chain stands: the old
        // number's chat, merging into it, counts from there, and the LID's, merging into the old
        // number's, keeps its 10 unread.
        {
            let home = tempfile::tempdir().unwrap();
            let store = Store::open(home.path()).unwrap();
            let mut changes = vec![from_them(NEW, "n0", AT), mine(NEW, "n1", AT + 1)];
            changes.extend((0..5).map(|i| from_them(OLD, &format!("o{i}"), AT + 10 + i)));
            changes.extend((0..210).map(|i| from_them(LID, &format!("l{i}"), AT + 1_000 + i)));
            store.apply(Lane::Live, changes).await.unwrap();
            store.apply(Lane::Live, vec![join(OLD, LID)]).await.unwrap();
            step(&store, LID).await;
            store.apply(Lane::Live, vec![join(LID, NEW)]).await.unwrap();
            step(&store, OLD).await;
            store.apply(Lane::Live, vec![on_the_phone(NEW)]).await.unwrap();
            keep(home.path(), "read-chain.db");
        }
    }
}
