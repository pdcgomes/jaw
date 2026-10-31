# Contact cards in the Mac app (parity step 3.9)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** [The parity plan](2026-10-27-jaw-mac-parity-plan.md)'s step 3.9 in jaw's Mac app, on what `send-contact` brings jawd: every card drawn as [the Mac design](2026-10-26-jaw-mac-design.md) has it ("the name, the numbers, Message (`chat.start` with the number) and Add to Contacts; several contacts open a sheet of them"), received or yours, one card or several, with its picture when the card has one; Message from a card, as the terminal's Enter starts a chat with whoever a card shares; and a card sent from the box's + menu with `send.contact`, as [the box design](2026-10-27-jaw-mac-box-design.md)'s Contact form has it (its B3, brought forward for contacts alone). A hostile or enormous card can't stall the main thread: a card is read once, as its row arrives, and only for what's drawn, as the contact audit's O5 asked of the terminal.

**Architecture:** JawKit reads a contact row's vCard as it decodes the row, on the client's reading thread, into what's drawn of each card (its name, its numbers with the WhatsApp account each is, and a small picture when the card has one), with jaw-proto's reader's rules and bounds ported to Swift, and lets the vCard go. It sends `send.contact` as it sends `send.text` (a key made for it, asked again with it after a lost answer, the cards handed back whole when jawd turns them down). JawUI lays the card out from those fields, a click on it a region; it has the sheet of several cards, and the contact form's model and sheet, which makes each card's vCard from the Contacts framework. The app shows the system's contact picker, asks for Contacts once, starts a chat from a card with `chat.start`, and opens the form again with jawd's words when jawd turns cards down.

**Tech Stack:** Swift 6 (AppKit, SwiftUI, Core Text, ImageIO, Contacts, ContactsUI, Swift Testing), Tuist through mise, xcodebuild on the build Mac through `mac/scripts/build-mac`; Python 3 for `make check-parity`.

## Where it starts

