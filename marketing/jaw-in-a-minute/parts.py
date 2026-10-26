"""What the scenes of jaw in a minute share: the Mimic, jaw's screen drawn in its Tokyo Night colours,
the glyphs the kit's fonts lack, and a message moving along a path."""

from pathlib import Path

from PIL import Image

from pixelkit import Rect
from pixelkit.fonts import FONTS

HERE = Path(__file__).parent
REPO = HERE.parent.parent
TOP, BOTTOM = 13, 169  # the content between the header's rule and the scene strip's

# The Mimic's own colours, from jaw's creatures/mimic.png (badcoffee.dev's creature).
ORANGE, EMBER, RUST, INK, CREAM, TONGUE = "#fc7537", "#f95524", "#bb2b15", "#1d0217", "#fbf6e6", "#d490e8"

# One colour per program, the same in every scene.
WHATSAPP, ENGINE, JAWD, UI = "green", "gold", ORANGE, "sky"

# Tokyo Night, as jaw draws itself (crates/jaw/src/theme.rs).
TN = {
    "bg": "#1a1b26", "surface": "#24283b", "selection": "#283457", "fg": "#c0caf5", "text": "#a9b1d6",
    "muted": "#737aa2", "comment": "#565f89", "gutter": "#3b4261", "blue": "#7aa2f7", "cyan": "#7dcfff",
    "teal": "#73daca", "purple": "#bb9af7", "green": "#9ece6a", "yellow": "#e0af68", "orange": "#ff9e64",
    "red": "#f7768e",
}
# Group senders' colours, in jaw's order.
SENDERS = [TN[k] for k in ("red", "yellow", "blue", "purple", "cyan", "green", "orange", "teal")]

SMALL = FONTS["small"]


# ------------------------------------------------------------------ the Mimic

# Its lid, eyes and upper teeth drop CHOMP pixels when it chomps; its eyes are rows 2 to 4
# (crates/jaw/src/art.rs).
LID, CHOMP, EYES = 10, 3, range(2, 5)
SPRITE = Image.open(REPO / "creatures" / "mimic.png").convert("RGBA")


def _white(p) -> bool:
    return 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2] > 190


def _mimic_image(shut: bool, asleep: bool, scale: int) -> Image.Image:
    src = SPRITE
    out = Image.new("RGBA", src.size, (0, 0, 0, 0))
    for y in range(src.height):
        if shut and y < CHOMP:
            continue
        sy = y - CHOMP if shut and y <= LID + CHOMP else y
        for x in range(src.width):
            p = src.getpixel((x, sy))
            if p[3] < 128:
                continue
            if asleep and sy in EYES:
                iris = _white(src.getpixel((x, EYES.start)))
                if iris and sy == EYES.stop - 1:
                    p = (0xfb, 0xf6, 0xe6, 255)
                elif _white(p):
                    p = src.getpixel((x, EYES.start - 1))
            if asleep:
                g = int((0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2]) * 0.55)
                p = (g, g, min(255, g + 8), 255)
            out.putpixel((x, y), p)
    return out.resize((out.width * scale, out.height * scale), Image.NEAREST) if scale > 1 else out


MIMICS = {(shut, asleep, scale): _mimic_image(shut, asleep, scale)
          for shut in (False, True) for asleep in (False, True) for scale in (1, 2)}
MIMIC_W, MIMIC_H = SPRITE.size


def mimic(c, x: int, y: int, *, shut: bool = False, asleep: bool = False, scale: int = 1) -> None:
    c.image(MIMICS[(shut, asleep, scale)], x, y, colors="keep")


# The Mimic at header size: its two eyes, its teeth, its tongue and its feet.
MARK = (
    [
        ".KKK....KKK.",
        ".KWK.OO.KWK.",
        "OOOOOOOOOOOO",
        "OKKKKKKKKKKO",
        "OKWKWKWKWKKO",
        "OKKKKKKKPPKO",
        "OKWKWKWKPPKO",
        "OOOOOOOOOOOO",
        "DDDDDDDDDDDD",
        ".OO......OO.",
    ],
    {"O": ORANGE, "K": INK, "W": CREAM, "P": TONGUE, "D": RUST},
)


