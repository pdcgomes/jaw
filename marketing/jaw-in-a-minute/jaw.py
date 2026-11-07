"""jaw in a minute: what jaw is, how it's built, how fast it is, and the palette and split panes, for
social posts (1280x720 MP4) and jaw's README (a GIF at half the size).

Drawn with the pixelartvisuals kit (pixelkit), which the pixel-graphics skill links to:

    PYTHONPATH=~/.cursor/skills/pixel-graphics/scripts python3 marketing/jaw-in-a-minute/jaw.py
    PYTHONPATH=~/.cursor/skills/pixel-graphics/scripts python3 marketing/jaw-in-a-minute/jaw.py frames 3 9.5 14

Sources, in this repository on 26 Oct 2026 (main at 9fe850b):
- What jaw is, its three programs, the engine link (length-prefixed protobuf frames on the engine's
  stdin and stdout), the socket (JSON lines on a Unix socket), "durable before acknowledged", the
  Engine trait and its Go, demo and Rust engines, the contract suite every engine runs, jawd
  restarting the engine, "interfaces only draw" and what jawd does for them, live windows, drafts in
  jawd, jaw-proto's generated JSON Schema, the work lanes, the writer's 3 ms background batches,
  keyset paging: docs/plans/2026-10-16-jaw-design.md (What it is, Principles 1 to 7, How it fits
  together, The engine, jawd: Work lanes, Storage, The socket and windows, Testing).
- The palette (Ctrl-K, grouped by kind, Tab narrows, actions first when typed, actions with their keys,
  every keystroke asks jawd and keeps only the latest answer, accent-blind search):
  crates/jaw/src/view/palette.rs and the design's "Finding anything" and "The terminal interface".
- Split panes (one chat a pane, split across or down, a key to each pane, drafts kept per chat, the
  layout kept in jawd for a desktop app): the design's "Later". Not built: tagged EXPLORING.
- Every number: README.md "What it costs", measured by scripts/perf.py on 25 Oct 2026 on a Mac Studio
  (M1 Ultra), release builds, jawd demo's made-up account of a million messages in 3,000 chats, the
  2,000-a-second flood from the demo engine: a keypress drawn 0.40 ms p99 (budget 16 ms); search
  15 ms p99 (50 ms); a 100-message page 2.0 ms p99 on the socket (5 ms); a message on its way to
  interfaces once committed 2.9 ms p99 (5 ms); jaw's memory 35 MB with the library's photos (under
  40 MB); jawd's memory 59 MB steady (under 100 MB); idle, jaw draws no frames. Only budgets it meets
  are shown; the README lists where it falls short.
- Names and chats: jaw demo's made-up account (crates/jaw/src/demo.rs); the lines are the design's
  example screen, in English.
- The Mimic: creatures/mimic.png (badcoffee.dev's), its chomp from crates/jaw/src/art.rs; jaw's colours
  from crates/jaw/src/theme.rs (Tokyo Night).
"""

import sys
from pathlib import Path

import numpy as np
from PIL import Image

from pixelkit import Canvas, Rect, animate, ease_back, ease_in_out, ease_out, phase, reveal, stagger

from parts import (BOTTOM, CREAM, EMBER, ENGINE, INK, JAWD, MARK, MIMIC_H, MIMIC_W, ORANGE, RUST, SENDERS,
                   SMALL, TN, TOP, UI, WHATSAPP, along, chat_list, conversation, dot, enter, hints,
                   jaw_header, mimic, msg, packet, pointer, sidebar, tbox, window)

HERE = Path(__file__).parent
WIPE = 0.9  # seconds the Mimic takes to chomp one scene into the next

ANA, RUI, INES, SOFIA, MAE = SENDERS[2], SENDERS[1], SENDERS[3], SENDERS[5], SENDERS[6]


def caption(c, s, title, sub, tag=None):
    """The scene's title, typed in, and its one-line takeaway under it; a tag says what's built."""
    c.text(8, 19, reveal(title, phase(s, 0.1, 0.6)), "white", font="large", scale=2, shadow="line")
    if s > 0.5:
        c.text(8, 37, reveal(sub, phase(s, 0.5, 1.2)), "dim")
    if tag:
        word, col = tag
        lift = round((1 - ease_back(phase(s, 0.6, 0.9))) * 3)
        if s > 0.6:
            c.tag(312, 20 + lift, word, col, border=f"{col}.dark", align="right")


# ------------------------------------------------------------------ 1. what jaw is

FOLDERS = [("ALL", 14, 0, True), ("UNREAD", 5, 1, None), ("FAMILY", 3, 1, None), ("WORK", 2, 0, False),
           ("COMMUNITIES", 0, 0, False)]
ROSTER = [("ANA RIBEIRO", True), ("RUI COSTA", True), ("MÃE", False), ("SOFIA MARTINS", False)]
TYPED = "SEE YOU SATURDAY"
SENT, TYPING, VOICE = 2.0, 3.3, 4.0


