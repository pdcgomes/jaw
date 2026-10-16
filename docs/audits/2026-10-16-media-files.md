# Audit: media files (jaw-media)

Range: origin/main..f9ed9ea in `media`, 2026-10-16, fourth look. Auditor: database-expert. Earlier rounds were at 2f18bdd (first), 8cccc79 (second) and 515af2f (third, approved).

The fourth look covers the branch rebased onto the latest `origin/main` and squashed into f9ed9ea. Since 515af2f, the only change in the data-layer paths is in `preview.rs`: previews with transparency are now PNG (`PreviewFormat::Png`, `<stem>.256.png` and `<stem>.768.png`, encoded by `raster::png`) instead of lossless WebP, to match the protocol.

- The names still come from `preview.rs`'s one `name` function, which both the claim and the writer use.
- Writing still goes through `Part` and `Claim`.
- `file.rs`, `prepare.rs`, `run.rs` and `job.rs` are byte-for-byte as they were at 515af2f.

The other changes are the workspace's `fast_image_resize` now being upstream's, with jaw-media adding its `image` feature in its own `Cargo.toml`, and doc comments. The branch's own change to `scripts/check-data-audit.py` is still just the five jaw-media paths; the `jawd/src/send` and `jawd/src/engine/` lines come from `origin/main`. Nothing here changes a finding, and the approval holds.

The change adds `crates/jaw-media/`, a library that works on files only: it probes them, writes previews, and makes SD and HD copies, in this process or through an ffmpeg child. Nothing in jawd, the engine, SQL, `jaw.db`, `session.db`, the spool or acks changes. Its data-layer part is the files it writes, which rows in jawd will later point to. The rules it touches are:

- A media file is synced and put in place before a row points to it.
- A media file is deleted only after a synced commit stops pointing to it.
- Nothing in flight is lost or left half-written on a crash, a kill, a power cut or a full disk.
- Files are bounded.
- Every rule has a test.

How a file is written now (`file.rs`):

1. A call (`Media::prepare`, `previews` or `instant_preview`) first claims every name it may write, posters included, in a per-process set (`Claim`). A name another call has claimed gets `Error::Busy`. The claim is held until the call returns, after any `undo`.
2. `Part::create` refuses a name that's taken (`Error::Exists`), and a name ending in `.part`.
3. It makes any missing folders and syncs them, deepest first, then the folder the top one was made in.
4. It claims a partial of its own, `<name>.<pid>-<n>.part`, with `create_new` and mode `0600`, before anything writes to it. ffmpeg is given that path, and its `-y` truncates the same inode.
5. `finish` checks that the path is still the claimed inode, then syncs it with `F_FULLFSYNC` (`sync_file`).
6. It puts the file in place with `hard_link`, which fails rather than replace anything.
7. It removes the partial and syncs the folder. If either step fails, it removes the name its own link made.
8. A call that fails or is cancelled removes the names it made before returning.

So within jawd's one process, a name is in one of three states:

- claimed by a call in flight, which other calls are told is `Busy`;
- holding a whole, synced file made by a call that succeeded or left whole by a crash, which is what `Exists` means;
- empty.

After a crash or a power cut, the name holds a whole, synced file or nothing, and any stale partial is removed by `remove_partial` at jawd's start.

## Findings

### Earlier rounds' findings, all fixed

Each fix below fails a test when reverted (see Measurements).

