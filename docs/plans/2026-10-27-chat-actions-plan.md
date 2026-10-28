# Chat actions from jaw: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** jawd serves `chat.pin`, `chat.archive`, `chat.mute`, `chat.mark_unread` and `star`: each changes its row at once, in one synced commit with the app-state patches that tell your other devices, and the patches go to WhatsApp through the outbox, as a deletion for you does.

**Architecture:** A handler in `send/chat_actions.rs` checks the account and the chat, builds each patch as whatsmeow's builders do with the time it writes, and hands the writer one `Change::ChatAction`. The writer (`store/write/chat_actions.rs`) compares the request with the row and either writes nothing or writes the row, with a new `v` and its event, and queues each patch as an outbox row of kind `app_state`, all in one transaction. The outbox sends the patches after the chat's earlier sends; a patch saying a chat is read sends the chat's read receipts first; a patch refused for good, or out of tries, goes to quarantine, what it set is put back, every interface is told, and its collection is fetched again. The engine sends each patch with jawd's time, and the demo engine echoes each patch as app state, so WhatsApp's echo finds the row as jawd wrote it.

**Tech Stack:** Rust (tokio, rusqlite with SQLite 3.53 bundled, prost, serde_json), Go (whatsmeow `v0.0.0-20261015105041-df33a485cc46`), JSON lines on a Unix socket.

The design is settled: `docs/plans/2026-10-27-jaw-mac-menus-and-folders-design.md`, "Chat actions from jaw" (Requests, How jawd does it, Tests), and `docs/plans/2026-10-16-jaw-design.md`, "Chat actions from jaw", "jawd › Storage › Data integrity" and "Migrations". This is a data-layer change: read `.cursor/rules/data-integrity.mdc` and `.cursor/agents/database-expert.md` before starting.

## Global Constraints

