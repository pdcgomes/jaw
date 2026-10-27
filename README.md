# jaw

WhatsApp in a terminal, fast, from the keyboard: your chats, photos, voice messages and polls. A daemon keeps the connection to WhatsApp, so the terminal is one of the interfaces that can show it. In the style of [ooze](https://github.com/pdcgomes/ooze), with [badcoffee.dev](https://badcoffee.dev)'s Mimic.

jaw is being built. [The design](docs/plans/2026-10-16-jaw-design.md) says how it works and what it costs, [the protocol](protocol/README.md) how an interface talks to the daemon, and [your first run](docs/first-run.md) what the first start on a real account does.

Its themes are [Omarchy's](https://github.com/omacom/omarchy), as Omarchy wrote them, MIT-licensed ([docs/licenses/omarchy.txt](docs/licenses/omarchy.txt)); Ctrl-, or `jaw settings` picks one.

jaw isn't affiliated with WhatsApp or Meta.

## What it costs

Measured by `make perf` on 25 October 2026, on a Mac Studio (Apple M1 Ultra, 20 cores, 128 GB, macOS 26.6.2) that other work kept at a load average of 10 to 21, with release builds against `jawd demo`'s made-up account of a million messages in 3,000 chats, and jaw attached as it would be in a terminal. The flood of 2,000 messages a second and the backlog acked in groups come from the demo engine, which hands jawd 256 messages before their acks come back. jaw-engine waits for each ack, since whatsmeow handles one message at a time, so through it a real account's burst drains at the rate of the backlog acked one at a time, about 150 a second.

| | Budget | Measured | Load |
| --- | --- | --- | --- |
| A keypress drawn | 16 ms p99 while 2,000 messages a second arrive | 0.40 ms p99, in the demo engine's flood | 10–11 |
| jaw's first frame | 50 ms after launch, with jawd running | 37 ms median, **58 ms** at worst; from jaw's `main`, 24 and 46 ms | 11–14 |
| Opening a chat | 30 ms | 22 ms p50, 28 ms p99; **53 ms** p99 during the flood | 11–14 |
| jaw's memory | under 40 MB with a screen of images | 35 MB with the library's photos, 14 MB with text | 11–17 |
| An incoming message durable and acknowledged | 10 ms p99 | 8.5 ms p99 over 1,000 one at a time, 8.3 ms through jaw-engine; **13 ms** the worst of 50 lone messages; **14.5 ms** p99 in a catch-up acked one at a time; **23 ms** p99 while the history goes in, 169 ms at worst; **34 ms** p99 in the flood, acked in groups | 10–21 |
| …a backlog draining | 200 a second with each ack waiting for its own flush, 5,000 once acks go in groups | **153** a second, a real account's rate; 8,692 a second with the demo engine's 256 in flight, which jaw-engine doesn't do | 10–11 |
| An incoming message on its way to interfaces, once committed | 5 ms p99 | 2.9 ms p99 | 11–14 |
| jawd's own work in the commit-acked round trip, apart from the flush | under 200 µs p50 | about 60 µs, 110 µs with jaw-engine's hop | 10–11 |
| A 100-message page | 5 ms at any depth | 2.0 ms p99 on the socket, 0.8 ms through the read pool | 11–17 |
| Search | 50 ms over a million messages | 15 ms p99 | 11–14 |
| jawd's memory | under 100 MB steady, 250 MB during a full history sync | 59 MB steady, 81 MB in the flood; **8.4 GB** during the history | 11–21 |
| jaw-engine's memory | under 80 MB steady, 200 MB during a full history sync | 21 to 27 MB, unlinked; not measured during a history | 11 |
| A 12-megapixel photo ready to upload | 300 ms | 96 to 240 ms, as JPEG and HEIC | 13–15 |
| Idle | no drawing, no wake-ups, no polling | jaw: no frames, one wake-up a minute for its clock; jawd: **9 wake-ups** a minute | 11–14 |

Where it falls short, and why:

- **jawd's memory during a full history sync.** The history goes in at 12,000 messages a second, but each blob of 20,000 messages is decoded whole into prost's structs, where a message takes 16 KB whatever it holds because WhatsApp's `Message` keeps its hundred-odd optional parts inline; `convert::history::remainder` then clones the whole blob, and what's freed isn't given back to the system before the next blob. It falls to 89 MB once the history is in, and a fresh jawd on it takes 59 MB. Boxing those parts in wa-proto's code generation, cloning only what `remainder` keeps, or decoding a conversation at a time would each cut it.
- **Indexing for search after the history takes 15 minutes.** An index step that finds the WAL past its soft limit gives way without waking the checkpointer, which copies only once 1,024 frames wait; with nothing else committing, each stall lasts until its 10-second idle timer, while the writer looks again every millisecond. `jawd bench store`, whose live message every 5 ms keeps the checkpointer awake, indexes the same million in 136 s.
- **Durable acks.** A lone message pays one flush, and the drive's `F_FULLFSYNC` alone is 5.0 ms p50 and 9.9 ms p99 here, so the budget sits on the drive's tail: 1,000 round trips came to 8.5 ms p99, but the worst of 50 lone messages, with their conversions and windows going on around them, took 13 ms, and a catch-up acked one at a time came to 14.5 ms p99. While the history goes in, the live messages handed over every 100 ms were acked at 7.8 ms p50, 22.6 ms p99 and 169 ms at worst. The design says history can delay a message by no more than one short transaction, so that's a defect, and its cause isn't established yet: the database expert's audit, which measured 26.9 ms p99 at a load of 21 to 24, didn't find it either. In the flood an ack waits for its group's commit behind the rows before it: 16 ms p50, 34 ms p99.
- **A backlog acked one at a time** drains at 153 a second, and that's a real account's rate, since jaw-engine acks one at a time: each ack waits for its own flush, and converting the message before it takes the writer between flushes.
- **Converting behind a flood.** With the demo engine's 256 acks in flight, acks keep up with 2,000 a second, but turning the inbox into messages on a million-message jaw.db ends 8.5 s after the flood, about 1,750 a second, so in the flood the open chat's messages reached the interface 1.4 s after they were handed over at p50 and 8.4 s at p99. A catch-up acked in groups in 3.5 s is converted in 20 s. Through jaw-engine, which acks one at a time, a burst like this waits on WhatsApp's side and drains at about 150 a second.
- **jaw's first frame and opening a chat.** Between 7 and 37 ms (11 ms median) pass from launch to jaw's `main`: the system starting the process, which on this Mac includes its endpoint security looking at each launch. Within jaw, the first frame and an opened chat each wait for the next frame: jaw-client wakes jaw at most once a frame and the loop draws at most once a frame, so a page jawd answers in 1 ms is drawn about 20 ms after the key. Waking and drawing at once for the answer to what was just asked would take most of that off.
- **Idle.** Six of jawd's nine wake-ups are the checkpointer's 10-second timer, which goes on waking after it has emptied the WAL; it could wait for the next commit instead.
- **Not measured.** jaw-engine's memory during a full history sync needs a linked account, and nothing here talks to WhatsApp. whatsmeow decodes each blob into Go structs, where an absent part is a nil pointer, and the engine encodes it again for the spool, so it should hold a few copies of one blob at a time, tens of MB for blobs like these, plus Go's garbage, which `GOMEMLIMIT` would bound; jawd doesn't set it yet. Nor has the design's small Linux machine been measured; `make perf` is written to run there too, but hasn't yet.

`make perf` takes half an hour, most of it the indexing above, and writes the full table, with how each number was taken, to `target/perf/`. The same account is there to try jaw without WhatsApp: `jawd demo` in one terminal, then `JAW_HOME=$TMPDIR/jaw-demo jaw` in another. Its history takes a minute and a half to arrive the first time, search fills in over the quarter hour after, and `--flood 20` adds some life.

## License

[MIT](LICENSE)
