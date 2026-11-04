# jaw in the terminal: parity with the Mac

27 October 2026. The places where the Mac app leads the terminal, in a plan of their own for the terminal's builders, as the owner asked on 27 October: the mirror of [the Mac's parity plan](2026-10-27-jaw-mac-parity-plan.md). The Mac is the reference for what, and [jaw's design](2026-10-16-jaw-design.md)'s terminal interface for how, as the Mac design is for the Mac; most of what the Mac leads on is in that design already. The calls below are this plan's own, for the owner to look over; the questions at the end are his.

## Where it starts

- **The Mac led in 14** of the 149 features the parity canvas compares (`jaw-feature-parity.canvas.tsx`, its "Mac ahead" filter), and leads in 12 since `settings` landed (ebbd905).
- **Four were being built** on the terminal's branches: asking the phone for older messages (`failure-states`), how messages are laid out (`settings`' IRC layout, landed), a settings panel (`settings`, landed), and who's typing (`presence-typing`).
- **Seven are in the terminal's design and not built:** the cards for documents, link previews, places and contact cards; polls with their bars; view-once said so; and emoji completed after a colon.
- **Three aren't in it:** chats' pictures, an emoji picker, and named layouts on a grid.
- **By the requests each sends,** the Mac asks jawd for two things the terminal doesn't (`make check-parity` lists them): `history.more`, which `failure-states` brings, and `link.cancel`, which the terminal has had no use for: its linking screen stays until jawd is linked.

## Being built now

These need nothing more than their branches, and are listed so nothing drops.

- **Older messages asked of the phone:** `history.more` when scrolling past a chat's oldest message, on `failure-states`.
- **How messages are laid out:** on the right as now, or under "You" as IRC clients put them; landed with `settings` (ebbd905).
- **A settings panel:** Ctrl-, in the chat list's place, and `jaw settings`; landed with `settings` (ebbd905).
- **Who's typing:** the chat list's row, the conversation's title and the status line, on `presence-typing`, as P7's typing half.

## In the terminal's design, to build

- **Cards** (M2's terminal side): what the design has as "documents, locations, contact cards and link previews as cards", in place of today's single line, each under the message's text at its indent:
  - a document's type, name, size and pages, with Enter opening it once media in lands;
  - a place's name, address and coordinates, with Enter opening it on a map in the browser, and a live location saying until when;
  - a contact's name and numbers, with Enter starting a chat with the number (`chat.start`);
  - a link preview's title, two lines of its description and its host, its thumbnail drawn as a picture where the terminal draws them, which jawd sends since `m2-content` landed (6c1c81f).
- **Polls,** as the design has them: each option's bar and count, the voters, your choice ticked, and voting with `poll.vote`, which jawd serves since `m2-content` landed. Built on `terminal-actions` (4 November), as the Mac parity plan's step 3.3 asked: Enter on a selected poll opens a vote picker of its own, where 1 to 9 or Enter on an option vote by the Mac's rules, and on the poll itself 1 to 6 still react, as on the Mac.
- **View-once:** "Photo, view once · open it on your phone", as the Mac says it, and never forwarded.
- **Emoji after a colon,** as the design's keys have it: `:` and a few letters offer emoji by name, from the `emojis` crate jaw already has, as `@` offers names, recents first.

## Not in the terminal's design

- **An emoji picker:** not needed. Completion after a colon and the palette's "any emoji by name", which reactions already use, give the terminal what the Mac's picker gives (this plan's call).
- **Chats' pictures** (Question 1): the chat's 256-pixel avatar, which jawd already serves, two rows tall in the chat list and in the conversation's title, only where the terminal draws pictures (kitty, iTerm2, sixel), and nothing in half-blocks, which are too coarse at that size. It's measured against jaw's memory budget (40 MB with a screen of images) and the keypress budget while scrolling the list.
- **Named layouts** (Question 2): the Mac's book of layouts on a grid, switched with ⌃⌘1 to ⌃⌘9. The terminal's panes are a tree of up to six; once P10 lands, the terminal could open the Mac's layouts that are trees.

## Tracking

The parity canvas's "Mac ahead" filter is the list, brought up to date whenever main moves; `make check-parity` prints the requests the Mac sends that the terminal doesn't. If the terminal's builders want the guard the Mac has, the check can grow a list of the terminal's own.

## Questions for the owner

1. **Chats' pictures in the terminal:** in the chat list and the conversation's title, where the terminal draws pictures? This plan's lean: yes, measured against the budgets, with nothing in half-blocks.
2. **Named layouts in the terminal:** a book of them, as the Mac has? Lean: not now; the tree of up to six panes stays, and opens the Mac's tree-shaped layouts once P10 lands.

## Who

The terminal's builders, scheduled on the orchestrator's board (`ws-jaw.canvas.tsx`): the cards, polls and view-once with the rest of M2's terminal side, emoji after a colon on a branch of its own, and the two questions' steps once the owner has answered.