- `chat.pin` (`chat`, `pinned`): pins the chat now (`chats.pinned` holds when, in milliseconds; 0 when it isn't) or unpins it. At most three chats are pinned, counting only unarchived pinned chats; a fourth is error 5 with `data` `{"limit": "pinned", "most": 3}`.
- `chat.archive` (`chat`, `archived`): archives the chat, unpinning it if it's pinned, as WhatsApp's apps do, or unarchives it.
- `chat.mute` (`chat`, `until`): null unmutes; a time in milliseconds mutes until then; -1, or any time at or past `FOREVER` (253_402_300_799_999, `crates/jawd/src/convert/app_state.rs`), mutes for ever (the terminal sends `i64::MAX` for always, the Mac -1); a time already past is -32602 with `data` `{"param": "until"}`.
- `chat.mark_unread` (`chat`, `unread`): true marks the chat unread; false clears the mark and reads the chat up to its newest message, sending read receipts as `read` does.
- `star` (`message`, `starred`, optional `chat`): stars or unstars a message.
- Each answers `{}` once its change is on disk; a chat or message jawd doesn't have is error 3; a community's own row (`kind` `community`) is error 4 for the four chat requests; an account that isn't linked is error 1; past the outbox's 1,000 it's error 5 `{"limit": "outbox", "waiting": n, "failed": n}`, as for sends.
- WhatsApp, as whatsmeow's `appstate/encode.go` builds them (`BuildPin`, `BuildArchive`, `BuildMuteAbs`, `BuildMarkChatAsRead`, `BuildStar`, `newMessageRange`):
  - pin: index `["pin_v1", chat]`, version 5, `regular_low`, `PinAction.pinned`.
  - archive: index `["archive", chat]`, version 3, `regular_low`, `ArchiveChatAction` with `archived` and the newest message's range; archiving a pinned chat is followed by `pin_v1` off, in `regular_low`.
  - mute: index `["mute", chat]`, version 2, `regular_high`, `MuteAction` with `muted` and `muteEndTimestamp` in milliseconds, -1 for ever, absent to unmute.
  - mark: index `["markChatAsRead", chat]`, version 3, `regular_low`, `MarkChatAsReadAction` with `read` and the newest message's range.
  - star: index `["star", chat, message ID, "1" if yours else "0", the sender's JID for someone else's message in a group, else "0"]`, version 2, `regular_high`, `StarAction.starred`.
  - A range is the newest message's key with its time in seconds, and `lastMessageTimestamp` in seconds; with no message, the time the action is done at.
  - Every patch's `SyncActionValue.timestamp` is the time jawd wrote, and a pin's row holds that same time.
- One transaction, synced, before the answer: the row's change, a new `v` for the row and its `events` row, and the outbox rows. Asking for what's already so writes, pushes and queues nothing.
- The patch goes after the chat's earlier sends, retried as the outbox retries, bounded by the outbox's 1,000 and counted in `jawd status` (`stored.outbox`); a patch refused for good, or out of tries, is kept in quarantine with its error, never dropped (`stored.quarantined`), what it set is put back, every interface gets a `notice`, and its collection is fetched again (`AppStateResync`).
- The change WhatsApp takes last wins, and jaw shows only what lasts (decided after task 8's review). WhatsApp takes app state in one order and sends it back in that order, which jaw keeps into its inbox as the decision on holding back says (the engine answers a patch once its echo is kept; one lock keeps jawd's patches and its full fetches apart), so while jaw has a patch for an index whose echo hasn't come back, whatever WhatsApp sends for that index came before jaw's patch: the converter keeps it raw (`app_state`) but doesn't apply it, since jaw's patch, taken after it, sets every device as the row already is, and jaw's echo then changes nothing. So an earlier echo can't set the row back, the row never shows a state that won't last, and a request is judged against what will.
- One new table, with an index of its own, and no new column: `app_state_awaiting` (migration 17, a schema step; class: jaw's own; its partial index `app_state_awaiting_by_echo` on `echo_by`) holds, for each app-state index jaw has queued a patch for, the time its latest patch carries (`at`), that patch's outbox row (`outbox`), and, once WhatsApp has taken it, the inbox's last `seq` then (`echo_by`), where its echo has to be. A row goes when that echo is converted, when the converter passes `echo_by` without it, when its outbox row has gone without going, or with its patch's refusal; there's at most one per index, and `jawd status` counts them (`stored.awaiting`). `chats.pinned`, `archived`, `muted_until`, `marked_unread`, `messages.starred`, the outbox's `app_state` rows and `quarantine` exist.
- The time a chat action is done at is taken under `inner.enqueue`, later than the last one jawd gave since it started, so no two of jaw's patches for one index carry one time; the writer refuses a patch whose time is already its index's awaited time.
- No `OFFSET`, no scans of large tables, no temporary b-tree sorts, no `ANALYZE`; every new statement is pinned in `crates/jawd/src/store/plans.txt` (`JAW_PLANS=write CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd plans`) and in its module's plan test.
- `protocol/fixtures/star.request.json` is exactly `{"id":20,"method":"star","params":{"message":"3EB0A1B2C3D4E5F6","starred":true,"chat":"351912345678@s.whatsapp.net"}}` and `star.result.json` exactly `{"id":20,"result":{}}`; the four chat requests take ids 16 to 19. Fixtures end with a newline.
- The terminal (`crates/jaw`) needs no change: `wire.rs` already sends the four chat requests, and its made-up account (`crates/jaw/src/demo.rs`) changes its own rows without asking jawd.
- Every commit that touches `crates/jawd/src/{store,sync,feed,find,send,engine}`, `crates/jawd/src/sock.rs`, `engine/internal/wa/`, `proto/engine/` or, from Task 3, `crates/jawd/src/convert/app_state.rs` carries `Data-Audit: docs/audits/<date>-chat-actions.md` before it's pushed (Task 14).
- jawd is a binary crate: its unit tests run as `CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd <path>`.
- Tasks 4 to 9 add code that only tests use until Task 10 wires the last of it, so check each of them with `CARGO_INCREMENTAL=0 cargo clippy -p jawd --tests -- -D warnings`; from Task 10 on, and at the gate, `make lint` passes whole.
- Work only in `/Users/pedrogomes/src/jaw-chat-actions`. Build with `CARGO_INCREMENTAL=0`, after `df -h /System/Volumes/Data` shows at least 60 GB free (the workspace's tests build 10 to 30 GB into `target/`). Never build in another checkout.
- Never open, copy or read `~/Library/Application Support/jaw`, never run the Go engine on a folder holding `session.db`, never stop processes by name (`pkill`, `killall`): stop only PIDs you started.
- Scratch goes in `/tmp/jaw-chat-actions/` and is deleted before you finish; `target/` in this worktree is deleted once the branch has landed.
- Commit messages are one or more plain, complete sentences saying what changed and why, no prefix such as `feat:`; British spelling. Code comments only state constraints the code can't show.

---

## File map

| File | Change |
| --- | --- |
| `engine/internal/wa/appstate.go` | `patchInfo`: the patch takes the time jawd wrote |
| `engine/internal/wa/send_test.go` | its test |
| `crates/jaw-proto/src/send.rs` | `ChatPin`, `ChatArchive`, `ChatMute`, `ChatMarkUnread`, `Star` |
| `crates/jaw-proto/tests/fixtures.rs` | the five requests and results |
| `protocol/fixtures/{chat.pin,chat.archive,chat.mute,chat.mark_unread,star}.{request,result}.json` | new, ten files |
| `crates/jawd/src/convert/app_state.rs` | a read from any device also unmarks the chat |
| `crates/jawd/src/store/rules.rs` | the rule's test |
| `scripts/check-data-audit.py` | `crates/jawd/src/convert/app_state.rs` joins the data layer |
| `crates/jawd/src/store/write/chat_actions.rs` | new: the writer's half (`Act`, `Does`, `Setting`, `Patch`, `Acted`, `Outcome`, `Undo`, `apply`, `undo`) and its tests |
| `crates/jawd/src/store/write.rs` | `Change::ChatAction`, its `apply` and `provenance` |
| `crates/jawd/src/store/write/outbox.rs` | `app_state_row`; `Step::Refused`, `REFUSED`; their tests |
| `crates/jawd/src/store/mod.rs` | re-exports `chat_actions` |
| `crates/jawd/src/store/plans.txt` | written again |
| `crates/jawd/src/engine/demo.rs` | takes, echoes, refuses and fetches again app state |
| `crates/jawd/src/send/chat_actions.rs` | new: the patches, the five requests, refusals |
| `crates/jawd/src/send/mod.rs` | `chat_actions`, the dispatch, `METHODS`, `Inner.live` |
| `crates/jawd/src/send/outbox.rs` | `attempt`: a read's receipts first, a refusal kept; `settle` visible to `send` |
| `crates/jawd/src/send/reads.rs` | `tell_now` |
| `crates/jawd/src/send/actions.rs` | `refused` loses its app-state arm |
| `crates/jawd/src/send/tests.rs` | the requests' tests, two crash scenes |
| `crates/jaw-proto/src/lib.rs` | `quarantined`'s doc |
| `protocol/README.md` | "Chat actions", the Methods table, the Errors table |
| `docs/plans/2026-10-16-jaw-design.md` | how the patches go, and how a refused one is put back |
| `docs/audits/<date>-chat-actions.md` | new, the database expert's |

## Decisions this plan makes

The design leaves these open, or the code forced them; the audit checks them.

- **Archive and unpin are two patches, in the chat's order.** The engine's `AppStatePatch` carries one mutation, and changing `proto/engine/engine.proto` needs `make proto` (buf through the Go proxy) for a gain only on the rare partial refusal. Both rows queue under the chat, the archive first, as `BuildArchive` orders its mutations; each refusal puts back its own field and fetches the collection again.
- **The engine carries jawd's time.** whatsmeow's `EncodePatch` writes `PatchInfo.Timestamp` (now, when unset) into every mutation's `timestamp`, and the converter takes a pin's time from that field, so without Task 1 the echo of a pin would carry the engine's time and change the row.
- **A patch saying a chat is read sends the chat's receipts first.** `set_read` treats WhatsApp's read at a position jaw has reached as the phone having sent those receipts (`whatsapps_position_replaces_a_guess_even_when_its_earlier_and_told_goes_with_it` pins it), so the echo of jaw's own `markChatAsRead` arriving before jaw's receipts would leave them never sent. The outbox's attempt at such a patch tells the chat's receipts at once (`reads::tell_now`) and only then sends it. Holding the row and releasing it after the receipts was rejected: the store's `committed` watch moves only on commits that write events, and telling receipts writes none.
- **A read on any device unmarks the chat.** The converter's `markChatAsRead` with `read` set also clears `marked_unread`; until now only a read further than jaw's position did, so jaw's own mark read, coming back after a resync replayed the phone's mark, left the chat marked.
- **A refusal puts the field back before the fetch.** A full fetch of a collection sends only the mutations WhatsApp holds, so a refused pin of a chat never pinned elsewhere would otherwise stay pinned. `undo` sets the field to how a chat is when nothing set it (unpinned, not archived, unmuted, not marked, not starred) in the transaction that quarantines the row, and the fetch then sets what the phone has.
- **A refused deletion for you is quarantined too**, where it was dropped, and fetches nothing: the message stays deleted here.
- **Star names your own message's sender "0"**, as jaw's `deleteMessageForMe` does; someone else's message in a group names its sender's JID, and a one-to-one chat's "0", which is what whatsmeow's `BuildStar` gives for them.
- **Mark read is "already so" when the chat has no mark and nothing unread**, which needs no look at the read position.
- **Starring a line about the chat or a message deleted for everyone is error 4**, as reacting to one is; a star can always be taken off.
- **Unarchiving a pinned chat while three others are pinned unpins it** (decided after task 4's review, to keep the limit of three): its `pin_v1` off is queued after the archive patch, as archiving does. The handler builds the unpin patch for every archive request, and the writer queues it only when it unpins: archiving a pinned chat, or unarchiving one past the limit.
- **A mark read on a chat jaw holds none of the messages of reads it when WhatsApp's echo comes** (decided before task 9, replacing task 4's reading it at the patch's time; the owner was asked and hadn't answered, so the plan takes the option that loses nothing): the writer clears the mark and queues the patch, and the echo, converted as any `markChatAsRead` is, reads the chat at the patch's time, which clears the phone's count. Asking again before the echo queues another patch, which is harmless: the first echo is held back and the second reads it. Read at the write, a refused patch would have lost the phone's count for good, since app state carries no counts.
- **A refused mark read leaves the chat read here** (decided after task 5's review): its undo puts the mark as a chat has it when nothing set it, and the fetch again sets the phone's; with the chat's messages here, the read up to its newest stays, since its receipts went first; with none here, nothing but the mark had changed, so the refusal loses nothing (since task 9). The design's account of how a refused patch is put back says so (task 12), and the audit weighs it.
- **What WhatsApp sends for an index while jaw's own patch for it is on its way is held back** (decided after task 8's review, replacing the design's "applied as it comes"). Applied as it came, an earlier echo set the row back after jaw's next change: a request in that window was judged already so and lost on every device (pin, unpin, pin ended unpinned everywhere), and a fourth chat could be pinned. Holding back goes by WhatsApp's order, never by times, which each device sets from its own clock. whatsmeow hands a fetch's mutations over only after releasing its lock, and a send's echo from a goroutine of its own after `SendAppState` has returned, so two fetches handed over at once can interleave in the inbox (decided after the fix's review). jaw keeps WhatsApp's order into its inbox as far as it can: the engine answers a patch's `Done` only once it has kept the patch's echo, known by its index and time, or fails it after 30 seconds; and one lock keeps jawd's app-state patches, each from its send until that answer, and its full fetches of app state, each until it has answered, from overlapping, so a mutation the converter reaches past `echo_by` came after jaw's patch, and is applied. A refused patch puts its field back only while it's its index's latest, since a later patch's intent stands, and its collection is fetched again, which brings back what was held. A refused deletion for you has no awaited row: it changes nothing a later mutation could set back.
- **What keeping WhatsApp's order leaves, for the audit** (from the two reviews of task 8's fix). whatsmeow hands some app state over by itself, which nothing orders against jaw's echo: a fetch it makes for a notification from WhatsApp; the conflicting changes it applies, from a goroutine of its own, when WhatsApp answers a send with 409; the fetch of every collection it makes when app-state keys arrive; and its recovery snapshots. The lock is also let go at the echo while whatsmeow may still be handing over the rest of that send's fetch, and after the 30-second timeout while the echo may still come. In each case, when your phone changes the same setting of the same chat at the same moment as jaw, jaw can show the other value until that setting next changes or the engine next starts, whose full fetch sets it as WhatsApp has it; and a full fetch, which hands a snapshot's values over before the patches after it, can show an older value for a moment before its own copy of jaw's patch. Closing these needs each mutation's version, which whatsmeow keeps (`appstate.Mutation.PatchVersion`) but its events don't carry (`appstate.go:193`). A patch sent again, after a timeout, a failed fetch once WhatsApp took it, or an engine restart mid-patch, is taken by WhatsApp after whatever another device changed meanwhile, undoing that change on every device; fetching the collection again rather than sending again would avoid it, at the cost of a full fetch. And at an engine start a chat's sends queued after one of its chat actions wait for the start's full fetch to answer, since the sends' lane keeps each chat's order (951 mutations took 6.4 to 11.4 seconds on the demo account, `docs/audits/2026-10-26-wal-bound.md`).

---

### Task 1: The engine sends each patch with the time jawd wrote

**Files:**
- Modify: `engine/internal/wa/appstate.go` (the whole file)
- Test: `engine/internal/wa/send_test.go` (one test, three imports)

**Interfaces:**
- Consumes: nothing.
- Produces: `func patchInfo(c *enginepb.AppStatePatch) (appstate.PatchInfo, error)`, whose `Timestamp` is `time.UnixMilli(value.timestamp)` when the value has one. jawd relies on WhatsApp's echo of a patch carrying the patch's `SyncActionValue.timestamp` unchanged.

- [ ] **Step 1: Start from origin/main**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && git fetch origin && git rebase origin/main`
Expected: `Successfully rebased and updated refs/heads/chat-actions.` (main's newer commits are docs only).

- [ ] **Step 2: Write the failing test**

In `engine/internal/wa/send_test.go`, add to the imports:

```go
	"go.mau.fi/whatsmeow/appstate"
	"go.mau.fi/whatsmeow/proto/waSyncAction"
	"google.golang.org/protobuf/proto"
```

so the block reads:

```go
import (
	"context"
	"errors"
	"fmt"
	"net"
	"testing"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/appstate"
	"go.mau.fi/whatsmeow/proto/waSyncAction"
	"google.golang.org/protobuf/proto"

	"github.com/pdcgomes/jaw/engine/internal/link"
	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)
```

and append:

```go
func TestAnAppStatePatchCarriesTheTimeJawdWroteForItsEchoToFindTheRowAsItIs(t *testing.T) {
	pin := func(value *waSyncAction.SyncActionValue) *enginepb.AppStatePatch {
		raw, err := proto.Marshal(value)
		if err != nil {
			t.Fatal(err)
		}
		index := []string{"pin_v1", "351910000002@s.whatsapp.net"}
		return &enginepb.AppStatePatch{Name: "regular_low", Index: index, Version: 5, Value: raw}
	}
	patch, err := patchInfo(pin(&waSyncAction.SyncActionValue{
		Timestamp: proto.Int64(1792000000123),
		PinAction: &waSyncAction.PinAction{Pinned: proto.Bool(true)},
	}))
	if err != nil {
		t.Fatal(err)
	}
	if got := patch.Timestamp.UnixMilli(); got != 1792000000123 {
		t.Errorf("the patch's time is %d, where jawd wrote 1792000000123", got)
	}
	if patch.Type != appstate.WAPatchRegularLow || len(patch.Mutations) != 1 || patch.Mutations[0].Version != 5 {
		t.Errorf("the patch isn't the mutation jawd built: %+v", patch)
	}
	bare, err := patchInfo(pin(&waSyncAction.SyncActionValue{PinAction: &waSyncAction.PinAction{Pinned: proto.Bool(false)}}))
	if err != nil {
		t.Fatal(err)
	}
	if !bare.Timestamp.IsZero() {
		t.Errorf("a value without a time gave the patch %v; whatsmeow then uses the time it encodes", bare.Timestamp)
	}
}
```

- [ ] **Step 3: Run it to see it fail**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions/engine && go test ./internal/wa -run TestAnAppStatePatch -v`
Expected: the build fails with `undefined: patchInfo`.

- [ ] **Step 4: Write the implementation**

Replace `engine/internal/wa/appstate.go` with:

```go
package wa

import (
	"errors"
	"fmt"
	"time"

	"go.mau.fi/whatsmeow/appstate"
	"go.mau.fi/whatsmeow/proto/waSyncAction"
	"google.golang.org/protobuf/proto"

	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// appStatePatch sends one app-state mutation jawd built, such as a chat pinned or a message deleted
// for you, which your other devices then apply. whatsmeow fetches the collection again afterwards,
// so the mutation comes back as an AppState event, which changes nothing jawd hasn't done already.
func (e *Engine) appStatePatch(c *enginepb.AppStatePatch) *enginepb.Reply {
	patch, err := patchInfo(c)
	if err != nil {
		return failure(err.Error(), false)
	}
	if err := e.client.SendAppState(e.ctx, patch); err != nil {
		return failure(fmt.Sprintf("can't change %s for your other devices: %v", c.Index[0], err), retryable(err))
	}
	return done()
}

// patchInfo is the patch whatsmeow sends for jawd's mutation. whatsmeow writes the patch's time into
// every mutation it encodes, so the patch takes the time jawd wrote, which a pinned chat's row holds.
func patchInfo(c *enginepb.AppStatePatch) (appstate.PatchInfo, error) {
	switch appstate.WAPatchName(c.Name) {
	case appstate.WAPatchRegular, appstate.WAPatchRegularLow, appstate.WAPatchRegularHigh,
		appstate.WAPatchCriticalBlock, appstate.WAPatchCriticalUnblockLow:
	default:
		return appstate.PatchInfo{}, fmt.Errorf("there's no app-state collection %q", c.Name)
	}
	if len(c.Index) == 0 {
		return appstate.PatchInfo{}, errors.New("an app-state mutation needs its index")
	}
	value := &waSyncAction.SyncActionValue{}
	if err := proto.Unmarshal(c.Value, value); err != nil {
		return appstate.PatchInfo{}, fmt.Errorf("that isn't an app-state value: %v", err)
	}
	patch := appstate.PatchInfo{
		Type:      appstate.WAPatchName(c.Name),
		Mutations: []appstate.MutationInfo{{Index: c.Index, Version: c.Version, Value: value}},
	}
	if value.GetTimestamp() > 0 {
		patch.Timestamp = time.UnixMilli(value.GetTimestamp())
	}
	return patch, nil
}
```

- [ ] **Step 5: Run the engine's tests**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions/engine && go test ./internal/wa -run TestAnAppStatePatch -v && go vet ./... && gofmt -l .`
Expected: `--- PASS: TestAnAppStatePatchCarriesTheTimeJawdWroteForItsEchoToFindTheRowAsItIs`, `--- PASS: TestAnAppStatePatchThatCantBeRightIsRefusedForGoodBeforeItGoes`, `ok  github.com/pdcgomes/jaw/engine/internal/wa`; `go vet` and `gofmt -l` print nothing.

- [ ] **Step 6: Commit**

```bash
cd /Users/pedrogomes/src/jaw-chat-actions
git add engine/internal/wa/appstate.go engine/internal/wa/send_test.go
git commit -m "The engine sends each app-state patch with the time jawd wrote as the patch's own, which whatsmeow writes into every mutation it encodes, where it used the time it encoded it: a pinned chat's row holds when jawd pinned it, so WhatsApp's echo of a chat pinned from jaw finds the row as jawd wrote it and changes nothing, and a value without a time goes at whatsmeow's time, as before."
```

---

### Task 2: jaw-proto's five requests and their fixtures

**Files:**
- Modify: `crates/jaw-proto/src/send.rs` (module doc, five types at the end)
- Modify: `crates/jaw-proto/tests/fixtures.rs`
- Create: `protocol/fixtures/chat.pin.request.json`, `chat.pin.result.json`, `chat.archive.request.json`, `chat.archive.result.json`, `chat.mute.request.json`, `chat.mute.result.json`, `chat.mark_unread.request.json`, `chat.mark_unread.result.json`, `star.request.json`, `star.result.json`

**Interfaces:**
- Consumes: nothing.
- Produces, in `jaw_proto::send`: `ChatPin { chat: String, pinned: bool }`, `ChatArchive { chat: String, archived: bool }`, `ChatMute { chat: String, until: Option<i64> }` (absent or null is `None`, written back as null), `ChatMarkUnread { chat: String, unread: bool }`, `Star { message: String, starred: bool, chat: Option<String> }` (`chat` left out when `None`). All derive `Clone, Debug, PartialEq, Eq, Serialize, Deserialize`.

- [ ] **Step 1: Write the fixtures, which fail the round-trip test**

Create each file with exactly this line and a newline after it:

`protocol/fixtures/chat.pin.request.json`:
```json
{"id":16,"method":"chat.pin","params":{"chat":"351912345678@s.whatsapp.net","pinned":true}}
```

`protocol/fixtures/chat.pin.result.json`:
```json
{"id":16,"result":{}}
```

`protocol/fixtures/chat.archive.request.json`:
```json
{"id":17,"method":"chat.archive","params":{"chat":"120363012345678901@g.us","archived":true}}
```

`protocol/fixtures/chat.archive.result.json`:
```json
{"id":17,"result":{}}
```

`protocol/fixtures/chat.mute.request.json`:
```json
{"id":18,"method":"chat.mute","params":{"chat":"120363012345678901@g.us","until":1792174800000}}
```

`protocol/fixtures/chat.mute.result.json`:
```json
{"id":18,"result":{}}
```

`protocol/fixtures/chat.mark_unread.request.json`:
```json
{"id":19,"method":"chat.mark_unread","params":{"chat":"351912345678@s.whatsapp.net","unread":false}}
```

`protocol/fixtures/chat.mark_unread.result.json`:
```json
{"id":19,"result":{}}
```

`protocol/fixtures/star.request.json`:
```json
{"id":20,"method":"star","params":{"message":"3EB0A1B2C3D4E5F6","starred":true,"chat":"351912345678@s.whatsapp.net"}}
```

`protocol/fixtures/star.result.json`:
```json
{"id":20,"result":{}}
```

- [ ] **Step 2: Run the fixtures' test to see it fail**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && CARGO_INCREMENTAL=0 cargo test -p jaw-proto --test fixtures`
Expected: FAIL, `no type for chat.archive.request.json's params`.

- [ ] **Step 3: Add the types**

In `crates/jaw-proto/src/send.rs`, replace the module doc's two lines with:

```rust
//! Sending: `send.text` and `send.retry`, `chat.start`, `draft.set`, `read` and `typing`; the message
//! actions: `react`, `edit`, `delete`, `forward`, `message.info` and `chat.info`; and the chat actions:
//! `chat.pin`, `chat.archive`, `chat.mute`, `chat.mark_unread` and `star`.
```

and append at the end of the file:

```rust
/// `chat.pin`'s params: the chat pinned now, or unpinned.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatPin {
    pub chat: String,
    pub pinned: bool,
}

/// `chat.archive`'s params: the chat archived, which unpins it, or brought back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatArchive {
    pub chat: String,
    pub archived: bool,
}

/// `chat.mute`'s params: muted until `until`, in milliseconds since the epoch, or for ever with -1 or
/// any time from the last millisecond of 9999 on; null unmutes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMute {
    pub chat: String,
    #[serde(default)]
    pub until: Option<i64>,
}

/// `chat.mark_unread`'s params: marked unread, or the mark taken off and the chat read up to its
/// newest message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMarkUnread {
    pub chat: String,
    pub unread: bool,
}

/// `star`'s params: a message starred, or its star taken off. `chat`, when given, is where it is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Star {
    pub message: String,
    pub starred: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
}
```

In `crates/jaw-proto/tests/fixtures.rs`, replace the `jaw_proto::send` import with:

```rust
use jaw_proto::send::{
    ChatArchive, ChatInfo, ChatInfoResult, ChatMarkUnread, ChatMute, ChatPin, ChatStart, ChatStarted, Delete, DraftSet,
    Edit, Forward, Forwarded, Info, MessageInfo, React, Read, SendRetry, SendText, Sent, Star, Typing,
};
```

In the `"request"` match, after `"chat.info" => params::<ChatInfo>(&name, &request.params),` add:

```rust
                    "chat.pin" => params::<ChatPin>(&name, &request.params),
                    "chat.archive" => params::<ChatArchive>(&name, &request.params),
                    "chat.mute" => params::<ChatMute>(&name, &request.params),
                    "chat.mark_unread" => params::<ChatMarkUnread>(&name, &request.params),
                    "star" => params::<Star>(&name, &request.params),
```

and in the `"result"` match, replace the arm of results that are `{}` with:

```rust
                    "chat.opened" | "unwatch" | "history.more" | "focus" | "send.retry" | "draft.set" | "read"
                    | "typing" | "react" | "edit" | "delete" | "chat.pin" | "chat.archive" | "chat.mute"
                    | "chat.mark_unread" | "star" => {
                        assert_eq!(result, &serde_json::json!({}), "{name} has a result")
                    }
```

- [ ] **Step 4: Run the test to see it pass**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && CARGO_INCREMENTAL=0 cargo test -p jaw-proto`
Expected: PASS, `test every_fixture_reads_and_writes_back_the_same ... ok`, and jaw-proto's other tests ok.

- [ ] **Step 5: Commit**

```bash
cd /Users/pedrogomes/src/jaw-chat-actions
cargo fmt --all
git add crates/jaw-proto/src/send.rs crates/jaw-proto/tests/fixtures.rs protocol/fixtures/chat.pin.request.json protocol/fixtures/chat.pin.result.json protocol/fixtures/chat.archive.request.json protocol/fixtures/chat.archive.result.json protocol/fixtures/chat.mute.request.json protocol/fixtures/chat.mute.result.json protocol/fixtures/chat.mark_unread.request.json protocol/fixtures/chat.mark_unread.result.json protocol/fixtures/star.request.json protocol/fixtures/star.result.json
git commit -m "jaw-proto reads and writes the chat actions' requests, chat.pin, chat.archive, chat.mute (null to unmute), chat.mark_unread and star (with an optional chat), each answered with nothing, with a fixture of each request and its result; star's are the ones the Mac's plan compares against."
```

---

### Task 3: A chat read on any device loses its unread mark

**Files:**
- Modify: `crates/jawd/src/convert/app_state.rs` (the `markChatAsRead` arm and its test)
- Modify: `crates/jawd/src/store/rules.rs` (imports, one test)
- Modify: `scripts/check-data-audit.py` (`DATA_PATHS`)

**Interfaces:**
- Consumes: nothing.
- Produces: `convert::app_state::mutation` for `["markChatAsRead", chat]` with `read` true gives `[Change::Read(..), Change::Chat(Chat { marked_unread: Some(false), .. })]`; with `read` false, `[Change::Chat(Chat { marked_unread: Some(true), .. })]` as before.

- [ ] **Step 1: Write the failing tests**

In `crates/jawd/src/convert/app_state.rs`, replace the test `marking_a_chat_read_reads_it_up_to_the_newest_message_its_range_names` with:

```rust
    #[test]
    fn marking_a_chat_read_reads_it_up_to_the_newest_message_its_range_names_and_unmarks_it() {
        let read = value(|v| {
            v.mark_chat_as_read_action = Some(action::MarkChatAsReadAction {
                read: Some(true),
                message_range: Some(action::SyncActionMessageRange {
                    last_message_timestamp: Some(1_791_999_000),
                    messages: vec![action::SyncActionMessage {
                        key: Some(wa_proto::wa_common::MessageKey { id: Some("3EB0AA".into()), ..Default::default() }),
                        timestamp: Some(1_791_999_000),
                    }],
                    ..Default::default()
                }),
            })
        });
        let applied = mutation(&index(&["markChatAsRead", ANA]), &read);
        let [Change::Read(read), Change::Chat(unmarked)] = applied.changes.as_slice() else {
            panic!("expected a read position and the chat unmarked")
        };
        assert_eq!(
            (read.chat.as_str(), read.at, read.ids.as_slice(), read.phone),
            (ANA, 1_791_999_000_000, &["3EB0AA".to_string()][..], None)
        );
        assert_eq!((unmarked.id.as_str(), unmarked.marked_unread), (ANA, Some(false)));
        let bare = value(|v| {
            v.mark_chat_as_read_action = Some(action::MarkChatAsReadAction { read: Some(true), ..Default::default() })
        });
        let applied = mutation(&index(&["markChatAsRead", ANA]), &bare);
        let [Change::Read(read), Change::Chat(_)] = applied.changes.as_slice() else {
            panic!("expected a read position and the chat unmarked")
        };
        assert_eq!(read.at, 1_792_000_000_000, "without a range, the mutation's own time");
    }
```

In `crates/jawd/src/store/rules.rs`, add below `use rusqlite::Connection;`:

```rust
use prost::Message as _;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobuf_sync_action::{
    MarkChatAsReadAction, SyncActionMessage, SyncActionMessageRange, SyncActionValue,
};
```

and append:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn a_chat_read_on_any_device_loses_its_mark_even_where_jaw_had_read_it_already() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(&store, vec![Change::Message(message(ANA, "a", AT, "a"))]).await;
    let here = Read { chat: ANA.into(), ids: vec!["a".into()], here: true, ..Read::default() };
    let marked = Change::Chat(Chat { id: ANA.into(), marked_unread: Some(true), ..Chat::default() });
    apply(&store, vec![Change::Read(here), marked]).await;
    let ana = chat(&store, ANA).await;
    assert!(ana.marked_unread && ana.unread == 0, "read here, the mark stays the phone's");
    // A read naming the message jaw has read up to, as jaw's own mark read comes back.
    let key = MessageKey { remote_jid: Some(ANA.into()), from_me: Some(false), id: Some("a".into()), participant: None };
    let range = SyncActionMessageRange {
        last_message_timestamp: Some(AT / 1000),
        messages: vec![SyncActionMessage { key: Some(key), timestamp: Some(AT / 1000) }],
        ..Default::default()
    };
    let value = SyncActionValue {
        timestamp: Some(AT + 5),
        mark_chat_as_read_action: Some(MarkChatAsReadAction { read: Some(true), message_range: Some(range) }),
        ..Default::default()
    };
    let index = ["markChatAsRead".to_string(), ANA.to_string()];
    apply(&store, crate::convert::app_state::mutation(&index, &value.encode_to_vec()).changes).await;
    assert!(!chat(&store, ANA).await.marked_unread, "read on any device, it isn't marked any more");
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd -- convert::app_state store::rules::a_chat_read_on_any_device`
Expected: FAIL: `marking_a_chat_read_reads_it_up_to_the_newest_message_its_range_names_and_unmarks_it` panics with `expected a read position and the chat unmarked`, and `a_chat_read_on_any_device_loses_its_mark_even_where_jaw_had_read_it_already` fails at `read on any device, it isn't marked any more`.

- [ ] **Step 3: Write the implementation**

In `crates/jawd/src/convert/app_state.rs`, replace the `markChatAsRead` arm with:

```rust
        (Some("markChatAsRead"), Some(jid)) => {
            let action = value.mark_chat_as_read_action.unwrap_or_default();
            if action.read.unwrap_or(false) {
                // Read up to the newest message the range names, or its last message's time, which
                // a replay of the mutation can't move back.
                let range = action.message_range.unwrap_or_default();
                let last = range.last_message_timestamp.map(ms).unwrap_or(at);
                let ids = range.messages.iter().filter_map(|m| m.key.as_ref()?.id.clone()).collect();
                applied.changes.push(Change::Read(Read {
                    chat: user_jid(jid),
                    at: last,
                    ids,
                    phone: None,
                    here: false,
                }));
                // Read on any device, the chat isn't marked unread, even where jaw has read as far.
                applied.changes.push(chat(Chat { marked_unread: Some(false), ..Chat::default() }));
            } else {
                applied.changes.push(chat(Chat { marked_unread: Some(true), ..Chat::default() }));
            }
        }
```

In `scripts/check-data-audit.py`, in `DATA_PATHS`, after `"crates/jawd/src/sync",` add:

```python
    "crates/jawd/src/convert/app_state.rs",
```

- [ ] **Step 4: Run the tests to see them pass, and what reads app state too**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd -- convert:: store::rules sync::tests::app_state`
Expected: PASS, including `app_state_pins_archives_and_marks_chats_as_the_phone_has_them`.

- [ ] **Step 5: Commit**

```bash
cd /Users/pedrogomes/src/jaw-chat-actions
cargo fmt --all
git add crates/jawd/src/convert/app_state.rs crates/jawd/src/store/rules.rs scripts/check-data-audit.py
git commit -m "A chat marked read on any device loses its unread mark, even where jaw has already read it as far: the converter's markChatAsRead with read set now unmarks the chat beside reading it, where the read alone unmarked it only when it read further than jaw had, so jaw's own mark read coming back through app state clears a mark the phone set meanwhile; convert/app_state.rs joins the paths the audit check counts as the data layer."
```

---

### Task 4: The writer does a chat action in one transaction

**Files:**
- Create: `crates/jawd/src/store/write/chat_actions.rs`
- Modify: `crates/jawd/src/store/write.rs` (module list, `Change`, `provenance`, `apply`)
- Modify: `crates/jawd/src/store/write/outbox.rs` (`app_state_row`; `delete_for_me` uses it)
- Modify: `crates/jawd/src/store/mod.rs` (re-export)
- Modify: `crates/jawd/src/store/plans.txt` (written again)

**Interfaces:**
- Consumes: `write.rs`'s `upsert_chat`, `set_read`, `located`, `apply_amendment`, `Doing`, `chat_jid`, `now_ms`, `Stamps`; `outbox::{waiting, OUTBOX_MOST}`.
- Produces, in `crate::store::chat_actions`:
  - `pub const PINNED_MOST: u64 = 3`
  - `pub struct Act { pub chat: String, pub does: Does, pub outcome: Outcome }`
  - `pub enum Does { Set(Setting), Star { id: String, starred: bool, patch: Patch } }`
  - `pub enum Setting { Pin { at: Option<i64>, patch: Patch }, Archive { archived: bool, patch: Patch, unpin: Option<Patch> }, Mute { until: i64, patch: Patch }, MarkUnread { unread: bool, newest: Option<String>, patch: Patch } }`
  - `pub struct Patch { pub id: String, pub raw: Vec<u8> }`
  - `pub enum Acted { Queued { chat: String, chat_n: i64, outbox: Vec<i64> }, Already, Gone, Community, Pinned, Full { waiting: u64, failed: u64 } }`
  - `pub struct Outcome` with `pub fn take(&self) -> Option<Acted>`
  - `pub enum Undo { Pin { chat: String }, Archive { chat: String }, Mute { chat: String }, MarkUnread { chat: String }, Star { chat: String, id: String } }`
  - `pub(super) fn undo(db: &Connection, stamps: &mut Stamps, undo: &Undo) -> Result<bool>` (used by Task 5)
- Produces, in `crate::store::outbox`: `pub(super) fn app_state_row(db: &Connection, chat: &str, id: &str, raw: &[u8]) -> Result<i64>`.
- Produces `Change::ChatAction(chat_actions::Act)`.

- [ ] **Step 1: Write the failing tests**

Create `crates/jawd/src/store/write/chat_actions.rs` holding only its tests for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::schema;
    use crate::store::{Amend, Change, Message};

    const ANA: &str = "351910000002@s.whatsapp.net";
    const RUI: &str = "351910000003@s.whatsapp.net";
    const COMMUNITY: &str = "120363000000000009@g.us";
    const AT: i64 = 1_792_000_000_000;

    fn db() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        schema::migrate(&mut db).unwrap();
        db
    }

    /// Makes `changes` in one transaction as the writer does, settle and all.
    fn written(db: &Connection, changes: &[Change]) {
        let mut stamps = Stamps::load(db).unwrap();
        stamps.begin();
        for change in changes {
            super::super::apply(db, &mut stamps, change).unwrap();
        }
        super::super::settle(db, &mut stamps).unwrap();
    }

    fn patch(id: &str) -> Patch {
        Patch { id: id.into(), raw: id.as_bytes().to_vec() }
    }

    fn pin(at: Option<i64>, id: &str) -> Does {
        Does::Set(Setting::Pin { at, patch: patch(id) })
    }

    /// Does `does` to `chat` as the writer does, and says what it came to.
    fn acted(db: &Connection, chat: &str, does: Does) -> Acted {
        let outcome = Outcome::default();
        written(db, &[Change::ChatAction(Act { chat: chat.into(), does, outcome: outcome.clone() })]);
        outcome.take().expect("a chat action says what it came to")
    }

    fn chats(db: &Connection, ids: &[&str]) {
        let changes: Vec<Change> = ids.iter().map(|id| Change::Chat(Chat { id: id.to_string(), ..Chat::default() })).collect();
        written(db, &changes);
    }

    /// Chat `id`'s pinned, archived, muted_until, marked_unread, unread and v.
    fn chat(db: &Connection, id: &str) -> (i64, bool, i64, bool, i64, i64) {
        db.query_row(
            "SELECT pinned, archived, muted_until, marked_unread, unread, v FROM chats WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .unwrap()
    }

    /// The app-state rows queued, oldest first, as (jid, id, raw).
    fn queued(db: &Connection) -> Vec<(String, String, Vec<u8>)> {
        db.prepare("SELECT jid, id, raw FROM outbox WHERE kind = 'app_state' ORDER BY n")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    fn from_ana(id: &str, at: i64) -> Change {
        Change::Message(Message {
            chat: ANA.into(),
            id: id.into(),
            at,
            sender: Some(ANA.into()),
            kind: "text",
            text: Some(id.into()),
            summary: id.into(),
            live: true,
            ..Message::default()
        })
    }

    #[test]
    fn a_pin_is_written_with_its_time_a_new_version_and_its_patch_and_asked_again_does_nothing() {
        let db = db();
        chats(&db, &[ANA]);
        let (.., v) = chat(&db, ANA);
        let Acted::Queued { chat: kept, outbox, .. } = acted(&db, ANA, pin(Some(AT), "p1")) else {
            panic!("not queued")
        };
        assert_eq!((kept.as_str(), outbox.len()), (ANA, 1));
        let (pinned, .., pinned_v) = chat(&db, ANA);
        assert_eq!(pinned, AT, "pinned when jawd said, the time its patch carries");
        assert!(pinned_v > v, "with a new version");
        let event: String = db.query_row("SELECT type FROM events WHERE seq = ?1", [pinned_v], |r| r.get(0)).unwrap();
        assert_eq!(event, "chat", "and its event, in the same transaction");
        assert_eq!(queued(&db), [(ANA.to_string(), "p1".to_string(), b"p1".to_vec())]);
        assert_eq!(acted(&db, ANA, pin(Some(AT + 5), "p2")), Acted::Already, "pinned already");
        let (pinned, .., again_v) = chat(&db, ANA);
        assert_eq!((pinned, again_v, queued(&db).len()), (AT, pinned_v, 1), "nothing written or queued");
        assert!(matches!(acted(&db, ANA, pin(None, "p3")), Acted::Queued { .. }));
        assert_eq!(chat(&db, ANA).0, 0);
        assert_eq!(acted(&db, ANA, pin(None, "p4")), Acted::Already);
        assert_eq!(queued(&db).len(), 2);
    }

    #[test]
    fn at_most_three_chats_are_pinned_and_archived_ones_dont_count() {
        let db = db();
        let ids = [
            "351910000011@s.whatsapp.net",
            "351910000012@s.whatsapp.net",
            "351910000013@s.whatsapp.net",
            "351910000014@s.whatsapp.net",
            "351910000015@s.whatsapp.net",
        ];
        chats(&db, &ids);
        written(&db, &[Change::Chat(Chat { id: ids[4].into(), pinned: Some(AT), archived: Some(true), ..Chat::default() })]);
        for (n, id) in ids[..3].iter().enumerate() {
            assert!(matches!(acted(&db, id, pin(Some(AT + n as i64), id)), Acted::Queued { .. }), "{id}");
        }
        assert_eq!(acted(&db, ids[3], pin(Some(AT + 9), "fourth")), Acted::Pinned);
        assert_eq!((chat(&db, ids[3]).0, queued(&db).len()), (0, 3), "not pinned, and nothing queued");
        assert!(matches!(acted(&db, ids[0], pin(None, "unpin")), Acted::Queued { .. }));
        assert!(matches!(acted(&db, ids[3], pin(Some(AT + 10), "now")), Acted::Queued { .. }), "a place came free");
    }

    #[test]
    fn archiving_a_pinned_chat_unpins_it_with_a_second_patch_after_the_first() {
        let db = db();
        chats(&db, &[ANA, RUI]);
        written(&db, &[Change::Chat(Chat { id: ANA.into(), pinned: Some(AT), ..Chat::default() })]);
        let archive = |archived: bool, id: &str| {
            let unpin = archived.then(|| patch(&format!("{id} unpin")));
            Does::Set(Setting::Archive { archived, patch: patch(id), unpin })
        };
        let Acted::Queued { outbox, .. } = acted(&db, ANA, archive(true, "a1")) else { panic!("not queued") };
        assert_eq!(outbox.len(), 2);
        let (pinned, archived, ..) = chat(&db, ANA);
        assert_eq!((pinned, archived), (0, true), "archived, and unpinned as WhatsApp's apps do");
        let ids: Vec<String> = queued(&db).into_iter().map(|(_, id, _)| id).collect();
        assert_eq!(ids, ["a1", "a1 unpin"], "the archive first, then the unpin");
        assert_eq!(acted(&db, ANA, archive(true, "a2")), Acted::Already);
        let Acted::Queued { outbox, .. } = acted(&db, RUI, archive(true, "r1")) else { panic!("not queued") };
        assert_eq!(outbox.len(), 1, "a chat that isn't pinned has nothing to unpin");
        assert!(matches!(acted(&db, ANA, archive(false, "a3")), Acted::Queued { .. }));
        assert!(!chat(&db, ANA).1);
    }

    #[test]
    fn a_mute_sets_until_when_and_the_same_again_does_nothing() {
        let db = db();
        chats(&db, &[ANA]);
        let mute = |until: i64, id: &str| Does::Set(Setting::Mute { until, patch: patch(id) });
        assert!(matches!(acted(&db, ANA, mute(AT, "m1")), Acted::Queued { .. }));
        assert_eq!(chat(&db, ANA).2, AT);
        assert_eq!(acted(&db, ANA, mute(AT, "m2")), Acted::Already);
        assert!(matches!(acted(&db, ANA, mute(crate::convert::app_state::FOREVER, "m3")), Acted::Queued { .. }));
        assert!(matches!(acted(&db, ANA, mute(0, "m4")), Acted::Queued { .. }));
        assert_eq!((chat(&db, ANA).2, queued(&db).len()), (0, 3));
    }

    #[test]
    fn marking_unread_sets_the_mark_and_marking_read_takes_it_off_and_reads_up_to_the_newest() {
        let db = db();
        written(&db, &[from_ana("m1", AT), from_ana("m2", AT + 10)]);
        assert_eq!(chat(&db, ANA).4, 2, "two unread");
        let mark = |unread: bool, id: &str| {
            Does::Set(Setting::MarkUnread { unread, newest: Some("m2".into()), patch: patch(id) })
        };
        assert!(matches!(acted(&db, ANA, mark(true, "u1")), Acted::Queued { .. }));
        assert!(chat(&db, ANA).3);
        assert_eq!(acted(&db, ANA, mark(true, "u2")), Acted::Already);
        let Acted::Queued { chat_n, .. } = acted(&db, ANA, mark(false, "r1")) else { panic!("not queued") };
        let (.., marked, unread, _) = chat(&db, ANA);
        assert_eq!((marked, unread), (false, 0), "the mark gone, and read up to the newest");
        let owes: bool =
            db.query_row("SELECT told_at IS NOT NULL FROM chats WHERE n = ?1", [chat_n], |r| r.get(0)).unwrap();
        assert!(owes, "read here, its read receipts are owed");
        assert_eq!(acted(&db, ANA, mark(false, "r2")), Acted::Already, "nothing marked and nothing unread");
        assert_eq!(queued(&db).len(), 2);
    }

    #[test]
    fn a_star_marks_the_message_with_a_new_version_and_the_same_again_does_nothing() {
        let db = db();
        written(&db, &[from_ana("m1", AT)]);
        let star = |starred: bool, id: &str| Does::Star { id: "m1".into(), starred, patch: patch(id) };
        let message = |db: &Connection| -> (bool, i64) {
            db.query_row("SELECT starred, v FROM messages WHERE id = 'm1'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
        };
        let (_, before) = message(&db);
        assert!(matches!(acted(&db, ANA, star(true, "s1")), Acted::Queued { .. }));
        let (starred, after) = message(&db);
        assert!(starred && after > before);
        assert_eq!(acted(&db, ANA, star(true, "s2")), Acted::Already);
        let missing = Does::Star { id: "nope".into(), starred: true, patch: patch("s3") };
        assert_eq!(acted(&db, ANA, missing), Acted::Gone);
        assert!(matches!(acted(&db, ANA, star(false, "s4")), Acted::Queued { .. }));
        assert_eq!((message(&db).0, queued(&db).len()), (false, 2));
    }

    #[test]
    fn a_chat_jaw_doesnt_have_or_a_communitys_own_row_is_left_alone() {
        let db = db();
        written(&db, &[Change::Chat(Chat { id: COMMUNITY.into(), kind: Some("community"), ..Chat::default() })]);
        assert_eq!(acted(&db, ANA, pin(Some(AT), "p1")), Acted::Gone);
        assert_eq!(acted(&db, COMMUNITY, pin(Some(AT), "p2")), Acted::Community);
        let chats: i64 = db.query_row("SELECT count(*) FROM chats", [], |r| r.get(0)).unwrap();
        assert_eq!((chats, queued(&db).len()), (1, 0), "no chat made, nothing queued");
    }

    #[test]
    fn without_room_in_the_outbox_for_its_patches_nothing_is_written() {
        let db = db();
        chats(&db, &[ANA, RUI]);
        written(&db, &[Change::Chat(Chat { id: ANA.into(), pinned: Some(AT), ..Chat::default() })]);
        for n in 0..OUTBOX_MOST as i64 - 1 {
            db.execute(
                "INSERT INTO outbox (jid, id, kind, raw, next_at, created_at) VALUES (?1, ?2, 'reaction', x'', 0, 0)",
                rusqlite::params![ANA, format!("w{n}")],
            )
            .unwrap();
        }
        let archive = Does::Set(Setting::Archive { archived: true, patch: patch("a1"), unpin: Some(patch("a1 unpin")) });
        assert_eq!(acted(&db, ANA, archive), Acted::Full { waiting: 999, failed: 0 }, "two places, one left");
        let (pinned, archived, ..) = chat(&db, ANA);
        assert_eq!((pinned, archived), (AT, false), "nothing written");
        assert!(matches!(acted(&db, RUI, pin(Some(AT), "r1")), Acted::Queued { .. }), "the last place");
        assert_eq!(acted(&db, RUI, pin(None, "r2")), Acted::Full { waiting: 1_000, failed: 0 });
    }

    #[test]
    fn undoing_puts_back_how_a_chat_is_when_nothing_set_it_and_makes_no_chat() {
        let db = db();
        written(&db, &[from_ana("m1", AT)]);
        let set = Chat {
            id: ANA.into(),
            pinned: Some(AT),
            archived: Some(true),
            muted_until: Some(AT),
            marked_unread: Some(true),
            ..Chat::default()
        };
        let star = Amend {
            chat: ANA.into(),
            target: "m1".into(),
            sender: None,
            mine: false,
            at: AT,
            what: Amendment::Star { starred: true },
            raw: None,
        };
        written(&db, &[Change::Chat(set), Change::Amend(star)]);
        let before = chat(&db, ANA).5;
        let undone = |db: &Connection, undos: &[Undo]| {
            let mut stamps = Stamps::load(db).unwrap();
            stamps.begin();
            for one in undos {
                undo(db, &mut stamps, one).unwrap();
            }
            super::super::settle(db, &mut stamps).unwrap();
        };
        undone(
            &db,
            &[
                Undo::Pin { chat: ANA.into() },
                Undo::Archive { chat: ANA.into() },
                Undo::Mute { chat: ANA.into() },
                Undo::MarkUnread { chat: ANA.into() },
                Undo::Star { chat: ANA.into(), id: "m1".into() },
            ],
        );
        let (pinned, archived, muted, marked, _, v) = chat(&db, ANA);
        assert_eq!((pinned, archived, muted, marked), (0, false, 0, false));
        assert!(v > before, "with a new version, so every window is sent it");
        let starred: bool = db.query_row("SELECT starred FROM messages WHERE id = 'm1'", [], |r| r.get(0)).unwrap();
        assert!(!starred);
        undone(&db, &[Undo::Pin { chat: RUI.into() }, Undo::Star { chat: RUI.into(), id: "x".into() }]);
        let chats: i64 = db.query_row("SELECT count(*) FROM chats", [], |r| r.get(0)).unwrap();
        assert_eq!(chats, 1, "a chat that isn't there isn't made");
    }

    /// Each statement a chat action runs, as SQLite plans it on a fresh database with no statistics:
    /// each reads an index and sorts nothing.
    #[test]
    fn every_statement_a_chat_action_runs_reads_an_index_and_sorts_nothing() {
        let db = db();
        let statements = [
            ("a chat's state", CHAT_STATE, "SEARCH chats USING INDEX sqlite_autoindex_chats_1 (id=?)"),
            ("chats pinned elsewhere", PINNED_ELSEWHERE, "SEARCH chats USING INDEX chats_by_sort (archived=? AND sort>?)"),
            ("starred", STARRED, "SEARCH messages USING INTEGER PRIMARY KEY (rowid=?)"),
        ];
        for (what, sql, expected) in statements {
            let mut explain = db.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let count = explain.parameter_count();
            let plan: Vec<String> = explain
                .query_map(rusqlite::params_from_iter(std::iter::repeat_n(0, count)), |r| r.get(3))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            assert_eq!(plan.join("\n"), expected, "{what}");
        }
    }
}
```

In `crates/jawd/src/store/write.rs`, change the module list at the top to:

```rust
pub mod chat_actions;
pub mod layout;
pub mod outbox;
```

and in `Change`, after `Outbox(outbox::Step),` add:

```rust
    /// A chat pinned, archived, muted or marked, or a message starred, from jaw, with the app-state
    /// patches that tell your other devices queued in the same transaction.
    ChatAction(chat_actions::Act),
```

In `crates/jawd/src/store/mod.rs`, after `pub use self::write::unpack;` add:

```rust
pub use self::write::chat_actions;
```

- [ ] **Step 2: Run them to see them fail**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd store::write::chat_actions`
Expected: the build fails: `cannot find type 'Act' in module 'chat_actions'`, `cannot find type 'Patch' in this scope`, and the like.

- [ ] **Step 3: Write the implementation**

At the top of `crates/jawd/src/store/write/chat_actions.rs`, above its tests, put:

```rust
//! Chat actions from jaw: a chat pinned, archived, muted, or marked unread or read, and a message
//! starred, from any interface. Each is done to its row at once, with a new version and its event,
//! and the app-state patches that tell your other devices go into the outbox in the same
//! transaction, to go after the chat's earlier sends, as a deletion for you does. What's already so
//! changes nothing and queues nothing, so asking again after a lost answer, or two interfaces asking
//! at once, end the same. A patch WhatsApp refuses for good is undone here (`undo`): what it set goes
//! back to how a chat is when nothing has set it, since fetching its collection again brings only
//! what WhatsApp holds, and the fetch then sets it as the phone has it.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};

use super::outbox::{OUTBOX_MOST, app_state_row, waiting};
use super::{Amendment, Chat, Doing, Read, Stamps, apply_amendment, chat_jid, located, now_ms, set_read, upsert_chat};

/// The most chats pinned at once, as WhatsApp's apps allow; archived chats don't count.
pub const PINNED_MOST: u64 = 3;

const CHAT_STATE: &str =
    "SELECT n, kind, pinned, archived, muted_until, marked_unread, unread FROM chats WHERE id = ?1";
/// Pinned chats sort first, so the unarchived ones are a short range of chats_by_sort.
const PINNED_ELSEWHERE: &str = "SELECT count(*) FROM chats WHERE archived = 0 AND sort >= '1~' AND n <> ?1";
const STARRED: &str = "SELECT starred FROM messages WHERE n = ?1";

/// A chat action as the writer takes it: the chat as the interface named it, what to do, and where
/// to say what it came to. Each patch is an AppStatePatch the caller made, carrying the time it's
/// done at.
#[derive(Clone, Debug)]
pub struct Act {
    pub chat: String,
    pub does: Does,
    pub outcome: Outcome,
}

#[derive(Clone, Debug)]
pub enum Does {
    /// Something set on the chat.
    Set(Setting),
    /// Message `id` in the chat starred, or its star taken off.
    Star { id: String, starred: bool, patch: Patch },
}

#[derive(Clone, Debug)]
pub enum Setting {
    /// Pinned at `at`, the time its patch carries, or unpinned when `None`.
    Pin { at: Option<i64>, patch: Patch },
    /// Archived or brought back; archiving a pinned chat unpins it, with `unpin` after `patch`.
    Archive { archived: bool, patch: Patch, unpin: Option<Patch> },
    /// Muted until `until`, in milliseconds (`FOREVER` for ever), or unmuted at 0.
    Mute { until: i64, patch: Patch },
    /// Marked unread; or the mark taken off and the chat read here up to `newest`, its newest
    /// message, which owes WhatsApp its read receipts.
    MarkUnread { unread: bool, newest: Option<String>, patch: Patch },
}

/// An app-state patch to queue: its outbox row's own ID, and the AppStatePatch, encoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    pub id: String,
    pub raw: Vec<u8>,
}

/// What a chat action came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Acted {
    /// Done, its patches queued as these outbox rows, in the order they go; `chat` is the chat's JID
    /// as jaw keeps it, and `chat_n` its n.
    Queued { chat: String, chat_n: i64, outbox: Vec<i64> },
    /// It was so already: nothing was written or queued.
    Already,
    /// The chat, or the message, isn't there.
    Gone,
    /// The chat is a community's own row, whose groups carry these.
    Community,
    /// Pinning it would pin more than PINNED_MOST chats.
    Pinned,
    /// The outbox holds as many as it may, so nothing was written.
    Full { waiting: u64, failed: u64 },
}