# ------------------------------------------------------------------ glyphs the fonts lack

def dot(c, x: int, y: int, col) -> None:
    """●, on a small-font line whose cap top is y."""
    c.rect(x, y + 1, 3, 3, col)


def ring(c, x: int, y: int, col) -> None:
    """○"""
    c.box(x, y + 1, 3, 3, col)


def pointer(c, x: int, y: int, col) -> None:
    """▸"""
    c.vline(x, y + 1, 3, col)
    c.px(x + 1, y + 2, col)


def enter(c, x: int, y: int, col) -> int:
    """⏎, 5 wide; returns its width."""
    c.sprite(x, y, ["....#", "....#", ".#..#", "#####", ".#..."], {"#": col})
    return 5


def heart(c, x: int, y: int, col) -> int:
    c.sprite(x, y + 1, [".#.#.", "#####", ".###.", "..#.."], {"#": col})
    return 5


def play(c, x: int, y: int, col) -> int:
    c.sprite(x, y, ["#..", "##.", "###", "##.", "#.."], {"#": col})
    return 3


def fit(s: str, w: int) -> str:
    """s cut to w pixels of the small font, with an ellipsis where it was cut."""
    if SMALL.measure(s) <= w:
        return s
    while s and SMALL.measure(s + "…") > w:
        s = s[:-1]
    return s.rstrip() + "…"


def wrap(s: str, w: int) -> list[str]:
    return SMALL.wrap(s, w) or [""]


# ------------------------------------------------------------------ moving along a path

def along(points, p: float) -> tuple[int, int]:
    """The point a share p of the way along an orthogonal path through `points` (from the Alfama
    series' series.py)."""
    segs = list(zip(points, points[1:]))
    d = max(0.0, min(1.0, p)) * sum(abs(x1 - x0) + abs(y1 - y0) for (x0, y0), (x1, y1) in segs)
    for (x0, y0), (x1, y1) in segs:
        n = abs(x1 - x0) + abs(y1 - y0)
        if d <= n:
            k = d / n if n else 0
            return round(x0 + (x1 - x0) * k), round(y0 + (y1 - y0) * k)
        d -= n
    return points[-1]


def packet(c, x: int, y: int, color) -> None:
    """A message on its way, centred on (x, y): a 3px square with a dark rim so it stands out on a line."""
    c.rect(x - 2, y - 2, 5, 5, "shadow")
    c.rect(x - 1, y - 1, 3, 3, color)


# ------------------------------------------------------------------ jaw's screen