def screen(c, s, *, conv=True):
    """jaw's screen as the design draws it, at s seconds into the first scene. Returns the window's
    content Rect."""
    r = window(c, 8, 43, 304, 125, "JAW")
    y = jaw_header(c, r, "· ONLINE", "14:02")
    side = tbox(c, 11, y + 2, 72, 95, "FOLDERS")
    sidebar(c, side, FOLDERS, ROSTER)

    climbing = 14 + (s >= 1.2) + (s >= 3.6)
    last = "INÊS: I'M IN!" if s >= 3.6 else ("RUI: SATURDAY AT 9?" if s < 1.2 else "SOFIA: WHO HAS A ROPE?")
    chats = [("ANA RIBEIRO", "VOICE MESSAGE 0:14" if s >= VOICE else
              (f"✓✓ {TYPED}" if s >= SENT else "YES, THE ONE AT THE LAKE"), 0, True),
             ("LISBON CLIMBING", last, climbing, False),
             ("MÃE", "✓✓ OK, CALL YOU LATER", 0, False),
             ("FAMÍLIA GOMES", "AVÓ LURDES: A PHOTO", 3, False),
             ("PEDRO & ANA", "TICKETS.PDF", 0, False)]
    lst = tbox(c, 85, y + 2, 86, 95, "CHATS", right="ALL")
    chat_list(c, lst, chats, selected=0)

    typing = TYPING <= s < VOICE
    title = "ANA RIBEIRO · TYPING…" if typing else "ANA RIBEIRO"
    inner = tbox(c, 173, y + 2, 136, 95, title, focus=True)
    if conv:
        msgs = [msg("ANA", "DID YOU GET THE PHOTOS FROM SUNDAY?", col=ANA),
                msg("ANA", kind="photo", col=ANA, react=2),
                msg(None, "YES, THE ONE AT THE LAKE", tick="read")]
        if s >= SENT:
            tick = "pending" if s < 2.3 else "sent" if s < 2.7 else "delivered" if s < 3.2 else "read"
            msgs.append(msg(None, TYPED, tick=tick))
        if s >= VOICE:
            msgs.append(msg("ANA", kind="voice", col=ANA))
        draft = reveal(TYPED, phase(s, 0.7, 1.7)) if s < SENT else ""
        conversation(c, inner, msgs, draft=draft, cursor=int(s * 3) % 2 == 0 or 0.7 < s < SENT,
                     played=phase(s, 4.4, 5.5))
    hints(c, 13, 159, [("⏎", "SEND"), ("↑", "MESSAGES"), ("CTRL-K", "FIND"), ("ALT-↑↓", "CHATS"),
                      ("CTRL-B", "SIDEBAR"), ("?", "KEYS")])
    return r


def scene_screen(c, p, s, d):
    caption(c, s + 0.6, "WHATSAPP, IN A TERMINAL",
            "CHATS, PHOTOS, VOICE AND POLLS, FROM THE KEYBOARD, ON MACOS AND LINUX")
    screen(c, s)


# ------------------------------------------------------------------ 2. one daemon does the work

STEPS = [(0.4, "WHATSAPP SENDS A MESSAGE"), (1.2, "THE ENGINE HANDS IT TO JAWD"),
         (2.0, "JAWD COMMITS IT, SYNCED TO DISK"), (2.9, "ONLY THEN: \"RECEIVED\""),
         (4.3, "JAWD PREPARES THE ROW"), (5.1, "JAW DRAWS WHAT IT'S SENT")]


def drum(c, x, y, w, h, col, lit):
    """A database: a drum with its lid's rim."""
    c.rect(x, y + 2, w, h - 4, c.dark(col) if not lit else col)
    c.rect(x + 1, y + 1, w - 2, 1, c.light(col))
    c.rect(x + 1, y + h - 2, w - 2, 2, c.dark(col))
    c.hline(x + 1, y + 5, w - 2, c.light(col) if lit else col)
    c.box(x, y + 1, w, h - 2, "line")


def scene_daemon(c, p, s, d):
    caption(c, s, "ONE DAEMON DOES THE WORK",
            "NOTHING IS ACKNOWLEDGED, TO WHATSAPP OR THE SCREEN, BEFORE IT'S ON DISK")
    heard, drawn = s >= 4.0, s >= 5.1
    wa = c.node(8, 46, 100, 19, "WHATSAPP", color=WHATSAPP, sub="JAW IS A LINKED DEVICE",
                badge="✓ RECEIVED" if heard else None, badge_color="green")
    eng = c.node(8, 75, 100, 19, "JAW-ENGINE", color=ENGINE, sub="GO · WHATSMEOW")
    jd = c.node(8, 104, 100, 19, "JAWD", color=JAWD, sub="RUST · DOES THE WORK")
    ui = c.node(8, 133, 100, 19, "JAW", color=UI, sub="RUST · ONLY DRAWS",
                badge="✓ DRAWN" if drawn else None, badge_color="sky.light")
    down, up = 18, 30
    for a, b in ((wa, eng), (eng, jd), (jd, ui)):
        c.arrow(down, a.y2, down, b.y - 1, "line")
    for a, b in ((jd, eng), (eng, wa)):
        c.arrow(up, a.y - 1, up, b.y2, "line", dotted=True)
    c.text(36, 68, "ITS OWN PROTOCOL", "dim")
    c.text(36, 97, "PROTOBUF · STDIO", "dim")
    c.text(36, 126, "JSON LINES · SOCKET", "dim")

    lit = 2.4 <= s < 2.9
    drum(c, 113, 101, 28, 25, JAWD, lit)
    c.text(127, 110, "JAW.DB", "white" if lit else "text", align="center")
    c.hline(108, 113, 5, "line")
    if s >= 2.5:
        c.text(127, 128, reveal("SYNCED", phase(s, 2.5, 2.8)), "green", align="center")

    # The message down, the ack up, then the row to the screen; each passes behind the boxes.
    def on_line(x, y, col):
        if not any(n.x <= x < n.x2 and n.y <= y < n.y2 for n in (wa, eng, jd, ui)):
            packet(c, x, y, col)

    if 0.4 <= s < 2.0:
        on_line(*along([(down, wa.y2 - 3), (down, jd.y + 9)], phase(s, 0.4, 2.0)), WHATSAPP)
    elif 2.0 <= s < 2.5:
        x, y = along([(jd.x2 + 1, 113), (116, 113)], phase(s, 2.0, 2.4))
        packet(c, x, y, JAWD)
    if 2.9 <= s < 4.1:
        on_line(*along([(up, jd.y + 2), (up, wa.y2 - 3)], phase(s, 2.9, 4.0)), "green.light")
    if 4.3 <= s < 5.1:
        on_line(*along([(down, jd.y2 - 3), (down, ui.y + 9)], phase(s, 4.3, 5.1)), JAWD)

    r = c.panel(146, 46, 166, 106, "A MESSAGE ARRIVES", color=JAWD)
    for i, (at, line) in enumerate(STEPS):
        y = r.y + 2 + i * 10
        lit = s >= at
        c.text(r.x, y, str(i + 1), JAWD if lit else "line")
        c.text(r.x + 8, y, line, "white" if lit else "dim")
    if s >= 3.2:
        c.paragraph(r.x, r.y + 66, reveal("A CRASH ANYWHERE BEFORE STEP 4, AND WHATSAPP SENDS IT AGAIN.",
                                          phase(s, 3.2, 3.9)), r.w, "text")