/// Where a chat action says what it came to. The last attempt at its transaction is the one that
/// stands.
#[derive(Clone, Debug, Default)]
pub struct Outcome(Arc<Mutex<Option<Acted>>>);

impl Outcome {
    pub fn take(&self) -> Option<Acted> {
        self.0.lock().expect("a chat action's outcome").take()
    }

    fn put(&self, acted: Acted) {
        *self.0.lock().expect("a chat action's outcome") = Some(acted);
    }
}

/// What a refused patch set, to put back: the chat's field, or the message's star.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Undo {
    Pin { chat: String },
    Archive { chat: String },
    Mute { chat: String },
    MarkUnread { chat: String },
    Star { chat: String, id: String },
}

/// A chat as a chat action finds it.
struct State {
    n: i64,
    community: bool,
    pinned: i64,
    archived: bool,
    muted_until: i64,
    marked: bool,
    unread: i64,
}

fn state(db: &Connection, jid: &str) -> Result<Option<State>> {
    Ok(db
        .prepare_cached(CHAT_STATE)?
        .query_row([jid], |r| {
            Ok(State {
                n: r.get(0)?,
                community: r.get::<_, String>(1)? == "community",
                pinned: r.get(2)?,
                archived: r.get(3)?,
                muted_until: r.get(4)?,
                marked: r.get(5)?,
                unread: r.get(6)?,
            })
        })
        .optional()?)
}

/// Where a chat action came from, for the quarantine.
pub(super) fn provenance(act: &Act) -> (&'static str, String, Option<&[u8]>) {
    let (what, patch) = match &act.does {
        Does::Set(Setting::Pin { patch, .. }) => ("pin", patch),
        Does::Set(Setting::Archive { patch, .. }) => ("archive", patch),
        Does::Set(Setting::Mute { patch, .. }) => ("mute", patch),
        Does::Set(Setting::MarkUnread { patch, .. }) => ("mark", patch),
        Does::Star { patch, .. } => ("star", patch),
    };
    ("chat action", format!("{} {what} {}", act.chat, patch.id), Some(patch.raw.as_slice()))
}

pub(super) fn apply(db: &Connection, stamps: &mut Stamps, act: &Act) -> Result<bool> {
    let jid = chat_jid(db, &act.chat)?;
    let (acted, changed) = match &act.does {
        Does::Set(setting) => set(db, stamps, &jid, setting)?,
        Does::Star { id, starred, patch } => star(db, stamps, &jid, id, *starred, patch)?,
    };
    act.outcome.put(acted);
    Ok(changed)
}

/// Sets `setting` on chat `jid`, unless it's so already, and queues its patches.
fn set(db: &Connection, stamps: &mut Stamps, jid: &str, setting: &Setting) -> Result<(Acted, bool)> {
    let Some(chat) = state(db, jid)? else { return Ok((Acted::Gone, false)) };
    if chat.community {
        return Ok((Acted::Community, false));
    }
    let mut row = Chat { id: jid.to_string(), ..Chat::default() };
    let mut patches = Vec::new();
    let mut read = None;
    match setting {
        Setting::Pin { at, patch } => {
            if at.is_some() == (chat.pinned > 0) {
                return Ok((Acted::Already, false));
            }
            if at.is_some() && pinned_elsewhere(db, chat.n)? >= PINNED_MOST {
                return Ok((Acted::Pinned, false));
            }
            row.pinned = Some(at.unwrap_or(0));
            patches.push(patch);
        }
        Setting::Archive { archived, patch, unpin } => {
            if *archived == chat.archived {
                return Ok((Acted::Already, false));
            }
            row.archived = Some(*archived);
            patches.push(patch);
            if let (true, Some(unpin)) = (chat.pinned > 0, unpin) {
                row.pinned = Some(0);
                patches.push(unpin);
            }
        }
        Setting::Mute { until, patch } => {
            if *until == chat.muted_until {
                return Ok((Acted::Already, false));
            }
            row.muted_until = Some(*until);
            patches.push(patch);
        }
        Setting::MarkUnread { unread: true, patch, .. } => {
            if chat.marked {
                return Ok((Acted::Already, false));
            }
            row.marked_unread = Some(true);
            patches.push(patch);
        }
        Setting::MarkUnread { unread: false, newest, patch } => {
            if !chat.marked && chat.unread == 0 {
                return Ok((Acted::Already, false));
            }
            row.marked_unread = Some(false);
            read = newest
                .as_ref()
                .map(|id| Read { chat: jid.to_string(), ids: vec![id.clone()], here: true, ..Read::default() });
            patches.push(patch);
        }
    }
    if let Some(full) = full(db, patches.len())? {
        return Ok((full, false));
    }
    let mut changed = upsert_chat(db, stamps, &row)?;
    if let Some(read) = &read {
        changed |= set_read(db, stamps, read)?;
    }
    let outbox =
        patches.iter().map(|patch| app_state_row(db, jid, &patch.id, &patch.raw)).collect::<Result<Vec<_>>>()?;
    Ok((Acted::Queued { chat: jid.to_string(), chat_n: chat.n, outbox }, changed))
}

/// Stars message `id` in chat `jid`, or takes its star off, unless it's so already, and queues its
/// patch.
fn star(db: &Connection, stamps: &mut Stamps, jid: &str, id: &str, starred: bool, patch: &Patch) -> Result<(Acted, bool)> {
    let Some((chat, n)) = located(db, stamps, jid, id)? else { return Ok((Acted::Gone, false)) };
    let was: bool = db.prepare_cached(STARRED)?.query_row([n], |r| r.get(0))?;
    if was == starred {
        return Ok((Acted::Already, false));
    }
    if let Some(full) = full(db, 1)? {
        return Ok((full, false));
    }
    let amendment = Amendment::Star { starred };
    let changed =
        apply_amendment(db, stamps, chat, n, Doing { what: &amendment, sender: None, mine: false, at: now_ms() })?;
    let outbox = app_state_row(db, jid, &patch.id, &patch.raw)?;
    Ok((Acted::Queued { chat: jid.to_string(), chat_n: chat, outbox: vec![outbox] }, changed))
}

/// How many chats but `n` are pinned and not archived.
fn pinned_elsewhere(db: &Connection, n: i64) -> Result<u64> {
    let count: i64 = db.prepare_cached(PINNED_ELSEWHERE)?.query_row([n], |r| r.get(0))?;
    Ok(count.max(0) as u64)
}

/// The outbox's answer when it has no room for `rows` more.
fn full(db: &Connection, rows: usize) -> Result<Option<Acted>> {
    let (waiting, failed) = waiting(db)?;
    Ok((waiting + rows as u64 > OUTBOX_MOST).then_some(Acted::Full { waiting, failed }))
}

/// Puts back what a refused patch set: the chat's field as it is when nothing has set it, or the
/// message unstarred. A chat or message that's gone has nothing to put back, and none is made.
pub(super) fn undo(db: &Connection, stamps: &mut Stamps, undo: &Undo) -> Result<bool> {
    let (chat, unset) = match undo {
        Undo::Star { chat, id } => {
            let Some((chat, n)) = located(db, stamps, &chat_jid(db, chat)?, id)? else { return Ok(false) };
            let unstarred = Amendment::Star { starred: false };
            return apply_amendment(db, stamps, chat, n, Doing { what: &unstarred, sender: None, mine: false, at: now_ms() });
        }
        Undo::Pin { chat } => (chat, Chat { pinned: Some(0), ..Chat::default() }),
        Undo::Archive { chat } => (chat, Chat { archived: Some(false), ..Chat::default() }),
        Undo::Mute { chat } => (chat, Chat { muted_until: Some(0), ..Chat::default() }),
        Undo::MarkUnread { chat } => (chat, Chat { marked_unread: Some(false), ..Chat::default() }),
    };
    let jid = chat_jid(db, chat)?;
    if state(db, &jid)?.is_none() {
        return Ok(false);
    }
    upsert_chat(db, stamps, &Chat { id: jid, ..unset })
}
```

In `crates/jawd/src/store/write.rs`, in `provenance`, after `Some(Change::Outbox(step)) => outbox::provenance(step),` add:

```rust
        Some(Change::ChatAction(act)) => chat_actions::provenance(act),
```

and in `apply`, after `Change::Outbox(step) => outbox::apply(db, stamps, step),` add:

```rust
        Change::ChatAction(act) => chat_actions::apply(db, stamps, act),
```

In `crates/jawd/src/store/write/outbox.rs`, after the function `bumped`, add:

```rust
/// Queues `raw`, an AppStatePatch for chat `chat` (its JID as jaw keeps it), as an outbox row of its
/// own, `id`, which counts toward OUTBOX_MOST.
pub(super) fn app_state_row(db: &Connection, chat: &str, id: &str, raw: &[u8]) -> Result<i64> {
    Ok(db.prepare_cached(APP_STATE)?.query_row(params![chat, id, raw, now_ms()], |r| r.get(0))?)
}
```

and in `delete_for_me`, replace

```rust
    let now = now_ms();
    let outbox: i64 = db.prepare_cached(APP_STATE)?.query_row(params![chat, id, raw, now], |r| r.get(0))?;
```

with

```rust
    let now = now_ms();
    let outbox = app_state_row(db, &chat, id, raw)?;
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd store::write`
Expected: PASS, the nine tests in `store::write::chat_actions::tests` and the outbox's own, `every_statement_sending_runs_reads_an_index_and_sorts_nothing` among them.

- [ ] **Step 5: Pin the new statements' plans**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && JAW_PLANS=write CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd store::plans && git diff crates/jawd/src/store/plans.txt`
Expected: the diff adds exactly three statements under `-- src/store/write/chat_actions.rs`, planned as `SEARCH chats USING INDEX sqlite_autoindex_chats_1 (id=?)`, `SEARCH chats USING INDEX chats_by_sort (archived=? AND sort>?)` and `SEARCH messages USING INTEGER PRIMARY KEY (rowid=?)`, with no `SCAN` and no `USE TEMP B-TREE`.
Then run: `CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd store::plans`
Expected: PASS.

- [ ] **Step 6: Check the store's other tests and lint**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && cargo fmt --all && CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd store:: && CARGO_INCREMENTAL=0 cargo clippy -p jawd --tests -- -D warnings`
Expected: PASS, and clippy finishes without warnings.

- [ ] **Step 7: Commit**

```bash
cd /Users/pedrogomes/src/jaw-chat-actions
git add crates/jawd/src/store/write/chat_actions.rs crates/jawd/src/store/write.rs crates/jawd/src/store/write/outbox.rs crates/jawd/src/store/mod.rs crates/jawd/src/store/plans.txt
git commit -m "The writer does a chat action from jaw in one transaction: the chat pinned at the time its patch carries, archived (unpinning a pinned chat, with a second patch after the first), muted, or marked unread or read here up to its newest message, or a message starred, each with a new version and its event and each patch queued as an outbox row of kind app_state; what's so already writes and queues nothing, and a fourth pinned chat (archived ones apart), a community's own row, a chat or message that isn't there and an outbox without room for every patch write nothing and say which; undo puts a refused patch's field back to how a chat is when nothing set it. Its three statements each read an index, pinned in plans.txt."
```

---

### Task 5: A refused patch goes into quarantine and is put back

**Files:**
- Modify: `crates/jawd/src/store/write/outbox.rs` (imports, `Step::Refused`, `REFUSED`, `REFUSED_ROW`, `refused`, `provenance`, `apply`, the plan test, one test)
- Modify: `crates/jawd/src/store/plans.txt` (written again)

**Interfaces:**
- Consumes: `chat_actions::{undo, Undo}` (Task 4), `write::record_quarantine`.
- Produces: `Step::Refused { outbox: i64, error: String, undo: Option<chat_actions::Undo> }` and `pub const REFUSED: &str = "app_state_patch"`, the quarantine's `source` for refused app state, its `ref` the row's `jid` and `id` with a space between.

- [ ] **Step 1: Write the failing test**

In `crates/jawd/src/store/write/outbox.rs`'s `mod tests`, below `use crate::store::schema;` add:

```rust
    use crate::store::{Change, Chat};
```

and append to the module:

```rust
    #[test]
    fn app_state_whatsapp_refused_goes_into_quarantine_with_its_patch_and_what_it_set_is_put_back() {
        let mut db = Connection::open_in_memory().unwrap();
        schema::migrate(&mut db).unwrap();
        let chat = "a@s.whatsapp.net";
        written(&db, &[Change::Chat(Chat { id: chat.into(), ..Chat::default() })]);
        let outcome = chat_actions::Outcome::default();
        let patch = chat_actions::Patch { id: "3EB0P1".into(), raw: b"the patch".to_vec() };
        let pin = chat_actions::Does::Set(chat_actions::Setting::Pin { at: Some(5), patch });
        written(&db, &[Change::ChatAction(chat_actions::Act { chat: chat.into(), does: pin, outcome: outcome.clone() })]);
        let Some(chat_actions::Acted::Queued { outbox, .. }) = outcome.take() else { panic!("not queued") };
        let pinned = |db: &Connection| -> (i64, i64) {
            db.query_row("SELECT pinned, v FROM chats WHERE id = ?1", [chat], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
        };
        let (_, before) = pinned(&db);
        let refused = Step::Refused {
            outbox: outbox[0],
            error: "server returned error 400".into(),
            undo: Some(Undo::Pin { chat: chat.into() }),
        };
        written(&db, &[Change::Outbox(refused.clone())]);
        type Kept = (String, String, String, Vec<u8>);
        let kept: Kept = db
            .query_row("SELECT source, ref, error, raw FROM quarantine", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .unwrap();
        assert_eq!(
            kept,
            (REFUSED.into(), format!("{chat} 3EB0P1"), "server returned error 400".into(), b"the patch".to_vec()),
            "kept with why, never dropped"
        );
        let rows: i64 = db.query_row("SELECT count(*) FROM outbox", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 0, "and out of the outbox's thousand");
        let (now, after) = pinned(&db);
        assert_eq!(now, 0, "what it set is put back");
        assert!(after > before, "with a new version, so every window is sent it");
        written(&db, &[Change::Outbox(refused)]);
        let quarantined: i64 = db.query_row("SELECT count(*) FROM quarantine", [], |r| r.get(0)).unwrap();
        assert_eq!(quarantined, 1, "done once");
    }
```

In the plan test `every_statement_sending_runs_reads_an_index_and_sorts_nothing`, after `("app state", APP_STATE, ""),` add:

```rust
            ("a refused row", REFUSED_ROW, "SEARCH outbox USING INTEGER PRIMARY KEY (rowid=?)"),
```

- [ ] **Step 2: Run them to see them fail**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd store::write::outbox`
Expected: the build fails: `no variant named 'Refused' found for enum 'Step'`, `cannot find value 'REFUSED'`, `cannot find value 'REFUSED_ROW'`.

- [ ] **Step 3: Write the implementation**

In `crates/jawd/src/store/write/outbox.rs`, below the `use super::{...};` block, add:

```rust
use super::chat_actions::{self, Undo};
```

In `Step`, after `Withdrawn { outbox: i64 },` add:

```rust
    /// App state at outbox row `outbox` that WhatsApp refused for good, or that ran out of tries:
    /// the row goes into quarantine with `error` and its patch, never dropped, and `undo` puts back
    /// what it set.
    Refused { outbox: i64, error: String, undo: Option<Undo> },
