# Links and polls in the Mac app (parity step 3.3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** [The parity plan](2026-10-27-jaw-mac-parity-plan.md)'s step 3.3 in jaw's Mac app, on what `m2-content` brought jawd on 28 October (6c1c81f): link previews drawn as [the Mac design](2026-10-26-jaw-mac-design.md) has them ("the 140-pixel thumbnail jawd keeps, the title, two lines of the description and the host, under the text; a click opens the link in the browser"), for messages received and sent; polls with their options, live tallies and how many voted, voted in from the card with `poll.vote`; a poll made from the box's + menu with `send.poll`, as [the box design](2026-10-27-jaw-mac-box-design.md)'s Poll form has it (its B3, brought forward for polls alone); and links in the palette's answers. The box's preview of a link while it's typed needs a request the protocol hasn't got, so this step proposes it (P11) rather than building it.

**Architecture:** JawKit reads a link preview's thumbnail and a poll's voters from the rows, and sends `send.poll` as it sends `send.text` (a key made for it, asked again with it after a lost answer, a refused poll handed back whole) and `poll.vote` as it sends `react` (an action with a key of its own). JawUI lays a link's card out under its text, its thumbnail decoded on the layout queue from jawd's folder through a `PictureSource` the layout spec carries, so the harness and the tests draw made-up thumbnails without files; it lays a poll's card out with a tick, a bar and a count for each option and how many voted, each option a region a click votes in; and it has the poll form's model and view. The app's model turns a vote and a poll into JawKit's requests, opens the form as a sheet, and opens it again when jawd turns a poll down. The palette lists `find`'s links, Enter opening one and ⌘↩ going to its message.

**Tech Stack:** Swift 6 (AppKit, SwiftUI, Core Text, ImageIO, Swift Testing), Tuist through mise, xcodebuild on the build Mac through `mac/scripts/build-mac`; Python 3 for `make check-parity`.

## Where it starts

- **jawd** (main since 6c1c81f): a message row's `link` has `url`, `title`, `description` and `preview`, the thumbnail's path under `media/links/` once jawd keeps it (140 pixels on its long side for what you send, as it came for what others send); a poll row's `poll` has `question`, `options` (`name`, `votes`, `mine`), `max` and `voters`, upserted with a new `v` as votes come; `send.poll` (`chat`, `question`, `options`, `max`, `key`) answers as `send.text` does; `poll.vote` (`message`, `options` by name, none to take your vote back, `chat`, `key`) is an action on a message, answered once it's on disk. `jawd --engine demo`, which `--demo` runs, has a link with a 96-pixel thumbnail in Ana's chat and Rui's poll in Lisbon Climbing ("Onde jantamos sexta?", two votes), and answers both requests.
- **The Mac** (main at 8e0a63d): JawKit decodes `link` without `preview` and `poll` without `voters`, and sends neither request. The cards draw a link's title, one line of its description and its host above the text, with no thumbnail, and a poll's options with bars against the most-chosen option, ending "vote on your phone". The + menu's Poll… says "polls come with jawd's send.poll". The palette counts `find`'s links in its strip but lists none: their rows decode as untyped JSON, and a hit with no pick is skipped.
- **The terminal** (main and every branch): sends neither `send.poll` nor `poll.vote` ("polls come with M2"), draws a poll as "☰ poll", and lists `find`'s links as "↗ … in the library". So `make check-parity` lists nothing for 3.3, and this step leads the terminal on polls: the check will say the Mac uses `send.poll` and `poll.vote` where the terminal doesn't, which isn't a failure.

## Questions

What the designs leave open, each with its options and the lean built; none blocks a task, and each is a small change in the one place named.