- **F1 (inconsistency, first round): two jobs on one destination shared one `.part`.** Each job now has a partial of its own, claimed with `create_new`, and `Drop` removes only its own. Test: `two_jobs_on_one_destination_leave_one_whole_file_and_never_a_mix`.
- **F2 (inconsistency, first round): a finish replaced the name.** Now `hard_link` and the check at `create` refuse a taken name. Tests: `a_name_thats_taken_is_left_as_it_was_and_the_job_says_so`, `names_ending_in_part_are_refused`.
- **F3 (data loss, first round): created folders weren't synced.** Now `create_folders` syncs them.
- **F4 (tests, first round):** sync order. Test: `the_file_is_synced_before_its_name_and_the_folders_made_for_it_after`.
- **F5 (tests, first round):** disk full. Test: `a_full_disk_fails_each_kind_of_job_and_leaves_nothing`.
- **F6 (inconsistency, second round): a name reported as `Exists` could then be removed by the call that made it.** Fixed with the second round's option (a), `Claim` (`file.rs:23-47`), taken first in `prepare` (`prepare.rs:73-74`), `previews` (`preview.rs:131-133`) and `instant_preview` (`preview.rs:174-175`).
  - In round two's probe, the second call now gets `Busy` while `sd.mp4` is in place. Once the first call is cancelled, the name holds nothing.
  - Round one's two-transcode race now gives `Ok(981784)` and `Busy`, with a whole file (6.01 s).
  - Two `previews` calls on one stem give one `Ok` and one `Busy`, with both previews whole.
  - `Exists`'s doc now matches what the crate guarantees.
  - Tests: `a_name_a_call_is_writing_is_busy_until_the_call_lets_go` and `a_name_another_call_is_writing_is_busy_and_never_reported_as_there`.
- **H6 (second round): the file's sync and its log line could drift apart.** They're now one function, `sync_file`. Removing the call fails the order test.
- **H7 (second round): two checks were untested.** Tests now cover both: `a_partial_file_replaced_while_it_was_written_is_never_put_in_place` (the inode check) and `previews_that_fail_halfway_leave_neither` (the large preview removed when the small one fails).
- **H1 to H4 (first round):** done in the second round.

### Data loss

None remain.

### Inconsistency

None remain.

### Unpredictable performance

**P1 (first round), stands as a follow-up (J7).** Every finished file costs two device flushes, 12.7 ms median for 40 KB on this Mac, and folders made on demand add more. Media finishing flat out raised a jaw.db-style synced commit's p99 from 9.7 to 14.2 ms. It doesn't break a budget inside this crate, whose work runs on jawd's pools.

### Hygiene (stand as follow-ups)

- **H5 (second round). Created folders can still go unsynced in two narrow cases.** A call that finds a folder another call has just made doesn't sync anything above it. And a folder sync that fails after `create_dir_all` is never retried. Both need a power cut within a few flushes. J9 removes them.
- **H8 (second round). Removed files can come back after a power cut.** The removals done by `undo`, and by `finish` after a failure, aren't followed by a folder sync, so after a power cut a removed file can come back, whole, with nothing pointing to it. jawd's start-up sweep (J3) removes it.
- **H9 (second round). A stale partial can collide with a new one.** If a crashed jawd's pid is reused and its stale partial is still there, one job fails with an `Io` error. A retry takes the next number, and J4 prevents it.
- **H10. The claims in `previews` and `instant_preview` have no test.** Removing either passes every test (measured). The mechanism itself is tested through `Claim`'s unit test and `prepare`'s pipeline test, and a scratch probe shows `previews` gives `Busy`. Add a test like `prepare`'s for a second `previews` call on one stem.
- **H11. Claims compare paths as given, within one process.**
  - `Path` equality ignores `.` components and repeated slashes, but not `..`, symlinks, or a relative against an absolute path. Two spellings of one destination would both pass the claim. They'd still meet at `hard_link`, so the result is a `Busy` turning into an `Exists` for a name its maker may still remove, not a corrupt file.
  - Another process writing into jaw's folders isn't seen at all; `jaw.lock` covers jawd itself.
  - jawd builds every destination one way, absolute and from its home (J1).

### Follow-ups for jawd's wiring (not this branch)

- **J1. Names.**
  - Choose one of two schemes: give every destination a fresh name never reused for other bytes (for example the message ID and quality), or use content-addressed names and treat `Exists` as done, which F6's fix now makes safe.
  - Build every destination one way, absolute and from jaw's home (H11).
  - Treat `Busy` as "another call is on it": wait for that call, or retry once it's done. It is neither done nor a failure to report.