```

After `pub const ACTIONS_KEPT: i64 = 1_000;` add:

```rust
/// The quarantine's source for app state WhatsApp refused, kept by its chat and its row's own ID.
pub const REFUSED: &str = "app_state_patch";
```

After the `APP_STATE` statement add:

```rust
const REFUSED_ROW: &str = "SELECT jid, id, raw FROM outbox WHERE n = ?1 AND kind = 'app_state' AND state = 'queued'";
```

In `provenance`, replace

```rust
        | Step::Withdrawn { outbox } => ("outbox", outbox.to_string(), None),
```

with

```rust
        | Step::Withdrawn { outbox }
        | Step::Refused { outbox, .. } => ("outbox", outbox.to_string(), None),
```

In `apply`, after `Step::Withdrawn { outbox } => withdrawn(db, stamps, *outbox),` add:

```rust
        Step::Refused { outbox, error, undo } => refused(db, stamps, *outbox, error, undo.as_ref()),
```

and after the function `withdrawn` add:

```rust
/// App state WhatsApp won't take goes into quarantine with why and its patch, in place of its row,
/// and what it set is put back. A row that's gone, or isn't app state still to send, is left alone.
fn refused(db: &Connection, stamps: &mut Stamps, outbox: i64, error: &str, undo: Option<&Undo>) -> Result<bool> {
    let row: Option<(String, String, Vec<u8>)> = db
        .prepare_cached(REFUSED_ROW)?
        .query_row([outbox], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()?;
    let Some((jid, id, raw)) = row else { return Ok(false) };
    super::record_quarantine(db, REFUSED, &format!("{jid} {id}"), error, Some(&raw))?;
    db.prepare_cached(DROP_ROW)?.execute([outbox])?;
    match undo {
        Some(undo) => chat_actions::undo(db, stamps, undo),
        None => Ok(false),
    }
}
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd store::write::outbox`
Expected: PASS, `app_state_whatsapp_refused_goes_into_quarantine_with_its_patch_and_what_it_set_is_put_back` and the plan test among them.

- [ ] **Step 5: Pin the new statement's plan**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && JAW_PLANS=write CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd store::plans && git diff crates/jawd/src/store/plans.txt`
Expected: one statement added under `-- src/store/write/outbox.rs`, `SELECT jid, id, raw FROM outbox WHERE n = ?1 AND kind = 'app_state' AND state = 'queued'`, planned `SEARCH outbox USING INTEGER PRIMARY KEY (rowid=?)`. Then `CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd store::plans` passes.

- [ ] **Step 6: Lint and commit**

```bash
cd /Users/pedrogomes/src/jaw-chat-actions
cargo fmt --all
CARGO_INCREMENTAL=0 cargo clippy -p jawd --tests -- -D warnings
git add crates/jawd/src/store/write/outbox.rs crates/jawd/src/store/plans.txt
git commit -m "App state WhatsApp refused for good, or that ran out of tries, is kept in quarantine with its error and its patch in place of its outbox row, never dropped, and what it set is put back in the same transaction (Step::Refused); a row that's gone or no longer queued is left alone, so the step done twice is done once."
```

---

### Task 6: The demo engine takes, echoes, refuses and fetches again app state

**Files:**
- Modify: `crates/jawd/src/engine/demo.rs` (module doc, `World`, two test knobs, `answer`, `patched`, `taken_in`, one test)

**Interfaces:**
- Consumes: nothing new.
- Produces, on `World` (test builds): `pub fn refuse_patches(&self, n: u32)` refuses the next `n` patches with `EngineError::Failed { message: "server returned error 400", retry: false }`; `pub fn hold_patches(&self, holding: bool)` takes patches without answering. `AppStatePatch` is recorded in `asked`, kept as the latest for its index, echoed as `pb::event::Kind::AppState` with the patch's index and value, acked before `Done`. `AppStateResync` is recorded in `asked` and sends the account's app state, if it has an account, then the patches taken in the named collections (every collection when `names` is empty), each waiting for its ack, then `Done`.

- [ ] **Step 1: Write the failing test**

In `crates/jawd/src/engine/demo.rs`'s `mod tests`, append:

```rust
    #[tokio::test]
    async fn an_app_state_patch_comes_back_as_app_state_before_its_answer_and_a_fetch_brings_back_what_was_taken() {
        let (tx, mut rx) = mpsc::channel(8);
        let engine = DemoEngine::with(Vec::new());
        let world = engine.world();
        let instance = engine.start(tx).await.unwrap();
        let pin = pb::AppStatePatch {
            name: "regular_low".into(),
            index: vec!["pin_v1".into(), ANA.into()],
            version: 5,
            value: vec![8, 1],
        };
        let answered = AtomicBool::new(false);
        let patching = async {
            let reply = instance.commands.call(pb::command::Kind::AppStatePatch(pin.clone())).await;
            answered.store(true, Ordering::SeqCst);
            reply
        };
        let acking = async {
            let incoming = tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.expect("no echo came").unwrap();
            let Some(pb::event::Kind::AppState(echo)) = &incoming.event.kind else { panic!("{:?}", incoming.event) };
            assert_eq!((&echo.index, &echo.value), (&pin.index, &pin.value), "the patch as it was sent");
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert!(!answered.load(Ordering::SeqCst), "answered before its echo was acked");
            incoming.ack.expect("an echo waits for its ack, as jaw-engine's does").send();
        };
        let (reply, ()) = tokio::join!(patching, acking);
        assert!(matches!(reply, Ok(pb::reply::Kind::Done(_))), "{reply:?}");
        world.refuse_patches(1);
        let other = pb::AppStatePatch { index: vec!["pin_v1".into(), CLIMBING.into()], ..pin.clone() };
        let refused = instance.commands.call(pb::command::Kind::AppStatePatch(other)).await;
        assert!(matches!(refused, Err(EngineError::Failed { retry: false, .. })), "{refused:?}");
        let fetch = |names: &[&str]| {
            let names = names.iter().map(|name| name.to_string()).collect();
            pb::command::Kind::AppStateResync(pb::AppStateResync { names })
        };
        let fetching = instance.commands.call(fetch(&["regular_low"]));
        let acking = async {
            let incoming =
                tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.expect("nothing came back").unwrap();
            let Some(pb::event::Kind::AppState(state)) = &incoming.event.kind else { panic!("{:?}", incoming.event) };
            assert_eq!(state.index, pin.index, "what WhatsApp took, not what it refused");
            incoming.ack.expect("a mutation needs an ack").send();
        };
        let (reply, ()) = tokio::join!(fetching, acking);
        assert!(matches!(reply, Ok(pb::reply::Kind::Done(_))), "{reply:?}");
        let elsewhere = instance.commands.call(fetch(&["regular_high"])).await;
        assert!(matches!(elsewhere, Ok(pb::reply::Kind::Done(_))), "{elsewhere:?}");
        assert!(rx.try_recv().is_err(), "nothing was taken in regular_high");
        let fetches =
            world.asked().into_iter().filter(|kind| matches!(kind, pb::command::Kind::AppStateResync(_))).count();
        assert_eq!(fetches, 2, "each fetch is noted");
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd engine::demo`
Expected: the build fails: `no method named 'refuse_patches' found for struct 'Arc<World>'`.

- [ ] **Step 3: Write the implementation**

In `crates/jawd/src/engine/demo.rs`, replace the module doc (its first eleven lines) with:

```rust
//! `DemoEngine`: a made-up account inside jawd, for `jawd --engine demo`, `jawd demo`, tests and the
//! load harness. It sends the same events a real engine does, waits for each message's ack as
//! whatsmeow would, and answers ping and bench. What it's sent goes to a made-up WhatsApp that
//! outlives any one engine, as the real one does: it answers with the server's time, the
//! recipients' phones send delivered and then read, and a message sent again with the same ID is
//! the same message. A reaction, an edit or a deletion for everyone is a message too, which the
//! recipients' phones say they have but never read; the server refuses an edit once WhatsApp's
//! window for it has passed; and app state sent for your other devices is taken, kept as the latest
//! for its index and sent back, as whatsmeow fetches a collection again after each patch, unless a
//! test has it refused. Given an `Account`, it answers for its contacts and groups, and writes each
//! history blob its phone offers to the spool as jaw-engine does; events can also be delivered from
//! outside its script, several waiting for their acks at once.
```

In `World`, after `fetched: Mutex<Vec<Fetched>>,` add:

```rust
    /// How many app-state patches to refuse next, as WhatsApp refuses one it won't take.
    refusing_patches: Mutex<u32>,
    /// Whether app-state patches are taken without an answer, as a connection that drops does.
    holding_patches: AtomicBool,
    /// The app-state patches the server has taken, the latest for each index, which a full fetch of
    /// their collection brings back, as WhatsApp keeps them for every device.
    taken: Mutex<Vec<pb::AppStatePatch>>,
```

After `pub fn refuse_next` add:

```rust
    /// Refuses the next `n` app-state patches for good.
    #[cfg(test)]
    pub fn refuse_patches(&self, n: u32) {
        *self.refusing_patches.lock().expect("the demo world") = n;
    }

    /// Takes app-state patches without answering them, until it's told otherwise.
    #[cfg(test)]
    pub fn hold_patches(&self, holding: bool) {
        self.holding_patches.store(holding, Ordering::SeqCst);
    }
```

After `fn ask` add:

```rust
    /// The patches taken in collections `names`, or in every collection when it names none, as a
    /// full fetch brings them.
    fn taken_in(&self, names: &[String]) -> Vec<pb::Event> {
        self.taken
            .lock()
            .expect("the demo world")
            .iter()
            .filter(|patch| names.is_empty() || names.contains(&patch.name))
            .map(|patch| {
                let state = pb::AppState { index: patch.index.clone(), value: patch.value.clone() };
                pb::Event { seq: 0, kind: Some(pb::event::Kind::AppState(state)) }
            })
            .collect()
    }
```

In `Sender::answer`, replace

```rust
            kind @ (pb::command::Kind::MarkRead(_)
            | pb::command::Kind::ChatPresence(_)
            | pb::command::Kind::Presence(_)
            | pb::command::Kind::AppStatePatch(_)) => {
                self.world.ask(kind);
                done
            }
```

with

```rust
            kind @ (pb::command::Kind::MarkRead(_)
            | pb::command::Kind::ChatPresence(_)
            | pb::command::Kind::Presence(_)) => {
                self.world.ask(kind);
                done
            }
            pb::command::Kind::AppStatePatch(patch) => {
                self.world.ask(pb::command::Kind::AppStatePatch(patch.clone()));
                self.patched(patch).await
            }
```

and replace the `AppStateResync` arm with:

```rust
            // Its app state, the account's and what it has taken since, comes a mutation at a time,
            // each waiting for its ack, as whatsmeow hands each over as it fetches a collection
            // again in full.
            pb::command::Kind::AppStateResync(resync) => {
                self.world.ask(pb::command::Kind::AppStateResync(resync.clone()));
                let mut mutations = self.world.account.as_ref().map(|account| account.app_state()).unwrap_or_default();
                mutations.extend(self.world.taken_in(&resync.names));
                let (began, mut acks) = (Instant::now(), Vec::new());
                for mutation in mutations {
                    let sent = Instant::now();
                    if !self.send(mutation).await {
                        return Err(EngineError::Gone);
                    }
                    acks.push(sent.elapsed());
                }
                self.world.fetched.lock().expect("the demo world").push(Fetched { took: began.elapsed(), acks });
                done
            }
```

After `fn account(&self)` add:

```rust
    /// WhatsApp takes an app-state patch, unless told to refuse it, and keeps it as the latest for
    /// its index; whatsmeow then fetches the collection again, so the patch comes back as an
    /// AppState event, acked before the patch is answered.
    async fn patched(&self, patch: pb::AppStatePatch) -> Result<pb::reply::Kind, EngineError> {
        let world = &self.world;
        {
            let mut refusing = world.refusing_patches.lock().expect("the demo world");
            if *refusing > 0 {
                *refusing -= 1;
                return Err(EngineError::Failed { message: "server returned error 400".into(), retry: false });
            }
        }
        if world.holding_patches.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        {
            let mut taken = world.taken.lock().expect("the demo world");
            taken.retain(|kept| kept.index != patch.index);
            taken.push(patch.clone());
        }
        let echo = pb::AppState { index: patch.index, value: patch.value };
        if !self.send(pb::Event { seq: 0, kind: Some(pb::event::Kind::AppState(echo)) }).await {
            return Err(EngineError::Gone);
        }
        Ok(pb::reply::Kind::Done(pb::Done {}))
    }
```

- [ ] **Step 4: Run the demo engine's tests, jawd demo's and sending's**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && cargo fmt --all && CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd -- engine::demo demo::tests send::tests`
Expected: PASS: the new test, `a_full_fetch_of_the_app_state_waits_for_each_mutations_ack_and_answers_once_all_are_in`, `a_first_runs_backlog_app_state_and_history_all_arrive_whole_at_once`, and every `send::tests` test (`deleting_for_you_is_done_at_once_and_your_other_devices_are_told` now sees its patch echoed).

- [ ] **Step 5: Lint and commit**

```bash
cd /Users/pedrogomes/src/jaw-chat-actions
CARGO_INCREMENTAL=0 cargo clippy -p jawd --tests -- -D warnings
git add crates/jawd/src/engine/demo.rs
git commit -m "The demo engine takes each app-state patch as WhatsApp would, keeping the latest for each index, and sends it back as an AppState event, acked before it answers, as whatsmeow fetches the collection again after a patch; a test can have it refuse the next patches or hold them unanswered, and a full fetch of a collection, now noted and served without an account, brings back what it took there after the account's own app state."
```

---

### Task 7: The patches, built as whatsmeow builds them

**Files:**
- Create: `crates/jawd/src/send/chat_actions.rs` (the builders and their tests)
- Modify: `crates/jawd/src/send/mod.rs` (declare the module)

**Interfaces:**
- Consumes: `send::actions::Target`, `convert::app_state::FOREVER`, `engine::pb`.
- Produces, in `send::chat_actions`:
  - `pub(super) struct Newest { pub id: String, pub at: i64, pub mine: bool, pub sender: Option<String> }`
  - `pub(super) fn pinned(chat: &str, pinned: bool, at: i64) -> pb::AppStatePatch`
  - `pub(super) fn archived(chat: &str, archived: bool, range: SyncActionMessageRange, at: i64) -> pb::AppStatePatch`
  - `pub(super) fn muted(chat: &str, until: i64, at: i64) -> pb::AppStatePatch` (0 unmutes, `FOREVER` or later is -1)
  - `pub(super) fn marked(chat: &str, unread: bool, range: SyncActionMessageRange, at: i64) -> pb::AppStatePatch`
  - `pub(super) fn starred(target: &Target, starred: bool, at: i64) -> pb::AppStatePatch`
  - `pub(super) fn range(chat: &str, group: bool, newest: Option<&Newest>, at: i64) -> SyncActionMessageRange`

- [ ] **Step 1: Write the failing tests**

In `crates/jawd/src/send/mod.rs`, after `pub mod bench;` add:

```rust
mod chat_actions;
```

Create `crates/jawd/src/send/chat_actions.rs` with only its tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const ANA: &str = "351912345678@s.whatsapp.net";
    const GROUP: &str = "120363012345678901@g.us";
    const RUI: &str = "351913333333@s.whatsapp.net";
    const AT: i64 = 1_792_000_000_123;

    fn value(patch: &pb::AppStatePatch) -> SyncActionValue {
        SyncActionValue::decode(patch.value.as_slice()).unwrap()
    }

    fn target(chat: &str, id: &str, mine: bool, sender: Option<&str>) -> Target {
        Target {
            chat: chat.into(),
            group: chat.ends_with("@g.us"),
            id: id.into(),
            mine,
            sender: sender.map(str::to_string),
            at: AT,
            kind: "text".into(),
            deleted: false,
            status: None,
            n: 1,
            chat_n: 1,
            view_once: false,
            forwarded: 0,
            raw: None,
            text: None,
            edited: false,
        }
    }

    #[test]
    fn a_pin_and_a_mute_are_the_mutations_whatsmeow_builds_with_the_time_jawd_wrote() {
        let pin = pinned(ANA, true, AT);
        assert_eq!(
            (pin.name.as_str(), pin.index.as_slice(), pin.version),
            ("regular_low", &["pin_v1".to_string(), ANA.to_string()][..], 5)
        );
        assert_eq!((value(&pin).timestamp, value(&pin).pin_action.unwrap().pinned), (Some(AT), Some(true)));
        assert_eq!(value(&pinned(ANA, false, AT)).pin_action.unwrap().pinned, Some(false));
        let mute = |until: i64| {
            let patch = muted(ANA, until, AT);
            let action = value(&patch).mute_action.unwrap();
            (patch.name, patch.index, patch.version, action.muted, action.mute_end_timestamp)
        };
        let index = vec!["mute".to_string(), ANA.to_string()];
        assert_eq!(mute(AT + 3_600_000), ("regular_high".into(), index.clone(), 2, Some(true), Some(AT + 3_600_000)));
        assert_eq!(mute(FOREVER), ("regular_high".into(), index.clone(), 2, Some(true), Some(-1)), "for ever is -1");
        assert_eq!(mute(0), ("regular_high".into(), index, 2, Some(false), None), "unmuted, with no end");
    }

    #[test]
    fn an_archive_and_a_read_name_the_chats_newest_message_in_seconds_as_whatsmeow_does() {
        let newest = Newest { id: "g9".into(), at: AT, mine: false, sender: Some(RUI.into()) };
        let archive = archived(GROUP, true, range(GROUP, true, Some(&newest), AT + 5), AT + 5);
        assert_eq!(
            (archive.name.as_str(), archive.index.as_slice(), archive.version),
            ("regular_low", &["archive".to_string(), GROUP.to_string()][..], 3)
        );
        let action = value(&archive).archive_chat_action.unwrap();
        let named = action.message_range.unwrap();
        assert_eq!((action.archived, named.last_message_timestamp), (Some(true), Some(AT / 1000)));
        let key = named.messages[0].key.clone().unwrap();
        assert_eq!(
            (key.remote_jid.as_deref(), key.id.as_deref(), key.from_me, key.participant.as_deref()),
            (Some(GROUP), Some("g9"), Some(false), Some(RUI))
        );
        assert_eq!(named.messages[0].timestamp, Some(AT / 1000));
        let mine = Newest { id: "d1".into(), at: AT, mine: true, sender: None };
        let read = marked(ANA, false, range(ANA, false, Some(&mine), AT), AT);
        assert_eq!((read.name.as_str(), read.index[0].as_str(), read.version), ("regular_low", "markChatAsRead", 3));
        let action = value(&read).mark_chat_as_read_action.unwrap();
        assert_eq!(action.read, Some(true));
        let key = action.message_range.unwrap().messages[0].key.clone().unwrap();
        assert_eq!((key.from_me, key.participant), (Some(true), None), "yours names nobody");
        let unread = marked(ANA, true, range(ANA, false, None, AT), AT);
        assert_eq!(value(&unread).mark_chat_as_read_action.unwrap().read, Some(false));
        let empty = range(ANA, false, None, AT);
        assert_eq!((empty.last_message_timestamp, empty.messages.len()), (Some(AT / 1000), 0), "no message: now");
    }

    #[test]
    fn a_star_names_its_message_as_a_deletion_for_you_does() {
        let index = |target: &Target| starred(target, true, AT).index;
        assert_eq!(index(&target(GROUP, "g1", false, Some(RUI))), ["star", GROUP, "g1", "0", RUI]);
        assert_eq!(index(&target(GROUP, "g2", true, None)), ["star", GROUP, "g2", "1", "0"]);
        assert_eq!(index(&target(ANA, "d1", false, Some(ANA))), ["star", ANA, "d1", "0", "0"]);
        assert_eq!(index(&target(ANA, "d2", true, None)), ["star", ANA, "d2", "1", "0"]);
        let star = starred(&target(ANA, "d1", false, Some(ANA)), false, AT);
        assert_eq!(
            (star.name.as_str(), star.version, value(&star).star_action.unwrap().starred, value(&star).timestamp),
            ("regular_high", 2, Some(false), Some(AT))
        );
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd send::chat_actions`
Expected: the build fails: `cannot find function 'pinned' in this scope`, `cannot find type 'Newest'`, and the like.

- [ ] **Step 3: Write the implementation**

Above the tests in `crates/jawd/src/send/chat_actions.rs`, put:

```rust
//! Chat actions from jaw: `chat.pin`, `chat.archive`, `chat.mute`, `chat.mark_unread` and `star`.
//! Each is done to its row at once, in one synced commit with the app-state patches that tell your
//! other devices, before it answers (store::chat_actions); the patches then go through the outbox
//! after the chat's earlier sends, as a deletion for you does. Each patch is built as whatsmeow's
//! builders build it and carries the time jawd wrote, so WhatsApp's echo of it through app state
//! finds the row as it is.

use prost::Message as _;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobuf_sync_action::{
    ArchiveChatAction, MarkChatAsReadAction, MuteAction, PinAction, StarAction, SyncActionMessage,
    SyncActionMessageRange, SyncActionValue,
};

use super::actions::Target;
use crate::convert::app_state::FOREVER;
use crate::engine::pb;

/// A chat's newest message, which an archive or a read names.
#[derive(Clone, Debug)]
pub(super) struct Newest {
    pub id: String,
    pub at: i64,
    pub mine: bool,
    /// Who sent it, by the JID it came from; `None` for yours.
    pub sender: Option<String>,
}

fn patch(name: &str, index: Vec<String>, version: i32, value: SyncActionValue) -> pb::AppStatePatch {
    pb::AppStatePatch { name: name.into(), index, version, value: value.encode_to_vec() }
}

/// Chat `chat` pinned at `at`, or unpinned (whatsmeow's BuildPin).
pub(super) fn pinned(chat: &str, pinned: bool, at: i64) -> pb::AppStatePatch {
    let action = PinAction { pinned: Some(pinned) };
    let value = SyncActionValue { timestamp: Some(at), pin_action: Some(action), ..Default::default() };
    patch("regular_low", vec!["pin_v1".into(), chat.into()], 5, value)
}

/// Chat `chat` archived up to `range`, or brought back: BuildArchive's first mutation. Its second,
/// the unpin, goes as a patch of its own (`pinned`), since the engine sends one mutation a patch.
pub(super) fn archived(chat: &str, archived: bool, range: SyncActionMessageRange, at: i64) -> pb::AppStatePatch {
    let action = ArchiveChatAction { archived: Some(archived), message_range: Some(range) };
    let value = SyncActionValue { timestamp: Some(at), archive_chat_action: Some(action), ..Default::default() };
    patch("regular_low", vec!["archive".into(), chat.into()], 3, value)
}

/// Chat `chat` muted until `until`, for ever from FOREVER on, or unmuted at 0 (BuildMuteAbs).
pub(super) fn muted(chat: &str, until: i64, at: i64) -> pb::AppStatePatch {
    let action = match until {
        0 => MuteAction { muted: Some(false), ..Default::default() },
        until if until >= FOREVER => MuteAction { muted: Some(true), mute_end_timestamp: Some(-1), ..Default::default() },
        until => MuteAction { muted: Some(true), mute_end_timestamp: Some(until), ..Default::default() },
    };
    let value = SyncActionValue { timestamp: Some(at), mute_action: Some(action), ..Default::default() };
    patch("regular_high", vec!["mute".into(), chat.into()], 2, value)
}

/// Chat `chat` marked unread, or read up to `range` (BuildMarkChatAsRead).
pub(super) fn marked(chat: &str, unread: bool, range: SyncActionMessageRange, at: i64) -> pb::AppStatePatch {
    let action = MarkChatAsReadAction { read: Some(!unread), message_range: Some(range) };
    let value = SyncActionValue { timestamp: Some(at), mark_chat_as_read_action: Some(action), ..Default::default() };
    patch("regular_low", vec!["markChatAsRead".into(), chat.into()], 3, value)
}

/// Message `target` starred, or its star taken off (BuildStar), named as a deletion for you names
/// it: its chat, its ID, whether it's yours, and who sent it in a group, "0" for yours and in a
/// one-to-one chat.
pub(super) fn starred(target: &Target, starred: bool, at: i64) -> pb::AppStatePatch {
    let sender = match (&target.sender, target.group && !target.mine) {
        (Some(sender), true) => sender.clone(),
        _ => "0".into(),
    };
    let mine = if target.mine { "1" } else { "0" };
    let index = vec!["star".into(), target.chat.clone(), target.id.clone(), mine.into(), sender];
    let action = StarAction { starred: Some(starred) };
    let value = SyncActionValue { timestamp: Some(at), star_action: Some(action), ..Default::default() };
    patch("regular_high", index, 2, value)
}

/// The messages an archive or a read names: the chat's newest, at its time in seconds, as app state
/// keeps them, or with none the time it's done at (whatsmeow's newMessageRange).
pub(super) fn range(chat: &str, group: bool, newest: Option<&Newest>, at: i64) -> SyncActionMessageRange {
    let Some(newest) = newest else {
        return SyncActionMessageRange { last_message_timestamp: Some(at.div_euclid(1000)), ..Default::default() };
    };
    let seconds = newest.at.div_euclid(1000);
    let key = MessageKey {
        remote_jid: Some(chat.into()),
        from_me: Some(newest.mine),
        id: Some(newest.id.clone()),
        participant: if group && !newest.mine { newest.sender.clone() } else { None },
    };
    SyncActionMessageRange {
        last_message_timestamp: Some(seconds),
        messages: vec![SyncActionMessage { key: Some(key), timestamp: Some(seconds) }],
        ..Default::default()
    }
}
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && cargo fmt --all && CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd send::chat_actions`
Expected: PASS, the three tests.

- [ ] **Step 5: Lint and commit**

```bash
cd /Users/pedrogomes/src/jaw-chat-actions
CARGO_INCREMENTAL=0 cargo clippy -p jawd --tests -- -D warnings
git add crates/jawd/src/send/chat_actions.rs crates/jawd/src/send/mod.rs
git commit -m "The app-state patches of the chat actions, built as whatsmeow's builders build them and carrying the time jawd writes: pin_v1 version 5, and archive and markChatAsRead version 3 naming the chat's newest message in seconds, in regular_low; mute version 2 with its end in milliseconds or -1 for ever, and star version 2 naming the message as a deletion for you does, in regular_high."
```

---

### Task 8: `chat.pin`, `chat.archive` and `chat.mute`

**Files:**
- Modify: `crates/jawd/src/send/chat_actions.rs` (module doc, imports, statements, the three requests, `act`)
- Modify: `crates/jawd/src/send/mod.rs` (module doc, imports, `Inner.live`, `call`, `METHODS`)
- Modify: `crates/jawd/src/send/tests.rs` (imports, helpers, seven tests, the plan test)
- Modify: `crates/jawd/src/store/plans.txt` (written again)

**Interfaces:**
- Consumes: Task 4's `Act`, `Does`, `Setting`, `Patch`, `Acted`, `Outcome`, `PINNED_MOST`; Task 7's builders; `send::outbox::{bad_param, message_id, queue}`, `actions::full`, `durably`, `query`, `no_chat`, `internal`, `now_ms`.
- Produces: `pub(super) async fn pin(inner: &Arc<Inner>, pin: ChatPin) -> Result<Value, Error>`, `archive(.., ChatArchive)`, `mute(.., ChatMute)`; `pub(super) const CHAT_TO_ACT_ON`, `pub(super) const NEWEST`; `fn act(inner: &Arc<Inner>, chat: String, does: Does, doing: &str) -> Result<Value, Error>` (Task 9 changes it); `Inner.live: Shared`.

- [ ] **Step 1: Write the failing tests**

In `crates/jawd/src/send/tests.rs`, add to the imports at the top:

```rust
use wa_proto::wa_web_protobuf_sync_action::{PinAction, SyncActionValue};
```

and remove the line `use wa_proto::wa_web_protobuf_sync_action::SyncActionValue;` at the start of `deleting_for_you_is_done_at_once_and_your_other_devices_are_told`, which the module now imports.

In `the_statements_sending_runs_itself_read_an_index_and_sort_nothing`, after the `actions::TARGET_ANYWHERE` entry add:

```rust
        (
            chat_actions::CHAT_TO_ACT_ON,
            "SEARCH c USING INDEX sqlite_autoindex_chats_1 (id=?)\nSEARCH m USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nSEARCH k USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN",
        ),
        (
            chat_actions::NEWEST,
            "SEARCH c USING INDEX sqlite_autoindex_chats_1 (id=?)\nSEARCH m USING INTEGER PRIMARY KEY (rowid=?)\nSEARCH j USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN",
        ),
```

Append to the file:

```rust
/// The app-state patches the engine was asked to send, in order.
fn patches_sent(running: &Running) -> Vec<pb::AppStatePatch> {
    asked(running, |k| match k {
        pb::command::Kind::AppStatePatch(patch) => Some(patch),
        _ => None,
    })
}

/// The collections the engine was asked to fetch again, each fetch's names.
fn fetched_again(running: &Running) -> Vec<Vec<String>> {
    asked(running, |k| match k {
        pb::command::Kind::AppStateResync(resync) => Some(resync.names),
        _ => None,
    })
}

fn value_of(patch: &pb::AppStatePatch) -> SyncActionValue {
    SyncActionValue::decode(patch.value.as_slice()).unwrap()
}

/// Chat `id` as the chat list shows it.
async fn chat_row(running: &Running, id: &str) -> read::ChatRow {
    let id = id.to_string();
    running
        .query(move |db| {
            let n = read::chat_n(db, &id)?.expect("a chat jawd has");
            Ok(read::chat(db, n)?.expect("its row"))
        })
        .await
}

/// When chat `id` was pinned, as jaw.db keeps it; 0 when it isn't.
async fn pinned_at(running: &Running, id: &str) -> i64 {
    let id = id.to_string();
    running.query(move |db| Ok(db.query_row("SELECT pinned FROM chats WHERE id = ?1", [id], |r| r.get(0))?)).await
}

/// Waits until WhatsApp's echo of `patch` through app state has been converted, which keeps its value
/// as the latest for its index.
async fn until_echoed(running: &Running, patch: &pb::AppStatePatch) {
    let (key, value) = (serde_json::to_string(&patch.index).unwrap(), patch.value.clone());
    eventually(running, "the patch's echo", move |db| {
        let kept: Option<Vec<u8>> =
            db.query_row("SELECT value FROM app_state WHERE key = ?1", [&key], |r| r.get(0)).optional()?;
        Ok(kept.as_ref() == Some(&value))
    })
    .await;
}

/// Fills the outbox with `rows` reactions waiting to send, which nothing sends.
fn fill_outbox(home: &Path, rows: i64) {
    let mut db = Connection::open(home.join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    let tx = db.transaction().unwrap();
    for n in 0..rows {
        tx.execute(
            "INSERT INTO outbox (jid, id, kind, raw, next_at, created_at) VALUES (?1, ?2, 'reaction', x'', 0, 0)",
            rusqlite::params![ANA, format!("w{n}")],
        )
        .unwrap();
    }
    tx.commit().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn pinning_a_chat_pins_it_now_tells_your_other_devices_and_its_echo_changes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    let before = chat_row(&running, ANA).await.v;
    let asked_at = now_ms();
    assert_eq!(running.call("chat.pin", json!({ "chat": ANA, "pinned": true })).await.unwrap(), json!({}));
    let pinned = chat_row(&running, ANA).await;
    let at = pinned_at(&running, ANA).await;
    assert!(pinned.pinned && pinned.v > before && at >= asked_at, "pinned now, with a new version");
    until_stored(&running).await;
    let sent = patches_sent(&running);
    assert_eq!(sent.len(), 1);
    assert_eq!(
        (sent[0].name.as_str(), sent[0].index.as_slice(), sent[0].version),
        ("regular_low", &["pin_v1".to_string(), ANA.to_string()][..], 5)
    );
    let value = value_of(&sent[0]);
    assert_eq!((value.timestamp, value.pin_action.unwrap().pinned), (Some(at), Some(true)), "the time jawd wrote");
    until_echoed(&running, &sent[0]).await;
    let echoed = chat_row(&running, ANA).await;
    assert_eq!((echoed.v, pinned_at(&running, ANA).await), (pinned.v, at), "the echo found it as it was");
    assert_eq!(running.call("chat.pin", json!({ "chat": ANA, "pinned": true })).await.unwrap(), json!({}));
    assert_eq!(chat_row(&running, ANA).await.v, pinned.v, "asked again, nothing changes");
    assert_eq!((patches_sent(&running).len(), running.store.count("outbox").unwrap()), (1, 0), "nothing goes");
    running.call("chat.pin", json!({ "chat": ANA, "pinned": false })).await.unwrap();
    until_stored(&running).await;
    let sent = patches_sent(&running);
    assert_eq!((sent.len(), value_of(&sent[1]).pin_action.unwrap().pinned), (2, Some(false)));
    assert!(!chat_row(&running, ANA).await.pinned);
}

#[tokio::test(flavor = "multi_thread")]
async fn at_most_three_chats_are_pinned_and_archived_ones_dont_count() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let chats: Vec<String> = (1..=5).map(|n| format!("35191000001{n}@s.whatsapp.net")).collect();
    let ids: Vec<&str> = chats.iter().map(String::as_str).collect();
    running.chats(&ids).await;
    // Pinned on the phone and archived there, as jaw shows it.
    let archived = Chat { id: chats[4].clone(), pinned: Some(1_000), archived: Some(true), ..Chat::default() };
    running.store.apply(Lane::Live, vec![Change::Chat(archived)]).await.unwrap();
    for id in &ids[..3] {
        running.call("chat.pin", json!({ "chat": id, "pinned": true })).await.unwrap();
    }
    let fourth = running.call("chat.pin", json!({ "chat": ids[3], "pinned": true })).await.unwrap_err();
    assert_eq!((fourth.code, fourth.data), (code::LIMIT, Some(json!({ "limit": "pinned", "most": 3 }))));
    assert!(!chat_row(&running, ids[3]).await.pinned, "not pinned");
    until_stored(&running).await;
    assert_eq!(patches_sent(&running).len(), 3, "and nothing sent for it");
    running.call("chat.pin", json!({ "chat": ids[0], "pinned": false })).await.unwrap();
    running.call("chat.pin", json!({ "chat": ids[3], "pinned": true })).await.unwrap();
    assert!(chat_row(&running, ids[3]).await.pinned, "once one is unpinned");
}

#[tokio::test(flavor = "multi_thread")]
async fn archiving_a_pinned_chat_unpins_it_and_tells_your_other_devices_in_two_patches_in_order() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.members(CLIMBING, &[RUI]).await;
    let at = now_ms() - 60_000;
    running.arrives(CLIMBING, RUI, "g1", at).await;
    running.call("chat.pin", json!({ "chat": CLIMBING, "pinned": true })).await.unwrap();
    running.call("chat.archive", json!({ "chat": CLIMBING, "archived": true })).await.unwrap();
    let group = chat_row(&running, CLIMBING).await;
    assert!(group.archived && !group.pinned, "archived, and unpinned as WhatsApp's apps do");
    until_stored(&running).await;
    let sent = patches_sent(&running);
    let kinds: Vec<(&str, i32)> = sent.iter().map(|p| (p.index[0].as_str(), p.version)).collect();
    assert_eq!(kinds, [("pin_v1", 5), ("archive", 3), ("pin_v1", 5)], "the archive, then the unpin");
    let archive = value_of(&sent[1]).archive_chat_action.unwrap();
    let named = archive.message_range.unwrap();
    let key = named.messages[0].key.clone().unwrap();
    assert_eq!((archive.archived, named.last_message_timestamp), (Some(true), Some(at / 1000)));
    assert_eq!((key.id.as_deref(), key.participant.as_deref()), (Some("g1"), Some(RUI)), "naming its newest");
    assert_eq!(value_of(&sent[2]).pin_action.unwrap().pinned, Some(false));
    until_echoed(&running, &sent[2]).await;
    assert_eq!(chat_row(&running, CLIMBING).await.v, group.v, "the echoes changed nothing");
    running.call("chat.archive", json!({ "chat": CLIMBING, "archived": false })).await.unwrap();
    until_stored(&running).await;
    let sent = patches_sent(&running);
    assert_eq!((sent.len(), value_of(&sent[3]).archive_chat_action.unwrap().archived), (4, Some(false)));
    let back = chat_row(&running, CLIMBING).await;
    assert!(!back.archived && !back.pinned, "brought back with one patch, and still unpinned");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_mute_lasts_until_a_time_or_for_ever_and_a_time_already_past_is_refused() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    let until = now_ms() + 8 * 3_600_000;
    running.call("chat.mute", json!({ "chat": ANA, "until": until })).await.unwrap();
    assert_eq!(chat_row(&running, ANA).await.muted_until, Some(until));
    running.call("chat.mute", json!({ "chat": ANA, "until": -1 })).await.unwrap();
    let forever = crate::convert::app_state::FOREVER;
    assert_eq!(chat_row(&running, ANA).await.muted_until, Some(forever), "for ever");
    running.call("chat.mute", json!({ "chat": ANA, "until": i64::MAX })).await.unwrap();
    running.call("chat.mute", json!({ "chat": ANA, "until": null })).await.unwrap();
    assert_eq!(chat_row(&running, ANA).await.muted_until, None, "unmuted");
    let past = running.call("chat.mute", json!({ "chat": ANA, "until": now_ms() - 1_000 })).await.unwrap_err();
    assert_eq!((past.code, past.data), (code::INVALID_PARAMS, Some(json!({ "param": "until" }))));
    until_stored(&running).await;
    let sent = patches_sent(&running);
    let ends: Vec<(Option<bool>, Option<i64>)> = sent
        .iter()
        .map(|p| {
            let action = value_of(p).mute_action.unwrap();
            (action.muted, action.mute_end_timestamp)
        })
        .collect();
    assert_eq!(ends, [(Some(true), Some(until)), (Some(true), Some(-1)), (Some(false), None)], "the largest time was for ever already");
    assert!(sent.iter().all(|p| (p.name.as_str(), p.version) == ("regular_high", 2)));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_jawd_doesnt_have_is_not_found_and_a_communitys_own_row_is_not_allowed() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let community = "120363000000000009@g.us";
    let own = Chat { id: community.into(), kind: Some("community"), ..Chat::default() };
    running.store.apply(Lane::Live, vec![Change::Chat(own)]).await.unwrap();
    for (chat, expected) in [("351999999999@s.whatsapp.net", code::NOT_FOUND), (community, code::NOT_ALLOWED)] {
        for (method, params) in [
            ("chat.pin", json!({ "chat": chat, "pinned": true })),
            ("chat.archive", json!({ "chat": chat, "archived": true })),
            ("chat.mute", json!({ "chat": chat, "until": -1 })),
        ] {
            assert_eq!(running.call(method, params).await.unwrap_err().code, expected, "{method} {chat}");
        }
    }
    assert_eq!(running.store.count("outbox").unwrap(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_action_needs_a_linked_account() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    running.chats(&[ANA]).await;
    for (method, params) in [
        ("chat.pin", json!({ "chat": ANA, "pinned": true })),
        ("chat.archive", json!({ "chat": ANA, "archived": true })),
        ("chat.mute", json!({ "chat": ANA, "until": null })),
    ] {
        assert_eq!(running.call(method, params).await.unwrap_err().code, code::NOT_LINKED, "{method}");
    }
    assert_eq!(running.store.count("outbox").unwrap(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn past_a_thousand_waiting_a_chat_action_is_refused_and_nothing_is_written() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA, CLIMBING]).await;
    running.call("chat.pin", json!({ "chat": CLIMBING, "pinned": true })).await.unwrap();
    until_stored(&running).await;
    until_offline(&running).await;
    fill_outbox(home.path(), 999);
    let full = |waiting: u64| Some(json!({ "limit": "outbox", "waiting": waiting, "failed": 0 }));
    let refused = running.call("chat.archive", json!({ "chat": CLIMBING, "archived": true })).await.unwrap_err();
    assert_eq!((refused.code, refused.data), (code::LIMIT, full(999)), "archiving a pinned chat takes two places");
    let group = chat_row(&running, CLIMBING).await;
    assert!(group.pinned && !group.archived, "nothing written");
    running.call("chat.pin", json!({ "chat": ANA, "pinned": true })).await.unwrap();
    let refused = running.call("chat.mute", json!({ "chat": ANA, "until": -1 })).await.unwrap_err();
    assert_eq!((refused.code, refused.data), (code::LIMIT, full(1_000)));
    assert_eq!(chat_row(&running, ANA).await.muted_until, None);
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd send::tests`
Expected: the build fails: `cannot find value 'CHAT_TO_ACT_ON' in module 'chat_actions'` and `cannot find value 'NEWEST'`. (Once those exist, the seven new tests fail with `jawd doesn't know chat.pin`, which Step 3 serves.)

- [ ] **Step 3: Write the implementation**

In `crates/jawd/src/send/chat_actions.rs`, replace the module doc and imports with:

```rust
//! Chat actions from jaw: `chat.pin`, `chat.archive`, `chat.mute`, `chat.mark_unread` and `star`.
//! Each is done to its row at once, in one synced commit with the app-state patches that tell your
//! other devices, before it answers (store::chat_actions); the patches then go through the outbox
//! after the chat's earlier sends, as a deletion for you does. Each patch is built as whatsmeow's
//! builders build it and carries the time jawd wrote, so WhatsApp's echo of it through app state
//! finds the row as it is.

use std::sync::Arc;

use jaw_proto::send::{ChatArchive, ChatMute, ChatPin};
use jaw_proto::{AccountState, Error, code};
use prost::Message as _;
use rusqlite::OptionalExtension;
use serde_json::Value;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobuf_sync_action::{
    ArchiveChatAction, MarkChatAsReadAction, MuteAction, PinAction, StarAction, SyncActionMessage,
    SyncActionMessageRange, SyncActionValue,
};

use super::actions::{self, Target};
use super::outbox::{bad_param, message_id, queue};
use super::{Inner, durably, internal, no_chat, now_ms, query};
use crate::convert::app_state::FOREVER;
use crate::engine::pb;
use crate::store::Change;
use crate::store::chat_actions::{Act, Acted, Does, Outcome, PINNED_MOST, Patch, Setting};

/// A chat an action names: the chat it's being merged into, or itself, with its kind.
pub(super) const CHAT_TO_ACT_ON: &str = "SELECT coalesce(k.id, c.id), coalesce(k.kind, c.kind) FROM chats c
    LEFT JOIN chat_merges m ON m.gone = c.n LEFT JOIN chats k ON k.n = m.keep WHERE c.id = ?1";
/// A chat's newest message, with who sent it.
pub(super) const NEWEST: &str = "SELECT m.id, m.at, m.mine, j.jid FROM chats c JOIN messages m ON m.n = c.last_message
    LEFT JOIN person_jids j ON j.n = m.sender WHERE c.id = ?1";
```

and below `range` add:

```rust
/// The chat an action names.
struct Named {
    /// Its JID as jaw keeps it.
    id: String,
    group: bool,
    newest: Option<Newest>,
}

/// Refuses what needs a linked account while none is.
fn linked(inner: &Inner) -> Result<(), Error> {
    match inner.live.lock().expect("live state").account.state {
        AccountState::Linked => Ok(()),
        AccountState::Unlinked | AccountState::Linking => {
            Err(Error::new(code::NOT_LINKED, "the account isn't linked; link it first"))
        }
    }
}

/// The chat `chat` names, with its newest message: error 3 for a chat jawd doesn't have, and 4 for
/// a community's own row, whose groups carry these.
async fn named(inner: &Inner, chat: &str) -> Result<Named, Error> {
    let asked = chat.to_string();
    let found = query(inner, move |db| {
        let Some((id, kind)) = db
            .prepare_cached(CHAT_TO_ACT_ON)?
            .query_row([&asked], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .optional()?
        else {
            return Ok(None);
        };
        let newest = db
            .prepare_cached(NEWEST)?
            .query_row([&id], |r| Ok(Newest { id: r.get(0)?, at: r.get(1)?, mine: r.get(2)?, sender: r.get(3)? }))
            .optional()?;
        Ok(Some((id, kind, newest)))
    })
    .await?;
    match found {
        None => Err(no_chat(chat)),
        Some((_, kind, _)) if kind == "community" => Err(community()),
        Some((id, kind, newest)) => Ok(Named { id, group: matches!(kind.as_str(), "group" | "announcement"), newest }),
    }
}

fn community() -> Error {
    Error::new(code::NOT_ALLOWED, "a community's own row has none of these; its groups do")
}

fn pinned_most() -> Error {
    let mut error = Error::new(
        code::LIMIT,
        format!("{PINNED_MOST} chats are pinned already, as many as WhatsApp allows; unpin one first"),
    );
    error.data = Some(serde_json::json!({ "limit": "pinned", "most": PINNED_MOST }));
    error
}

/// A patch to queue, with an ID of its own for its outbox row.
fn queued(patch: pb::AppStatePatch) -> Result<Patch, Error> {
    let id = message_id().map_err(|e| internal("make an ID for the patch", e.into()))?;
    Ok(Patch { id, raw: patch.encode_to_vec() })
}

pub(super) async fn pin(inner: &Arc<Inner>, pin: ChatPin) -> Result<Value, Error> {
    linked(inner)?;
    let chat = named(inner, &pin.chat).await?;
    let at = now_ms();
    let setting = Setting::Pin { at: pin.pinned.then_some(at), patch: queued(pinned(&chat.id, pin.pinned, at))? };
    act(inner, chat.id, Does::Set(setting), if pin.pinned { "pin the chat" } else { "unpin the chat" }).await
}

pub(super) async fn archive(inner: &Arc<Inner>, archive: ChatArchive) -> Result<Value, Error> {
    linked(inner)?;
    let chat = named(inner, &archive.chat).await?;
    let at = now_ms();
    let range = range(&chat.id, chat.group, chat.newest.as_ref(), at);
    let unpin = Some(queued(pinned(&chat.id, false, at))?);
    let patch = queued(archived(&chat.id, archive.archived, range, at))?;
    let setting = Setting::Archive { archived: archive.archived, patch, unpin };
    let doing = if archive.archived { "archive the chat" } else { "unarchive the chat" };
    act(inner, chat.id, Does::Set(setting), doing).await
}

pub(super) async fn mute(inner: &Arc<Inner>, mute: ChatMute) -> Result<Value, Error> {
    let at = now_ms();
    let until = match mute.until {
        None => 0,
        Some(until) if until == -1 || until >= FOREVER => FOREVER,
        Some(until) if until <= at => {
            return Err(bad_param("until", "that time has passed; mute until a later one, or -1 for ever"));
        }
        Some(until) => until,
    };
    linked(inner)?;
    let chat = named(inner, &mute.chat).await?;
    let setting = Setting::Mute { until, patch: queued(muted(&chat.id, until, at))? };
    act(inner, chat.id, Does::Set(setting), if until == 0 { "unmute the chat" } else { "mute the chat" }).await
}

/// Commits a chat action, synced, and queues the patches it wrote after the chat's earlier sends.
async fn act(inner: &Arc<Inner>, chat: String, does: Does, doing: &str) -> Result<Value, Error> {
    let outcome = Outcome::default();
    let _order = inner.enqueue.lock().await;
    let act = Act { chat: chat.clone(), does, outcome: outcome.clone() };
    durably(inner, vec![Change::ChatAction(act)], doing).await?;
    match outcome.take() {
        Some(Acted::Queued { chat, outbox, .. }) => {
            for outbox in outbox {
                queue(inner, outbox, chat.clone(), false);
            }
            Ok(Value::Object(Default::default()))
        }
        Some(Acted::Already) => Ok(Value::Object(Default::default())),
        Some(Acted::Gone) => Err(Error::new(code::NOT_FOUND, format!("{chat} went before jawd could {doing}"))),
        Some(Acted::Community) => Err(community()),
        Some(Acted::Pinned) => Err(pinned_most()),
        Some(Acted::Full { waiting, failed }) => Err(actions::full(waiting, failed)),
        None => Err(Error::new(code::INTERNAL, format!("jawd kept nothing, so it couldn't {doing}"))),
    }
}
```

In `crates/jawd/src/send/mod.rs`:

Replace the module doc's last sentence, from `reactions, edits and deletions go`, with:

```rust
//! name their people as WhatsApp writes them (`quote`); reactions, edits and deletions go as
//! messages of their own through the outbox (`actions`); `forward` sends messages again as new
//! ones of yours (`forward`); `message.info` gives each recipient's receipts, and `chat.info` a
//! chat's members (`info`); and what you set on a chat, or a star, goes to your other devices by
//! app state through the outbox (`chat_actions`).
```

Replace the `jaw_proto::send` import with:

```rust
use jaw_proto::send::{
    ChatArchive, ChatInfo, ChatMute, ChatPin, ChatStart, Delete, DraftSet, Edit, Forward, MessageInfo, React, Read,
    SendRetry, SendText, Typing,
};
```

In `Inner`, after `pushes: broadcast::Sender<Push>,` add:

```rust
    /// The account and the connection as sync keeps them, for what needs a linked account.
    live: Shared,
```

In `Sending::start`, in the `Inner { ... }` literal, after `pushes: notices,` add `live,`.

Replace `call`'s two-line doc with:

```rust
    /// Answers one of the sending methods: `send.text`, `send.retry`, `chat.start`, `draft.set`,
    /// `read`, `typing`, `chat.info`, the actions on a message and the chat actions.
```

and after the `"message.info"` arm add:

```rust
            "chat.pin" => chat_actions::pin(inner, params_of::<ChatPin>(method, params)?).await,
            "chat.archive" => chat_actions::archive(inner, params_of::<ChatArchive>(method, params)?).await,
            "chat.mute" => chat_actions::mute(inner, params_of::<ChatMute>(method, params)?).await,
```

and to `METHODS`, after `"message.info",` add:

```rust
        "chat.pin",
        "chat.archive",
        "chat.mute",
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && cargo fmt --all && CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd send::`
Expected: PASS, the seven new tests and every other `send::` test.

- [ ] **Step 5: Pin the new statements' plans**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && JAW_PLANS=write CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd store::plans && git diff crates/jawd/src/store/plans.txt`
Expected: two statements added under `-- src/send/chat_actions.rs`, planned as the plan test above says, with no `SCAN` and no `USE TEMP B-TREE`. Then `CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd store::plans` passes.

- [ ] **Step 6: Lint and commit**

```bash
cd /Users/pedrogomes/src/jaw-chat-actions
CARGO_INCREMENTAL=0 cargo clippy -p jawd --tests -- -D warnings
git add crates/jawd/src/send/chat_actions.rs crates/jawd/src/send/mod.rs crates/jawd/src/send/tests.rs crates/jawd/src/store/plans.txt
git commit -m "jawd serves chat.pin, chat.archive and chat.mute: each checks the account is linked (error 1) and the chat is one jawd has (3) and not a community's own row (4), commits the chat's change and its patches synced before it answers with nothing, and queues the patches after the chat's earlier sends; a fourth pinned chat is error 5 with limit pinned, an outbox without room is error 5 as for sends, a mute until a time already past is -32602 naming until, and -1 or any time from the last millisecond of 9999 on mutes for ever. Its two statements read an index each, pinned in plans.txt."
```

---

### Task 9: `chat.mark_unread`, with a read's receipts first, and `star`

**Files:**
- Modify: `crates/jawd/src/send/chat_actions.rs` (imports, `mark_unread`, `star`, `act`, `reads_the_chat`, `receipts_first`, one test)
- Modify: `crates/jawd/src/send/reads.rs` (`tell_now`)
- Modify: `crates/jawd/src/send/outbox.rs` (`attempt`)
- Modify: `crates/jawd/src/send/mod.rs` (imports, `call`, `METHODS`)
- Modify: `crates/jawd/src/send/tests.rs` (four tests)

**Interfaces:**
- Consumes: Task 8's `named`, `linked`, `queued`, `act`; Task 7's `marked`, `starred`, `range`; `actions::find`; `reads::want`.
- Produces: `pub(super) async fn mark_unread(inner: &Arc<Inner>, mark: ChatMarkUnread) -> Result<Value, Error>`, `pub(super) async fn star(inner: &Arc<Inner>, star: Star) -> Result<Value, Error>`, `pub(super) fn reads_the_chat(patch: &pb::AppStatePatch) -> bool`, `pub(super) async fn receipts_first(inner: &Inner, chat: &str) -> Result<(), EngineError>`, and in `reads`: `pub(super) async fn tell_now(inner: &Inner, chat: i64) -> anyhow::Result<()>`.

- [ ] **Step 1: Write the failing tests**

In `crates/jawd/src/send/chat_actions.rs`'s tests, append:

```rust
    #[test]
    fn only_a_patch_saying_the_chat_is_read_goes_after_its_receipts() {
        let none = range(ANA, false, None, AT);
        assert!(reads_the_chat(&marked(ANA, false, none.clone(), AT)));
        assert!(!reads_the_chat(&marked(ANA, true, none, AT)));
        assert!(!reads_the_chat(&pinned(ANA, true, AT)));
    }
```

In `crates/jawd/src/send/tests.rs`, append:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn marking_a_chat_unread_marks_it_and_marking_it_read_reads_it_its_receipts_going_first() {
    let home = tempfile::tempdir().unwrap();
    // A second between a chat's receipts, so a patch that didn't wait would go well before them.
    let timing = Timing { read_every: Duration::from_secs(1), ..quick() };
    let running = Running::timed(home.path(), world(), true, true, timing).await;
    running.arrives(ANA, ANA, "m1", now_ms() - 2_000).await;
    running.call("read", json!({ "chat": ANA, "up_to": "m1" })).await.unwrap();
    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::MarkRead(_)))).await;
    running.arrives(ANA, ANA, "m2", now_ms() - 1_000).await;
    running.call("chat.mark_unread", json!({ "chat": ANA, "unread": true })).await.unwrap();
    let marked = chat_row(&running, ANA).await;
    assert!(marked.marked_unread && marked.unread == 1);
    running.call("chat.mark_unread", json!({ "chat": ANA, "unread": false })).await.unwrap();
    let read = chat_row(&running, ANA).await;
    assert!(!read.marked_unread && read.unread == 0, "the mark gone, and read up to its newest");
    let receipt = |asked: &[pb::command::Kind]| {
        asked.iter().position(|k| matches!(k, pb::command::Kind::MarkRead(m) if m.ids == ["m2"]))
    };
    tokio::time::timeout(Duration::from_secs(5), running.world.until(|asked| receipt(asked).is_some()))
        .await
        .expect("m2's read receipt never went");
    until_stored(&running).await;
    let sent = patches_sent(&running);
    let reads: Vec<Option<bool>> = sent.iter().map(|p| value_of(p).mark_chat_as_read_action.unwrap().read).collect();
    assert_eq!(reads, [Some(false), Some(true)]);
    assert_eq!((sent[1].name.as_str(), sent[1].index[0].as_str(), sent[1].version), ("regular_low", "markChatAsRead", 3));
    let named = value_of(&sent[1]).mark_chat_as_read_action.unwrap().message_range.unwrap();
    assert_eq!(named.messages[0].key.clone().unwrap().id.as_deref(), Some("m2"));
    let asked = running.world.asked();
    let patch = asked.iter().position(|k| matches!(k, pb::command::Kind::AppStatePatch(p) if p == &sent[1]));
    assert!(matches!((receipt(&asked), patch), (Some(r), Some(p)) if r < p), "its receipt went before it");
    until_echoed(&running, &sent[1]).await;
    let echoed = chat_row(&running, ANA).await;
    assert_eq!((echoed.v, echoed.unread, echoed.marked_unread), (read.v, 0, false), "the echo changed nothing");
    running.call("chat.mark_unread", json!({ "chat": ANA, "unread": false })).await.unwrap();
    assert_eq!(patches_sent(&running).len(), 2, "read already, nothing more goes");
}

#[tokio::test(flavor = "multi_thread")]
async fn starring_a_message_stars_it_and_names_it_as_whatsapp_does() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.members(CLIMBING, &[RUI]).await;
    running.arrives(CLIMBING, RUI, "g1", now_ms() - 3_000).await;
    running.arrives(ANA, ANA, "d1", now_ms() - 2_000).await;
    let mine = running.send(ANA, "o meu").await;
    running.until_status(ANA, &mine, "sent").await;
    let before = row(&running, CLIMBING, "g1").await.unwrap().v;
    for (message, chat) in [("g1", Some(CLIMBING)), ("d1", None), (mine.as_str(), Some(ANA))] {
        let mut params = json!({ "message": message, "starred": true });
        if let Some(chat) = chat {
            params["chat"] = json!(chat);
        }
        assert_eq!(running.call("star", params).await.unwrap(), json!({}), "{message}");
    }
    let g1 = row(&running, CLIMBING, "g1").await.unwrap();
    assert!(g1.starred && g1.v > before, "starred, with a new version");
    until_stored(&running).await;
    let sent = patches_sent(&running);
    let indexes: Vec<Vec<String>> = sent.iter().map(|p| p.index.clone()).collect();
    let index = |parts: [&str; 5]| parts.iter().map(|part| part.to_string()).collect::<Vec<_>>();
    assert_eq!(
        indexes,
        [
            index(["star", CLIMBING, "g1", "0", RUI]),
            index(["star", ANA, "d1", "0", "0"]),
            index(["star", ANA, mine.as_str(), "1", "0"]),
        ]
    );
    assert!(sent.iter().all(|p| (p.name.as_str(), p.version) == ("regular_high", 2)));
    until_echoed(&running, &sent[2]).await;
    assert_eq!(row(&running, CLIMBING, "g1").await.unwrap().v, g1.v, "the echo changed nothing");
    running.call("star", json!({ "message": "g1", "starred": true })).await.unwrap();
    running.call("star", json!({ "message": "g1", "starred": false })).await.unwrap();
    assert!(!row(&running, CLIMBING, "g1").await.unwrap().starred);
    until_stored(&running).await;
    let sent = patches_sent(&running);
    assert_eq!((sent.len(), value_of(&sent[3]).star_action.unwrap().starred), (4, Some(false)), "asked again, nothing");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_line_about_the_chat_or_a_deleted_message_isnt_starred_but_a_star_can_always_go() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let line = Message {
        chat: ANA.into(),
        id: "s1".into(),
        at: now_ms() - 3_000,
        kind: "system",
        summary: "Ana changed her number".into(),
        live: true,
        ..Message::default()
    };
    running.store.apply(Lane::Live, vec![Change::Message(line)]).await.unwrap();
    running.arrives(ANA, ANA, "gone", now_ms() - 2_000).await;
    running.call("star", json!({ "message": "gone", "starred": true })).await.unwrap();
    let revoke = Amend {
        chat: ANA.into(),
        target: "gone".into(),
        sender: Some(ANA.into()),
        mine: false,
        at: now_ms(),
        what: Amendment::Revoke,
        raw: None,
    };
    running.store.apply(Lane::Live, vec![Change::Amend(revoke)]).await.unwrap();
    for message in ["s1", "gone"] {
        let refused = running.call("star", json!({ "message": message, "starred": true, "chat": ANA })).await;
        assert_eq!(refused.unwrap_err().code, code::NOT_ALLOWED, "{message}");
    }
    running.call("star", json!({ "message": "gone", "starred": false })).await.unwrap();
    assert!(!row(&running, ANA, "gone").await.unwrap().starred, "a deleted message's star can go");
}