- **jawd** (`send-contact`, approved in its second audit, landing after a fix to main's tests): a message row of kind `contact` has `contact`: `name`, the name it goes by (a card's, or for several what the message calls them, "2 contacts"), and `vcard`, the card or each card one after another, as it came, which can be as long as a line jawd sends (16 MiB); WhatsApp's phones mark a number on WhatsApp `TEL;waid=351912345678:+351 912 345 678`. `send.contact` (`chat`, `cards`, each a `name` and one vCard 3.0, `key`) answers as `send.text` does and leaves the chat's draft; no cards, a blank name or a `vcard` that isn't one vCard is -32602, past 64 cards, a name of 256 characters or 256 KiB of vCards it's error 5, in jawd's words. `jawd --engine demo` has Ana's dentist's card in her chat (Dr.ª Marta Lopes, +351 21 345 6789 on WhatsApp and 21 000 00 00) and Rui's two in Lisbon Climbing (Inês Gomes, Tiago Martins), and takes every number but those starting 999 for `chat.start`.
- **The terminal** (`send-contact`): draws a card as its name and its numbers, several as each card's name and numbers, reading the vCard with jaw-proto's reader each time it lays one out (the audit's O5); Enter starts a chat with the card's number, or lists the numbers when it has several; the palette's Send a contact sends a card made from one of jawd's people's name and number.
- **The Mac** (main at 7e04780): JawKit decodes `contact` as its `name` alone; the card draws a person symbol, the name and "Contact card · open it on your phone"; the + menu's Contact… says "sending contacts comes with jawd's send.contact". `mac/parity.toml` has no row for `send.contact`, which `send-contact`'s terminal sends.

## Questions

What the designs leave open, each with its options and the lean built; none blocks a task.

1. **Message and Add to Contacts.** The design gives a card both; the parity plan's 3.9 puts both in Phase 4. A: Message now, since the terminal's Enter starts a chat from a card and the parity plan's first rule closes what the terminal does, and Add to Contacts in Phase 4, since it's the Mac's own and writes to the owner's Contacts (lean); B: both in Phase 4. Tasks 3 and 5.
2. **What's kept of a card.** A: what's drawn (names to 256 characters, 16 numbers a card to 64 characters each, 256 cards, a picture), and not the vCard, since a window of 200 rows could hold 16 MiB in each; Add to Contacts asks jawd for the row again, or keeps vCards under a bound, when it comes (lean); B: the vCard too. Task 1.
3. **Where a card is read.** A: in JawKit's decoder, on the client's reading thread, once a row version, so neither the layout nor the main thread ever reads a vCard (lean); B: in the layout, cached. Task 1.
4. **The picture.** A card can carry its photo (`PHOTO`, base64, or a `data:` URI); WhatsApp's phones send theirs without. A: the card's own photo, read into a 96-pixel thumbnail as the row arrives, JPEG or PNG only, none past 512 KiB as written or 4,096 pixels a side, and only for the first 8 cards; never fetched from a URL (lean); B: the person's WhatsApp picture as well, which the layout can't reach yet (2.2's image queue could). Task 1.
5. **Which number Message dials.** A: the WhatsApp account's when the card marks one, else the number written, as the terminal dials; a card with several numbers offers each, and one with none says so in the terminal's words (lean). Task 3.
6. **Several cards.** A: the bubble shows up to three pictures, the names and "N contacts", with View All across its foot, and a click or Enter opens a sheet of every card, each with its numbers and Message, as the design says (lean); B: each card in the bubble. Tasks 2 and 3.
7. **The picker.** A: the system's (`CNContactPicker`, as the box design says), under the form's Add a Contact…; the contact it gives is fetched again with what a vCard needs, so Contacts is asked for once, the first time, and a refusal says where to allow it (lean); B: a list of the Mac's contacts of Jaw's own. Task 4.
8. **Several cards in one message.** A: the form takes up to 64, Add a Contact… again for each, sent as one message, as jawd sends several (lean); B: one card. Task 4.
9. **A card's photo, sending.** A: left out, as WhatsApp's phones send theirs, which also keeps a card well under jawd's 256 KiB (lean); B: sent. Task 4.
10. **jawd's limits.** A: the form counts them as jawd does (64 cards, names of 256 Unicode scalars, trimmed, and 256 KiB of vCards) and says what's in the way before Send; what jawd still refuses opens the form again with jawd's words (lean). Task 4.
11. **jawd's people as cards,** as the terminal sends them. A: not now: the design picks from the Mac's Contacts, and the terminal's way can follow if the owner wants it (lean). Task 5.

## Global Constraints

- The spec is the parity plan's step 3.9 as `send-contact` writes it, the Mac design's "Contact cards", the box design's "The forms" (Contact), and `send-contact`'s `protocol/README.md` (the message row's `contact`, Sending's `send.contact`) with its fixtures. Where this plan and they differ, they win: stop and say so.
- The branch is `mac-contacts`, in its own worktree, `~/src/jaw-mac-contacts`, from main at 7e04780, rebased onto main once `send-contact` is on it. Work only there; read other branches with `git show`. Never push, never pass `--no-verify`.
- Never open `~/Library/Application Support/jaw`; the app runs only with `--demo` or `--home` on a folder of its own with a `jawd demo` started for it; never stop a process by name; nothing is sent to WhatsApp.
- Until `send-contact` lands: JawKit's tests read its two fixtures from `mac/Tests/JawKitTests/Fixtures/`, copied byte for byte, as `protocol/fixtures` has them on the branch; `mac/parity.toml` holds `send.contact` with `from = "send-contact"`, under the parity plan's 3.9 as the branch writes it, until the Mac sends it (Task 1); the live tests run on a jawd built from the branch's code (`git archive` into `/tmp/jaw-mac-contacts/`, `CARGO_INCREMENTAL=0`), copied into the worktree's `target/debug`.
- Swift 6, macOS 26, warnings are errors; `check-components` and `check-harness` pass; everything drawn is in JawUI, made from values; `AppModel` is the only code that talks to jawd. Every new component has a harness scene, registered and in `ScenesTests`' list, its `covers` kept right by hand.
- Draw what jawd says: a card shows as jawd's row, never before. Nothing here is on jaw's data paths: the app reads rows and writes nothing of jawd's.
- Scratch goes in `/tmp/jaw-mac-contacts/` and the worktree's ignored `mac/build/` and `target/`, removed before the report. One commit a task, at least one an hour, in the project's voice.

## File structure

- New: `mac/JawKit/Sources/Protocol/VCard.swift` (Task 1); `mac/JawUI/Sources/Layout/ContactCards.swift` (Task 2); `mac/JawUI/Sources/Conversation/ContactCardsView.swift` (Task 3), `ContactForm.swift` (Task 4); `mac/JawApp/Sources/ContactPicking.swift`, `mac/JawApp/Jaw.entitlements` (Task 4); `mac/JawHarness/Sources/Scenes/ContactScenes.swift` (Tasks 2 to 4); tests `VCardTests.swift`, `ContactClientTests.swift` and `Fixtures/` (JawKit), `ContactCardTests.swift`, `ContactFormTests.swift` and `contacts-{bubbles,lines}-{dark,light}.png` (JawUITests).
- Changed: JawKit's `Rows.swift`, `Actions.swift`, `Jawd.swift`; JawUI's `Cards.swift`, `MessageLayout.swift`, `Samples.swift`, `ConversationController.swift`, `MessageView.swift`; the app's `AppModel.swift`, `Commands.swift`, `MainWindowController.swift`, `Bench.swift`; `Project.swift`; the harness's `BuiltInScenes.swift`; tests `FixtureTests.swift`, `LiveTests.swift`, `LayoutTests.swift` (and the five `conversation-*` references), `PaneMenuTests.swift`, `TypingTests.swift`, `ScenesTests.swift`; `mac/parity.toml`, `mac/README.md`, the parity plan's 3.9.

---

### Task 1: JawKit reads each card once, as its row arrives, and sends cards

**Interfaces:**
- `VCard.read(_:)`: jaw-proto's reader in Swift, in one pass whose cost grows with the card's length however its lines are folded: cards from `BEGIN:VCARD` to `END:VCARD`, 256 at most, an open one at the end counting; each named by its last `FN`, failing that its `N` read as a person reads it; its `TEL`s in order, 16 at most, `tel:` taken off, each with the WhatsApp account its `waid` says (6 to 20 digits, as `+` and the digits); folded lines, quoted-printable and its soft breaks, escapes, groups (`item1.TEL`) and any case; a line whose colon comes 1,024 bytes in or later is no property. Beyond jaw-proto, each card's first `PHOTO` given inline, at most 512 KiB as written. What it doesn't draw it skips without keeping.
- `Contact` gains `cards: [ContactCard]`, read in its decoder; `name` is clipped to 256 characters, and the vCard isn't kept. `ContactCard` has `name`, `phones` (`number`, `whatsapp`, `dialled`) and `picture: CardPicture?`, a 96-pixel thumbnail decoded with ImageIO from a JPEG or PNG of at most 4,096 pixels a side, for the first 8 cards.
- `SendContact` (`chat`, `cards` of `SendContact.Card`, `name` and `vcard`, `key`), written as its fixture is; `Jawd.sendContact(chat:cards:key:)`, answered as `.sent`; a lost answer is `.unsure` and asked again with its key once jawd is back, within `keyKept`; past it, or refused, it's `.contactUnsent(chat:key:cards:why:)` with the cards whole and jawd's words, or "sending contacts comes with the next step" from a jawd without it. The app says it in the status line until Task 4.

**Tests:**
- `send.contact's request is its fixture, and its answer's row reads as two cards`, from the copies until the fixtures are on main.
- `VCardTests`: jaw-proto's cases, giving the same names and numbers for the same cards; pictures inline in 2.1, 3.0 and 4.0, and none too large, not an image, too many pixels or at a URL; and hostile cards (quoted-printable broken softly, a quote left open, folded after each character, a photo folded at 75 columns) read in time that grows with their length, a 16 MiB one within a bound.
- `ContactClientTests` against `FakeJawd`: cards go with their key and come back as jawd's row; a lost answer is asked again with the same key, and one lost past a day comes back whole; a refusal comes back whole in jawd's words; a jawd without `send.contact` says it comes with the next step; a row with a 16 MiB card is read on the client's thread, and the main thread's drain of it reads no vCard.
- A live test, on `send-contact`'s jawd: the demo's cards come read (Ana's dentist's with its two numbers, the first on WhatsApp; the climbers' two), and a card sent comes back as yours. It's enabled where the jawd built serves `send.contact`.

**Done when** JawKit passes on the build Mac with `send-contact`'s jawd, the app builds, and `make check-parity` passes against main's terminal and `send-contact`'s, the ledger's row gone now that the Mac sends `send.contact`.

- [ ] Step 1: Write the tests, and run them to see them fail.
- [ ] Step 2: `VCard`, `ContactCard`, `CardPicture` and `Contact`'s decoder.
- [ ] Step 3: `SendContact`, `sendContact` and `.contactUnsent`; the ledger's row out.
- [ ] Step 4: Run JawKit and build the app on the build Mac; `make check-parity` both ways; commit.

### Task 2: The card in its bubble

**Interfaces:**
- One card: its picture, or the person symbol, in a 40-point circle; the name it goes by (the row's, failing that its card's); its numbers on a line; and Message across its foot when it has a number. Several: up to three pictures overlapping, their names, "N contacts", and View All across the foot. The same in both looks, in the bubble or at the message's indent; its size comes from the row, so a picture read or not moves nothing; a deleted card has none. `Region.card`, the whole card.
- VoiceOver reads a card as its name and numbers, several as how many and their names.
- `Samples.contacts`: a card with a picture, one without, one with three numbers, one without any, several, a long name, and yours; the sample K5 gets a vCard. `HarnessScene.contactCards` (`contact-cards`, Conversation, axes `.look`).

**Tests:** the card's parts and regions for one, several and none; the same height with or without a picture; long names cut; VoiceOver's words; `contacts-{bubbles,lines}-{dark,light}` as their references, and the five `conversation-*` drawn again; `ScenesTests`' list.

**Done when** JawUITests and JawHarnessTests pass on the build Mac with the new references looked at, and the scene's pictures have been looked at in both looks.

- [ ] Step 1: Write the tests, and run them to see them fail.
- [ ] Step 2: The card, its region, VoiceOver.
- [ ] Step 3: The samples and the scene; run the suites, look at the references, and commit.

### Task 3: Message from a card, and the sheet of several

**Interfaces:**
- A click on a card, Enter on it selected, and its menu (Message, or Message ▸ with each number; View Contacts… for several) go through `ConversationController`: one number asks `ConversationHost.conversation(_:startChat:)` with the number to dial; several pop a menu of them; none says "there's no number on this card to start a chat with"; several cards ask `conversation(_:cardsOf:)`. Copy copies the card's lines as they're drawn.
- `ContactCardsView`: the sheet of a message's cards, each with its picture, name, numbers (the WhatsApp one marked) and Message; ↑ and ↓ move, Return messages, Esc closes. `HarnessScene.contactSheet` (`contact-sheet`).
- The app starts the chat with `chat.start` and opens it, or says jawd's words ("no chat started: …"). The snapshot's `card:<row>` step does what a click on that row's card does.

**Tests:** a click on one number, on several and on none; Enter; several asking for the sheet; the menu's items; copy's words; the sheet's model and keys; `ScenesTests`' list.

**Done when** JawUITests and JawHarnessTests pass, the app builds, and on `--demo` with `send-contact`'s jawd, `card:` on Ana's dentist's card opens a chat with +351 21 345 6789 and on the climbers' opens the sheet of two.

- [ ] Step 1: Write the tests, and run them to see them fail.
- [ ] Step 2: The click, Enter, the menu and copy; the host's methods.
- [ ] Step 3: The sheet, its scene, the app's side and the snapshot step; run the suites and the snapshot, and commit.

### Task 4: A card from the box's + menu

**Interfaces:**
- `@Observable public final class ContactForm`: `chat` (the name it's to), `cards`, `said` (jawd's words after a refusal), `problem` (what's in the way, in jaw's words), `ready`, `add`, `remove`; `ContactForm.card(_: CNContact)`: the name by `CNContactFormatter` or the organisation, and the vCard from `CNContactVCardSerialization` without the photo. `ContactFormView`: the sheet, its cards each with an ×, Add a Contact…, the problem or jawd's words, Cancel and Send; Return sends, Esc cancels.
- `attach.contact` (Contact… in the + menu, Send a contact in the palette) opens it for the focused pane's chat and shows the system's picker under Add a Contact…; `ContactPicking` asks for Contacts once and fetches the picked contact with what a vCard needs; Send asks `jawd.sendContact` with a key made for it; `.contactUnsent` opens it again as it was with jawd's words. `Project.swift` gives the app `NSContactsUsageDescription` and the hardened runtime's address-book entitlement.
- The snapshot's `contact:<name>|<number>[|…]` sends cards made from those as Send does, and `contact-form:` opens the sheet with them for the picture. `HarnessScene.contactForm` (`contact-form`, four states).

**Tests:** a made-up contact's card is one vCard 3.0 with its name, numbers and email and no photo; one with only an organisation is named by it; what's ready and what's in the way, counted as jawd counts; a refused form opens again as it was; `ScenesTests`' list.

**Done when** JawUITests and JawHarnessTests pass, the scene's pictures have been looked at, the app builds, and on `--demo` with `send-contact`'s jawd `contact:Marta Lopes|+351 213 456 789` sends a card that Ana's chat then shows as yours, as jawd's row has it.

- [ ] Step 1: Write the tests, and run them to see them fail.
- [ ] Step 2: `ContactForm` and its view.
- [ ] Step 3: The picker, the command, the sheet, the send and the refusal in the app; the project's keys.
- [ ] Step 4: The scene and the snapshot's steps; run the suites and the snapshot, and commit.

### Task 5: The step in the docs, the check, and end to end

**Interfaces:** the parity plan's 3.9 says what's done and what waits (Add to Contacts in Phase 4, the WhatsApp picture with 2.2, jawd's people if the owner wants them); the README says what the cards and the form do, and the snapshot's `card:`, `contact:` and `contact-form:` steps.

**Done when** `make check-parity` passes against both terminals, the four suites pass on the build Mac with `send-contact`'s jawd and with main's, `build jaw` succeeds, the snapshot on `--demo` shows the demo's cards in both looks, and, once `send-contact` is on main, the branch is rebased onto it with its fixtures' copies gone.

- [ ] Step 1: The docs.
- [ ] Step 2: The check, the suites, the build and the snapshots.
- [ ] Step 3: Commit; rebase onto main once `send-contact` is on it, and run them again.
