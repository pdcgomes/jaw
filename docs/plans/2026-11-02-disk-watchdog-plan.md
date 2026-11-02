# jawd: the disk watchdog

**The finding:** P11 of the data review, with what's left of D7. Disk space isn't watched: a history sync can need several times jaw.db's size in WAL, and media downloads and the spool share the disk, so `SQLITE_FULL` rolls a transaction back and then nothing can commit, the inbox included. Main already does D7's first half: an inbox commit that fails is kept and tried again from 10 ms doubling to 1 s, a full disk says "jaw's disk is full…" in an error `notice` and in `status`'s `storage.trouble`, and media checks the free space before each download (512 MB beside a file nobody asked for, 64 MB beside one asked for). What's left is D7's "pause background lanes and downloads" and P11's watchdog and ballast.

## What changes

- **`disk.rs`, the watchdog.** `Disk` reads the free space and size of the disk jaw's folder is on with `statvfs`, as media and the upgrade's check do, each time it's asked (`room`). Below the floor, the background work and downloads below wait, and they go on once the disk has the floor and 256 MB more, so they don't stop and start at its edge. The floor is 2 GB, or 5% of the disk where that's less (1 GB of a 20 GB disk). A store opened without one (`Store::open`, every test but these, and `jawd`'s commands other than serving) has a disk that's never short of room and keeps no ballast; `jawd` serving watches its folder's.
- **The writer** waits for room before each background transaction, as it waits for the WAL's (`wait_for_disk`, beside `wait_for_room`), taking inbox rows and live writes meanwhile, so the watchdog never holds a live write up. Indexing for search is background work too: a step waits while the disk is short of room. Every background producer (history, its blobs' conversion, the backfills, media's rows, avatars, merges) waits at its next transaction, so what it holds back is bounded by what it does before one.
- **Downloads.** Media's scheduler starts no download while the disk is short of room, one an interface asked for included: it stays asked and goes once there's room, when the scheduler is woken. `media.paused` says why. The history fetcher waits for room before it asks the engine for a blob. Avatars, one small picture at a time, wait at their next transaction.
- **Interfaces are told** with a `notice`, as the protocol has room for: `warning` as the disk goes below the floor ("jaw's disk has 1.8 GB free, under the 2.0 GB it keeps…, so history, downloads and other background work wait until there's room; messages still arrive and send"), and `info` once it has room again. Main's error notice for a write a full disk refused stays as it is.
- **The ballast.** jawd keeps a 64 MB file, `ballast`, in jaw's folder: reserved with `F_PREALLOCATE` on macOS and `posix_fallocate` on Linux, or written as zeros where neither works, under `ballast.partial` and renamed once it's whole. When a write to jaw.db fails for a full disk (the writer's `failing`, and the checkpointer's passes), jawd deletes it, so the next try of the commit, the checkpoint or the rollback has room. The watchdog makes it again, every 10 s, once the disk is above the floor with room for it beside the floor; a ballast of another size is made again, and a partial one is removed.
- **`jawd status`** gets `disk`: `free`, `size`, the `floor`, `paused` while background work and downloads wait, the `ballast` bytes kept (0 once given up, until it's made again), how often it was `ballast_given_up`, and the background transactions' `waits` for room and `waited_ms`; `jaw status` and `jawd status` print it. The protocol's `status` row says so.
- **`scripts/check-data-audit.py`** lists `crates/jawd/src/disk.rs` as the data layer.
- **Left alone:** the free space read is `statvfs`'s, which leaves out what macOS can purge; and on APFS a local snapshot taken while the ballast was there holds its blocks until macOS purges it, so the ballast's room comes at once only where no snapshot holds it.

## The tests

All with a fake free-space source; the real disk is never filled. A test hook in jaw's VFS (`noted::fill`) has writes to jaw.db's files fail with `SQLITE_FULL` while the fake disk has nothing free, so a full disk is SQLite's own failure.

- `disk.rs`: the floor (2 GB, or 5% where that's less); `room` going below the floor and back, at the floor and 256 MB more, saying why while it's short; the ballast made at its size under its partial name and renamed, not made while the disk is short of room, given up, made again once there's room for it, a partial one removed and one of another size made again.
- `store`: below the floor a background transaction waits, and indexing for search, while inbox rows and live writes go; above it they go on; `status` says so and counts the wait. On a full disk an inbox row's commit fails, the ballast is given up, and the row is committed with its room; a checkpoint that fails for a full disk gives it up too.
- `media`: below the floor, a photo that downloads by itself and one an interface asks for aren't downloaded, and `media.paused` says why; above it both are, without asking again.
- `sync`: below the floor, a history notification's blob isn't asked for; above it, it is and is stored.
- `main` (the notices): the disk going below the floor sends a warning notice, and its room coming back an info one.

## Each rule's switch

Each switched off in turn and its test shown failing, the results beside the report: no wait before a background transaction; a wait that doesn't take inbox rows and live writes; indexing not waiting; the scheduler starting downloads below the floor; it not woken once there's room; the fetcher not waiting; no notice either way; resuming at the floor rather than past it; the floor not the lesser of the two; the ballast not given up by the writer, nor by the checkpointer; made while the disk is short of room, or never made again; and a partial one taken as whole.

## Done when

The tests pass and each switch fails its test; the gate (`make check-generated`, `make lint`, `make build`, `make test`) passes as the brief says; and the database expert's audit approves the branch, `make check-audit` passing once its note and trailers are in.
