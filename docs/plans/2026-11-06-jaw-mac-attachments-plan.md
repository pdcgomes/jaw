# Jaw for the Mac: attaching photos, videos and files

6 November 2026. The plan for parity step 2.5, the box design's B2: what's attached waits in a strip above the box's field and goes with `send.media`, its progress coming as `job` pushes. It builds what [the box design](2026-10-27-jaw-mac-box-design.md) and [the Mac design](2026-10-26-jaw-mac-design.md)'s "Sending media" say, in [the parity plan](2026-10-27-jaw-mac-parity-plan.md)'s step 2.5, on the protocol's [Media out](../../protocol/README.md#media-out). The owner chose it on 6 November, after finding the + menu's Photos & Videos… and Document… only saying they come later, and approved the design below as it was put to him.

## Where it starts

- **jawd serves `send.media`** on main, with its `job` pushes, and the terminal attaches with Ctrl-O since 26 October (`crates/jaw/src/view/attach.rs`). jawd's demo engine uploads, so the demo account sends media end to end.
- **The Mac's + menu** has Photos & Videos… (⌘O) and Document… (⇧⌘O) as commands that say "sending photos and videos comes later" and "sending documents comes later". Contact… and Poll… are built; Camera…, Event… and Location… come with B3.
- **`mac/parity.toml`** holds `send.media` and the `job` push for step 2.5.

## Decisions

- **The text is the first file's caption,** as the box design has it (Owner, 27 Oct) and the terminal sends it; the Mac design's "each with its caption" is the earlier wording. A caption for each file can follow.
- **⌘O opens the open panel on photos and videos,** several at once, which reaches the Photos library from its sidebar; **⇧⌘O on any file,** which goes as a document whatever it is (`kind` `document`). Pasted and dropped files go with no `kind`, so jawd sends each as what it is.
- **Pasting** (⌘V) files from the Finder attaches them all; an image with no file, as a screenshot copied, is written as a PNG into the run's own folder (`Opened.folder`) and attached; anything else pastes as text. **Dropping** files, images or the promises Photos and browsers drag out does the same, on the box, or on a pane while ⌥ is held, which stages them in that pane's chat and gives it the keys; a plain drop on a pane stays a chat from the list.
- **The strip** sits between the bar and the field: a tile for each file, its thumbnail from Quick Look made off the main thread (a document's icon until then) and a document's name, an × on each, at most 30 (`SendMedia.filesMost`, as jawd takes), and SD or HD at its end while it holds a photo or a video. Tab switches SD and HD, as the terminal's does; Settings › Messages sets the default ("Send photos and videos in HD", off); ⌥ held as Send is clicked sends HD.
- **Sending:** Return or Send sends the files with what's typed, as a reply when the box is replying, with a key; the strip and the field empty at once. jawd turning them down puts them back in the strip and the words back in the field, with "Not sent: " and jawd's words in the bar, as text comes back; an answer a dropped connection lost is asked again with its key once jawd is back, and past a day they come back as text does. A file the app wrote for a paste or a drop goes once jawd has answered, jawd having kept its own copy.
- **Esc** drops what the bar says first (a reply, an edit, what didn't send), then the strip. Attaching while editing is turned down in the bar: files go with a new message.
- **A file of yours on its way** shows how far it has got, from `job` pushes: a ring over its picture, or in a file's circle, filling as the copy is made and then as it uploads, with × to stop it, and "making it ready 40%" or "uploading 70%" across a picture's foot or on a file's second line, as the terminal says it. The progress is part of the message's layout context, a whole percent, so a push lays out that one message again and nothing else. Stop Sending, in the message's menu, the Message menu and the palette, asks `media.cancel`; the message is then failed, and Send Again sends it again.
- **ffmpeg:** the app asks jawd's `status` quietly once it's connected; while the strip holds a video, a GIF or an audio file and `status` says ffmpeg is missing, the strip says so in jawd's words.
- **Not in it:** the camera (B3), events and places (B3), a caption for each file, editing pictures before they go, and the system's Photos picker.

## Tasks

### Task 1: `send.media` and the `job` push in JawKit

**Files:** `mac/JawKit/Sources/Protocol/Media.swift`, `mac/JawKit/Sources/Client/Incoming.swift`, `mac/JawKit/Sources/Client/Client.swift`, `mac/JawKit/Sources/Client/Jawd.swift`; tests in `mac/Tests/JawKitTests/MediaOutClientTests.swift` (new) and `mac/Tests/JawKitTests/FixtureTests.swift`.

**Produces:**

- `MediaKind` (`image`, `video`, `gif`, `audio`, `document`), `SendMedia` (`chat`, `files: [SendMedia.File]`, `quality: Variant`, `reply`, `key`; `SendMedia.filesMost = 30`), `SendMedia.File` (`path`, `kind: MediaKind?`, `caption: String?`), `SentMedia` (`messages`, `album`, `rows`), `Job` (`id`, `kind: Job.Kind` of `transcode`, `upload`, `download` or `other`, `message`, `variant`, `progress`).
- `Expect.sentMedia` and `Reply.sentMedia(SentMedia)`; the `job` push read as `Incoming.job(Job)` and drained as `JawUpdate.job(Job)`.
- `Jawd.sendMedia(chat:files:quality:reply:key:)`, answered as `JawUpdate.mediaSent(key:messages:rows:)`, turned down as `JawUpdate.mediaUnsent(chat:key:files:why:)` in jawd's words, a lost answer `.unsure` and asked again with its key once jawd is back, or handed back past a day; `Jawd.cancel(chat:message:what:)` naming what's stopped.
- `Jawd.status(shown:)`: `.status(Status)` as now, or, not shown, `JawUpdate.statusRead(Status)`.

**Tests:** every fixture decodes, `send.media.result.json` as `SentMedia` and `job.push.json` as a typed `job`; `SendMedia` writes `send.media.request.json`'s params exactly; files go with their key, the caption on the first and none on the rest, the quality and the reply, and come back as jawd's rows; a lost answer is asked again with the same key and, past a day, comes back whole; jawd's refusals (a file it can't read, too many files, a file over 2 GB, the outbox full, a jawd without `send.media`) come back whole in its words; `job` pushes drain as `.job` in order.

**Done when** `mac/scripts/build-mac test JawKit` passes.

### Task 2: what's attached, as a value, and what's pasted or dropped

**Files:** `mac/JawUI/Sources/Conversation/Attachments.swift` (new); tests in `mac/Tests/JawUITests/AttachmentsTests.swift` (new).

**Produces:** `Attachment` (`url`, `kind: MediaKind?`, `name`, `isVisual`, `needsFFmpeg`, `written`, for a file the app wrote); `Attachments` (`files`, `hd`, `add(_:) -> Int` of those left out past 30, `remove(at:)`, `showsQuality`, `needsFFmpeg`, `files(caption:) -> [SendMedia.File]`); `Pasted.attachments(from: NSPasteboard, into: URL) -> [Attachment]?`, nil for text, which reads file URLs first, then an image, written as a PNG into the folder given.

**Tests:** files keep the order they came in, a file already there isn't added twice, and past 30 the rest are left out and counted; SD or HD shows only with a photo or a video; ffmpeg is needed for a video, a GIF or an audio file; the caption goes on the first file alone, trimmed, none when empty, and a document keeps its `kind`; a pasteboard of file URLs gives them, one of an image gives a PNG written into the folder that reads back as the image, and one of text gives nil.

**Done when** `mac/scripts/build-mac test JawUITests` passes.

### Task 3: the strip in the box, and the box sending files

**Files:** `mac/JawUI/Sources/Conversation/AttachmentStrip.swift` (new), `mac/JawUI/Sources/Conversation/Composer.swift`, `mac/JawUI/Sources/Conversation/ConversationController.swift`; tests in `mac/Tests/JawUITests/ComposerAttachTests.swift` (new).

**Produces:**

- `AttachmentStrip`, an `NSView`: the tiles in a row that scrolls across, each with its thumbnail, a document's name and an ×; SD or HD at its end, a click switching it; the ffmpeg line under it when it's needed. Thumbnails come through `thumbnails`, Quick Look's by default, so the harness can give its own.
- `ComposerController.attach(_:)`, `attachments`, `hdByDefault`, `ffmpegMissing`, `unsent(files:caption:why:)`; `ComposerDelegate.composer(_:send:caption:hd:key:reply:)`; Return and Send with files, Tab, Esc, ⌥ on Send, ⌘V, and drops on the box, files and promises alike.
- `ConversationHost.conversation(_:send:caption:hd:key:reply:)`, with a default that does nothing, and `ConversationController.unsent(files:caption:why:)`, `attach(_:)`.

**Tests:** attaching shows the strip and Send; Return sends the files with the text as the caption, the reply, a key and SD, and empties the box; ⌥ on Send and Tab send HD, Tab only with a photo or a video; Esc drops the reply, then the files; a 31st file is left out, the bar saying 30 at most; files turned down come back with the words in the field and "Not sent: " in the bar; attaching while editing is turned down; pasting files attaches them, an image attaches a PNG, and text types.

**Done when** `mac/scripts/build-mac test JawUITests` passes.

### Task 4: a file of yours on its way

**Files:** `mac/JawUI/Sources/Layout/MessageLayout.swift`, `mac/JawUI/Sources/Layout/Cards.swift`, `mac/JawUI/Sources/Files/MediaOffer.swift`, `mac/JawUI/Sources/Conversation/MessageView.swift`, `mac/JawUI/Sources/Conversation/ConversationController.swift`; tests in `mac/Tests/JawUITests/UploadTests.swift` (new).

**Produces:** `Sending` (`step`: making it ready or uploading, `percent`) and `MessageContext.sending`, by message, for the row and its album's items; `ConversationController.jobs(_:)`, which takes jawd's jobs and lays out again only the rows whose progress changed; `MediaOffer.sending`, its state act Stop Sending.

**Tests:** a photo of yours with a job draws a ring and × and says "uploading 70%", one with none draws neither; a file's second line says "making it ready 40%"; an album's tile with a job has its ring; a click on the ring asks `.stop`; a pending file of yours offers Stop Sending, someone else's or a sent one doesn't; a pane given jobs puts each in its row's context, a whole percent, and a job for a message it doesn't hold changes nothing.

**Done when** `mac/scripts/build-mac test JawUITests` passes.

### Task 5: the app: ⌘O, ⇧⌘O, sending, and what jawd answers

**Files:** `mac/JawApp/Sources/Attaching.swift` (new), `mac/JawApp/Sources/Commands.swift`, `mac/JawApp/Sources/AppModel.swift`, `mac/JawApp/Sources/Bench.swift`, `mac/JawUI/Sources/Style/Preferences.swift`, `mac/JawUI/Sources/Settings/SettingsView.swift`, `mac/JawUI/Sources/Panes/GridView.swift`.

**Produces:** Photos & Videos… and Document… open the panel over the window and attach what's chosen to the focused pane's box; the box's sends go to `Jawd.sendMedia`, and its answers, refusals and jobs to the panes; the written files go once jawd has answered; `status` read once connected, for ffmpeg; Settings' HD default; files dropped on a pane with ⌥ held go to that pane's box; the snapshot's steps `attach:<path>|<path>`, `attach-doc:<path>` and `rows`, which says the focused pane's newest rows with their state and progress.

**Done when** the app builds, and on the demo a snapshot run attaches a photo and a document, the strip shows them, Return sends them, and `rows` says the messages pending with their progress and then sent.

### Task 6: the harness

**Files:** `mac/JawHarness/Sources/Scenes/AttachmentScenes.swift` (new), `mac/JawHarness/Sources/BuiltInScenes.swift`, `mac/JawUI/Sources/Samples.swift`.

**Produces:** the scenes "attachment-strip" (one photo; three photos and a document in HD; 30 files; turned down; ffmpeg missing) and "upload-states" (making a photo ready, uploading a video, an album's items, a document, one stopped).

**Done when** `mac/scripts/build-mac test JawHarnessTests` passes and `build-mac shot` draws both scenes as their notes say.

### Task 7: the docs and the parity list

**Files:** `mac/parity.toml`, `mac/README.md`, `docs/plans/2026-10-27-jaw-mac-parity-plan.md`, `docs/plans/2026-10-27-jaw-mac-box-design.md`, this plan.

**Done when** `make check-parity` passes with `send.media` and `job` out of the list, and the README, the parity plan's step 2.5 and the box design's build order say what's built and what's left.

### Task 8: end to end, review, and landing

`make mac-test`; a snapshot run on the demo from attaching to sent; a review of the branch; the owner trying it on his account. It lands on main once he says so.