# ------------------------------------------------------------------ 3. any engine behind it

ENGINES = [("GO ENGINE", "WHATSMEOW", "TODAY", ENGINE, True),
           ("DEMO ENGINE", "A MADE-UP ACCOUNT", "TODAY", "violet", True),
           ("RUST ENGINE", "WHATSAPP-RUST", "NEXT", "cyan", False)]
CART_W, CART_H = 92, 24
JD_X = 216
REST_X, DOCK_X, DOCK_Y = 8, JD_X - 16 - CART_W, 79
CYCLE, START = 2.1, 0.2


def cartridge(c, x, y, name, sub, status, col, real):
    """An engine as a game cartridge: its label, and its pins on the side that plugs into jawd."""
    if real:
        c.rect(x, y, CART_W, CART_H, c.dark(col))
        c.hline(x, y, CART_W, col)
        c.rect(x + 3, y + 3, CART_W - 12, CART_H - 6, "panel")
        for k in range(5):
            c.rect(x + CART_W - 6, y + 4 + k * 4, 4, 2, "muted")
        txt, subc = "white", "dim"
    else:
        for xx, yy, w, vertical in ((x, y, CART_W, False), (x, y + CART_H - 1, CART_W, False),
                                    (x, y, CART_H, True), (x + CART_W - 1, y, CART_H, True)):
            c.dashes(xx, yy, w, col, vertical=vertical)
        txt, subc = "text", "dim"
    c.text(x + 6, y + 6, name, txt, check=False)
    c.text(x + 6, y + 13, sub, subc, check=False)
    c.text(x + CART_W - 9, y + 6, status, "green" if status == "TODAY" else col, align="right", check=False)


def scene_engines(c, p, s, d):
    caption(c, s, "ANY ENGINE BEHIND IT",
            "THE ENGINE ONLY SPEAKS WHATSAPP; JAWD UNDERSTANDS IT, SO ENGINES SWAP")
    jd = c.node(JD_X, 46, 312 - JD_X, 88, "JAWD", color=JAWD, sub="ENGINE TRAIT")
    for i, line in enumerate(("EVENTS IN AND", "COMMANDS OUT,", "SHAPED LIKE", "WHATSAPP", "",
                              "JAWD PARSES", "ITS PROTOBUFS")):
        if line:
            c.text(jd.x + 8, jd.y + 21 + i * 8, line, "text")
    c.rect(DOCK_X + CART_W, DOCK_Y + 11, jd.x - DOCK_X - CART_W, 3, "raised")
    c.rect(jd.x, DOCK_Y + 2, 3, CART_H - 4, "shadow")

    k = min(len(ENGINES) - 1, max(0, int((s - START) / CYCLE)))
    u = (s - START) - k * CYCLE
    for i, (name, sub, status, col, real) in enumerate(ENGINES):
        ry = 50 + i * 30
        if i != k or u < 0:
            cartridge(c, REST_X, ry, name, sub, status, col, real)
            continue
        out = ease_in_out(phase(u, 0.0, 0.35)) if u < 1.6 else ease_in_out(1 - phase(u, 1.8, 2.05))
        drop = ease_in_out(phase(u, 0.35, 0.55)) if u < 1.6 else ease_in_out(1 - phase(u, 1.6, 1.8))
        x = round(REST_X + (DOCK_X - REST_X) * out)
        y = round(ry + (DOCK_Y - ry) * drop)
        c.dashes(REST_X, ry, CART_W, "line")
        c.dashes(REST_X, ry + CART_H - 1, CART_W, "line")
        cartridge(c, x, y, name, sub, status, col, real)
        if 0.6 <= u < 1.6:
            for j in range(3):
                q = phase(u, 0.6 + j * 0.25, 0.95 + j * 0.25)
                if 0 < q < 1:
                    px = round(DOCK_X + CART_W + 2 + (jd.x - DOCK_X - CART_W - 3) * q)
                    if real:
                        packet(c, px, DOCK_Y + 12, col)
                    else:
                        c.box(px - 2, DOCK_Y + 10, 5, 5, col)

    r = c.panel(JD_X - 16, 138, 312 - JD_X + 16, 30, None)
    c.text(r.x, r.y - 1, "ONE CONTRACT SUITE", "muted")
    for i, (name, _, status, col, real) in enumerate(ENGINES):
        ran = s >= START + i * CYCLE + 1.2
        label = name.split()[0]
        x = r.x + i * 36
        if real:
            c.text(x, r.y + 8, label, "white" if ran else "dim")
            if ran:
                c.text(x + SMALL.measure(label) + 3, r.y + 8, "✓", "green")
        else:
            c.text(x, r.y + 8, label, col if ran else "dim")
    c.paragraph(8, 142, "JAW-ENGINE IS A PROCESS OF ITS OWN: JAWD STARTS IT, AND RESTARTS IT IF IT FALLS OVER.",
                184, "dim")


# ------------------------------------------------------------------ 4. any interface in front

DOES = ["NAMES", "QUOTES", "REACTION TOTALS", "POLL TALLIES", "PREVIEWS", "SEARCH", "DRAFTS", "FOLDERS",
        "VOICE", "NOTIFICATIONS"]
DRAFT = "SEE YOU AT 9"


def mini_terminal(c, r, s, got, draft):
    c.rect(r.x, r.y, r.w, r.h, TN["bg"])
    c.rect(r.x, r.y, r.w, 6, TN["surface"])
    for i, col in enumerate(("red", "yellow", "green")):
        c.rect(r.x + 2 + i * 4, r.y + 2, 2, 2, col)
    c.box(r.x, r.y, r.w, r.h, TN["gutter"])
    x = r.x + 4
    c.text(x, r.y + 9, "RUI", RUI)
    c.text(x + 16, r.y + 9, "SATURDAY AT 9?", TN["text"])
    if got:
        c.text(x, r.y + 17, "INÊS", INES)
        c.text(x + 20, r.y + 17, "I'M IN!", TN["text"])
    c.box(x - 1, r.y2 - 13, r.w - 6, 10, TN["blue"])
    tw = c.text(x + 1, r.y2 - 10, draft, TN["fg"]) if draft else 0
    if int(s * 3) % 2 == 0:
        c.rect(x + 1 + tw + (1 if tw else 0), r.y2 - 11, 2, 7, TN["fg"])