- **J2. Pointing to a file.** Commit the row that points to a file only after the call has returned `Ok` with it, by which point it's synced and in place. An outbox row is committed synced (`FULL`, `fullfsync`) before `send.*` replies. A preview row is derived and may commit at `NORMAL`.
- **J3. Deleting a file.** For cache eviction, a cancelled send or a cleared chat, commit the change that stops pointing to the file synced first, and unlink the file after. At start, sweep the media folders for files no row points to: what H8 leaves, and a crash between a finish and its commit. Count them in `jawd status`.
- **J4. A crash's partials.** At start, after `jaw.lock` and before the media pools, run `remove_partial` on every media folder. Log the count.
- **J5. Untouched copies.** `Prepared.path` is the user's own file when a copy goes untouched. Keep the outbox from depending on it staying as it was, with one of these:
  - Clone it (`clonefile` on APFS) or copy it through the crate into jaw's folder before the outbox row points to it.
  - Record its size, mtime and hash, and check them before the upload, failing the send visibly if they changed.
- **J6. Disk.** Check free space before a transcode, at least twice the plan's estimated size. Pause background media before the disk fills. Put partials, cache size and jobs in flight in `jawd status`.
- **J7. Flushes (P1).** Bound synced finishes to one at a time across the image and transcode pools. Measure the writer's synced-commit p99 during a photo-heavy history sync in `make perf`. Ask the owner whether previews, which are derived and can be regenerated, need `F_FULLFSYNC`.
- **J8. Landing.** f9ed9ea touches `file.rs`, `job.rs`, `prepare.rs`, `preview.rs` and `run.rs`, so it needs the trailer `Data-Audit: docs/audits/2026-10-16-media-files.md`, with this note committed in the branch. Without them, `scripts/check-data-audit.py` fails today, as it should. Run `make check-audit` before pushing.
- **J9. Folders (H5).** Make and sync the media folders, fan-out included, at start, so no job makes a folder.

## Measurements

The machine for every round was a Mac Studio, Apple M1 Ultra (20 cores), internal SSD, APFS, macOS 26.6.2, with rustc 1.99.1 and ffmpeg 8.1.2 from Homebrew. Probes were scratch-only tests and scripts, in `/tmp/jaw-media-audit` (2f18bdd), `/tmp/jaw-media-audit2` (8cccc79), `/tmp/jaw-media-audit3` (515af2f) and `/tmp/jaw-media-audit4` (f9ed9ea). Nothing in the worktree, `~/src/jaw` or jaw's home was touched.

### Fourth look (f9ed9ea)

Run in a scratch clone at `/tmp/jaw-media-audit4`.

- **The crate's tests:** `cargo test -p jaw-media` passed three runs in a row, with 22 unit tests, 17 pipeline tests and the full-disk child process.
- **Two `previews` calls on one stem for a transparent source** (a 4000×3000 RGBA PNG with noisy alpha, the second call 20 ms later):
  - The first returned `Ok`: `Png` at 768×576, 1,064,106 bytes, with its alpha kept, and a small preview of 163,887 bytes.
  - The second returned `Busy`.
  - `p.256.png` and `p.768.png` were left, and no partials.
- **Size:** a 768 PNG of this worst-case noisy image is about 1 MB, under its 2.4 MB raw bound. It counts toward the cache's size limit like any preview.

### Third round (515af2f)

**The crate's tests.** `cargo test -p jaw-media` passed three runs in a row: 22 unit tests, 17 pipeline tests and the full-disk child process (the child is listed as ignored when the suite runs directly).

**Reverting each fix:**

| Reverted | What fails |
| --- | --- |
| No claim in `prepare` | `a_name_another_call_is_writing_is_busy_and_never_reported_as_there` |
| Claim released as soon as it's taken in `prepare` | `a_name_another_call_is_writing_is_busy_and_never_reported_as_there` |
| `Claim` never refuses | `a_name_a_call_is_writing_is_busy_until_the_call_lets_go`, `a_name_another_call_is_writing_is_busy_and_never_reported_as_there` |
| `Claim` never released | `a_name_a_call_is_writing_is_busy_until_the_call_lets_go` |
| The file's `sync_file` call removed | `the_file_is_synced_before_its_name_and_the_folders_made_for_it_after` |
| Inode check removed | `a_partial_file_replaced_while_it_was_written_is_never_put_in_place` |
| Previews keep the large one on a failure | `previews_that_fail_halfway_leave_neither` |
| No claim in `previews` | nothing (H10) |
| No claim in `instant_preview` | nothing (H10) |