1. **Where a link's card sits.** The Mac design puts it under the text; WhatsApp's apps draw it above. A: under the text, as the design says (lean; the parity plan's rule 1, the design wins); B: above, as WhatsApp. Task 2, in `MessageLayout.Builder`.
2. **Decoding the thumbnail.** Mac-M2's image queue (four at a time, cancelled when scrolled past, the 64 MB cache) is the parity plan's step 2.2, not built. A: decoded on the layout queue as the message is laid out, from a bounded cache of decoded thumbnails, since a link's is a few kilobytes and 140 pixels (lean); B: wait for 2.2. Task 2, in `FolderPictures`; 2.2 can take it over.
3. **A poll's bars.** The card today measures each option against the most-chosen. A: against how many voted, as WhatsApp's do, so half the voters is half a bar (lean); B: against the most-chosen. Task 3.
4. **Who voted.** jawd sends how many voted (`voters`) and, for each option, how many chose it and whether you did; not who. A: "4 voted" under the options, as the Mac design's "how many voted" (lean); B: names, which needs jawd to say them (WhatsApp's View Votes), a proposal for later if the owner wants it. Task 3.
5. **A click on your own choice.** A: in a one-choice poll it takes your vote back (`options` empty), as WhatsApp's apps do, and in a poll of several it takes that one out (lean); B: nothing. Task 4, in `Voting.choosing`.
6. **One choice past a poll's most.** jawd refuses a vote choosing more than `max` (error 4). A: the card says so ("this poll takes 2 at most; take one back first") and sends nothing (lean); B: send it and say jawd's refusal. Task 4.
7. **Voting from the keys.** The digits 1 to 6 react to a selected message, in the terminal and the Mac alike. A: Enter on a selected poll opens its Vote menu under it, each option on 1 to 9, and the message's menu has Vote ▸ beside React ▸ (lean); B: the digits vote on a poll. Task 4.
8. **The poll form's Return.** The box design has "options added with Return" and "Return sends". A: Return in the question goes to the first option; in an option with text, it goes to the next or adds one (12 at most); in an empty option, it sends once the poll is ready; ⌘↩ sends from anywhere, and Esc cancels (lean); B: Return always sends. Task 5, in `PollForm.returned(in:)`.
9. **"Allow several answers".** A: `max` is every option, as WhatsApp's "allow multiple answers" sends it (jawd sends that as any number) (lean); B: a number to pick. Task 5.
10. **A poll jawd turns down.** A: the sheet opens again with the poll as it was and jawd's words at its foot, as the box gets its text back (lean); B: said in the status line only. Task 5.
11. **The box's preview while a link is typed.** The protocol has no request for a preview before sending: jawd fetches the page as `send.text` sends it. A: propose `link.preview` (P11) and show nothing in the box until jawd serves it (lean); B: show only that a preview will go, with an × sending `preview: false`. Task 7.
12. **The tallies' growth.** The design's animations table has bars growing to new widths in 200 ms. A: with Mac-M5's animations, every animation at once with its Reduce Motion form (lean); B: here. The tallies change as rows come either way.
13. **A link found in the palette.** A: Enter opens it in the browser and ⌘↩ goes to its message, as the library's rows will (lean); B: Enter goes to the message. Task 6. *Amended while it was built:* the palette already has ⌘G (and ⌃G) for going to a message found, as the terminal's library has Ctrl-G, so ⌘G goes to a link's message too, and ⌘↩ on a link does what Enter does, as the palette's ⌘↩ does on anything but a chat.

## Global Constraints