def mini_desktop(c, r, got, draft):
    c.rect(r.x, r.y, r.w, r.h, "#f3f4f9")
    c.rect(r.x, r.y, r.w, 6, "#d8dbe4")
    for i, col in enumerate(("red", "yellow", "green")):
        c.rect(r.x + 3 + i * 4, r.y + 2, 2, 2, col)
    c.rect(r.x + 4, r.y + 8, 64, 9, "#d8dbe4")
    c.text(r.x + 7, r.y + 10, "SATURDAY AT 9?", "#131723")
    if got:
        c.rect(r.x + 4, r.y + 19, 40, 9, "#d8dbe4")
        c.text(r.x + 7, r.y + 21, "I'M IN!", "#131723")
    c.box(r.x + 3, r.y2 - 13, r.w - 6, 10, "#45a5ff")
    if draft:
        c.text(r.x + 6, r.y2 - 10, draft, "#131723")
    c.box(r.x, r.y, r.w, r.h, "line")


def mini_json(c, r, got):
    for xx, yy, w, vertical in ((r.x, r.y, r.w, False), (r.x, r.y2 - 1, r.w, False),
                                (r.x, r.y, r.h, True), (r.x2 - 1, r.y, r.h, True)):
        c.dashes(xx, yy, w, "dim", vertical=vertical)
    c.text(r.x + 4, r.y + 5, "{\"METHOD\":", "muted")
    c.text(r.x + 8, r.y + 12, "\"MESSAGES.WATCH\"}", "text")
    if got:
        c.text(r.x + 4, r.y + 21, "{\"UPSERT\": …}", "green.light")