#[tokio::test(flavor = "multi_thread")]
async fn marking_and_starring_refuse_what_the_other_chat_actions_refuse() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let community = "120363000000000009@g.us";
    let own = Chat { id: community.into(), kind: Some("community"), ..Chat::default() };
    running.store.apply(Lane::Live, vec![Change::Chat(own)]).await.unwrap();
    running.arrives(ANA, ANA, "m1", now_ms() - 1_000).await;
    let mark = |chat: &str| json!({ "chat": chat, "unread": true });
    let missing = running.call("chat.mark_unread", mark("351999999999@s.whatsapp.net")).await.unwrap_err();
    let own_row = running.call("chat.mark_unread", mark(community)).await.unwrap_err();
    let nothing = running.call("star", json!({ "message": "nope", "starred": true })).await.unwrap_err();
    assert_eq!((missing.code, own_row.code, nothing.code), (code::NOT_FOUND, code::NOT_ALLOWED, code::NOT_FOUND));
    until_offline(&running).await;
    fill_outbox(home.path(), 1_000);
    let full = Some(json!({ "limit": "outbox", "waiting": 1_000, "failed": 0 }));
    for (method, params) in [("chat.mark_unread", mark(ANA)), ("star", json!({ "message": "m1", "starred": true }))] {
        let refused = running.call(method, params).await.unwrap_err();
        assert_eq!((refused.code, refused.data), (code::LIMIT, full.clone()), "{method}");
    }
    assert!(!chat_row(&running, ANA).await.marked_unread && !row(&running, ANA, "m1").await.unwrap().starred);
    let elsewhere = tempfile::tempdir().unwrap();
    let unlinked = Running::unconnected(elsewhere.path()).await;
    unlinked.arrives(ANA, ANA, "m1", now_ms()).await;
    for (method, params) in [("chat.mark_unread", mark(ANA)), ("star", json!({ "message": "m1", "starred": true }))] {
        assert_eq!(unlinked.call(method, params).await.unwrap_err().code, code::NOT_LINKED, "{method}");
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd send::`
Expected: the build fails with `cannot find function 'reads_the_chat' in this scope`; once that exists, the new `send::tests` fail with `jawd doesn't know chat.mark_unread` and `jawd doesn't know star`.

- [ ] **Step 3: Write the implementation**

In `crates/jawd/src/send/chat_actions.rs`, add to the module doc, after `finds the row as it is.`:

```rust
//! A patch saying a chat is read goes after the chat's read receipts (`receipts_first`).
```

Replace the file's `use` lines with:

```rust
use std::sync::Arc;

use jaw_proto::send::{ChatArchive, ChatMarkUnread, ChatMute, ChatPin, Star};
use jaw_proto::{AccountState, Error, code};
use prost::Message as _;
use rusqlite::OptionalExtension;
use serde_json::Value;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobuf_sync_action::{
    ArchiveChatAction, MarkChatAsReadAction, MuteAction, PinAction, StarAction, SyncActionMessage,
    SyncActionMessageRange, SyncActionValue,
};

use super::actions::{self, Target};
use super::outbox::{bad_param, message_id, queue};
use super::{Inner, durably, internal, no_chat, now_ms, query};
use crate::convert::app_state::FOREVER;
use crate::engine::{EngineError, pb};
use crate::store::chat_actions::{Act, Acted, Does, Outcome, PINNED_MOST, Patch, Setting};
use crate::store::{Change, read};
```

After `mute` add:

```rust
pub(super) async fn mark_unread(inner: &Arc<Inner>, mark: ChatMarkUnread) -> Result<Value, Error> {
    linked(inner)?;
    let chat = named(inner, &mark.chat).await?;
    let at = now_ms();
    let range = range(&chat.id, chat.group, chat.newest.as_ref(), at);
    let patch = queued(marked(&chat.id, mark.unread, range, at))?;
    let setting = Setting::MarkUnread { unread: mark.unread, newest: chat.newest.map(|newest| newest.id), at, patch };
    let doing = if mark.unread { "mark the chat unread" } else { "mark the chat read" };
    act(inner, chat.id, Does::Set(setting), doing).await
}

pub(super) async fn star(inner: &Arc<Inner>, star: Star) -> Result<Value, Error> {
    linked(inner)?;
    let target = actions::find(inner, star.chat, star.message).await?;
    if star.starred && matches!(target.kind.as_str(), "system" | "call") {
        return Err(Error::new(code::NOT_ALLOWED, "a line about the chat can't be starred"));
    }
    if star.starred && target.deleted {
        return Err(Error::new(code::NOT_ALLOWED, "a deleted message can't be starred"));
    }
    let patch = queued(starred(&target, star.starred, now_ms()))?;
    let does = Does::Star { id: target.id.clone(), starred: star.starred, patch };
    act(inner, target.chat.clone(), does, if star.starred { "star the message" } else { "unstar the message" }).await
}

/// Whether a patch says its chat is read: it goes after the chat's read receipts, since WhatsApp's
/// echo of it at jaw's own position settles the receipts jaw owes there.
pub(super) fn reads_the_chat(patch: &pb::AppStatePatch) -> bool {
    patch.index.first().is_some_and(|kind| kind == "markChatAsRead")
        && SyncActionValue::decode(patch.value.as_slice())
            .ok()
            .and_then(|value| value.mark_chat_as_read_action)
            .and_then(|action| action.read)
            .unwrap_or(false)
}

/// Sends the read receipts chat `chat` owes, at once, before its patch saying it's read; a failure is
/// the patch's, tried again as it is.
pub(super) async fn receipts_first(inner: &Inner, chat: &str) -> Result<(), EngineError> {
    let failed = |why: String| EngineError::Failed { message: format!("its read receipts didn't go first: {why}"), retry: true };
    let jid = chat.to_string();
    let Some(n) = query(inner, move |db| read::chat_n(db, &jid)).await.map_err(|e| failed(e.message))? else {
        return Ok(());
    };
    super::reads::tell_now(inner, n).await.map_err(|e| failed(format!("{e:#}")))
}
```

Replace `act` with:

```rust
/// Commits a chat action, synced, and queues the patches it wrote after the chat's earlier sends; a
/// chat read here owes its read receipts.
async fn act(inner: &Arc<Inner>, chat: String, does: Does, doing: &str) -> Result<Value, Error> {
    let owes_receipts = matches!(does, Does::Set(Setting::MarkUnread { unread: false, .. }));
    let outcome = Outcome::default();
    let _order = inner.enqueue.lock().await;
    let act = Act { chat: chat.clone(), does, outcome: outcome.clone() };
    durably(inner, vec![Change::ChatAction(act)], doing).await?;
    match outcome.take() {
        Some(Acted::Queued { chat, chat_n, outbox }) => {
            for outbox in outbox {
                queue(inner, outbox, chat.clone(), false);
            }
            if owes_receipts {
                super::reads::want(inner, chat_n);
            }
            Ok(Value::Object(Default::default()))
        }
        Some(Acted::Already) => Ok(Value::Object(Default::default())),
        Some(Acted::Gone) => Err(Error::new(code::NOT_FOUND, format!("{chat} went before jawd could {doing}"))),
        Some(Acted::Community) => Err(community()),
        Some(Acted::Pinned) => Err(pinned_most()),
        Some(Acted::Full { waiting, failed }) => Err(actions::full(waiting, failed)),
        None => Err(Error::new(code::INTERNAL, format!("jawd kept nothing, so it couldn't {doing}"))),
    }
}
```

In `crates/jawd/src/send/reads.rs`, after `want` add:

```rust
/// Tells WhatsApp at once everything chat `chat` owes, a receipt's worth at a time, for a patch
/// saying the chat is read, which goes after them.
pub(super) async fn tell_now(inner: &Inner, chat: i64) -> anyhow::Result<()> {
    loop {
        match tell(inner, chat).await? {
            Told::More => {}
            Told::All => return Ok(()),
            Told::Merging { .. } => anyhow::bail!("the chat is being merged; its receipts go once it is"),
        }
    }
}
```

In `crates/jawd/src/send/outbox.rs`, replace `attempt` with:

```rust
/// Tries to send outbox row `outbox` once. It reads the row again first, so a message sent, failed
/// or deleted meanwhile isn't sent, and nothing is tried while WhatsApp can't be reached. What it
/// waits for before the next try, it waits for without holding a place in the sends' lane.
async fn attempt(inner: Arc<Inner>, outbox: i64, resumed: bool) -> Next {
    let mut connected = inner.connected.subscribe();
    if !*connected.borrow_and_update() {
        return Next::After(Box::pin(async move {
            let _ = connected.wait_for(|connected| *connected).await;
        }));
    }
    let row = match inner.store.reads().query(Priority::Interactive, move |db| outbox::outboxed_by_n(db, outbox)).await
    {
        Ok(Some(row)) => row,
        Ok(None) => return Next::Done,
        Err(e) => {
            eprintln!("jawd: can't read outbox row {outbox}, so it's tried again: {e:#}");
            return Next::After(Box::pin(tokio::time::sleep(inner.timing.retry_most)));
        }
    };
    if row.state != "queued" {
        return Next::Done;
    }
    if row.withdrawn() {
        let applied = settle(&inner, vec![Change::Outbox(Step::Withdrawn { outbox })]).await;
        if applied.changed > 0 {
            super::actions::withdrawn(&inner, &row.kind);
        }
        return Next::Done;
    }
    if row.held() {
        inner.held.hold(outbox);
        return Next::Done;
    }
    if reached_server(row.status.as_deref()) {
        settle(&inner, vec![Change::Outbox(Step::Sent { outbox })]).await;
        return Next::Done;
    }
    // App state is the same however often it's sent, and no receipt ever says it arrived.
    let app_state = row.kind == "app_state";
    let may_have_gone = (resumed || row.attempts > 0) && !app_state;
    let patch = if app_state {
        match pb::AppStatePatch::decode(row.raw.as_slice()) {
            Ok(patch) => Some(patch),
            Err(e) => {
                let error = format!("its app state doesn't decode: {e}");
                settle(&inner, vec![Change::Outbox(Step::Failed { outbox, error, next_at: None })]).await;
                return Next::Done;
            }
        }
    } else {
        None
    };
    let command = match &patch {
        Some(patch) => pb::command::Kind::AppStatePatch(patch.clone()),
        None => pb::command::Kind::Send(pb::Send { chat: row.chat.clone(), id: row.id.clone(), raw: row.raw.clone() }),
    };
    let receipts = match &patch {
        Some(patch) if super::chat_actions::reads_the_chat(patch) => {
            super::chat_actions::receipts_first(&inner, &row.chat).await
        }
        _ => Ok(()),
    };
    let failure = match receipts {
        Err(failure) => failure,
        Ok(()) => match engine(&inner, command).await {
            Ok(pb::reply::Kind::Sent(sent)) => {
                settle(&inner, sent_changes(&row, &sent)).await;
                return Next::Done;
            }
            Ok(pb::reply::Kind::Done(_)) if app_state => {
                settle(&inner, vec![Change::Outbox(Step::Sent { outbox })]).await;
                return Next::Done;
            }
            Ok(other) => {
                EngineError::Failed { message: format!("the engine answered a send with {other:?}"), retry: false }
            }
            Err(e) => e,
        },
    };
    if may_have_gone {
        eprintln!("jawd: WhatsApp answered {} in {} sent again with: {failure}", row.id, row.chat);
    }
    let gives_up = !retryable(&failure) || row.attempts + 1 >= ATTEMPTS_MOST;
    let step = match (gives_up, may_have_gone) {
        (false, _) => {
            let next_at = now_ms() + pause(&inner, row.attempts + 1).as_millis() as i64;
            Step::Failed { outbox, error: failure.to_string(), next_at: Some(next_at) }
        }
        (true, true) => Step::Unconfirmed {
            outbox,
            error: failure.to_string(),
            until: now_ms() + inner.timing.unconfirmed_for.as_millis() as i64,
        },
        (true, false) => Step::Failed { outbox, error: failure.to_string(), next_at: None },
    };
    let unconfirming = matches!(step, Step::Unconfirmed { .. });
    let refused = matches!(step, Step::Failed { next_at: None, .. });
    settle(&inner, vec![Change::Outbox(step)]).await;
    if unconfirming {
        unconfirmed(&inner, outbox, inner.timing.unconfirmed_for);
    }
    if gives_up {
        eprintln!("jawd: WhatsApp refused {} in {}: {failure}", row.id, row.chat);
        if refused {
            super::actions::refused(&inner, &row.kind, &failure.to_string());
        }
        return Next::Done;
    }
    let pause = pause(&inner, row.attempts + 1);
    Next::After(Box::pin(async move {
        tokio::select! {
            _ = tokio::time::sleep(pause) => {}
            _ = connected.changed() => {}
        }
    }))
}
```

In `crates/jawd/src/send/mod.rs`, replace the `jaw_proto::send` import with:

```rust
use jaw_proto::send::{
    ChatArchive, ChatInfo, ChatMarkUnread, ChatMute, ChatPin, ChatStart, Delete, DraftSet, Edit, Forward, MessageInfo,
    React, Read, SendRetry, SendText, Star, Typing,
};
```

after the `"chat.mute"` arm add:

```rust
            "chat.mark_unread" => chat_actions::mark_unread(inner, params_of::<ChatMarkUnread>(method, params)?).await,
            "star" => chat_actions::star(inner, params_of::<Star>(method, params)?).await,
```

and to `METHODS`, after `"chat.mute",` add:

```rust
        "chat.mark_unread",
        "star",
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && cargo fmt --all && CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd send::`
Expected: PASS, the four new `send::tests`, `only_a_patch_saying_the_chat_is_read_goes_after_its_receipts`, and every other `send::` test.

- [ ] **Step 5: Check the receipts test fails without receipts first**

Run: in `attempt`, replace `Some(patch) if super::chat_actions::reads_the_chat(patch) =>` with `Some(patch) if false && super::chat_actions::reads_the_chat(patch) =>`, then `CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd send::tests::marking_a_chat_unread_marks_it_and_marking_it_read_reads_it_its_receipts_going_first`
Expected: FAIL with `m2's read receipt never went`. Then put the line back and run it again: PASS.

- [ ] **Step 6: Lint and commit**

```bash
cd /Users/pedrogomes/src/jaw-chat-actions
git diff --stat
CARGO_INCREMENTAL=0 cargo clippy -p jawd --tests -- -D warnings
git add crates/jawd/src/send/chat_actions.rs crates/jawd/src/send/reads.rs crates/jawd/src/send/outbox.rs crates/jawd/src/send/mod.rs crates/jawd/src/send/tests.rs
git commit -m "jawd serves chat.mark_unread and star: marking unread sets the mark, marking read takes it off and reads the chat here up to its newest message, owing its read receipts, and star stars or unstars a message, refusing to star a line about the chat or a message deleted for everyone (error 4); a patch saying a chat is read sends the chat's receipts at once before it goes, since WhatsApp's echo of it at jaw's own position settles the receipts owed there, which a patch that went first would leave never sent, as its test shows with the wait switched off."
```

---

### Task 10: A refused patch is kept, put back, said, and its collection fetched again

> **Since task 8's fix:** the store's `refused` releases the index's `app_state_awaiting` row and runs the undo only when that row named the refused patch, so a refused patch a later one has superseded puts nothing back. The handler below still passes the undo its index gives, and still fetches the collection again whenever there is one, which brings back what the converter held back. Its tests queue their patches through the requests, so each has its awaited row. Its fetch again (`fetch_again`) takes the app-state lock (`sync::AppStateTurn`, `Inner.app_state`) the outbox's app-state attempts and the engine start's full fetch take, across its `AppStateResync`, so it's never handed over while a patch is on its way; the attempt has let go of the lock by the time it handles the engine's answer, so calling `fetch_again` from there is safe. A patch the demo engine holds unanswered (`hold_patches`) keeps the lock until it's answered or its engine goes, so a test that holds a patch and then expects a fetch again or another patch to go lets it go first.

**Files:**
- Modify: `crates/jawd/src/send/chat_actions.rs` (module doc, imports, `undo_of`, `what_it_did`, `refused`, `fetch_again`, `say`, `FETCH_TRIES`, one test)
- Modify: `crates/jawd/src/send/outbox.rs` (`attempt`; `settle` becomes `pub(super)`)
- Modify: `crates/jawd/src/send/actions.rs` (`refused` loses its app-state arm)
- Modify: `crates/jaw-proto/src/lib.rs` (`quarantined`'s doc)
- Modify: `crates/jawd/src/send/tests.rs` (two tests)

**Interfaces:**
- Consumes: Task 5's `Step::Refused`; Task 6's `World::refuse_patches`; Task 9's `attempt`.
- Produces: `pub(super) fn undo_of(index: &[String]) -> Option<Undo>`, `pub(super) fn what_it_did(patch: &pb::AppStatePatch) -> &'static str`, `pub(super) async fn refused(inner: &Inner, outbox: i64, patch: Option<&pb::AppStatePatch>, error: String)`.

- [ ] **Step 1: Write the failing tests**

In `crates/jawd/src/send/chat_actions.rs`'s tests, append:

```rust
    #[test]
    fn a_refused_patch_puts_back_what_it_set_and_is_said_as_what_it_did() {
        let none = || range(ANA, false, None, AT);
        let undo = |patch: pb::AppStatePatch| undo_of(&patch.index);
        assert_eq!(undo(pinned(ANA, true, AT)), Some(Undo::Pin { chat: ANA.into() }));
        assert_eq!(undo(archived(ANA, true, none(), AT)), Some(Undo::Archive { chat: ANA.into() }));
        assert_eq!(undo(muted(ANA, FOREVER, AT)), Some(Undo::Mute { chat: ANA.into() }));
        assert_eq!(undo(marked(ANA, true, none(), AT)), Some(Undo::MarkUnread { chat: ANA.into() }));
        let star = starred(&target(ANA, "d1", false, Some(ANA)), true, AT);
        assert_eq!(undo(star), Some(Undo::Star { chat: ANA.into(), id: "d1".into() }));
        let deletion = pb::AppStatePatch {
            name: "regular_high".into(),
            index: ["deleteMessageForMe", ANA, "d1", "0", "0"].iter().map(|part| part.to_string()).collect(),
            version: 3,
            value: Vec::new(),
        };
        assert_eq!(undo_of(&deletion.index), None, "a message deleted for you stays deleted here");
        let said: Vec<&str> = [
            pinned(ANA, true, AT),
            pinned(ANA, false, AT),
            archived(ANA, true, none(), AT),
            muted(ANA, 0, AT),
            marked(ANA, false, none(), AT),
            starred(&target(ANA, "d1", false, None), false, AT),
            deletion,
        ]
        .iter()
        .map(what_it_did)
        .collect();
        assert_eq!(
            said,
            [
                "pinning a chat",
                "unpinning a chat",
                "archiving a chat",
                "unmuting a chat",
                "marking a chat read",
                "unstarring a message",
                "telling your other devices you deleted a message"
            ]
        );
    }
```

In `crates/jawd/src/send/tests.rs`, append:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn a_patch_whatsapp_refuses_is_kept_in_quarantine_put_back_as_the_phone_has_it_and_said() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA, CLIMBING]).await;
    let mut pushes = running.pushes.subscribe();
    running.call("chat.pin", json!({ "chat": CLIMBING, "pinned": true })).await.unwrap();
    until_stored(&running).await;
    // WhatsApp won't take the unpin, so the group ends pinned, as WhatsApp keeps it.
    running.world.refuse_patches(1);
    running.call("chat.pin", json!({ "chat": CLIMBING, "pinned": false })).await.unwrap();
    assert!(!chat_row(&running, CLIMBING).await.pinned, "shown at once");
    running
        .until("the group pinned again", |db| {
            Ok(db.query_row("SELECT pinned FROM chats WHERE id = ?1", [CLIMBING], |r| r.get::<_, i64>(0))? > 0)
        })
        .await;
    // Nor the pin of a chat nothing ever pinned, which ends unpinned, as WhatsApp has it.
    running.world.refuse_patches(1);
    running.call("chat.pin", json!({ "chat": ANA, "pinned": true })).await.unwrap();
    until_stored(&running).await;
    running
        .until("Ana's chat unpinned again", |db| {
            Ok(db.query_row("SELECT pinned FROM chats WHERE id = ?1", [ANA], |r| r.get::<_, i64>(0))? == 0)
        })
        .await;
    type Kept = (String, String, Vec<u8>);
    let kept: Vec<Kept> = running
        .query(|db| {
            Ok(db
                .prepare("SELECT source, error, raw FROM quarantine ORDER BY n")?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await;
    let indexes: Vec<(String, String, Vec<String>)> = kept
        .into_iter()
        .map(|(source, error, raw)| (source, error, pb::AppStatePatch::decode(raw.as_slice()).unwrap().index))
        .collect();
    let index = |chat: &str| vec!["pin_v1".to_string(), chat.to_string()];
    let refused = |chat: &str| ("app_state_patch".to_string(), "server returned error 400".to_string(), index(chat));
    assert_eq!(indexes, [refused(CLIMBING), refused(ANA)], "each kept with why and its patch, never dropped");
    let said = notices(&mut pushes, Duration::from_millis(300)).await;
    assert_eq!(
        said,
        ["WhatsApp refused unpinning a chat: server returned error 400", "WhatsApp refused pinning a chat: server returned error 400"]
    );
    let low = |asked: &[pb::command::Kind]| {
        asked.iter().filter(|k| matches!(k, pb::command::Kind::AppStateResync(r) if r.names == ["regular_low"])).count()
    };
    // Each refusal fetches its collection again.
    running.world.until(|asked| low(asked) == 2).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn telling_your_other_devices_of_a_deletion_for_you_that_whatsapp_refuses_is_kept_in_quarantine() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.arrives(ANA, ANA, "m1", now_ms() - 1_000).await;
    let mut pushes = running.pushes.subscribe();
    running.world.refuse_patches(1);
    running.call("delete", json!({ "message": "m1", "for": "me" })).await.unwrap();
    until_stored(&running).await;
    assert!(row(&running, ANA, "m1").await.is_none(), "deleted here all the same");
    assert_eq!(running.store.count("quarantine").unwrap(), 1, "what WhatsApp refused is kept, not dropped");
    let said = notices(&mut pushes, Duration::from_millis(300)).await;
    assert_eq!(said, ["WhatsApp refused telling your other devices you deleted a message: server returned error 400"]);
    let high = fetched_again(&running).iter().any(|names| names == &["regular_high".to_string()]);
    assert!(!high, "nothing to put back, so nothing fetched");
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd send::`
Expected: the build fails with `cannot find function 'undo_of'` and `cannot find function 'what_it_did'`; once they exist, `a_patch_whatsapp_refuses_...` fails at `the group pinned again` (nothing is fetched again) and the deletion's test at `what WhatsApp refused is kept, not dropped` (left 0).

- [ ] **Step 3: Write the implementation**

In `crates/jawd/src/send/chat_actions.rs`, replace the module doc's last line, `//! A patch saying a chat is read goes after the chat's read receipts (`receipts_first`).`, with:

```rust
//! A patch saying a chat is read goes after the chat's read receipts (`receipts_first`); one
//! WhatsApp refuses for good, or that runs out of tries, is kept in quarantine, what it set is put
//! back, every interface is told, and its collection is fetched again, so the chat ends as the
//! phone has it (`refused`).
```

Replace the file's `use` lines with:

```rust
use std::sync::Arc;

use jaw_proto::send::{ChatArchive, ChatMarkUnread, ChatMute, ChatPin, Star};
use jaw_proto::sync::{Notice, Tone};
use jaw_proto::{AccountState, Error, Push, code};
use prost::Message as _;
use rusqlite::OptionalExtension;
use serde_json::Value;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobuf_sync_action::{
    ArchiveChatAction, MarkChatAsReadAction, MuteAction, PinAction, StarAction, SyncActionMessage,
    SyncActionMessageRange, SyncActionValue,
};

use super::actions::{self, Target};
use super::outbox::{bad_param, message_id, queue, settle};
use super::{Inner, durably, internal, no_chat, now_ms, query};
use crate::convert::app_state::FOREVER;
use crate::engine::{EngineError, pb};
use crate::store::chat_actions::{Act, Acted, Does, Outcome, PINNED_MOST, Patch, Setting, Undo};
use crate::store::outbox::Step;
use crate::store::{Change, read};
```

After `receipts_first` add:

```rust
/// How many times a fetch after a refusal is tried while the engine that refused runs; the next
/// engine to start fetches every collection again in full anyway.
const FETCH_TRIES: u32 = 10;

/// What putting back a refused patch undoes, by its index; nothing for a deletion for you, whose
/// message stays deleted here.
pub(super) fn undo_of(index: &[String]) -> Option<Undo> {
    let chat = index.get(1)?.clone();
    match index.first()?.as_str() {
        "pin_v1" => Some(Undo::Pin { chat }),
        "archive" => Some(Undo::Archive { chat }),
        "mute" => Some(Undo::Mute { chat }),
        "markChatAsRead" => Some(Undo::MarkUnread { chat }),
        "star" => Some(Undo::Star { chat, id: index.get(2)?.clone() }),
        _ => None,
    }
}

/// What a patch did, as a notice says WhatsApp refused it.
pub(super) fn what_it_did(patch: &pb::AppStatePatch) -> &'static str {
    let value = SyncActionValue::decode(patch.value.as_slice()).unwrap_or_default();
    let on = |set: Option<bool>| set.unwrap_or(false);
    match patch.index.first().map(String::as_str) {
        Some("pin_v1") if on(value.pin_action.as_ref().and_then(|a| a.pinned)) => "pinning a chat",
        Some("pin_v1") => "unpinning a chat",
        Some("archive") if on(value.archive_chat_action.as_ref().and_then(|a| a.archived)) => "archiving a chat",
        Some("archive") => "unarchiving a chat",
        Some("mute") if on(value.mute_action.as_ref().and_then(|a| a.muted)) => "muting a chat",
        Some("mute") => "unmuting a chat",
        Some("markChatAsRead") if on(value.mark_chat_as_read_action.as_ref().and_then(|a| a.read)) => {
            "marking a chat read"
        }
        Some("markChatAsRead") => "marking a chat unread",
        Some("star") if on(value.star_action.as_ref().and_then(|a| a.starred)) => "starring a message",
        Some("star") => "unstarring a message",
        Some("deleteMessageForMe") => "telling your other devices you deleted a message",
        _ => "telling your other devices of a change",
    }
}

/// Keeps app-state row `outbox`, which WhatsApp refused for good or which ran out of tries, in
/// quarantine with why, puts back what its patch set, says so to every interface, and fetches the
/// patch's collection again, so the chat ends as the phone has it.
pub(super) async fn refused(inner: &Inner, outbox: i64, patch: Option<&pb::AppStatePatch>, error: String) {
    let undo = patch.and_then(|patch| undo_of(&patch.index));
    // Empty names are every collection.
    let fetch = match patch {
        Some(patch) if undo.is_some() => Some(vec![patch.name.clone()]),
        Some(_) => None,
        None => Some(Vec::new()),
    };
    let what = patch.map_or("telling your other devices of a change", what_it_did);
    settle(inner, vec![Change::Outbox(Step::Refused { outbox, error: error.clone(), undo })]).await;
    say(inner, format!("WhatsApp refused {what}: {error}"));
    if let Some(names) = fetch {
        fetch_again(inner, names);
    }
}

/// Asks the engine running now to fetch collections `names` again in full, trying again while it
/// runs.
fn fetch_again(inner: &Inner, names: Vec<String>) {
    let Some(engine) = inner.engine.clone() else { return };
    let timing = inner.timing;
    tokio::spawn(async move {
        let restarts = engine.status().restarts;
        let mut pause = timing.retry_first;
        for _ in 0..FETCH_TRIES {
            let resync = pb::command::Kind::AppStateResync(pb::AppStateResync { names: names.clone() });
            match engine.call(resync).await {
                Ok(_) => return,
                Err(e) => eprintln!("jawd: can't fetch app state again after WhatsApp refused a patch: {e}"),
            }
            tokio::time::sleep(pause).await;
            if engine.status().restarts != restarts {
                return;
            }
            pause = (pause * 2).min(timing.retry_most);
        }
        eprintln!("jawd: gave up fetching app state again after a refusal; the next engine to start fetches it");
    });
}

/// Tells every interface what WhatsApp refused.
fn say(inner: &Inner, text: String) {
    let params = serde_json::to_value(Notice { text, tone: Tone::Warning }).expect("jawd's own types serialize");
    let _ = inner.pushes.send(Push { method: "notice".into(), params });
}
```

In `crates/jawd/src/send/outbox.rs`, change `async fn settle(` to `pub(super) async fn settle(`, and in `attempt` replace

```rust
            Err(e) => {
                let error = format!("its app state doesn't decode: {e}");
                settle(&inner, vec![Change::Outbox(Step::Failed { outbox, error, next_at: None })]).await;
                return Next::Done;
            }
```

with

```rust
            Err(e) => {
                super::chat_actions::refused(&inner, outbox, None, format!("its app state doesn't decode: {e}")).await;
                return Next::Done;
            }
```

and replace

```rust
    let gives_up = !retryable(&failure) || row.attempts + 1 >= ATTEMPTS_MOST;
    let step = match (gives_up, may_have_gone) {
```

with

```rust
    let gives_up = !retryable(&failure) || row.attempts + 1 >= ATTEMPTS_MOST;
    if gives_up && app_state {
        eprintln!("jawd: WhatsApp refused app state {} in {}: {failure}", row.id, row.chat);
        super::chat_actions::refused(&inner, outbox, patch.as_ref(), failure.to_string()).await;
        return Next::Done;
    }
    let step = match (gives_up, may_have_gone) {
```

In `crates/jawd/src/send/actions.rs`, in `refused`, delete the line

```rust
        "app_state" => "telling your other devices you deleted a message",
```

In `crates/jaw-proto/src/lib.rs`, replace `/// Changes that failed to convert, kept with their errors.` with:

```rust
    /// Changes that failed to convert, and app state WhatsApp refused, kept with their errors.
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && cargo fmt --all && CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd send:: && CARGO_INCREMENTAL=0 cargo test -p jaw-proto`
Expected: PASS, the two new `send::tests`, `a_refused_patch_puts_back_what_it_set_and_is_said_as_what_it_did`, and every other test.

- [ ] **Step 5: Check the whole crate lints clean**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings: every item the earlier tasks added is used now.

- [ ] **Step 6: Commit**

```bash
cd /Users/pedrogomes/src/jaw-chat-actions
git add crates/jawd/src/send/chat_actions.rs crates/jawd/src/send/outbox.rs crates/jawd/src/send/actions.rs crates/jaw-proto/src/lib.rs crates/jawd/src/send/tests.rs
git commit -m "A patch WhatsApp refuses for good, or that runs out of tries, is kept in quarantine with WhatsApp's words, what it set is put back to how a chat is when nothing set it, every interface is told what was refused and why, and its collection is fetched again while the engine that refused runs, so the chat ends as the phone has it; a refused deletion for you is quarantined too, where it was dropped, and fetches nothing, the message staying deleted here, and a patch that doesn't decode fetches every collection."
```

---

### Task 11: Through the socket, the phone meanwhile, and a crash either side of the commit

> **Since task 8's fix:** add the three tests the fix's second review asked for. (1) The patch going first at an engine start: the demo world holds the pin's echo's ack, or its answer, until the start's full fetch has been asked for, so the fetch waits for the patch, the outbox holding the app-state turn across its call to the engine; the row and the world's latest end as WhatsApp has them, with the row's `v` unmoved, and moving `drop(turn)` above the call in `send/outbox.rs` makes it fail. (2) In Go, `echoed`'s branches for jawd going and the engine stopping, each a retryable failure. (3) The comment in `a_pin_that_goes_while_the_engine_start_fetches_app_state_again_ends_pinned_as_asked` describes the run without the turn; say what the run with it does.

**Files:**
- Modify: `crates/jawd/src/send/tests.rs` (two scenes in `a_scene_of_jawd_to_be_killed`, four tests)

**Interfaces:**
- Consumes: everything above; `World::hold_patches` (Task 6).
- Produces: tests only.

- [ ] **Step 1: Write the tests**

In `a_scene_of_jawd_to_be_killed`'s `match scene.as_str()`, before `other => panic!("there's no scene {other}"),` add:

```rust
            "pin asked" | "pin answered" => {
                world.hold_patches(true);
                let running = Running::on(home, world, true).await;
                running.chats(&[ANA]).await;
                let holder = Connection::open(home.join("jaw.db")).unwrap();
                holder.busy_timeout(Duration::from_secs(5)).unwrap();
                holder.execute_batch("BEGIN IMMEDIATE").unwrap();
                let sending = running.sending.clone();
                let answer =
                    tokio::spawn(async move { sending.call("chat.pin", json!({ "chat": ANA, "pinned": true })).await });
                tokio::time::sleep(Duration::from_millis(300)).await;
                if scene == "pin asked" {
                    at("asked");
                    // The scene stops here, the pin waiting for jaw.db's write lock, until it's killed.
                    std::thread::sleep(Duration::from_secs(3_600));
                } else {
                    holder.execute_batch("COMMIT").unwrap();
                    answer.await.unwrap().unwrap();
                    let db = Connection::open(home.join("jaw.db")).unwrap();
                    let pinned: i64 =
                        db.query_row("SELECT pinned FROM chats WHERE id = ?1", [ANA], |r| r.get(0)).unwrap();
                    at(&pinned.to_string());
                }
                drop(holder);
            }
```

Append to the file:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn chat_actions_go_through_the_socket_and_a_window_watched_again_from_its_seq_is_sent_their_rows() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA, CLIMBING]).await;
    running.arrives(ANA, ANA, "m1", now_ms() - 60_000).await;
    let socket = serve_watched(&running, home.path());
    let all = json!({ "folder": "all", "around": "top", "before": 0, "after": 60 });
    let watched = ask_socket(&socket, vec![("chats.watch", all)]).await;
    let seq = watched[0]["result"]["seq"].as_i64().unwrap();
    let until = now_ms() + 8 * 3_600_000;
    let replies = ask_socket(
        &socket,
        vec![
            ("chat.pin", json!({ "chat": ANA, "pinned": true })),
            ("chat.mute", json!({ "chat": ANA, "until": until })),
            ("chat.mark_unread", json!({ "chat": ANA, "unread": true })),
            ("star", json!({ "message": "m1", "starred": true, "chat": ANA })),
            ("chat.archive", json!({ "chat": CLIMBING, "archived": true })),
            ("chat.pin", json!({ "chat": "351999999999@s.whatsapp.net", "pinned": true })),
            ("chat.mute", json!({ "chat": ANA, "until": 1_000 })),
            ("star", json!({ "message": "nope", "starred": true })),
        ],
    )
    .await;
    for reply in &replies[..5] {
        assert_eq!(reply["result"], json!({}), "{reply}");
    }
    assert_eq!(replies[5]["error"]["code"], code::NOT_FOUND);
    assert_eq!(
        (&replies[6]["error"]["code"], &replies[6]["error"]["data"]),
        (&json!(code::INVALID_PARAMS), &json!({ "param": "until" }))
    );
    assert_eq!(replies[7]["error"]["code"], code::NOT_FOUND);
    let again = json!({ "folder": "all", "around": "top", "before": 0, "after": 60, "since": seq, "have": [ANA, CLIMBING] });
    let rewatched = ask_socket(&socket, vec![("chats.watch", again)]).await;
    let result = &rewatched[0]["result"];
    let rows = result["rows"].as_array().unwrap();
    let ana = rows.iter().find(|row| row["id"] == ANA).unwrap_or_else(|| panic!("{result}"));
    assert_eq!(
        (&ana["pinned"], &ana["muted_until"], &ana["marked_unread"]),
        (&json!(true), &json!(until), &json!(true))
    );
    assert!(ana["v"].as_i64().unwrap() > seq, "with a version past where the window was watched");
    assert_eq!(result["remove"], json!([CLIMBING]), "archived, it left All");
    let archived = json!({ "folder": "archived", "around": "top", "before": 0, "after": 60 });
    let archived = ask_socket(&socket, vec![("chats.watch", archived)]).await;
    assert_eq!(archived[0]["result"]["rows"][0]["id"], CLIMBING);
    assert!(row(&running, ANA, "m1").await.unwrap().starred);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_from_the_phone_while_a_pin_waits_is_held_back_and_the_pin_goes_after_it() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    until_offline(&running).await;
    running.call("chat.pin", json!({ "chat": ANA, "pinned": true })).await.unwrap();
    let pinned = chat_row(&running, ANA).await;
    let at = pinned_at(&running, ANA).await;
    // The phone unpins it meanwhile, with a later clock; WhatsApp takes jaw's pin after it all the same.
    let unpin = SyncActionValue {
        timestamp: Some(now_ms() + 1_000),
        pin_action: Some(PinAction { pinned: Some(false) }),
        ..Default::default()
    };
    let value = unpin.encode_to_vec();
    let state = pb::AppState { index: vec!["pin_v1".into(), ANA.into()], value: value.clone() };
    running.world.deliver(pb::Event { seq: 0, kind: Some(pb::event::Kind::AppState(state)) }).await.unwrap();
    let key = serde_json::to_string(&["pin_v1", ANA]).unwrap();
    running
        .until("the phone's unpin, kept raw", move |db| {
            let kept: Option<Vec<u8>> =
                db.query_row("SELECT value FROM app_state WHERE key = ?1", [&key], |r| r.get(0)).optional()?;
            Ok(kept.as_deref() == Some(value.as_slice()))
        })
        .await;
    let held = chat_row(&running, ANA).await;
    assert!(held.pinned && held.v == pinned.v, "held back: jaw's pin goes after it, so it's how every device ends");
    assert!(patches_sent(&running).is_empty(), "jaw's pin still waits");
    running.connection(jaw_proto::ConnectionState::Online);
    until_stored(&running).await;
    let sent = patches_sent(&running);
    assert_eq!(sent.len(), 1);
    until_echoed(&running, &sent[0]).await;
    let ana = chat_row(&running, ANA).await;
    assert!(ana.pinned && ana.v == pinned.v, "jaw's echo changes nothing");
    assert_eq!(pinned_at(&running, ANA).await, at, "at the time jaw pinned it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pin_killed_before_its_commit_leaves_nothing_and_asked_again_goes_once() {
    let home = tempfile::tempdir().unwrap();
    assert_eq!(crash(home.path(), "pin asked").await, "asked");
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    let (pinned, rows): (i64, i64) = db
        .query_row(
            "SELECT (SELECT pinned FROM chats WHERE id = ?1), (SELECT count(*) FROM outbox WHERE kind = 'app_state')",
            [ANA],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((pinned, rows), (0, 0), "nothing of it was written, and nobody was told it was");
    drop(db);
    let running = Running::on(home.path(), world(), true).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(patches_sent(&running).is_empty(), "so nothing goes");
    running.call("chat.pin", json!({ "chat": ANA, "pinned": true })).await.unwrap();
    until_stored(&running).await;
    assert_eq!(patches_sent(&running).len(), 1, "asked again, it's pinned once");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pin_answered_was_on_disk_first_and_after_a_crash_goes_once_and_asked_again_changes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let pinned: i64 = crash(home.path(), "pin answered").await.parse().unwrap();
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    let (kept, v, rows): (i64, i64, i64) = db
        .query_row(
            "SELECT pinned, v, (SELECT count(*) FROM outbox WHERE kind = 'app_state' AND state = 'queued')
            FROM chats WHERE id = ?1",
            [ANA],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((kept, rows), (pinned, 1), "the pin and its patch outlived jawd");
    drop(db);
    let running = Running::on(home.path(), world(), true).await;
    until_stored(&running).await;
    let sent = patches_sent(&running);
    assert_eq!(sent.len(), 1, "sent once jawd started again");
    assert_eq!(value_of(&sent[0]).timestamp, Some(pinned), "with the time jawd wrote before it stopped");
    until_echoed(&running, &sent[0]).await;
    let ana = chat_row(&running, ANA).await;
    assert_eq!((ana.v, pinned_at(&running, ANA).await), (v, pinned), "and its echo changed nothing");
    running.call("chat.pin", json!({ "chat": ANA, "pinned": true })).await.unwrap();
    assert_eq!(patches_sent(&running).len(), 1, "asked again, its answer lost, nothing more goes");
}
```

- [ ] **Step 2: Run them**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && cargo fmt --all && CARGO_INCREMENTAL=0 cargo test -p jawd --bin jawd -- send::tests::chat_actions_go send::tests::a_change_from_the_phone send::tests::a_pin_killed send::tests::a_pin_answered`
Expected: PASS, four tests. They test what Tasks 4 to 10 built; each fails without its rule: without the engine's time and the demo's echo there is no echo to wait for, without the transaction's outbox row `the pin and its patch outlived jawd` fails, and without "already so" `asked again ... nothing more goes` fails.

- [ ] **Step 3: Run the crate's tests whole**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && CARGO_INCREMENTAL=0 cargo test -p jawd`
Expected: PASS, every unit test and `crates/jawd/tests/` (`starting`, `stopping`, `upgrading`).

- [ ] **Step 4: Commit**

```bash
cd /Users/pedrogomes/src/jaw-chat-actions
git add crates/jawd/src/send/tests.rs
git commit -m "Tests that the chat actions go through the socket and a window watched again from its seq is sent their rows, that a change from the phone while a pin waits is kept but held back, since jaw's pin goes after it, at the time jaw pinned it, and its echo changes nothing, and that a pin killed waiting for its commit leaves nothing while one answered was on disk first, goes once after the crash with the time jawd wrote, comes back changing nothing and, asked again, sends nothing more. Tests only"
```

---

### Task 12: The protocol's text and the design's decisions

**Files:**
- Modify: `protocol/README.md` (the Errors table, the Methods table, a new section)
- Modify: `docs/plans/2026-10-16-jaw-design.md` ("Chat actions from jaw")

**Interfaces:**
- Consumes: the behaviour of Tasks 1 to 11.
- Produces: docs only.

- [ ] **Step 1: The Errors table**

In `protocol/README.md`, replace

```markdown
| 4 | Not allowed, and why: editing a message older than WhatsApp allows, deleting someone else's message for everyone, voting twice on a single-choice poll |
```

with

```markdown
| 4 | Not allowed, and why: editing a message older than WhatsApp allows, deleting someone else's message for everyone, voting twice on a single-choice poll, pinning, archiving, muting or marking a community's own row, starring a line about the chat or a deleted message |
```

and in the row for 5 replace `the outbox, a forward's chats, a connection's windows or its finds and saves being answered, the last four with `data.limit` saying so` with `the outbox, a forward's chats, a fourth pinned chat, a connection's windows or its finds and saves being answered, the last five with `data.limit` saying so`.

- [ ] **Step 2: The Methods table**

Replace

```markdown
| `star` | `message`, `starred` | |
```

with

```markdown
| `star` | `message`, `starred`, and optionally `chat` | See Chat actions |
```

and replace

```markdown
| `chat.pin`, `chat.archive`, `chat.mute`, `chat.mark_unread`, `chat.clear`, `chat.delete` | `chat`, and `pinned`, `archived`, `until` or `unread` | |
```

with

```markdown
| `chat.pin`, `chat.archive`, `chat.mute`, `chat.mark_unread` | `chat`, and `pinned`, `archived`, `until` or `unread` | See Chat actions |
| `chat.clear`, `chat.delete` | `chat` | Not served yet: clearing and deleting a chat come in a later step |
```

- [ ] **Step 3: The section**

Insert, just before the line `## Library (proposed)`:

````markdown
## Chat actions

What you set on a chat from jaw, as your phone sets it: pinned, archived, muted, marked unread or read; and a message starred. Each is done to its row at once and told to your other devices by WhatsApp's app state, as deleting a message for you is.

```json
{"id":16,"method":"chat.pin","params":{"chat":"351912345678@s.whatsapp.net","pinned":true}}
{"id":16,"result":{}}
{"id":18,"method":"chat.mute","params":{"chat":"120363012345678901@g.us","until":-1}}
{"id":20,"method":"star","params":{"message":"3EB0A1B2C3D4E5F6","starred":true,"chat":"351912345678@s.whatsapp.net"}}
{"id":20,"result":{}}
```

- **`chat.pin`** (`chat`, `pinned`) pins the chat now, or unpins it. Pinned chats sort first, the one pinned last on top. At most three chats are pinned, as WhatsApp's apps allow, archived ones apart; pinning a fourth is error 5, with `data` `{"limit": "pinned", "most": 3}`.
- **`chat.archive`** (`chat`, `archived`) archives the chat, which unpins it, as WhatsApp's apps do, or brings it back. An archived chat leaves `all` and comes into `archived`.
- **`chat.mute`** (`chat`, `until`): null unmutes; a time in milliseconds since the epoch mutes until then; -1, or any time from the last millisecond of 9999 on, mutes for ever, and the row's `muted_until` is then 253402300799999. A time already past is -32602, with `data` `{"param": "until"}`.
- **`chat.mark_unread`** (`chat`, `unread`): true marks the chat unread, leaving its messages as they are; false takes the mark off and reads the chat up to its newest message, as `read` does, read receipts and all.
- **`star`** (`message`, `starred`, and optionally `chat`, where it is, as the message actions take it) stars a message or takes its star off. A line about the chat, or a message deleted for everyone, can't be starred (error 4); a star can always be taken off.
- **Answers.** Each answers with nothing once its change is on disk, synced, with what tells your other devices. The row it changes comes to every window that shows it with a new `v`, so a window watched again from an earlier `seq` is sent it. Asking for what's already so changes nothing and answers the same, so an interface that lost an answer asks again.
- **Errors.** A chat or message jawd doesn't have is error 3; a community's own row, whose groups carry these, is error 4 for the four chat requests; an account that isn't linked is error 1; past the outbox's thousand, error 5, as for `send.text`.
- **Your other devices** hear of it after the chat's earlier sends, waiting while WhatsApp can't be reached, as an action on a message does; reading a chat sends its read receipts first. A change your phone makes to the same setting before jaw's reaches WhatsApp isn't shown, since jaw's goes after it and is what every device ends with; one made after it shows as it comes. If WhatsApp refuses it, the chat is put back as your phone has it, every interface is sent a `notice` saying why, and jawd keeps what was refused in its quarantine (`status`'s `quarantined`).

````

- [ ] **Step 4: The design**

In `docs/plans/2026-10-16-jaw-design.md`, in "### Chat actions from jaw", after the bullet that starts `- **Refused for good.**`, insert:

```markdown
- **How the patches go.** Archiving a pinned chat is two patches in the chat's order, the archive and then the unpin, since the engine's `AppStatePatch` carries one mutation. The engine sends each with the time jawd wrote as the patch's own, which whatsmeow writes into every mutation it encodes, so the echo of a pin finds the time jawd keeps. A patch saying a chat is read goes after the chat's read receipts, which its attempt sends first, since WhatsApp's read at jaw's own position settles the receipts jaw owes there; and a chat read on any device loses its unread mark, so jaw's mark read coming back clears a mark the phone set meanwhile.
- **Put back as the phone has it.** Fetching a collection again brings only what WhatsApp holds, so a refused patch's field first goes back to how a chat is when nothing set it (unpinned, not archived, unmuted, not marked, not starred), in the transaction that quarantines it, and the fetch then sets what the phone has. A refused deletion for you is quarantined and fetches nothing: the message stays deleted here.
```

- [ ] **Step 5: Check and commit**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && rg -n '^## Chat actions|See Chat actions|a fourth pinned chat' protocol/README.md && rg -n 'How the patches go|Put back as the phone has it' docs/plans/2026-10-16-jaw-design.md`
Expected: the section's heading, the two Methods rows, the Errors row and the two design bullets each found once.

```bash
cd /Users/pedrogomes/src/jaw-chat-actions
git add protocol/README.md docs/plans/2026-10-16-jaw-design.md
git commit -m "The protocol's chat actions, as jawd serves them: chat.pin, chat.archive, chat.mute, chat.mark_unread and star, their answers, errors and what your other devices hear, with the Methods table pointing to them and leaving chat.clear and chat.delete for a later step; and jaw's design says how the patches go and how a refused one is put back as the phone has it. Docs only"
```

---

### Task 13: The gate

**Files:** none changed; `/tmp/jaw-chat-actions/` holds the demo's scratch and is deleted at the end.

**Interfaces:** consumes the branch; produces nothing.

- [ ] **Step 0: Up to date with main, its migration renumbered**

main has a migration 13 of its own (`settings`, `81a8a99`), so rebase onto `origin/main` and give this branch's migration (`app_state_awaiting`) the next number: its comment in `schema.rs`, its line in `migrations.lock` (its kind and hash, at the new number), the frozen-text and upgrade tests' ranges, and every mention of "migration 13" in the plan, the design and comments. The gate, the audit and the landing then see what lands.

- [ ] **Step 1: Room on the disk**

Run: `df -h /System/Volumes/Data`
Expected: at least 60 GB available. If not, stop and say so.

- [ ] **Step 2: Lint and tests, as CI runs them**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && export CARGO_INCREMENTAL=0 && make lint && make test`
Expected: `make lint` prints nothing past cargo's `Finished` lines and exits 0; `make test` ends with `ok  github.com/pdcgomes/jaw/engine/internal/wa` among the Go packages and `test result: ok.` for every Rust test binary. Under a heavy load (the reviews saw load averages near 50) these fail by their timing and pass alone: `indexing_for_search_waiting_for_the_wal_gets_room_with_nothing_else_committing`, `a_reader_that_lets_go_while_a_restart_waits_for_it_doesnt_keep_the_wal_from_starting_again`, `feed::tests::a_lone_change_after_quiet_goes_at_once_rather_than_waiting_to_coalesce`, `the_checkpointer_flushes_jaw_db_after_a_pass_the_writer_outran_and_not_after_one_that_reached_the_end` and `the_checkpointer_flushes_jaw_db_without_letting_go_of_this_processs_locks_on_it`; run one that fails alone and report both results, and any other failure, or one that fails alone, is a failure. `make check-generated` isn't run here: the engine's proto and generated code are untouched (`git diff origin/main --stat -- proto/ engine/internal/link` prints nothing), and CI runs it.

- [ ] **Step 3: The round trip on jawd demo**

Create `/tmp/jaw-chat-actions/smoke.py`:

```python
import json
import socket
import sys
import time


def connect(path):
    for _ in range(600):
        try:
            s = socket.socket(socket.AF_UNIX)
            s.connect(path)
            return s.makefile("rw")
        except OSError:
            time.sleep(0.2)
    sys.exit("jawd demo never listened")


f = connect(sys.argv[1])
asked = 0


def ask(method, params):
    global asked
    asked += 1
    f.write(json.dumps({"id": asked, "method": method, "params": params}) + "\n")
    f.flush()
    while True:
        line = json.loads(f.readline())
        if line.get("id") == asked:
            return line


ask("hello", {"protocol": 1, "client": "smoke", "version": "0"})
for _ in range(600):
    watched = ask("chats.watch", {"folder": "all", "around": "top", "before": 0, "after": 20})["result"]
    ask("unwatch", {"watch": watched["watch"]})
    pinned = [r["id"] for r in watched["rows"] if r["pinned"]]
    loose = [r["id"] for r in watched["rows"] if not r["pinned"] and r["kind"] in ("direct", "group")]
    if len(pinned) == 3 and loose:
        break
    time.sleep(0.2)
print("pinned:", len(pinned))
print("a fourth pin:", ask("chat.pin", {"chat": loose[0], "pinned": True})["error"]["data"])
print("archiving a pinned chat:", ask("chat.archive", {"chat": pinned[0], "archived": True})["result"])
print("the pin now:", ask("chat.pin", {"chat": loose[0], "pinned": True})["result"])
print("muted for ever:", ask("chat.mute", {"chat": loose[0], "until": -1})["result"])
time.sleep(2)
stored = ask("status", None)["result"]["stored"]
print("outbox:", stored["outbox"], "quarantined:", stored["quarantined"])
```

Run:

```bash
cd /Users/pedrogomes/src/jaw-chat-actions
mkdir -p /tmp/jaw-chat-actions
CARGO_INCREMENTAL=0 cargo build -p jawd
./target/debug/jawd demo --folder /tmp/jaw-chat-actions/demo --chats 40 --people 60 --messages 5000 --first-run --catch-up 100 > /tmp/jaw-chat-actions/demo.log 2>&1 &
echo $! > /tmp/jaw-chat-actions/demo.pid
python3 /tmp/jaw-chat-actions/smoke.py /tmp/jaw-chat-actions/demo/jaw.sock
kill "$(cat /tmp/jaw-chat-actions/demo.pid)"
sleep 3
tail -3 /tmp/jaw-chat-actions/demo.log
```

Expected:

```
pinned: 3
a fourth pin: {'limit': 'pinned', 'most': 3}
archiving a pinned chat: {}
the pin now: {}
muted for ever: {}
outbox: 0 quarantined: 0
```

and the log ending with `jawd demo: stopped in N ms`. The outbox at 0 says the demo engine answered each patch, and the quarantine at 0 that each echo converted.

- [ ] **Step 4: The audit check, before the audit**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && make check-audit`
Expected: exits 1 with `The data layer changed without an approved audit:`, listing each commit of Tasks 1, 3 to 11 as `touches the data layer (...) but has no Data-Audit trailer`. Task 14 settles it.

- [ ] **Step 5: Clean up the scratch**

Run: `rm -rf /tmp/jaw-chat-actions`
Expected: nothing printed. Nothing is committed in this task.

---

### Task 14: The database expert's audit

**Files:**
- Create (the auditor writes it): `docs/audits/<date>-chat-actions.md`, the date the audit runs on.
- Every commit in `origin/main..HEAD` gains the trailer `Data-Audit: docs/audits/<date>-chat-actions.md`.

**Interfaces:** consumes the branch; produces an approved note and the trailers.

- [ ] **Step 1: Launch a fresh auditor**

Launch the `database-expert` subagent (if that type isn't offered, a general-purpose subagent told to read `/Users/pedrogomes/src/jaw-chat-actions/.cursor/agents/database-expert.md` first and follow it as its brief). If you can't launch subagents, stop here, don't push, and report back asking for the audit. Give it this brief:

```
Audit jawd's step "Chat actions from jaw" on branch chat-actions in the worktree /Users/pedrogomes/src/jaw-chat-actions, range origin/main..HEAD. Read .cursor/agents/database-expert.md and follow it as your brief. The design is docs/plans/2026-10-27-jaw-mac-menus-and-folders-design.md ("Chat actions from jaw") and docs/plans/2026-10-16-jaw-design.md (the subsection of that name, "Data integrity" and "Migrations"); the plan is docs/plans/2026-10-27-chat-actions-plan.md, whose "Decisions this plan makes" lists what the design left open. Write your note to docs/audits/<today's date>-chat-actions.md in that worktree, the only file you write there, ending in its verdict line; if a note of this name exists from an earlier round, add your round above the earlier ones, keeping them as written. Keep everything you make in /tmp/jaw-audits/chat-actions (one clone, one target, CARGO_INCREMENTAL=0) and delete that folder once the note is written. Never open anything under ~/Library/Application Support/jaw, never run the Go engine on a folder holding session.db, and stop only PIDs you started.

Beyond your standard checks, look at:
1. Each request's row, its new v and event, and its outbox rows commit in one transaction, synced, before the answer (Change::ChatAction, store/write/chat_actions.rs; send/chat_actions.rs act).
2. "Already so" writes nothing: pin by pinned > 0, archive, mute by muted_until, mark unread by the mark, mark read by no mark and nothing unread, star by starred.
3. The pin limit counts unarchived pinned chats through chats_by_sort (sort >= '1~'); two pins at once are serialised by the writer.
4. The echo: whatsmeow's EncodePatch writes PatchInfo.Timestamp into each mutation, and engine/internal/wa/appstate.go now sets it from jawd's value; the demo engine echoes the value as sent.
5. The converter now unmarks a chat on markChatAsRead read=true even where jaw has read as far (convert/app_state.rs).
6. A read-marking patch sends the chat's owed receipts first (send/outbox.rs attempt, send/chat_actions.rs receipts_first, send/reads.rs tell_now), keeping set_read's rule that WhatsApp's read at jaw's position settles what's owed.
7. A refusal: the row moved to quarantine with its patch, the field put back to how a chat is when nothing set it, then AppStateResync of its collection; a refused deleteMessageForMe is now quarantined where it was dropped. Every engine start already fetches every collection in full (sync/refresh.rs), which covers a crash between the refusal's commit and its fetch.
8. Archive and unpin as two patches in the chat's order, and the outcomes when one of them is refused.
9. The plans pinned in plans.txt and the module plan tests; one new table, `app_state_awaiting` (migration 17, a schema step, class jaw's own), and no new column.
10. The hold-back (task 8's fix): while jaw's patch for an index hasn't echoed, what WhatsApp sends for that index is kept raw and not applied; the awaited row goes with its echo, once the drain passes `echo_by`, when its outbox row goes without going, or with its refusal; a refusal undoes only its index's latest patch; the time each patch carries is unique to its index; `stored.awaiting` counts the rows. The engine answers a patch only once its echo is kept, and one lock keeps jawd's app-state patches and its full fetches of app state from overlapping. What's left, a notification's fetch or a 409's conflicting changes, handed over by whatsmeow beside jaw's echo when the phone changes the same setting at the same moment, is weighed as the decision says.
11. Edges the task reviews found and left for the audit to weigh:
    - A revoke committed between a star's or a reaction's check and its write stars or reacts to a message deleted for everyone, since the check reads before `act`.
    - A mark read on a chat jaw holds none of the messages of, in the instant a merge into it has started but its first step hasn't shown its newest message, reads nothing at the write, and its echo settles "told" past the gone chat's moved messages, whose receipts never go.
    - Receipts a merge moves to the kept chat wait for the next `want` of it or the next start's `reads::resume`, as before this branch; and a WhatsApp read of one chat converted after a merge into another has started settles "told" at that position in the kept chat, as before this branch.
    - A message reaching jaw between a no-message mark read and its echo, stamped at or before the patch's second, gets no read receipt from jaw; whether the phone sends one is WhatsApp's behaviour.
    - Each patch given up on starts its own fetch again, so a burst of them in one collection queues that many full fetches, each holding the app-state turn; bounded by the outbox and `FETCH_TRIES`, with nothing lost.
    - A patch that doesn't decode is quarantined and fetches every collection but puts nothing back, so if WhatsApp holds nothing for its setting the chat keeps jaw's unsent change; only corruption makes jaw's own bytes undecodable.
    - The engine start's own fetch again (`resync_again`) spends its tries whether or not WhatsApp can be reached, as before this branch, so if WhatsApp stays out of reach through its ten tries (about nine minutes) the start's full fetch, which heals what the order guarantee can't, waits for the next engine.
    - A refused archive of a pinned chat still sends its unpin, a change of its own, so the chat ends unarchived and unpinned on every device; the protocol says so.
```

- [ ] **Step 2: Fix what it marks, and audit again**

For each finding marked data loss or inconsistency, write a failing test, fix it, run `CARGO_INCREMENTAL=0 make lint test`, and commit the fix with a message naming the finding as the note numbers it. Then launch a fresh auditor with the same brief, naming the round. Repeat until the note's last line is `Verdict: approved`. Findings of hygiene or performance that break no budget may stand as follow-ups written in the note.

- [ ] **Step 3: Commit the note**

```bash
cd /Users/pedrogomes/src/jaw-chat-actions
NOTE=$(ls docs/audits/*-chat-actions.md)
tail -1 "$NOTE"
git add "$NOTE"
git commit -m "The database expert's audit of chat actions from jaw, approved; its note says what each round found and what stands as a follow-up."
```

Expected: `tail -1` prints `Verdict: approved`.

- [ ] **Step 4: Add the trailer to every commit**

```bash
cd /Users/pedrogomes/src/jaw-chat-actions
NOTE=$(ls docs/audits/*-chat-actions.md)
git fetch origin
git rebase origin/main -x "git commit --amend --no-edit --trailer \"Data-Audit: $NOTE\""
make check-audit
```

Expected: the rebase ends `Successfully rebased and updated refs/heads/chat-actions.`; `make check-audit` prints nothing and exits 0. If the rebase brought in data-layer changes from main, run `CARGO_INCREMENTAL=0 make lint test` again, and if it had conflicts in data-layer files, have a fresh auditor look again before landing.

---

### Task 15: Land

**Files:** none.

**Interfaces:** consumes the approved branch.

- [ ] **Step 1: Only if nobody said to hold**

If whoever started you said to hold, stop here and report the branch's head. Otherwise go on, from this worktree only.

- [ ] **Step 2: Up to date with main, and the hook in place**

```bash
cd /Users/pedrogomes/src/jaw-chat-actions
git fetch origin
git rebase origin/main
test -x "$(git rev-parse --git-common-dir)/hooks/pre-push" || make hooks
make check-audit
```

Expected: the rebase is a no-op or replays the branch cleanly (the trailers stay on each commit); `make check-audit` exits 0. If main moved, run `CARGO_INCREMENTAL=0 make lint test` again before pushing.

- [ ] **Step 3: Push, with the pre-push hook**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && git push origin HEAD:main`
Expected: the hook runs `check-data-audit.py` silently and the push lands; never use `--no-verify`. Then `git log origin/main --oneline -3` shows the branch's last commits.

- [ ] **Step 4: Delete the build folder**

Run: `cd /Users/pedrogomes/src/jaw-chat-actions && du -sh target && cargo clean && ls /tmp | rg 'jaw-chat-actions|jaw-audits' || true`
Expected: `cargo clean` removes `target/`; nothing of this work is left in `/tmp`. Say in your report that the worktree itself is left for whoever made it to remove (`git worktree remove /Users/pedrogomes/src/jaw-chat-actions`).

---

## Coverage

Each rule in the design's Tests, and where it fails without its change:

| Rule | Tests |
| --- | --- |
| Each request's row and patch, on the demo engine | Task 8: pinning, archiving, muting; Task 9: marking, starring |
| Through the socket | Task 11: `chat_actions_go_through_the_socket_...` |
| The fourth pin, archived ones apart | Task 4 `at_most_three_chats_...`; Task 8 `at_most_three_chats_...` |
| Archiving unpinning, two patches in order | Task 4 `archiving_a_pinned_chat_...`; Task 8 `archiving_a_pinned_chat_...` |
| Each form of mute's `until` | Task 7 `a_pin_and_a_mute_...`; Task 8 `a_mute_lasts_...` |
| Marking read sending receipts | Task 9 `marking_a_chat_unread_..._receipts_going_first` (fails with the wait switched off, Step 5) |
| The echo leaving the row's version alone | Task 1 (the engine's time); Task 6 (the demo's echo); Tasks 8, 9 and 11 (`until_echoed`, then `v`) |
| Asking twice | Task 4 (every "already" case); Tasks 8, 9; Task 11 `a_pin_answered_...` |
| A refusal quarantined and the collection fetched again | Task 5 `app_state_whatsapp_refused_...`; Task 10 `a_patch_whatsapp_refuses_...`, `telling_your_other_devices_of_a_deletion_...` |
| A crash between the commit and the answer, and one before the commit | Task 11 `a_pin_answered_...`, `a_pin_killed_...` |
| A phone change arriving while jaw's waits | Task 11 `a_change_from_the_phone_...` |
| An earlier echo converted after jaw's next change | Task 8's fix: the writer's and the requests' tests of the hold-back |
| A chat read anywhere unmarks it | Task 3 `a_chat_read_on_any_device_...` |
| Errors 1, 3, 4, 5 and -32602 | Tasks 4, 8, 9, 11 |
| Plans pinned | Task 4 `every_statement_a_chat_action_runs_...`, Task 5 and Task 8 plan tests, `plans.txt` |
| `jawd demo` shows the round trip | Task 6; Task 13 Step 3 |
| The protocol's text and fixtures | Tasks 2 and 12 |