- The spec is the parity plan's step 3.3, the Mac design's "Link previews" and "Polls" (its "Every inline kind"), the box design's "The forms" (Poll) and the protocol's message rows, `send.poll`, `poll.vote` and Finding's links (`protocol/README.md`), with their fixtures. Where this plan and they differ, they win: stop and say so.
- The branch is `mac-links-polls`, in its own worktree, `~/src/jaw-mac-links`, from main at 8e0a63d. Work only there. Never push, never pass `--no-verify`. Never edit, build in or run a git command that changes anything in another worktree (`~/src/jaw-mac-folders`, `~/src/jaw-panes`, `~/src/jaw-mac-perf` among them) or the owner's checkout; read other branches with `git show <branch>:<path>`.
- Never open `~/Library/Application Support/jaw`; the app runs only with `--demo` or `--home` on a folder of its own with a `jawd demo` started for it; never stop a process by name, only a PID started here; nothing is sent to WhatsApp, and nothing typed in a test is a URL jawd would fetch.
- Build and test on the build Mac with `mac/scripts/build-mac` (`test JawKit`, `test JawThemes`, `test JawUITests`, `test JawHarnessTests`, `build jaw`, `snapshot`, `shot`), or `make mac-test` with `CARGO_INCREMENTAL=0`. jawd is built here, `cargo build -p jawd --bin jawd` in the worktree, since the build Mac has no Rust. If the build Mac doesn't answer, say so and run what can run here.
- Swift 6 language mode, macOS 26, warnings are errors. `mac/scripts/check-components` refuses fonts, radii and line widths set by hand outside `JawUI/Sources/Components` (the message layout's own fonts are `FontSet`'s); `mac/scripts/check-harness` refuses a view in the app target and `Jawd` or `Client` named in the harness. Everything drawn is in JawUI, made from values; `AppModel` is the only code that talks to jawd.
- Every new component has a harness scene (`JawHarness/Sources/Scenes/`, registered in `BuiltInScenes.swift`, its `covers` naming it), and the scenes' list in `ScenesTests` grows with it. `check-harness`'s third rule, which fails on a public JawUI view no scene covers, is the harness's third phase and isn't on main; until it is, `covers` is kept right by hand.
- Draw what jawd says: a vote and a poll show when jawd's rows come back, never before. A request jawd doesn't serve says what it waits on, in Jawd's words ("voting comes with the next step").
- Nothing here is on jaw's data paths (`scripts/check-data-audit.py`): the app reads rows and thumbnails jawd wrote and writes nothing of jawd's. P11 is a proposal for jawd's builders, written into the Mac design and the protocol's Composing (proposed); building it is jawd's, audited by the database expert, since it keeps thumbnails under `media/links/`.
- Scratch goes in `/tmp/jaw-mac-links/` and the worktree's ignored `mac/build/`, removed before the report; `make prune-builds` before it too.
- Commit messages are one long sentence in the project's voice (`git log -5`), no prefixes; one commit a task, at least one an hour. Code matches its surroundings: doc comments in plain sentences, British spelling, no comments narrating a change, 4-space indentation, trailing commas in multi-line argument lists.

## File structure

New:

- `mac/JawUI/Sources/Layout/Pictures.swift` (Task 2): `PictureSource`, `FolderPictures`, and the layout's `.picture` part's drawing.
- `mac/JawUI/Sources/Conversation/Voting.swift` (Task 4): what a click on an option votes for.
- `mac/JawUI/Sources/Conversation/PollForm.swift` (Task 5): the poll form's model, `PollForm`, and its sheet, `PollFormView`.
- `mac/JawHarness/Sources/Scenes/LinksAndPollsScenes.swift` (Tasks 2, 3, 5): `link-previews`, `polls`, `poll-form`.
- Tests: `mac/Tests/JawKitTests/PollClientTests.swift` (Task 1); `mac/Tests/JawUITests/LinkPreviewTests.swift` (Task 2), `PollTests.swift` (Tasks 3 and 4), `PollFormTests.swift` (Task 5), `PaletteLinkTests.swift` (Task 6); references `links-{bubbles,lines}-{dark,light}.png` and `polls-{bubbles,lines}-{dark,light}.png`.

Changed:

- `mac/JawKit/Sources/Protocol/Rows.swift` (`LinkPreview`, `Poll`), `Actions.swift` (`SendPoll`, `PollVote`), `Find.swift` (a link hit's row, Task 6); `mac/JawKit/Sources/Client/Jawd.swift` (`sendPoll`, `vote`, `JawUpdate.pollUnsent`).
- `mac/JawUI/Sources/Layout/Cards.swift`, `MessageLayout.swift`, `Layouts.swift` (the spec's pictures); `mac/JawUI/Sources/Samples.swift` (links, polls, their pictures); `mac/JawUI/Sources/Conversation/ConversationController.swift`, `MessageView.swift` (voting, the Vote menu, VoiceOver); `mac/JawUI/Sources/Palette/Palette.swift` (Task 6).
- `mac/JawApp/Sources/AppModel.swift`, `Commands.swift`, `MainWindowController.swift`, `Bench.swift` (the snapshot's `vote:` and `poll:` steps).
- `mac/JawHarness/Sources/BuiltInScenes.swift`, `Scenes/ConversationScenes.swift`, `Scenes/PaletteScenes.swift`, `Fixtures/Screens.swift`.
- `mac/Tests/JawKitTests/FixtureTests.swift`; `mac/Tests/JawUITests/LayoutTests.swift` (and the five `conversation-*` references, drawn again as the sample link's card moves under its text), `PaneMenuTests.swift`, `TypingTests.swift` (their hosts vote); `mac/Tests/JawHarnessTests/ScenesTests.swift`.
- `mac/README.md`, `docs/plans/2026-10-27-jaw-mac-parity-plan.md`, `docs/plans/2026-10-26-jaw-mac-design.md` (P11), `protocol/README.md` (P11 in Composing (proposed)) (Tasks 7 and 8). `mac/parity.toml` keeps its rows: 3.3 has none.

---

### Task 1: JawKit reads links' thumbnails and polls' voters, and sends polls and votes

**Files:**
- Modify: `mac/JawKit/Sources/Protocol/Rows.swift` (`LinkPreview`, `Poll`), `mac/JawKit/Sources/Protocol/Actions.swift`, `mac/JawKit/Sources/Client/Jawd.swift`, `mac/JawApp/Sources/AppModel.swift` (`apply`: `.pollUnsent` said until Task 5)
- Create: `mac/Tests/JawKitTests/PollClientTests.swift`
- Test: `mac/Tests/JawKitTests/FixtureTests.swift`

**Interfaces:**
- `LinkPreview` gains `preview: String?`, the thumbnail's path under jawd's root, and `host`, the URL's host without `www.` (the URL itself when it has none); `public init(url:title:description:preview:)`.
- `Poll` gains `voters: Int` (0 when absent) and `mine: [String]`, the options you chose, in their order; `public init(question:options:max:voters:)` and `Poll.Option.init(name:votes:mine:)`.
- `public struct SendPoll: Encodable, Sendable, Hashable` (`chat`, `question`, `options`, `max`, `key`) and `public struct PollVote: Encodable, Sendable, Hashable` (`message`, `options`, `chat`, `key`), written as their fixtures are.
- `Jawd.sendPoll(chat:question:options:max:key:)`: answered as `.sent(key:message:row:)`; a lost answer is `.unsure` and asked again with the same key once jawd is back, within `keyKept`; past it, or refused, it's `.pollUnsent(chat:key:poll:why:)` with the poll whole.
- `Jawd.vote(chat:message:options:) -> String`: an action with a key of its own, "voting", as `react` is: `.acted(key:said:)` once done or refused in jawd's words, `.unsure` and asked again when lost.

**Tests:**
- `each request JawKit writes equals its fixture` gains `send.poll.request.json` and `poll.vote.request.json`; `every result and push fixture decodes` reads `send.poll.result.json` as `send.text`'s answer.
- `a link's preview and a poll's tallies decode as jawd sends them`: `send.poll.result.json`'s row is a poll of three options, none chosen, 0 voted; a row with a link has its `preview` and its host without `www.`; a poll without `voters` reads 0.
- `PollClientTests` against `FakeJawd`: a poll goes with its key and comes back as the box's message does; a lost answer is asked again with the same key, and one lost past a day comes back whole; one refused comes back whole with jawd's words; a vote goes with a key of its own and is asked again with it once jawd is back; a vote refused says why in jawd's words, and one a jawd without `poll.vote` turns down says it comes with the next step.

**Done when** the JawKit suite passes on the build Mac, the app builds, and `make check-parity` passes, saying the Mac uses `link.cancel`, `poll.vote` and `send.poll` where the terminal doesn't.

- [ ] Step 1: Write the tests, and run them to see them fail.
- [ ] Step 2: The rows' fields and the requests' params.
- [ ] Step 3: `sendPoll` and `vote` in `Jawd`, beside `sendText` and `react`, with `JawUpdate.pollUnsent`; the app says a poll turned down in the status line until Task 5 gives it its sheet.
- [ ] Step 4: Run the JawKit suite and build the app on the build Mac; `make check-parity`.
- [ ] Step 5: Commit.

### Task 2: A link's preview, under its text, with its thumbnail

**Files:**
- Create: `mac/JawUI/Sources/Layout/Pictures.swift`, `mac/JawHarness/Sources/Scenes/LinksAndPollsScenes.swift`, `mac/Tests/JawUITests/LinkPreviewTests.swift`
- Modify: `mac/JawUI/Sources/Layout/Cards.swift`, `MessageLayout.swift`, `Layouts.swift`, `mac/JawUI/Sources/Samples.swift`, `mac/JawApp/Sources/AppModel.swift` (`spec`, hello's root), `mac/JawHarness/Sources/BuiltInScenes.swift`, `Scenes/ConversationScenes.swift`, `Fixtures/Screens.swift`, `mac/Tests/JawUITests/LayoutTests.swift`, `mac/Tests/JawHarnessTests/ScenesTests.swift`

**Interfaces:**
- `public protocol PictureSource: Sendable { var id: String { get }; func picture(_ path: String, pixels: Int) -> CGImage? }`: a picture by its path under jawd's root, decoded for drawing `pixels` across.
- `public final class FolderPictures: PictureSource`: `init(root:)`; decodes with ImageIO on the thread that asks (the layout queue), keeps at most 64 decoded thumbnails, and keeps no miss, so a file jawd writes later is read then.
- `LayoutSpec(style:me:pictures:)`, `pictures: (any PictureSource)?` defaulting to none, its `id` part of the spec's equality; `MessageLayout.lay(…, pictures:)`.
- `Laid.Part.picture(CGImage, CGRect, CGFloat)`: a picture filling its rect from the centre, clipped to the rect's rounded corners.
- The link card: under the text in both looks, the time on a line of its own under it in bubbles; a 70-point square thumbnail on the left when the row has `preview` (the well and a link symbol when it can't be read), the accent's bar when it hasn't; the title on one line, the description on two at most, cut with an ellipsis, and the host; a click on it opens the URL. A card shows when the link has a title or a description.
- `Samples.links` (a conversation of link previews), `Samples.pictures` (their thumbnails, drawn in code), and the sample K6's link with a thumbnail.
- `HarnessScene.linkPreviews` (`link-previews`, Conversation, axes `.look`, covers `ConversationController`); the scenes that make a `LayoutSpec` give it `Samples.pictures`.

**Tests:**
- `a link's preview sits under its text, with its thumbnail, title, two lines of description and host`: the card's rect is below the text's; a picture part is drawn when the row has a thumbnail, none when it hasn't; the description is cut at two lines; the card's region opens the URL; in lines, the card is at the message's indent; yours draws the same in your bubble.
- `a preview with neither title nor description has no card, and one whose thumbnail can't be read keeps its place`: the same height with or without the picture, so nothing moves when it's read.
- `jawd's folder gives a thumbnail by its path, and a file that isn't there yet is read once it is`: `FolderPictures` on a folder under `/tmp/jaw-mac-links/` (made and removed by the test).
- `every link preview draws as its reference, in both looks, dark and light`: `links-{bubbles,lines}-{dark,light}.png`; the five `conversation-*` references drawn again, since the sample's card moves under its text.
- `ScenesTests`' list has `link-previews`.

**Done when** JawUITests and JawHarnessTests pass on the build Mac with the new references looked at, the `link-previews` scene's pictures have been looked at in both looks, and the app builds.

- [ ] Step 1: Write the tests, and run them to see them fail.
- [ ] Step 2: `PictureSource`, `FolderPictures`, the spec's pictures, the `.picture` part.
- [ ] Step 3: The card laid out under the text, in both looks.
- [ ] Step 4: The samples, the scene, and the app's spec from hello's root.
- [ ] Step 5: Run the suites, look at the new references and the scene's shots, and commit them.

### Task 3: A poll's card: its options, their tallies, and how many voted

**Files:**
- Create: `mac/Tests/JawUITests/PollTests.swift`
- Modify: `mac/JawUI/Sources/Layout/Cards.swift`, `mac/JawUI/Sources/Conversation/MessageView.swift` (VoiceOver), `mac/JawUI/Sources/Samples.swift`, `mac/JawHarness/Sources/Scenes/LinksAndPollsScenes.swift`, `BuiltInScenes.swift`, `mac/Tests/JawHarnessTests/ScenesTests.swift`

**Interfaces:**
- The card: the question, on three lines at most; under it what a vote may choose ("Choose one", "Choose up to 2", "Choose any"); each option with its tick (a circle for one choice, a square for several, filled in the accent when it's yours), its name on one line, its count, and its bar under it as a share of how many voted; then "4 voted", "1 voted" or "No votes yet".
- `MessageLayout.Builder.pollRule(_:)` and `pollVoted(_:)`, the card's two lines, for the tests and VoiceOver, which reads a poll as its question, what it takes, each option with its count and whether it's yours, and how many voted.
- `Samples.polls`: one choice with your vote, several with no choice of yours, none voted, yours still pending, a long question and long option names; the sample K4 gains `voters`.
- `HarnessScene.polls` (`polls`, Conversation, axes `.look`, covers `ConversationController`).

**Tests:**
- `a poll's card has each option with its count and tick, a bar for its share of the voters, and how many voted`: the rule and the foot's words for one, several and every option; a bar's width is its share of the voters; yours ticked; the card's height grows with the options and a long question, and doesn't change when only the tallies do.
- `VoiceOver reads a poll's options, their tallies and yours`.
- `every poll draws as its reference, in both looks, dark and light`: `polls-{bubbles,lines}-{dark,light}.png`.

**Done when** JawUITests and JawHarnessTests pass with the references looked at, and the `polls` scene's pictures have been looked at in both looks.

- [ ] Step 1: Write the tests, and run them to see them fail.
- [ ] Step 2: The card.
- [ ] Step 3: VoiceOver, the samples and the scene.
- [ ] Step 4: Run the suites, look at the references and the shots, and commit them.

### Task 4: Voting from the card, its menu and the keys

**Files:**
- Create: `mac/JawUI/Sources/Conversation/Voting.swift`
- Modify: `mac/JawUI/Sources/Layout/MessageLayout.swift` (`Region.vote`), `Cards.swift`, `mac/JawUI/Sources/Conversation/ConversationController.swift`, `mac/JawApp/Sources/AppModel.swift`, `mac/JawApp/Sources/Bench.swift` (the snapshot's `vote:` step), `mac/Tests/JawUITests/PollTests.swift`, `PaneMenuTests.swift`, `TypingTests.swift`, `mac/README.md` (the step)

**Interfaces:**
- `Region.vote(String)`: an option of a poll that can be voted in, its whole row, the cursor a pointing hand over it.
- `public enum Voting { enum Choice: Equatable { case vote([String]), tooMany(Int) }; static func choosing(_ name: String, in poll: Poll) -> Choice }`: in a one-choice poll, an option votes for it alone, and your choice again takes your vote back; in a poll of several, it's added or taken out, and one past the poll's most is `tooMany`.
- `ConversationHost.conversation(_:vote:options:)`; a click on an option, the message menu's Vote ▸ (each option ticked when yours, on 1 to 9, and Take Back My Vote when you have one) and Enter on a selected poll, which opens that menu under it, all go through `ConversationController.vote(_:on:)`, which says `tooMany` where you acted and sends nothing.
- `AppModel.conversation(_:vote:options:)` asks `jawd.vote`.
- The snapshot's `vote:<option>` step votes in the focused pane's newest poll, as a click on that option does, and says the poll as its row is 0.3 s later.

**Tests:**
- `a click on an option votes: one replaces, your own again takes it back, several toggle within the most`: `Voting.choosing` for each case.
- `a click on a poll's option asks the host to vote, and one past the most says why and asks nothing`: a pane in an offscreen window, a click at an option's region.
- `a poll's menu has Vote with its options, yours ticked, on 1 to 9, and a deleted poll has neither regions nor Vote`.

**Done when** JawUITests passes, the app builds, and on `--demo` the snapshot's `opt+down,vote:Pizza,vote:Pizza` moves to Lisbon Climbing, votes Pizza in Rui's poll (its tallies 1, 1 and 1, yours Pizza, 3 voted, as jawd's row says) and takes the vote back (Pizza 0, 2 voted).

- [ ] Step 1: Write the tests, and run them to see them fail.
- [ ] Step 2: `Voting`, the regions, the click and the host's method.
- [ ] Step 3: The Vote menu, and Enter on a poll.
- [ ] Step 4: The app's model, and the snapshot's step.
- [ ] Step 5: Run the suites and the snapshot on `--demo`, and commit.

### Task 5: A poll from the box

**Files:**
- Create: `mac/JawUI/Sources/Conversation/PollForm.swift`, `mac/Tests/JawUITests/PollFormTests.swift`
- Modify: `mac/JawApp/Sources/Commands.swift` (`compose.poll`), `AppModel.swift`, `MainWindowController.swift` (the sheet), `Bench.swift` (the `poll:` step, and the sheet in the snapshot's picture), `mac/JawHarness/Sources/Scenes/LinksAndPollsScenes.swift`, `BuiltInScenes.swift`, `mac/Tests/JawHarnessTests/ScenesTests.swift`, `mac/README.md`

**Interfaces:**
- `@Observable public final class PollForm`: `chat` (the name it's to), `question`, `options` (two empty to start), `several`, `said` (jawd's words after a refusal), `focus: Field?` (`question`, `option(Int)`); `problem: String?`, the first thing in the way, in jaw's words ("a question", "two options at least", "Pizza is there twice", "12 options at most", "a question of 255 characters at most", "options of 100 characters at most"), counted and trimmed as jawd counts and trims them; `poll: (question: String, options: [String], max: Int)?` once it's ready, empty options left out, `max` every option with `several`; `returned(in:) -> Returned` (`.moved`, `.added`, `.send`, `.notReady`), `add()`, `remove(at:)`; `init(chat:question:options:several:said:)`, so a refused poll opens again as it was.
- `public struct PollFormView: View`: a sheet with the chat's name, the question, the options with a × each past the second and Add Option, "Allow several answers", the problem or jawd's words, Cancel and Send; Tab moves, Return as `returned(in:)` says, ⌘↩ sends, Esc cancels.
- `compose.poll` (the + menu's Poll…, and New poll in the palette) opens it for the focused pane's chat; Send closes it and asks `jawd.sendPoll` with a key made for it; `.pollUnsent` opens it again with the poll and jawd's words, or says them when its chat has no pane.
- The snapshot's `poll:<question>|<option>|<option>[|several]` step sends as Send does, and `poll-form`, filled in from the same after a colon, opens the sheet, which the picture then shows. The keys go through the build Mac's shell, so a run with `|` in them is quoted: `--keys "'opt+down,poll:Jantar?|Sábado|Domingo'"`.

**Tests:**
- `a poll is ready with a question and two options, trimmed as jawd trims them`, `an option named twice, too many or too long says so`, `several answers take every option`, `Return goes on, adds an option, and sends from an empty one`, `a poll jawd turned down opens again as it was, with jawd's words`.
- `ScenesTests`' list has `poll-form`.

**Done when** JawUITests and JawHarnessTests pass, the `poll-form` scene's pictures have been looked at, and on `--demo` the snapshot's `opt+down,poll:Jantar?|Sábado|Domingo` sends a poll that Lisbon Climbing then shows as yours, as jawd's row has it.

- [ ] Step 1: Write the tests, and run them to see them fail.
- [ ] Step 2: `PollForm` and `PollFormView`.
- [ ] Step 3: The command, the sheet, the send and the refusal in the app.
- [ ] Step 4: The scene and the snapshot's step.
- [ ] Step 5: Run the suites and the snapshot on `--demo`, and commit.

### Task 6: Links in the palette's answers

**Files:**
- Create: `mac/Tests/JawUITests/PaletteLinkTests.swift`
- Modify: `mac/JawKit/Sources/Protocol/Find.swift`, `mac/JawUI/Sources/Palette/Palette.swift`, `mac/JawApp/Sources/MainWindowController.swift` (`picked`), `mac/JawHarness/Sources/Scenes/PaletteScenes.swift`, `mac/Tests/JawKitTests/FixtureTests.swift`

**Interfaces:**
- `public struct FoundLink: Decodable, Sendable, Hashable` (`id`, `message`, `chat`, `chatName`, `from: Sender?`, `mine`, `at`, `link`: `url`, `host`, `title`, `description`), as the protocol's Finding has a link's row; `Found.Row.link(FoundLink)`.
- The palette lists links under their heading with jawd's marked text, and who, where and when beside; `Pick.link(FoundLink)`, Enter, opens the link, and `Pick.linkMessage(FoundLink)`, ⌘G through `PaletteModel.goToLink()`, goes to its message (amended: question 13).

**Tests:**
- `a link find found decodes with its message, its chat and its host` (from a row written as the protocol's Finding has it, since `find.result.json` has no link).
- `the palette lists the links found, Enter opens one, and ⌘G goes to its message`.

**Done when** JawKit and JawUITests pass, and the palette scene's band of links has been looked at.

- [ ] Step 1: Write the tests, and run them to see them fail.
- [ ] Step 2: The row, the list and the picks.
- [ ] Step 3: The app opens the link or goes to the message; the scene's band.
- [ ] Step 4: Run the suites, and commit.

### Task 7: The box's preview of a link, proposed (P11)

**Files:**
- Modify: `docs/plans/2026-10-26-jaw-mac-design.md` (Protocol additions: P11), `protocol/README.md` (Composing (proposed): `link.preview`)

**Interfaces:** none in the code. P11, for jawd's builders: `link.preview` (`url`) answers with the `link` a message row would carry (`url`, `title`, `description`, `preview`), fetched under `send.text`'s rules (3 seconds, public addresses, 5 redirects, 512 KB of page and 2 MB of image, no cookies, saying it's jaw) and kept for a few minutes by URL, so the `send.text` that follows sends what the box showed without fetching again; a newer `link.preview` from the same connection cancels the one before (error 7), as `find`'s do. The box asks once a URL has stayed typed for half a second, shows the preview above the field with an × that sends the text with `preview` false, and asks nothing while no URL is typed. It keeps thumbnails under `media/links/`, so it's jawd's data layer, audited before it lands.

**Done when** the two documents say it and the check passes.

- [ ] Step 1: Write P11 in both.
- [ ] Step 2: Commit.

### Task 8: The step in the docs, the check, and end to end

**Files:**
- Modify: `docs/plans/2026-10-27-jaw-mac-parity-plan.md` (3.3), `mac/README.md`

**Interfaces:** the parity plan's 3.3 says what's done, what waits (the tallies' growth with Mac-M5, the box's preview with P11, who voted if the owner wants it, the library's links with 2.4) and that the terminal sends neither request yet; the README says what the cards, voting and the form do, and the snapshot's `vote:` and `poll:` steps.

**Done when** `make check-parity` passes, the four suites pass on the build Mac (`make mac-test`) and `build jaw` succeeds, the snapshot on `--demo` shows Ana's link preview with its thumbnail and Lisbon Climbing's polls, and the branch is rebased onto `origin/main`.

- [ ] Step 1: The docs.
- [ ] Step 2: `make check-parity`, `make mac-test`, `mac/scripts/build-mac build jaw`, and the snapshot on `--demo`.
- [ ] Step 3: Commit; rebase onto `origin/main`, keeping both sides, and run the suites again.