def scene_interfaces(c, p, s, d):
    caption(c, s, "ANY INTERFACE IN FRONT",
            "JAWD PREPARES EVERYTHING ONCE; AN INTERFACE ONLY DRAWS WHAT IT'S SENT")
    jd = c.node(110, 46, 100, 19, "JAWD", color=JAWD, sub="FOR EVERY INTERFACE")
    for i, word in enumerate(DOES):
        lit = s >= 0.4 + i * 0.12
        col = "text" if lit else "line"
        if i < 5:
            c.text(102, 47 + i * 7 - (i // 5) * 35, word, col, align="right")
        else:
            c.text(218, 47 + (i - 5) * 7, word, col)
    bus = c.bus(20, 85, 280, "line", [(jd, jd.cx)])
    term = Rect(8, 108, 96, 44)
    desk = Rect(112, 108, 96, 44)
    js = Rect(216, 108, 96, 44)
    c.text((term.cx + desk.cx) // 2, 93, "JAW.SOCK · JSON LINES", "dim", align="center")
    c.text((desk.cx + js.cx) // 2, 93, "EACH WATCHES ITS WINDOWS", "dim", align="center")
    for r in (term, desk, js):
        c.arrow(r.cx, bus.y2, r.cx, r.y - 1, "line")
    got = s >= 1.3
    typed = reveal(DRAFT, phase(s, 2.0, 3.0))
    mini_terminal(c, term, s, got, typed)
    mini_desktop(c, desk, got, DRAFT if s >= 4.4 else "")
    mini_json(c, js, got)
    for r, label, tag in ((term, "TERMINAL", ("TODAY", "green")), (desk, "DESKTOP APP", ("LATER", "gold")),
                          (js, "YOURS · JSON SCHEMA", None)):
        c.text(r.x, r.y2 + 3, label, "text")
        if tag:
            c.text(r.x2, r.y2 + 3, tag[0], tag[1], align="right")

    # A new message fans out to every window; then the terminal's draft reaches the desktop app.
    if 0.6 <= s < 1.3:
        q = phase(s, 0.6, 1.3)
        for r in (term, desk, js):
            x, y = along([(jd.cx, jd.y2 + 1), (jd.cx, bus.y), (r.cx, bus.y), (r.cx, r.y - 2)], q)
            packet(c, x, y, JAWD)
    if 3.2 <= s < 3.8:
        x, y = along([(term.cx + 6, term.y - 2), (term.cx + 6, bus.y), (jd.cx + 6, bus.y), (jd.cx + 6, jd.y2 + 1)],
                     phase(s, 3.2, 3.8))
        packet(c, x, y, UI)
    if 3.8 <= s < 4.4:
        x, y = along([(jd.cx + 6, jd.y2 + 1), (jd.cx + 6, bus.y), (desk.cx + 6, bus.y), (desk.cx + 6, desk.y - 2)],
                     phase(s, 3.8, 4.4))
        packet(c, x, y, UI)
    if s >= 4.4:
        c.tag(160, 72, "DRAFTS LIVE IN JAWD", "sky", border="sky.dark", align="center")


# ------------------------------------------------------------------ 5. nothing ever waits

LANES = [("REALTIME", "MESSAGES · SENDS", "red"), ("INTERACTIVE", "WHAT'S ON SCREEN", "sky"),
         ("BACKGROUND", "HISTORY · MEDIA", "violet")]


def scene_lanes(c, p, s, d):
    caption(c, s, "NOTHING EVER WAITS",
            "LIVE MESSAGES GO FIRST, AND THE SCREEN GETS ONLY THE ROWS IT SHOWS")
    r = c.panel(8, 46, 304, 62, "THREE LANES", color="white", sub="ONE WRITER")
    x0, x1 = r.x + 66, r.x + 226
    gate = Rect(x1 + 6, r.y - 9, r.x2 - x1 - 6, 59)
    for i, (name, what, col) in enumerate(LANES):
        y = r.y + 1 + i * 16
        c.text(r.x, y, name, col)
        c.text(r.x, y + 7, what, "dim")
        c.rect(x0, y + 3, x1 - x0, 5, "raised")
        if i == 0:
            for k in range(6):
                q = (s * 0.9 + k / 6) % 1
                packet(c, x0 + 2 + round((x1 - x0 - 4) * q), y + 5, col)
        elif i == 1:
            for k in range(2):
                q = (s * 0.45 + k / 2) % 1
                packet(c, x0 + 2 + round((x1 - x0 - 4) * q), y + 5, col)
        else:
            for k in range(2):
                q = (s * 0.12 + k / 2) % 1
                bx = x0 + round((x1 - x0 - 26) * q)
                c.rect(bx, y + 3, 24, 5, c.dark(col))
                for j in range(4):
                    c.vline(bx + 5 + j * 5, y + 3, 5, col)
    c.rect(gate.x, gate.y, gate.w, gate.h, "panel")
    c.box(gate.x, gate.y, gate.w, gate.h, JAWD)
    c.text(gate.cx, gate.y + 4, "WRITER", JAWD, align="center")
    for i, line in enumerate(("LIVE FIRST", "", "HISTORY IN", "3 MS SLICES")):
        if line:
            c.text(gate.cx, gate.y + 18 + i * 8, line, "text", align="center")

    w = c.panel(8, 112, 304, 56, "A WINDOW, NOT A FIREHOSE", color=UI)
    c.text(w.x, w.y + 1, "1,000,000 MESSAGES IN JAW.DB", "text")
    rib = Rect(w.x, w.y + 9, 190, 9)
    c.rect(*rib, "raised")
    c.pattern(rib.x, rib.y + 1, rib.w, rib.h - 2, "line", "vlines")
    pages = [1, 2500, 7000, 10000]
    k = min(len(pages) - 1, int(s / 1.4))
    q = ease_in_out(phase(s - k * 1.4, 0.0, 0.5))
    frm = pages[k - 1] if k else pages[0]
    page = round(frm + (pages[k] - frm) * q)
    wx = rib.x + round((rib.w - 12) * (page - 1) / (pages[-1] - 1))
    c.rect(wx, rib.y - 1, 12, rib.h + 2, UI)
    c.box(wx, rib.y - 1, 12, rib.h + 2, "sky.light")
    c.text(w.x, rib.y2 + 4, f"PAGE {c.num(page)} OF 10,000", "white")
    c.text(w.x + 190, rib.y2 + 4, "100 MESSAGES · 2.0 MS P99", "sky.light", align="right")
    c.paragraph(w.x + 200, w.y + 1, "KEYSET PAGING, NEVER OFFSET: THE LAST PAGE COMES AS FAST AS THE FIRST",
                w.x2 - w.x - 200, "dim")


# ------------------------------------------------------------------ 6. measured, not promised

BUDGETS = [("SEARCH, A MILLION MESSAGES", 15, 50, "MS"),
           ("A 100-MESSAGE PAGE", 2.0, 5, "MS"),
           ("A NEW MESSAGE TO THE SCREEN", 2.9, 5, "MS"),
           ("JAW'S MEMORY, WITH PHOTOS", 35, 40, "MB"),
           ("JAWD'S MEMORY, STEADY", 59, 100, "MB")]


def scene_measured(c, p, s, d):
    caption(c, s, "MEASURED, NOT PROMISED",
            "MAKE PERF, 25 OCT 2026: M1 ULTRA, A MILLION MESSAGES IN 3,000 CHATS")
    h = c.panel(8, 46, 116, 122, "A KEYPRESS", color="green", sub="DRAWN")
    g = ease_out(phase(s, 0.3, 1.8))
    v = 16 - (16 - 0.40) * g
    w = c.text(h.x, h.y + 2, f"{v:.2f}", "white", font="large", scale=3, shadow="dim")
    c.text(h.x + w + 3, h.y + 16, "MS", "dim")
    c.text(h.x, h.y + 28, "P99, WHILE 2,000", "text")
    c.text(h.x, h.y + 35, "MESSAGES A SECOND", "text")
    c.text(h.x, h.y + 42, "ARRIVE", "text")
    c.rect(h.x, h.y + 52, h.w, 4, "raised")
    c.rect(h.x, h.y + 52, max(1, round(h.w * v / 16)), 4, "green")
    c.text(h.x, h.y + 59, "BUDGET 16 MS", "dim")
    if s >= 1.9:
        c.text(h.x2, h.y + 59, "40× INSIDE", "green", align="right")
    zz = s >= 2.4
    mimic(c, h.x - 2, h.y2 - MIMIC_H + 1, asleep=True)
    if zz:
        for k in range(3):
            q = (s * 0.6 + k / 3) % 1
            c.text(h.x + 36 + k * 3 + round(q * 6), h.y2 - 22 - round(q * 10), "Z", "muted", check=False)
    c.text(h.x2, h.y2 - 14, "IDLE, JAW", "text", align="right")
    c.text(h.x2, h.y2 - 7, "DRAWS NOTHING", "text", align="right")

    b = c.panel(128, 46, 184, 122, "BUDGET", color="white", sub="AND WHAT IT TOOK")
    for i, (label, got, budget, unit) in enumerate(BUDGETS):
        y = b.y + 2 + i * 21
        q = ease_out(stagger(phase(s, 0.6, 3.0), i, len(BUDGETS), overlap=0.6))
        c.text(b.x, y, label, "text")
        val = got * q
        txt = f"{val:.1f}" if isinstance(got, float) else f"{round(val)}"
        c.spans(b.x2, y, [(txt, "white"), (f" OF {budget} {unit}", "dim")], align="right")
        c.rect(b.x, y + 8, b.w, 5, "raised")
        fw = round(b.w * got / budget * q)
        if fw:
            c.rect(b.x, y + 8, fw, 5, "green")
            c.hline(b.x, y + 8, fw, "green.light")
        c.vline(b.x2 - 1, y + 7, 7, "muted")


# ------------------------------------------------------------------ 7. the palette

def quiet(c, r):
    """What's behind the palette, quieter: everything but the ground in the gutter's grey, as jaw does."""
    a = np.asarray(c.img).copy()
    bg = np.array(c.rgb(TN["bg"]), dtype=a.dtype)
    region = a[r.y:r.y2, r.x:r.x2]
    keep = np.all(region == bg, axis=2)
    region[~keep] = c.rgb(TN["gutter"])
    a[r.y:r.y2, r.x:r.x2] = region
    c.img.paste(Image.fromarray(a))


def marked(c, x, y, parts, base):
    """Text with the matched part marked: bright and underlined, as jaw marks a search's hits."""
    for text, hit in parts:
        w = c.text(x, y, text, TN["fg"] if hit else base, check=False)
        if hit:
            c.hline(x, y + 6, w, TN["fg"])
        x += w + 1 if text else 0
    return x


# Each item: ("h", heading, more) or (lead, who, [(text, marked)], detail).
PALETTE = {
    "LIS": ("ALL", [("ALL", None), ("CHATS", 1), ("MESSAGES", 214), ("LINKS", 3), ("FILES", 2)], "220 FOUND", [
        ("h", "CHATS", ""),
        ("chat", "", [("LIS", True), ("BON CLIMBING", False)], "GROUP · 16 UNREAD"),
        ("h", "MESSAGES", "214, TAB SHOWS THEM ALL"),
        ("msg", ("RUI", RUI, "IN LISBON CLIMBING"), [("THE ", False), ("LIS", True), ("BON CRAG AT 9?", False)], "THU"),
        ("msg", ("ANA", ANA, ""), [("BACK IN ", False), ("LIS", True), ("BON ON SUNDAY", False)], "WED"),
        ("h", "LINKS", ""),
        ("link", "", [("LIS", True), ("BONCLIMBING.PT/ROUTES", False)], "IN THE LIBRARY"),
        ("h", "FILES", ""),
        ("link", "", [("LIS", True), ("BON-TOPO.PDF", False)], "IN THE LIBRARY"),
    ]),
    "LIS·TAB": ("MESSAGES", [("ALL", None), ("CHATS", 1), ("MESSAGES", 214), ("LINKS", 3), ("FILES", 2)],
                "214 FOUND", [
        ("h", "MESSAGES", "214"),
        ("msg", ("RUI", RUI, "IN LISBON CLIMBING"), [("THE ", False), ("LIS", True), ("BON CRAG AT 9?", False)], "THU"),
        ("msg", ("ANA", ANA, ""), [("BACK IN ", False), ("LIS", True), ("BON ON SUNDAY", False)], "WED"),
        ("msg", ("SOFIA", SOFIA, "IN LISBON CLIMBING"), [("", False), ("LIS", True), ("T OF ROUTES IS UP", False)],
         "MON"),
        ("msg", ("YOU", TN["comment"], "IN PEDRO & ANA"), [("FLIGHTS TO ", False), ("LIS", True), ("BON", False)],
         "12 OCT"),
        ("msg", ("TIAGO", SENDERS[4], ""), [("", False), ("LIS", True), ("TEN TO THIS", False)], "9 OCT"),
    ]),
    "NAO": ("MESSAGES", [("ALL", None), ("MESSAGES", 38)], "38 FOUND", [
        ("h", "MESSAGES", "38"),
        ("msg", ("MÃE", MAE, ""), [("", False), ("NÃO", True), (" TE ESQUEÇAS DO CASACO", False)], "TUE"),
        ("msg", ("RUI", RUI, "IN JANTAR DE SEXTA"), [("", False), ("NÃO", True), (" POSSO NA SEXTA", False)], "SUN"),
        ("msg", ("YOU", TN["comment"], "IN ANA RIBEIRO"), [("AINDA ", False), ("NÃO", True), (", LOGO VEJO", False)],
         "SAT"),
    ]),
    "REC": ("ALL", [("ALL", None), ("ACTIONS", 1), ("MESSAGES", 12)], "13 FOUND", [
        ("h", "ACTIONS", ""),
        ("action", "", [("REC", True), ("ORD A VOICE MESSAGE", False)], ("IN THIS CHAT", "CTRL-R")),
        ("h", "MESSAGES", "12"),
        ("msg", ("MÃE", MAE, ""), [("THE ", False), ("REC", True), ("IPE YOU ASKED FOR", False)], "TUE"),
        ("msg", ("RUI", RUI, "IN LISBON CLIMBING"), [("", False), ("REC", True), ("ORDED THE ROUTE", False)], "THU"),
    ]),
}
PAL = Rect(40, 54, 240, 104)


def palette_box(c, s, query, key, answered):
    narrow, strip, found, items = PALETTE[key]
    c.rect(*PAL, TN["surface"])
    c.box(*PAL, TN["blue"])
    for px, py in ((PAL.x, PAL.y), (PAL.x2 - 1, PAL.y), (PAL.x, PAL.y2 - 1), (PAL.x2 - 1, PAL.y2 - 1)):
        c.px(px, py, TN["gutter"])
    c.rect(PAL.x + 4, PAL.y, SMALL.measure("PALETTE") + 4, 1, TN["surface"])
    c.text(PAL.x + 6, PAL.y - 2, "PALETTE", TN["text"], check=False)
    x, y = PAL.x + 5, PAL.y + 5
    c.text(x, y, ">", TN["comment"])
    tw = c.text(x + 6, y, query, TN["fg"]) if query else 0
    if int(s * 3) % 2 == 0:
        c.rect(x + 6 + tw + (1 if tw else 0), y - 1, 3, 7, TN["fg"])
    c.text(PAL.x2 - 5, y, found if answered else "FINDING…", TN["comment"], align="right")
    sx = x
    for name, n in strip:
        label = name if n is None else f"{name} {n}"
        w = SMALL.measure(label) + 4
        if name == narrow:
            c.rect(sx, y + 9, w, 7, TN["selection"])
        c.text(sx + 2, y + 10, label, TN["fg"] if name == narrow else TN["comment"])
        sx += w + 3
    c.text(PAL.x2 - 5, y + 10, "TAB NARROWS", TN["gutter"], align="right")
    if not answered:
        return
    y += 21
    first = True
    for item in items:
        if y > PAL.y2 - 10:
            break
        if item[0] == "h":
            _, head, more = item
            hw = c.text(x, y, head, TN["muted"])
            if more:
                hw += c.text(x + hw + 3, y, f"· {more}", TN["comment"]) + 3
            c.hline(x + hw + 3, y + 2, PAL.x2 - 5 - (x + hw + 3), TN["gutter"])
            y += 8
            continue
        lead, who, parts, detail = item
        if first:
            c.rect(PAL.x + 1, y - 1, PAL.w - 2, 7, TN["selection"])
            first = False
        lx = x + 4
        if lead == "chat":
            dot(c, lx, y, TN["green"])
        elif lead == "msg":
            c.vline(lx + 1, y, 5, who[1])
        elif lead == "link":
            c.text(lx, y, "→", TN["cyan"])
        else:
            pointer(c, lx + 1, y, TN["comment"])
        tx = lx + 7
        if lead == "msg":
            tx += c.text(tx, y, who[0], who[1]) + 4
            if who[2]:
                tx += c.text(tx, y, who[2], TN["comment"]) + 4
        marked(c, tx, y, parts, TN["text"])
        if isinstance(detail, tuple):
            kx = c.text(PAL.x2 - 5, y, detail[1], TN["blue"], align="right")
            c.text(PAL.x2 - 5 - kx - 5, y, detail[0], TN["comment"], align="right")
        else:
            c.text(PAL.x2 - 5, y, detail, TN["comment"], align="right")
        y += 8
    # The palette's own keys, set into its bottom border.
    keys = ("↑↓ MOVES · TAB NARROWS · ", "⏎", " DOES IT · ESC CLOSES")
    kw = sum(6 if k == "⏎" else SMALL.measure(k) + 1 for k in keys)
    c.rect(PAL.x + 4, PAL.y2 - 1, kw + 4, 1, TN["surface"])
    kx = PAL.x + 6
    for piece in keys:
        if piece == "⏎":
            kx += enter(c, kx, PAL.y2 - 3, TN["muted"]) + 1
        else:
            kx += c.text(kx, PAL.y2 - 3, piece, TN["muted"], check=False) + 1


PALETTE_TIMES = [  # (from, query typed between, key into PALETTE, answered at)
    (0.0, ("LIS", 0.6, 1.0), "LIS", 1.15),
    (2.6, ("LIS", 0, 0), "LIS·TAB", 2.75),
    (3.9, ("NAO", 4.0, 4.4), "NAO", 4.55),
    (5.8, ("REC", 5.9, 6.3), "REC", 6.45),
]
KEYCAPS = [("CTRL-K", 0.0, 0.5), ("TAB", 2.6, 3.0), ("BKSP", 5.75, 5.95)]


def scene_palette(c, p, s, d):
    behind = c.sub(c.w, c.h)
    r = screen(behind, 5.5)
    quiet(behind, r)
    c.paste(behind.img.crop((r.x - 1, r.y - 7, r.x2 + 1, r.y2 + 1)), r.x - 1, r.y - 7)
    caption(c, s, "CTRL-K FINDS ANYTHING",
            "EVERY KEYSTROKE ASKS JAWD · ACCENTS IGNORED · 15 MS P99 OVER A MILLION",
            tag=("IN JAW TODAY", "green"))
    pop = ease_back(phase(s, 0.1, 0.45))
    if pop < 1:
        h = max(3, round(PAL.h * pop))
        y0 = PAL.y + (PAL.h - h) // 2
        c.rect(PAL.x, y0, PAL.w, h, TN["surface"])
        c.box(PAL.x, y0, PAL.w, h, TN["blue"])
    else:
        at, (word, a, b), key, answered = [x for x in PALETTE_TIMES if s >= x[0]][-1]
        query = word if b == 0 else reveal(word, phase(s, a, b))
        palette_box(c, s, query, key, s >= answered)
    for label, a, b in KEYCAPS:
        if a <= s < b:
            c.tag(PAL.x2 + 16, 146, label, "bg", fill="sky", border="sky.dark", align="center")


# ------------------------------------------------------------------ 8. split panes

PANE1 = [msg("ANA", "DID YOU GET THE PHOTOS FROM SUNDAY?", col=ANA), msg(None, "YES, THE ONE AT THE LAKE", tick="read"),
         msg("ANA", kind="voice", col=ANA)]
PANE2 = [msg("RUI", "SATURDAY AT 9?", col=RUI), msg("INÊS", "I'M IN!", col=INES)]
PANE3 = [msg("MÃE", "HAVE YOU EATEN?", col=MAE), msg(None, "NOT YET", tick="read")]
DRAFTS = [("SEE YOU SATURDAY", 0.2, 0.7), ("I'LL BRING THE ROPE", 3.1, 3.7), ("OK, SEE YOU SUNDAY", 4.5, 5.1)]
FOCUS = [(0.0, 0), (2.9, 1), (4.3, 2), (5.7, 0)]


def scene_panes(c, p, s, d):
    caption(c, s, "SEVERAL CHATS AT ONCE",
            "SPLIT ACROSS OR DOWN, ONE CHAT A PANE, EACH BOX KEEPING ITS OWN DRAFT",
            tag=("EXPLORING", "violet"))
    r = window(c, 8, 43, 304, 125, "JAW")
    y = jaw_header(c, r, "· ONLINE", "14:02")
    top, bot = y + 2, 157
    left, right = 11, 309
    split = ease_out(phase(s, 0.8, 1.3))
    down = ease_out(phase(s, 1.9, 2.4))
    vx = round(right - (right - 152) * split)
    hy = round(bot - (bot - 108) * down)
    focus = [f for at, f in FOCUS if s >= at][-1]

    pane2 = list(PANE2) + ([msg("SOFIA", "WHO HAS A ROPE?", col=SOFIA)] if s >= 3.9 else [])
    pane3 = list(PANE3) + ([msg("MÃE", "COME FOR DINNER ON SUNDAY", col=MAE)] if s >= 2.6 else [])
    panes = [("ANA RIBEIRO", Rect(left, top, vx - left - 1, bot - top), PANE1)]
    if vx < right - 40:
        panes.append(("LISBON CLIMBING", Rect(vx + 1, top, right - vx - 1, hy - top - 1), pane2))
    if hy < bot - 30:
        panes.append(("MÃE", Rect(vx + 1, hy + 1, right - vx - 1, bot - hy - 1), pane3))
    for i, (name, pr, msgs) in enumerate(panes):
        on = i == focus
        inner = tbox(c, pr.x, pr.y, pr.w, pr.h, name, focus=on, right=str(i + 1))
        text, a, b = DRAFTS[i]
        draft = reveal(text, phase(s, a, b)) if s >= a else ""
        if inner.w > 40 and inner.h > 20:
            conversation(c, inner, msgs, draft=draft, cursor=on and int(s * 3) % 2 == 0, focus=on)
    if s >= 5.6:
        c.text(13, 160, reveal("JAWD KEEPS THE LAYOUT, SO A DESKTOP APP OPENS THE SAME PANES", phase(s, 5.6, 6.3)),
               TN["comment"])
    else:
        hints(c, 13, 160, [("⏎", "SEND"), ("CTRL-K", "FIND"), ("?", "KEYS")])


# ------------------------------------------------------------------ 9. the end card

def scene_end(c, p, s, d):
    drop = ease_back(phase(s, 0.1, 0.7))
    c.chunky(222, 22 + round((1 - drop) * -40), "JAW", ["#ffc29e", ORANGE, EMBER], scale=6, depth=3,
             side=[RUST, "#7a1c0e"], light="#ffe2d0", dark=INK, outline="shadow")
    shut = 1.2 < s < 1.35 or 2.2 < s < 2.35
    mimic(c, 14, 30 + int(s * 2.5) % 2, scale=2, shut=shut)
    c.strip(122, 76, 190, 2, c.theme.series)
    c.text(160, 86, reveal("WHATSAPP IN A TERMINAL, FAST", phase(s, 0.5, 1.1)), "white", font="large",
           align="center")
    if s > 1.0:
        c.text(160, 104, "FROM THE KEYBOARD, AND IT NEVER MAKES YOU WAIT", "text", align="center")
    if s > 1.3:
        c.text(160, 122, "RUST · GO · MACOS · LINUX · BEING BUILT", "muted", align="center")
        c.text(160, 150, "NOT AFFILIATED WITH WHATSAPP OR META", "dim", align="center")


SCENES = [
    (scene_screen, 5.5),
    (scene_daemon, 6.5),
    (scene_engines, 6.6),
    (scene_interfaces, 6.0),
    (scene_lanes, 6.0),
    (scene_measured, 6.5),
    (scene_palette, 8.0),
    (scene_panes, 7.0),
    (scene_end, 3.5),
]
TOTAL = sum(d for _, d in SCENES)


def frame(c, i):
    c.header("JAW", right="WHATSAPP IN A TERMINAL, FAST", mark=MARK)
    c.hline(0, BOTTOM + 1, c.w, "line")
    x0 = c.w // 2 - (len(SCENES) * 6 - 2) // 2
    for k in range(len(SCENES)):
        c.rect(x0 + k * 6, BOTTOM + 5, 4, 3, "white" if k == i else ("dim" if k < i else "raised"))


def teeth(c, bx, local):
    """The Mimic's jaw between two scenes: its lip, and its teeth opening and shutting on the old scene."""
    bite = 5 if int(local * 12) % 2 == 0 else 2
    for y in range(TOP, BOTTOM + 1):
        c.rect(bx - 7, y, 7, 1, ORANGE)
        c.px(bx - 7, y, EMBER)
        c.px(bx - 1, y, INK)
    for y0 in range(TOP + 2, BOTTOM - 6, 9):
        for r in range(7):
            w = max(0, round(bite * (1 - abs(r - 3) / 3.6)))
            if w:
                c.rect(bx, y0 + r, w, 1, CREAM)
                c.px(bx + w, y0 + r, INK)


def draw(c, t):
    s = t * TOTAL
    start = 0.0
    for i, (scene, d) in enumerate(SCENES):
        if s < start + d or i == len(SCENES) - 1:
            break
        start += d
    local = min(s - start, d)
    if i > 0 and local < WIPE:
        old_scene, old_d = SCENES[i - 1]
        old, new = c.sub(c.w, c.h), c.sub(c.w, c.h)
        old_scene(old, 1.0, old_d, old_d)
        scene(new, local / d, local, d)
        bx = round(-8 + ease_in_out(local / WIPE) * (c.w + 70))
        cut = max(0, min(c.w, bx))
        if cut:
            c.paste(new.img.crop((0, TOP, cut, BOTTOM + 1)), 0, TOP)
        if cut < c.w:
            c.paste(old.img.crop((cut, TOP, c.w, BOTTOM + 1)), cut, TOP)
        teeth(c, bx, local)
        mimic(c, bx - MIMIC_W - 6, BOTTOM - MIMIC_H + 1 - int(local * 14) % 2, shut=int(local * 12) % 2 == 1)
    else:
        scene(c, local / d, local, d)
    frame(c, i)


if __name__ == "__main__":
    if len(sys.argv) > 2 and sys.argv[1] == "frames":
        for at in sys.argv[2:]:
            c = Canvas(preset="wide")
            draw(c, float(at) / TOTAL)
            for note in c.check():
                print(f"warning ({at} s): {note}")
            c.save(Path("/tmp") / f"jaw-{at}.png", quiet=True)
            print(f"/tmp/jaw-{at}.png")
    else:
        # The GIF first: both write jaw-poster.png, and the poster should be the MP4's full size.
        animate(draw, HERE / "jaw.gif", preset="wide", seconds=TOTAL, fps=15, hold=3, scale=2)
        animate(draw, HERE / "jaw.mp4", preset="wide", seconds=TOTAL, fps=20, hold=3)