**Scratch probes at 515af2f:**

- **Round two's F6 probe:** `sd.mp4 there=true, second call -> Err("Busy")`. After the first call was cancelled, `sd.mp4 there=false`.
- **Round one's two-transcode race** (one `Destination`, x264 HD of a 6 s 1080p clip, the second call 300 ms later): `["Ok(981784)", "Err(Busy)"]`. Only `hd.mp4` was left, and ffprobe read a duration of 6.013991 s with no errors.
- **Two `previews` calls on one stem** (a 5000×4000 PNG, the second call 20 ms later): first `Ok`, second `Busy`. `p.256.jpg` and `p.768.jpg` were left, and no partials.

### Second round (8cccc79)

- Reverting `hard_link` to `rename`, the taken-name check, the per-job partial, the created folders' syncs, or `undo` each failed its test.
- Round two's probe at that commit: call B was told `Exists` for `sd.mp4`, which call A's `undo` then removed (F6).

### First round (2f18bdd), still valid

**Disk full.** Writes were made to fail with `RLIMIT_FSIZE` and `SIGXFSZ` ignored, since the sandbox refuses `hdiutil attach`. ffmpeg, with the crate's arguments, exited 229 whenever the limit fell below the output's size:

- an x264 MP4 with `+faststart`, including a failure at the trailer;
- a stream-copy MP4;
- AAC in `ipod`;
- Opus in Ogg.

**Encode determinism.** Two VideoToolbox runs of one 640×360 encode gave different MD5s. Two x264 runs matched.

**Cost of a finish.** A Python replica: a fresh read-only handle, the file sync, a rename, then the folder sync, 40 runs each, median and p95:

| File | No sync | `fsync` | `F_FULLFSYNC` |
| --- | --- | --- | --- |
| 40 KB | 1.53 / 5.09 ms | 1.40 / 4.94 ms | 12.72 / 16.92 ms |
| 400 KB | 1.88 / 4.32 ms | 2.30 / 5.17 ms | 12.72 / 18.92 ms |
| 8 MB | 1.45 / 7.21 ms | 2.42 / 9.77 ms | 17.50 / 32.80 ms |

The crate's own write of 40 KB took 16.4 ms median and 25.5 ms p95.

**Contention.** A jaw.db-style database (WAL, `FULL`, `fullfsync`, `checkpoint_fullfsync`, `wal_autocheckpoint=0`) committed one 600 B row every 20 ms for 15 s. A separate process meanwhile finished 200 KB files with two full flushes each.

| Background | Commits | p50 | p90 | p99 | max |
| --- | --- | --- | --- | --- | --- |
| none | 538 | 4.97 | 6.29 | 9.66 | 33.71 ms |
| 10 files/s | 531 | 5.23 | 7.55 | 11.96 | 14.58 ms |
| flat out | 506 | 6.22 | 9.49 | 14.24 | 17.04 ms |
| none, again | 520 | 5.65 | 8.04 | 10.60 | 12.21 ms |

## Verdict

This holds for f9ed9ea: the switch to PNG for transparent previews changes only an encoder and an extension, inside the same claimed, partial-then-linked writes. Nothing marked data loss or inconsistency remains. F1 to F6 are fixed, and each fix has a test that fails when the fix is reverted. Every rule the change touches is tested:

- whole-or-nothing under a name, after a crash, a kill, a cancel, a failure or a full disk;
- the sync before the link, and the folder and created folders after it;
- never replacing a name;
- a name claimed by a call in flight is `Busy`, never `Exists`;
- a crash's partials removed, and nothing else.

P1 and H5, H8 to H11 stand as follow-ups. J1 to J9 are for jawd's wiring, and J8 must be done before this lands.

Verdict: approved