def window(c, x: int, y: int, w: int, h: int, title: str = "") -> Rect:
    """A terminal window: its title bar with three lights, then Tokyo Night's ground. Returns the
    content Rect."""
    c.claim(Rect(x, y, w, h), f"window {title!r}")
    c.rect(x, y, w, h, TN["bg"])
    c.rect(x, y, w, 7, TN["surface"])
    for i, col in enumerate(("red", "yellow", "green")):
        c.rect(x + 3 + i * 5, y + 2, 3, 3, col)
    if title:
        c.text(x + w // 2, y + 1, title, TN["comment"], align="center")
    c.box(x, y, w, h, TN["gutter"])
    return Rect(x + 1, y + 7, w - 2, h - 8)


def jaw_header(c, r: Rect, status: str, right: str, *, status_col=None) -> int:
    """jaw's top line: its name in a teal pill, how it's connected and the time. Returns the y below."""
    y = r.y + 2
    w = SMALL.measure("JAW")
    c.rect(r.x + 2, y - 1, w + 4, 7, TN["teal"])
    c.text(r.x + 4, y, "JAW", TN["bg"])
    c.text(r.x + w + 9, y, status, status_col or TN["muted"])
    c.text(r.x2 - 3, y, right, TN["comment"], align="right")
    return y + 8


def tbox(c, x: int, y: int, w: int, h: int, title: str = "", *, focus: bool = False, title_col=None,
         right: str = "") -> Rect:
    """One of jaw's rounded boxes, its title set into the top border. Returns the inside."""
    border = TN["blue"] if focus else TN["gutter"]
    c.box(x, y, w, h, border)
    for px, py in ((x, y), (x + w - 1, y), (x, y + h - 1), (x + w - 1, y + h - 1)):
        c.px(px, py, TN["bg"])
    if title:
        tw = SMALL.measure(title)
        c.rect(x + 4, y, tw + 4, 1, TN["bg"])
        c.text(x + 6, y - 2, title, title_col or (TN["fg"] if focus else TN["text"]))
    if right:
        rw = SMALL.measure(right)
        c.rect(x + w - rw - 8, y, rw + 4, 1, TN["bg"])
        c.text(x + w - 6, y - 2, right, TN["comment"], align="right")
    return Rect(x + 4, y + 5, w - 8, h - 8)


def hints(c, x: int, y: int, items) -> int:
    """The keys along jaw's bottom line: (key, what) pairs, the key in Tokyo Night's blue; a key of
    "⏎" is drawn. Returns the x after them."""
    for key, what in items:
        if key == "⏎":
            x += enter(c, x, y, TN["blue"]) + 3
        else:
            x += c.text(x, y, key, TN["blue"]) + 3
        x += c.text(x, y, what, TN["comment"]) + 7
    return x


def sidebar(c, r: Rect, folders, roster) -> None:
    """Folders with their unread counts, then the roster: online first."""
    y = r.y
    for name, n, depth, open_ in folders:
        x = r.x + depth * 6
        if open_ is not None:
            c.text(x, y, "▼" if open_ else "→", TN["comment"])
            x += 6
        c.text(x, y, name, TN["fg"] if depth == 0 else TN["text"])
        if n:
            c.text(r.x2, y, str(n), TN["fg"], align="right")
        y += 7
    y += 4
    c.text(r.x, y, "ROSTER", TN["muted"])
    y += 8
    for name, online in roster:
        (dot if online else ring)(c, r.x, y, TN["green"] if online else TN["comment"])
        c.text(r.x + 6, y, name, TN["fg"] if online else TN["muted"])
        y += 8


def chat_list(c, r: Rect, chats, selected: int | None = None) -> None:
    """Chats newest first, two lines each: the name with its unread count, then the last message."""
    y = r.y
    for i, (name, preview, unread, live) in enumerate(chats):
        if y + 12 > r.y2:
            break
        if i == selected:
            c.rect(r.x - 2, y - 1, r.w + 4, 15, TN["selection"])
        x = r.x
        if live:
            dot(c, x, y, TN["green"])
        x += 5
        badge = str(unread) if unread else ""
        bw = SMALL.measure(badge) + 4 if badge else 0
        c.text(x, y, fit(name, r.x2 - x - bw - 2), TN["fg"])
        if badge:
            c.text(r.x2, y, badge, TN["orange"] if unread >= 10 else TN["fg"], align="right")
        c.text(x, y + 8, fit(preview, r.x2 - x), TN["comment"])
        y += 17


def photo(c, x: int, y: int, w: int, h: int) -> None:
    """A photo from Sunday: the lake under the hills, sun up."""
    c.rect(x, y, w, h, TN["blue"])
    c.circle(x + w - 9.5, y + 5.5, 2.5, TN["yellow"])
    c.polygon([(x, y + h * 6 // 10), (x + w // 4, y + 4), (x + w // 2, y + h * 6 // 10)], fill=TN["purple"])
    c.polygon([(x + w // 3, y + h * 6 // 10), (x + w * 2 // 3, y + 7), (x + w - 1, y + h * 6 // 10)],
              fill=TN["comment"])
    c.rect(x, y + h * 6 // 10, w, h - h * 6 // 10, TN["cyan"])
    c.hline(x + 3, y + h * 6 // 10 + 3, w // 3, TN["fg"])
    c.hline(x + w // 2, y + h * 6 // 10 + 6, w // 4, TN["fg"])
    c.box(x, y, w, h, TN["gutter"])


WAVE = [2, 3, 5, 7, 6, 4, 2, 3, 5, 6, 5, 3, 2, 4, 6, 7, 5, 3, 2, 2]


def voice(c, x: int, y: int, played: float = 0.0, length: str = "0:14") -> int:
    """A voice message: play, its waveform with the playhead, its length. Returns its width."""
    play(c, x, y, TN["cyan"])
    for i, v in enumerate(WAVE):
        hh = max(1, v * 5 // 7)
        col = TN["cyan"] if i < played * len(WAVE) else TN["text"]
        c.vline(x + 6 + i * 2, y + 5 - hh, hh, col)
    return 6 + len(WAVE) * 2 + 3 + c.text(x + 6 + len(WAVE) * 2 + 3, y, length, TN["comment"])


def ticks(c, x: int, y: int, state: str) -> None:
    """Where a message of yours has got, right-aligned at x: on its way, sent, delivered, read."""
    if state == "pending":
        ring(c, x - 3, y, TN["comment"])
    elif state == "sent":
        c.text(x, y, "✓", TN["comment"], align="right")
    else:
        c.text(x, y, "✓✓", TN["cyan"] if state == "read" else TN["comment"], align="right")


def msg(who=None, text="", kind="text", col=None, tick=None, react=0):
    """A message for conversation(): who sent it (None for you), what it says, its kind (text, photo,
    voice, day), its sender's colour, your ticks and its hearts."""
    return {"who": who, "text": text, "kind": kind, "col": col or SENDERS[0], "tick": tick, "react": react}


def _height(m, w: int) -> int:
    if m["kind"] == "photo":
        return 20 + (8 if m["react"] else 0)
    if m["kind"] in ("voice", "day"):
        return 6
    if m["who"]:
        indent = SMALL.measure(m["who"]) + 4
        return len(wrap(m["text"], w - indent)) * 7 - 1
    return len(wrap(m["text"], w - 12)) * 7 - 1


def _draw(c, m, x: int, y: int, w: int, played: float) -> None:
    if m["kind"] == "day":
        c.text(x + w // 2, y, m["text"], TN["comment"], align="center")
        return
    indent = 0
    if m["who"]:
        indent = c.text(x, y, m["who"], m["col"]) + 4
    if m["kind"] == "photo":
        photo(c, x + indent, y, 40, 20)
        if m["react"]:
            heart(c, x + indent, y + 22, TN["red"])
            c.text(x + indent + 7, y + 22, str(m["react"]), TN["text"])
        return
    if m["kind"] == "voice":
        voice(c, x + indent, y, played)
        return
    if m["who"]:
        for k, line in enumerate(wrap(m["text"], w - indent)):
            c.text(x + indent, y + k * 7, line, TN["text"])
        return
    lines = wrap(m["text"], w - 12)
    for k, line in enumerate(lines):
        c.text(x + w - 11, y + k * 7, line, TN["fg"], align="right")
    if m["tick"]:
        ticks(c, x + w, y + (len(lines) - 1) * 7, m["tick"])


def conversation(c, r: Rect, msgs, *, draft: str = "", cursor: bool = False, focus: bool = True,
                 played: float = 0.0, hint: str = "A MESSAGE") -> None:
    """An open chat: its messages anchored at the bottom, newest last, over the message box."""
    box_y = r.y2 - 11
    border = TN["blue"] if focus else TN["gutter"]
    c.box(r.x, box_y, r.w, 11, border)
    if draft:
        tw = c.text(r.x + 3, box_y + 3, fit(draft, r.w - 10), TN["fg"])
    else:
        tw = 0
        if not cursor:
            c.text(r.x + 3, box_y + 3, hint, TN["gutter"])
    if cursor:
        c.rect(r.x + 3 + tw + (1 if tw else 0), box_y + 2, 3, 7, TN["fg"])
    y = box_y - 3
    for m in reversed(msgs):
        h = _height(m, r.w)
        if y - h < r.y + 2:
            break
        y -= h
        _draw(c, m, r.x, y, r.w, played)
        y -= 3
