# Jaw for the Mac: the box's attachments, emoji, GIFs and Send

27 October 2026. A design. It extends the box in [the grid layouts design](2026-10-26-jaw-mac-grid-layouts-design.md) and [the Mac design](2026-10-26-jaw-mac-design.md)'s "The box". The owner's answers are marked "Owner, 27 Oct".

## What it is

The box gets the official app's controls, laid out more plainly: a **+** button for what's attached or composed (photos and videos, documents, the camera, a contact, a poll, an event, a place), **Emoji** and **GIF** as two separate buttons (Owner, 27 Oct: "the way this works on the official app has always been confusing"), and a **Send** button. Files and pictures can also be pasted into the box or dropped on it.

What's chosen waits in a strip above the field, each with its thumbnail and an ×, until it's sent with the text as its caption, as the official app does.

## Decisions

- **The + menu has seven items:** Photos & Videos, Document, Camera (Continuity Camera, from the iPhone), Contact (from the Mac's Contacts), Poll, Event, and Location, which the official Mac app doesn't offer (Owner, 27 Oct).
- **Emoji opens Jaw's own picker** (Owner, 27 Oct): search by name, recents first, skin tones, every key working, in the theme. `:` in the box still completes by name.
- **GIF search asks Giphy, through jawd** (Owner, 27 Oct), and nothing leaves the Mac until the GIF picker is opened and something typed. jawd makes the request, so the app never talks to the network itself, as for everything else.
- **Send shows when there's something to send** (Owner, 27 Oct); with an empty box, a microphone takes its place once voice recording comes (jawd's M5). Return still sends, and ⇧↩ makes a new line.

## The box

From left to right, in one row with the field:

- **+**, opening its menu above the box. Each item has its key: ⌘O Photos & Videos, ⇧⌘O Document, and the rest from the menu and the palette.
- **The field**, after the chip that names the chat.
- **Emoji** (⌃⌘E) and **GIF** (⌃⌘G), each opening its picker above the box.
- **Send** (↩), only when there's text or an attachment.

## Attachments

- **Pasting** (⌘V) an image puts it in the strip; pasting files from Finder puts them all. Text pastes as text.
- **Dropping** files or images on the box, or on a pane's middle while holding ⌥, stages them for that pane's chat. A plain drop on a pane stays as it is, a chat from the list.
- **The strip:** above the field, a thumbnail of each (Quick Look's, made off the main thread), its name for a document, an × to take it out, at most 30 as WhatsApp allows. The text becomes the first one's caption.
- **Sending** asks `send.media` with each file's path, kind (photo, video, document, from its type), the caption and the quality (SD, or HD with ⌥ held as Send is pressed, and a setting for the default).

## The forms

Each is a sheet over the window, keys first: Tab moves, Return sends, Esc cancels.

- **Poll:** a question, options added with Return (at most 12, as WhatsApp allows), and "allow several answers". `send.poll`.
- **Event:** a name, a description, a start date and time, an optional end, a place, and "add a WhatsApp call link". `send.event`.
- **Contact:** the system's contact picker; the card goes as a vCard. `send.contact`.
- **Location:** a search for places (MapKit's search, on the Mac), or the Mac's current place with Location Services, on a small map. `send.location`.
- **Camera:** Continuity Camera's menu (Take Photo, Scan Documents) from the iPhone; the picture goes in the strip.

## Emoji and GIFs

- **The emoji picker:** a field, recents, then every emoji by category, a grid that the arrows move through; Return puts the emoji in the box and keeps the picker open, Esc closes it; a skin tone chosen once sticks. From Unicode's own names, as `:` completion does now.
- **The GIF picker:** a field, then Giphy's trending, or its results for what's typed, as a grid of moving previews; Return or a click sends the GIF at once, as the official app does. The previews come from jawd (`gif.search` answers with each GIF's preview, which jawd fetches into its cache), so the app shows files, never URLs.

## What jawd needs

| Method | What it does | Status |
| --- | --- | --- |
| `send.media` | files by path, kind, caption, quality, reply | in the protocol, not served on main (M4) |
| `send.poll` | question, options, max | in the protocol, not served on main (M2) |
| `send.event` | name, description, start, end, place, call link | proposed in the protocol's Composing (M2) |
| `send.contact` | vCards | proposed in the protocol's Composing (M4) |
| `send.location` | latitude, longitude, name, address | proposed in the protocol's Composing (M2) |
| `gif.search` | query (empty for trending), limit; answers with ids, sizes and preview paths | proposed in the protocol's Composing (after M4); jawd asks Giphy |
| `send.gif` | a GIF from `gif.search` by its id: jawd fetches its MP4 and sends it as a GIF | proposed in the protocol's Composing (after M4) |

Until jawd serves one, the app makes the request anyway and says what jawd answers ("attaching comes with jawd's M4"), as the chat actions do, so nothing in the app changes when jawd does.

## Build order

1. **B1:** the box's row (+, Emoji, GIF, Send), the + menu, and the emoji picker.
2. **B2:** attachments: pasting, dropping, Photos & Videos and Document, the strip, `send.media`.
3. **B3:** the forms: poll, event, contact, location, Continuity Camera.
4. **B4:** the GIF picker, on `gif.search` and `send.gif`.
