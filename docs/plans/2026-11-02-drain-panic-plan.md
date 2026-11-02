# jawd: a panicking conversion can't stop the inbox's drain

**The finding:** X1 of the contact cards audit (`docs/audits/2026-10-29-send-contact.md`). The converter runs `convert_rows` on a blocking thread (`sync/drain.rs:382`); a panic there comes back from `spawn_blocking` as an error, and the loop reads the same rows again without moving on or pausing. One message the converter can't take stops every later one from being converted, spins a core, and does it again at every start. The design wants a change that fails kept in quarantine, never dropped, and never blocking the changes after it ("Exactly once from at least once"), as the backfills do with `catch_unwind` (`store/write/kinds.rs`, `contacts.rs`, `copies.rs`). The history an earlier jawd kept raw has the same fault the other way (`drain.rs:483`): a panic ends its conversion for that run, at the same row each start.

## What changes

- **`drain::convert_rows`** converts each inbox row on its own, under `catch_unwind`, into a `Converted` of its own, which joins the batch's only once the row has converted. A row whose conversion panics leaves nothing of itself in the batch: no changes, no history offer, no picture, no thumbnail, no name. In its place goes `Change::Quarantined { source: "inbox", reference: <seq> }` with "converting it panicked: …", and no `Drained`, so its row stays in the inbox, raw, as one whose own conversion failed does today. The rows after it go on in the same batch, and the converter reads past it.
- **`drain::history_rows`** does the same for each row of `history_raw`: one that panics is kept in quarantine as `history_raw` `"<chat> <id>"`, the reference its `Drained::History` clears, and stays in `history_raw`.
- **`write::groups`** ends a group at a `Change::Quarantined` as it does at a `Drained`: each is where a raw row's conversion ends. A row kept in quarantine is then a savepoint of its own, never rolled back with the next row's group when that one fails, nor quarantined under the next row's name.
- **Nothing else.** The quarantine's unique `(source, ref)` already keeps a row that fails again once, with the latest error; a `Drained` already takes a row's quarantine row with it once a later jawd converts it; and `jawd status` already counts it, in `stored.quarantined`, and its row in `stored.inbox` (or `stored.history`). No schema, protocol or interface change.
- **Left alone:** a history blob whose walk panics (`sync/fetch.rs`) fails as `Failed::Later` and waits, tried again with back-off up to a minute while the blobs after it go first; it doesn't hold the inbox up. Catching a panic per conversation inside the walk is a follow-up.

## The tests

- `sync/tests.rs`, `an_inbox_row_whose_conversion_panics_is_kept_in_quarantine_once_and_the_rows_after_it_go_on`: rows from before the converter starts (the background lane) and rows that arrive as it runs (the live lane), each with a message whose conversion panics once it has made its changes (the tests' hook, for IDs starting `3EB0PANICS`, as the backfills' tests have it; the live one's panic says only a literal, a `&str` where the other's is a `String`) among others. The rows after each are converted; nothing of the panicking message is stored; its inbox row stays; the quarantine holds one row for it, `inbox` and its seq, saying it panicked and the panic's whole message; `jawd status` counts both. Then jawd starts again on the same folder with a new row behind them: the new row is converted, and each panicking row is still in the inbox and in quarantine once.
- `sync/tests.rs`, `a_history_row_whose_conversion_panics_is_kept_in_quarantine_once_and_the_rows_after_it_go_on`: the same for `history_raw`, converted twice as two starts would.
- `store/mod.rs`, `a_row_kept_in_quarantine_is_a_step_of_its_own`: a batch of a `Change::Quarantined` and then a row whose change fails, in both lanes; both rows are in quarantine, each under its own name.

## Each rule's switch

Each is switched off in turn and its test shown failing, the results kept beside the report: no `catch_unwind` in `convert_rows`; one in `history_rows`; the row that panicked dropped from the inbox (a `Drained` for it); it not kept in quarantine; its partial changes kept (converting into the batch in place, as main does); a reference that changes at each start; and `groups` not ending at a `Change::Quarantined`.

## Done when

The three tests pass and each switch fails its test; `make check-generated`, `make lint`, `make build` and `make test` pass as the brief's gate says; and the database expert's audit approves the branch, `make check-audit` passing once its note and trailers are in.
