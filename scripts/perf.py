#!/usr/bin/env python3
"""Every budget in the design's Budgets table, measured on release builds on this machine, as one
table (`make perf`).

    perf.py [--quick] [--small]

It runs `jawd demo` on a made-up account of a million messages in 3,000 chats, in a folder of its
own: the full history sync; then, in a fresh jawd, lone messages, chats opened, pages at any depth
and finds, jaw launched twenty times, a walk down the chat list and a quiet minute; a flood of
2,000 messages a second for a minute; and catch-ups after time offline, one message at a time as
whatsmeow acks them today and in groups. jaw is attached through `jaw bench --socket` for its keys,
opens, start, memory and wake-ups. Then the engine's round trip (`jawd bench link`, on the demo and
the Go engines, the Go engine unlinked so nothing reaches WhatsApp), the windows' fan-out
(`jawd bench feed`), the store alone (`jawd bench store`), jaw's own flood with pictures
(`jaw bench`), a 12-megapixel photo (`media-bench`), a 12-megapixel photo's arrival to its 256
pixel preview (`jawd bench media`), a 12-megapixel photo of yours sent in SD and HD while texts
arrive (`jawd bench media-out`) and the outbox's background steps in a group of a thousand (their
tests, which time each step only in a release build); --quick leaves out the last six, and --small
runs everything on 50,000 messages and short floods, to check the harness itself, not the budgets.

Each row says the load average while it ran. Everything runs with JAW_HOME and TMPDIR in one
temporary folder, removed afterwards, so nothing touches jaw's own folder; only the processes it
started are stopped. The table goes to target/perf/<when>.md, with each run's report and log
beside it.
"""

import json
import os
import platform
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
JAWD = ROOT / "target/release/jawd"
JAW = ROOT / "target/release/jaw"
QUICK = "--quick" in sys.argv[1:]
SMALL = "--small" in sys.argv[1:]

MESSAGES = 50_000 if SMALL else 1_000_000
CHATS = 300 if SMALL else 3_000
FLOOD = 2_000
FLOOD_SECONDS = 15 if SMALL else 60
LAUNCHES = 5 if SMALL else 20
IDLE = 40 if SMALL else 60
# The engine's round trips: as many as the writer keeps transactions' times (1,024), so the two
# cover the same messages.
TRIPS = 1_000
# Acks the demo engine's flood and second catch-up wait for at once; jaw-engine waits for each.
IN_FLIGHT = 256
# The WAL's bound, in MiB.
WAL_LIMIT = 64
# The 12-megapixel photos `jawd bench media` hands over, one at a time.
PHOTOS_IN = 3 if SMALL else 10
# The 12-megapixel photos `jawd bench media-out` sends, one at a time.
PHOTOS_OUT = 3 if SMALL else 10
# The tests that time the outbox's steps of regrouping and letting go, each a background
# transaction, in a group of 1,024.
OUTBOX_STEPS = [
    "store::write::outbox::tests::in_a_group_of_a_thousand_a_step_of_regrouping_looks_at_one_message_so_it_stays_short",
    "store::write::outbox::tests::when_most_of_a_thousand_leave_steps_are_sized_by_who_was_sent_to_and_skip_what_is_read",
]


def load() -> float:
    return os.getloadavg()[0]


class Step:
    """A measured step: when it ran and the load average at its start and end."""

    def __init__(self, name: str):
        self.name = name
        self.loads = [load()]
        print(f"perf: {name} (load {self.loads[0]:.1f})", flush=True)

    def done(self) -> "Step":
        self.loads.append(load())
        return self

    def load(self) -> str:
        return "–".join(f"{x:.1f}" for x in (min(self.loads), max(self.loads)))


class Demo:
    """`jawd demo` on the demo folder, its output read as it comes."""

    def __init__(self, folder: Path, raw: Path, name: str, args: list[str], env: dict):
        self.folder = folder
        self.log = open(raw / f"demo-{name}.log", "w")
        self.report = raw / f"demo-{name}.json"
        command = [str(JAWD), "demo", "--folder", str(folder), "--exit", "--report", str(self.report), *args]
        self.process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=self.log, text=True, env=env)
        self.lines: list[str] = []
        self.said = threading.Condition()
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        for line in self.process.stdout:
            self.log.write(line)
            self.log.flush()
            with self.said:
                self.lines.append(line)
                self.said.notify_all()

    def wait_for(self, text: str, timeout: float) -> str:
        end = time.monotonic() + timeout
        with self.said:
            while True:
                for line in self.lines:
                    if text in line:
                        return line
                left = end - time.monotonic()
                if left <= 0 or self.process.poll() is not None:
                    raise RuntimeError(f"jawd demo never said {text!r}; see {self.log.name}")
                self.said.wait(min(left, 1.0))

    def socket(self, timeout: float = 120) -> Path:
        path = self.folder / "jaw.sock"
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            if self.process.poll() is not None:
                raise RuntimeError(f"jawd demo stopped as it started; see {self.log.name}")
            try:
                with socket.socket(socket.AF_UNIX) as s:
                    s.connect(str(path))
                return path
            except OSError:
                time.sleep(0.05)
        raise RuntimeError(f"jawd demo didn't listen on {path}")

    def finish(self, timeout: float) -> dict:
        try:
            self.process.wait(timeout)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            self.process.wait(10)
            raise RuntimeError(f"jawd demo didn't finish within {timeout:.0f} s; see {self.log.name}")
        if self.process.returncode != 0:
            raise RuntimeError(f"jawd demo failed; see {self.log.name}")
        return json.loads(self.report.read_text())


class Jaw:
    """`jaw bench`, attached to a jawd or on its own, with its report as JSON."""

    def __init__(self, raw: Path, name: str, args: list[str], env: dict):
        self.report = raw / f"jaw-{name}.json"
        self.log = raw / f"jaw-{name}.log"
        command = [str(JAW), "bench", *args, "--json", str(self.report)]
        self.process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, env=env)

    def finish(self, timeout: float) -> dict:
        try:
            out = self.process.communicate(timeout=timeout)[0]
        except subprocess.TimeoutExpired:
            self.process.kill()
            out = self.process.communicate()[0]
        self.log.write_text(out)
        if self.process.returncode != 0:
            raise RuntimeError(f"jaw bench failed; see {self.log}")
        return json.loads(self.report.read_text())


def run(command: list[str], raw: Path, name: str, env: dict, timeout: float) -> str:
    out = subprocess.run(command, capture_output=True, text=True, env=env, timeout=timeout)
    (raw / f"{name}.log").write_text(out.stdout + out.stderr)
    if out.returncode != 0:
        raise RuntimeError(f"{' '.join(command[:4])} failed; see {raw / name}.log")
    return out.stdout


def children_rss(parent: int) -> int:
    """The resident memory of `parent`'s children, in bytes, as ps says."""
    out = subprocess.run(["ps", "-A", "-o", "pid=,ppid=,rss="], capture_output=True, text=True).stdout
    total = 0
    for line in out.splitlines():
        fields = line.split()
        if len(fields) == 3 and fields[1] == str(parent):
            total += int(fields[2]) * 1024
    return total


def link(raw: Path, name: str, args: list[str], env: dict) -> tuple[str, list[int]]:
    """`jawd bench link`, sampling its engine's memory if the engine is a child process."""
    command = [str(JAWD), *args]
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, env=env)
    samples = []
    while process.poll() is None:
        rss = children_rss(process.pid)
        if rss:
            samples.append(rss)
        time.sleep(0.1)
    out = process.stdout.read()
    (raw / f"{name}.log").write_text(out)
    if process.returncode != 0:
        raise RuntimeError(f"jawd bench link failed; see {raw / name}.log")
    return out, samples


def machine() -> str:
    if platform.system() == "Darwin":
        sysctl = lambda key: subprocess.run(["sysctl", "-n", key], capture_output=True, text=True).stdout.strip()
        cpu, cores, memory = sysctl("machdep.cpu.brand_string"), sysctl("hw.ncpu"), int(sysctl("hw.memsize"))
        version = subprocess.run(["sw_vers", "-productVersion"], capture_output=True, text=True).stdout.strip()
        return f"{cpu}, {cores} cores, {memory / 2**30:.0f} GB, macOS {version}"
    cpu = next((line.split(":", 1)[1].strip() for line in open("/proc/cpuinfo") if line.startswith("model name")), "?")
    memory = next(int(line.split()[1]) for line in open("/proc/meminfo") if line.startswith("MemTotal"))
    return f"{cpu}, {os.cpu_count()} cores, {memory / 2**20:.0f} GB, Linux {platform.release()}"


def ms(x) -> str:
    if x is None:
        return "–"
    return f"{x:.2f} ms" if x < 10 else f"{x:.0f} ms" if x >= 100 else f"{x:.1f} ms"


def p(summary: dict, q: str = "p99") -> float | None:
    return summary.get(q) if summary and summary.get("n") else None


def timing(summary: dict) -> str:
    if not summary or not summary.get("n"):
        return "none timed"
    return f"{ms(summary['p99'])} p99, {ms(summary['p50'])} p50 ({summary['n']:,})"


def numbers(pattern: str, text: str) -> list[float]:
    found = re.search(pattern, text)
    if not found:
        raise RuntimeError(f"no {pattern!r} in the bench's output")
    return [float(g) for g in found.groups()]


def main() -> int:
    for binary in (JAWD, JAW):
        if not binary.exists():
            print(f"perf: {binary} isn't built; make perf builds it", file=sys.stderr)
            return 1
    when = time.strftime("%Y-%m-%d-%H%M")
    out = ROOT / "target/perf"
    raw = out / when
    raw.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix="jaw-perf-"))
    env = dict(os.environ, JAW_HOME=str(scratch / "home"), TMPDIR=str(scratch), JAW_NOTIFY="off")
    folder = scratch / "demo"
    steps: dict[str, Step] = {}
    results: dict[str, dict] = {}
    failed: list[str] = []

    def step(name: str, measure):
        steps[name] = Step(name)
        try:
            results[name] = measure()
        except Exception as e:
            failed.append(f"{name}: {e}")
            print(f"perf: {name} failed: {e}", file=sys.stderr, flush=True)
        steps[name].done()

    def history():
        demo = Demo(folder, raw, "history", ["--messages", str(MESSAGES), "--chats", str(CHATS)], env)
        sock = str(demo.socket())
        jaw = Jaw(raw, "history", ["--socket", sock, "--keys", "typing", "--warm", "3", "--seconds", "30"], env)
        keys = jaw.finish(120)
        return {"demo": demo.finish(3600)["history"], "jaw": keys}

    def steady():
        demo = Demo(folder, raw, "steady", ["--steady", "--idle", str(IDLE), "--after", "70"], env)
        sock = str(demo.socket())
        # jaw starts "with jawd running": once the contacts, groups and avatars a starting jawd
        # brings in are in.
        time.sleep(35)
        launches = [Jaw(raw, f"launch-{n}", ["--socket", sock, "--first"], env).finish(30) for n in range(LAUNCHES)]
        walk = Jaw(raw, "walk", ["--socket", sock, "--keys", "walk", "--every", "250", "--warm", "2", "--seconds", "16"], env)
        walked = walk.finish(60)
        # jaw attaches before jawd settles, so its windows and any read it sends come first, and
        # counts its own quiet stretch inside jawd's, which begins 15 s after jawd's last commit
        # and ends with the demo.
        demo.wait_for("steady, with", 300)
        idle = Jaw(raw, "idle", ["--socket", sock, "--keys", "none", "--warm", "25", "--seconds", str(IDLE - 20)], env)
        quiet = idle.finish(IDLE + 90)
        report = demo.finish(300)
        return {"demo": report, "launches": launches, "walk": walked, "idle": quiet}

    def flood():
        args = ["--flood", str(FLOOD), "--seconds", str(FLOOD_SECONDS), "--in-flight", str(IN_FLIGHT), "--after", "8"]
        demo = Demo(folder, raw, "flood", args, env)
        sock = str(demo.socket())
        jaw = Jaw(raw, "flood", ["--socket", sock, "--keys", "typing", "--warm", "10", "--seconds", str(FLOOD_SECONDS - 5)], env)
        keys = jaw.finish(FLOOD_SECONDS + 60)
        return {"demo": demo.finish(600)["flood"], "jaw": keys}

    def catch_up(backlog: int, in_flight: int, seconds: int):
        def measure():
            args = ["--catch-up", str(backlog), "--in-flight", str(in_flight), "--after", "5"]
            demo = Demo(folder, raw, f"catch-up-{in_flight}", args, env)
            sock = str(demo.socket())
            jaw = Jaw(raw, f"catch-up-{in_flight}", ["--socket", sock, "--keys", "typing", "--warm", "6", "--seconds", str(seconds)], env)
            keys = jaw.finish(seconds + 60)
            return {"demo": demo.finish(900)["catch_up"], "jaw": keys}

        return measure

    def round_trip(text: str) -> dict:
        return {
            "trip": numbers(r"p50 (\d+) µs, p99 (\d+) µs", text),
            "transaction": numbers(r"the inbox's transactions p50 (\d+) µs, p99 (\d+) µs", text),
            "flush": numbers(r"the drive's flush alone.*?p50 (\d+) µs, p99 (\d+) µs", text),
            "own": numbers(r"jawd's own work in a round trip is about (\d+) µs at p50: (\d+) µs outside the inbox's transaction, and (\d+) µs", text),
        }

    def link_demo():
        text, _ = link(raw, "link-demo", ["--engine", "demo", "bench", "link", "--commit", "--count", str(TRIPS)], env)
        return round_trip(text)

    def link_go():
        text, rss = link(raw, "link-go", ["--engine", "go", "bench", "link", "--commit", "--count", str(TRIPS)], env)
        alone, alone_rss = link(raw, "link-go-alone", ["--engine", "go", "bench", "link", "--count", "20000"], env)
        samples = rss + alone_rss
        return {
            **round_trip(text),
            "alone": numbers(r"p50 (\d+) µs, p99 (\d+) µs", alone),
            # Half a second in, once it has opened its session and said hello.
            "rss_first": samples[min(5, len(samples) - 1)] if samples else 0,
            "rss_most": max(samples or [0]),
        }

    def feed():
        text = run([str(JAWD), "bench", "feed", "--rate", str(FLOOD), "--seconds", "5", "--lone", "50"], raw, "feed", env, 300)
        return {
            "burst": numbers(r"commit to push\s+p50 ([\d.]+) ms, p99 ([\d.]+) ms, max ([\d.]+) ms", text),
            "lone": numbers(r"a lone message\s+p50 ([\d.]+) ms, p99 ([\d.]+) ms, max ([\d.]+) ms", text),
        }

    def store():
        text = run([str(JAWD), "bench", "store", "--messages", str(MESSAGES), "--chats", str(CHATS)], raw, "store", env, 1800)
        row = r"\s+p50\s+([\d.]+) ms\s+p99\s+([\d.]+) ms\s+max\s+([\d.]+) ms"
        return {
            "every_page": numbers(r"every page of the biggest chat, \d+ pages" + row, text),
            "oldest": numbers(r"its oldest 100 messages" + row, text),
            "searches": [numbers(f"a search for \"{q}\"[^\\n]*?" + row, text) for q in ("nao", "zefiro", "cafe amanha")],
        }

    def jaw_alone():
        typing = Jaw(raw, "own-flood", ["--keys", "typing", "--seconds", "10"], env).finish(120)
        pictures = Jaw(raw, "own-pictures", ["--keys", "typing", "--seconds", "10", "--library", "--graphics", "kitty"], env).finish(120)
        return {"typing": typing, "pictures": pictures}

    def photo():
        media = scratch / "media"
        command = ["cargo", "run", "--release", "-q", "-p", "jaw-media", "--example", "media-bench", "--", str(media), "--photos"]
        text = run(command, raw, "media", env, 1800)
        rows = re.findall(r"^(12 MP [^,]+), ([\d.]+ MB) to SD and HD, as planned\s+(\d+) ms", text, re.M)
        if not rows:
            raise RuntimeError("media-bench timed no photo")
        return {"photos": [(name, size, int(t)) for name, size, t in rows]}

    def photo_in():
        text = run([str(JAWD), "bench", "media", "--count", str(PHOTOS_IN)], raw, "photo-in", env, 600)
        row = r"\s+p50 (\d+) ms, max (\d+) ms, min (\d+) ms"
        return {
            "photo": numbers(r"a (\d+)x(\d+) photo \(([\d.]+) MB\)", text),
            "thumbnail": numbers(r"its thumbnail" + row, text),
            "file": numbers(r"its file, downloaded" + row, text),
            "small": numbers(r"its 256 px preview" + row, text),
        }

    def photo_out():
        text = run([str(JAWD), "bench", "media-out", "--count", str(PHOTOS_OUT), "--hd"], raw, "photo-out", env, 600)
        row = r"\s+p50 ([\d.]+) ms, p99 ([\d.]+) ms, max ([\d.]+) ms \((\d+) "
        return {
            "photo": numbers(r"a (\d+)x(\d+) photo \(([\d.]+) MB\)", text),
            "answer": numbers(r"send\.media's answer" + row, text),
            "ready": numbers(r"its copy made and in the cache \(ready\)" + row, text),
            "sent": numbers(r"it sent, once uploaded" + row, text),
            "acks_between": numbers(r"a text's ack, between photos" + row, text),
            "acks_during": numbers(r"a text's ack, while a photo went" + row, text),
            "shown_between": numbers(r"a text shown to the interface, between photos" + row, text),
            "shown_during": numbers(r"a text shown to the interface, while a photo went" + row, text),
        }

    def background():
        command = ["cargo", "test", "--release", "-q", "-p", "jawd", "--bin", "jawd", "--", "--exact", "--nocapture", "--test-threads=1", *OUTBOX_STEPS]
        run(command, raw, "background", env, 1800)
        found = re.findall(
            r"steps of (regrouping|letting go) at their quickest of three: (\d+), ([\d.]+) ms at the median, ([\d.]+) ms at most, (\d+) of 3 ms or more",
            (raw / "background.log").read_text(),
        )
        if len(found) != 2 * len(OUTBOX_STEPS):
            raise RuntimeError(f"the outbox's step tests timed {len(found)} kinds of step; see {raw / 'background'}.log")
        return {"steps": [(what, int(n), float(median), float(most), int(over)) for what, n, median, most, over in found]}

    started = time.monotonic()
    load_before = os.getloadavg()
    try:
        step("history", history)
        step("steady", steady)
        step("flood", flood)
        step("catch-up, one at a time", catch_up(500 if SMALL else 3_000, 1, 12))
        step("catch-up, in groups", catch_up(3_000 if SMALL else 30_000, IN_FLIGHT, 8))
        step("link, demo engine", link_demo)
        step("link, Go engine", link_go)
        step("feed", feed)
        if not QUICK:
            step("store", store)
            step("jaw alone", jaw_alone)
            step("photo", photo)
            step("photo in", photo_in)
            step("photo out", photo_out)
            step("background", background)
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
    load_after = os.getloadavg()

    table = render(results, steps)
    header = (
        f"Measured {time.strftime('%d %B %Y')} on {machine()}, release builds of {git_head()}, "
        f"in {(time.monotonic() - started) / 60:.0f} minutes; load average {load_before[0]:.1f} "
        f"({load_before[1]:.1f} over 5 minutes) before and {load_after[0]:.1f} after, and each row's own while it ran."
    )
    text = header + "\n\n" + table + "\n"
    if failed:
        text += "\nNot measured, because the step failed:\n\n" + "\n".join(f"- {f}" for f in failed) + "\n"
    (out / f"{when}.md").write_text(text)
    (raw / "results.json").write_text(json.dumps(results, indent=2))
    print("\n" + text)
    print(f"perf: written to {out / when}.md, with each run's report and log in {raw}")
    return 1 if failed else 0


def git_head() -> str:
    git = lambda *args: subprocess.run(["git", "-C", str(ROOT), *args], capture_output=True, text=True).stdout.strip()
    changed = git("status", "--porcelain", "--untracked-files=no")
    return git("rev-parse", "--short", "HEAD") + (" with changes not yet committed" if changed else "")


def render(r: dict, steps: dict) -> str:
    rows = []

    def row(budget: str, target: str, measured: str, how: str, step: str, met: bool | None):
        verdict = "–" if met is None else "met" if met else "**missed**"
        load = steps[step].load() if step in steps else "–"
        rows.append(f"| {budget} | {target} | {measured} | {how} | {load} | {verdict} |")

    def at_most(value, limit):
        return None if value is None else value <= limit

    flood, history, steady = r.get("flood", {}), r.get("history", {}), r.get("steady", {})
    one, groups = r.get("catch-up, one at a time", {}), r.get("catch-up, in groups", {})
    demo_steady = steady.get("demo", {}).get("steady", {})
    idle = steady.get("demo", {}).get("idle", {})

    # The demo engine's flood and grouped catch-up hand over IN_FLIGHT messages before their acks
    # come, as no engine does today: jaw-engine waits for each, so a real burst drains at the
    # one-at-a-time catch-up's rate.
    grouped = f"the demo engine's {IN_FLIGHT} waiting for their acks at once; jaw-engine waits for each"
    keys = flood.get("jaw", {}).get("keys")
    row("A keypress drawn", "16 ms p99 while 2,000 messages a second arrive", timing(keys),
        f"jaw attached to `jawd demo --flood {FLOOD}`, {flood.get('demo', {}).get('achieved', 0):,.0f} a second achieved with {grouped}", "flood", at_most(p(keys), 16))
    for name, label in (("history", "while the history goes in"), ("catch-up, in groups", "while 30,000 messages catch up")):
        keys = r.get(name, {}).get("jaw", {}).get("keys")
        if keys:
            row("", label, timing(keys), "jaw attached", name, at_most(p(keys), 16))

    launched = [l for l in steady.get("launches", []) if l.get("launched_ms") is not None]
    if launched:
        launches = sorted(l["launched_ms"] for l in launched)
        mains = sorted(l["first_frame_ms"] for l in launched)
        worst, median = launches[-1], launches[len(launches) // 2]
        row("jaw's first frame", "50 ms after launch, with jawd running", f"{ms(worst)} at worst, {ms(median)} median ({len(launches)} launches)",
            "from the kernel's start of the process to the first frame with a conversation, on the million-message jawd", "steady", worst <= 50)
        row("", "", f"{ms(mains[-1])} at worst, {ms(mains[len(mains) // 2])} median",
            "the same from jaw's main, after the system has started it", "steady", mains[-1] <= 50)

    opens = steady.get("walk", {}).get("opens")
    row("Opening a chat", "30 ms", timing(opens), "jaw walking the chat list, from the key to the chat's messages drawn", "steady", at_most(p(opens), 30))
    answer = demo_steady.get("open")
    if answer:
        row("", "", timing(answer), "jawd's answer to messages.watch, 100 rows, a third of them the biggest chats", "steady", at_most(p(answer), 30))
    flood_opens = flood.get("jaw", {}).get("opens")
    if flood_opens and flood_opens.get("n"):
        row("", "during the flood", timing(flood_opens), "jaw's keys opening the next chat and back", "flood", at_most(p(flood_opens), 30))

    alone = r.get("jaw alone", {})
    if alone:
        row("jaw's memory", "under 40 MB with a screen of images", f"{alone['pictures']['peak_mb']:.0f} MB",
            "jaw's own account, the library's grid of photos in kitty's protocol, 2,000 messages a second", "jaw alone", alone["pictures"]["peak_mb"] < 40)
    if flood.get("jaw"):
        row("", "text", f"{flood['jaw']['peak_mb']:.0f} MB", "jaw attached to the flood, at most", "flood", flood["jaw"]["peak_mb"] < 40)

    lone = demo_steady.get("lone_ack")
    row("An incoming message durable and acknowledged", "10 ms p99", timing(lone), "lone messages through the demo engine, each acked after its synced commit", "steady", at_most(p(lone), 10))
    if "link, Go engine" in r:
        trip = r["link, Go engine"]["trip"]
        row("", "", f"{trip[1] / 1000:.1f} ms p99, {trip[0] / 1000:.1f} ms p50", f"`jawd bench link --commit` through jaw-engine (Go), {TRIPS:,} one at a time", "link, Go engine", trip[1] / 1000 <= 10)
    history_ack = history.get("demo", {}).get("ack")
    if history_ack and history_ack.get("n"):
        row("", "while the history goes in", f"{timing(history_ack)}, {ms(history_ack['max'])} at worst",
            "a live message every 100 ms through the demo engine during the full history sync", "history", at_most(p(history_ack), 10))
    one_ack = one.get("demo", {}).get("ack")
    if one_ack and one_ack.get("n"):
        row("", "in a catch-up", f"{timing(one_ack)}, {ms(one_ack['max'])} at worst",
            "the backlog acked one at a time, as jaw-engine acks it", "catch-up, one at a time", at_most(p(one_ack), 10))
    if "photo out" in r:
        got = r["photo out"]
        during, between = got["acks_during"], got["acks_between"]
        shown, shown_between = got["shown_during"], got["shown_between"]
        row("", "while photos of yours are sent", f"{during[1]:.1f} ms p99, {during[0]:.1f} ms p50, {during[2]:.1f} ms at worst",
            f"`jawd bench media-out`: a text through the demo engine every 50 ms, {during[3]:.0f} of them while {PHOTOS_OUT} 12-megapixel photos "
            f"went one at a time in SD and HD; {between[1]:.1f} ms p99 and {between[0]:.1f} ms p50 for the {between[3]:.0f} in the quiet spells "
            f"between them. Shown to the interface, from the engine handing it over, at {shown[1]:.0f} ms p99 while a photo went and "
            f"{shown_between[1]:.0f} ms between", "photo out", during[1] <= 10)
    ack = flood.get("demo", {}).get("ack")
    if ack:
        row("", "during the flood", timing(ack), f"each acked after its group's synced commit, with {grouped}", "flood", at_most(p(ack), 10))
    if one.get("demo"):
        rate = one["demo"]["rate"]
        row("", "a backlog drains at 200 a second, each ack waiting for its own flush", f"{rate:,.0f} a second",
            f"{one['demo']['messages']:,} messages one at a time, as jaw-engine acks them, so as a real account's backlog drains", "catch-up, one at a time", rate >= 200)
    if groups.get("demo"):
        rate = groups["demo"]["rate"]
        row("", "5,000 a second or more once acks are released in groups", f"{rate:,.0f} a second",
            f"{groups['demo']['messages']:,} messages with {grouped}", "catch-up, in groups", rate >= 5000)

    commit = demo_steady.get("commit_to_interface")
    row("An incoming message on its way to interfaces, once committed", "5 ms p99", timing(commit), "lone messages, from the conversion's commit to the demo's interface", "steady", at_most(p(commit), 5))
    if "feed" in r:
        lone_feed, burst = r["feed"]["lone"], r["feed"]["burst"]
        row("", "", f"{ms(lone_feed[1])} p99, {ms(lone_feed[0])} p50", "`jawd bench feed`, lone messages after the burst", "feed", lone_feed[1] <= 5)
        row("", "in a burst", f"{ms(burst[1])} p99, {ms(burst[0])} p50", "`jawd bench feed` at 2,000 a second, coalesced up to 50 ms by design", "feed", None)
    flood_commit = flood.get("demo", {}).get("commit_to_interface")
    if flood_commit:
        row("", "during the flood", timing(flood_commit), "the open chat's messages, coalesced up to 50 ms by design", "flood", None)
        lag = flood["demo"].get("converted_after_ms", 0) / 1000
        row("", "", f"{timing(flood['demo']['arrival'])} from hand-over",
            f"the conversions, after the inbox's commits, ended {lag:.1f} s after the last ack, with {grouped}", "flood", None)

    if "link, demo engine" in r:
        d = r["link, demo engine"]
        own, outside, inside = d["own"]
        row("jawd's own work in the commit-acked round trip", "under 200 µs p50, apart from the flush", f"about {own:.0f} µs p50",
            f"through the demo engine, in jawd: the round trip ({d['trip'][0]:.0f} µs p50) less the inbox's transaction ({d['transaction'][0]:.0f} µs), {outside:.0f} µs, "
            f"and that transaction's own work without its flush, {inside:.0f} µs; the drive's flush alone is {d['flush'][0]:.0f} µs p50", "link, demo engine", own < 200)
    if "link, Go engine" in r:
        go = r["link, Go engine"]
        own, outside, inside = go["own"]
        row("", "", f"about {own:.0f} µs p50 with the engine's hop",
            f"the same through jaw-engine (Go): round trip {go['trip'][0]:.0f} µs p50, transaction {go['transaction'][0]:.0f} µs; the link alone, acked unstored, {go['alone'][0]:.0f} µs p50", "link, Go engine", None)

    pages = [(what, demo_steady.get(f"page_{what}")) for what in ("newest", "middle", "oldest")]
    worst = max((p(s) for _, s in pages if p(s) is not None), default=None)
    if worst is not None:
        detail = ", ".join(f"{what} {ms(p(s))}" for what, s in pages)
        row("A 100-message page", "5 ms at any depth", f"{ms(worst)} p99 at worst ({detail})", "jawd's answer on the socket, the biggest chat (100,000 messages)", "steady", worst <= 5)
    if "store" in r:
        every = r["store"]["every_page"]
        row("", "", f"{ms(every[1])} p99, {ms(every[0])} p50", "every page of the biggest chat through the read pool, `jawd bench store`", "store", every[1] <= 5)

    finds = demo_steady.get("find")
    if finds:
        row("Search", "50 ms over a million messages", timing(finds), "the palette's find, messages only, six queries", "steady", at_most(p(finds), 50))
    if "store" in r:
        worst = max(s[1] for s in r["store"]["searches"])
        row("", "", f"{ms(worst)} p99 at worst", "`jawd bench store`'s three searches through the read pool", "store", worst <= 50)

    if history.get("demo"):
        h = history["demo"]
        peak = max(h["peak_mb"], h.get("lifetime_peak_mb", 0))
        after = f", {h['after_mb']:.0f} MB once it's in" if "after_mb" in h else ""
        row("jawd's memory", "250 MB during a full history sync", f"{peak:.0f} MB at most{after}",
            f"{h['messages']:,} messages in blobs of 20,000, stored in {h['stored_seconds']:.0f} s ({h['rate']:,.0f} a second), indexed for search in {h['indexed_seconds']:.0f} s", "history", peak < 250)
    if demo_steady:
        row("", "100 MB steady", f"{idle.get('footprint_mb', demo_steady['footprint_mb']):.0f} MB",
            "a fresh jawd on the million messages after lone messages, opens, pages and finds, then a quiet minute", "steady", idle.get("footprint_mb", demo_steady["footprint_mb"]) < 100)
    if flood.get("demo"):
        peak = max(flood["demo"]["peak_mb"], flood["demo"].get("lifetime_peak_mb", 0))
        row("", "during the flood", f"{peak:.0f} MB at most", "", "flood", peak < 100)

    if "link, Go engine" in r:
        go = r["link, Go engine"]
        row("jaw-engine's memory", "under 80 MB steady", f"{go['rss_first'] / 1e6:.0f} MB at start, {go['rss_most'] / 1e6:.0f} MB at most",
            f"jaw-engine unlinked (no account, so nothing reaches WhatsApp) through {TRIPS + 20_000:,} round trips; resident memory", "link, Go engine", go["rss_most"] < 80e6)
        row("", "200 MB during a full history sync", "not measured", "it needs a linked account; see the notes", "link, Go engine", None)

    phases = (
        ("history", "during the history"),
        ("flood", "during the flood"),
        ("catch-up, one at a time", "in the catch-up one at a time"),
        ("catch-up, in groups", "in the catch-up in groups"),
    )
    walls = [(name, label, r.get(name, {}).get("demo", {})) for name, label in phases]
    for n, (name, label, d) in enumerate((w for w in walls if "wal_peak_mib" in w[2])):
        budget, target = ("The WAL", f"under {WAL_LIMIT} MiB, as Data integrity bounds it") if n == 0 else ("", "")
        row(budget, target, f"{d['wal_peak_mib']:.0f} MiB {label}", "jaw.db-wal's size on disk at most, ten times a second", name, d["wal_peak_mib"] <= WAL_LIMIT)

    if "background" in r:
        steps = r["background"]["steps"]
        n, over, most = sum(s[1] for s in steps), sum(s[4] for s in steps), max(s[3] for s in steps)
        regrouping, letting_go = (" and ".join(ms(s[2]) for s in steps if s[0] == what) for what in ("regrouping", "letting go"))
        row("A background transaction", "3 ms, commit included, as Storage caps it", f"{ms(most)} at most, {over} of {n:,} at 3 ms or more",
            f"the outbox's steps of regrouping and letting go in a group of 1,024, each step at its quickest of three runs, from their "
            f"tests in a release build, which allow one in thirty at 3 ms; the medians {regrouping} regrouping and {letting_go} "
            f"letting go, with one member leaving and with 1,000", "background", over == 0)

    if "photo" in r:
        for n, (name, size, took) in enumerate(r["photo"]["photos"]):
            budget, target = ("A 12-megapixel photo ready to upload", "300 ms") if n == 0 else ("", "")
            row(budget, target, f"{took} ms", f"{name}, {size}, to SD and HD as planned, `media-bench`", "photo", took <= 300)
    if "photo in" in r:
        got = r["photo in"]
        width, height, size = got["photo"]
        small, thumbnail, file = got["small"], got["thumbnail"], got["file"]
        budget = "" if "photo" in r else "A 12-megapixel photo ready to upload"
        row(budget, "arriving, to its 256 px preview", f"{small[0]:.0f} ms p50, {small[1]:.0f} ms at worst ({PHOTOS_IN} photos)",
            f"`jawd bench media`: a {width:.0f}x{height:.0f} photo ({size} MB) handed over by the demo engine, to the push whose row has its "
            f"256 px preview, through the inbox's synced commit, the download into the cache and jaw-media; its thumbnail came at "
            f"{thumbnail[0]:.0f} ms p50 and its file at {file[0]:.0f} ms", "photo in", small[0] <= 300)
    if "photo out" in r:
        got = r["photo out"]
        width, height, size = got["photo"]
        answer, ready, sent = got["answer"], got["ready"], got["sent"]
        row("", "sending, ready to upload", f"{ready[0]:.0f} ms p50, {ready[2]:.0f} ms at worst ({PHOTOS_OUT} photos)",
            f"`jawd bench media-out`: a {width:.0f}x{height:.0f} photo ({size} MB) of yours sent with `send.media` on the socket, in SD and HD, "
            f"to the push whose row shows its copy made and in the cache; `send.media` answered in {answer[0]:.0f} ms p50, "
            f"and the row said sent at {sent[0]:.0f} ms, uploaded by the demo engine", "photo out", ready[0] <= 300)

    if idle:
        row("Idle", "no drawing, no wake-ups, no polling", f"jawd {idle['wakeups_per_minute']:.1f} wake-ups a minute, {idle['pushes']} pushes",
            f"a quiet minute with an interface watching; {idle['cpu_ms']:.0f} ms of CPU", "steady", idle["wakeups"] == 0)
    quiet = steady.get("idle")
    if quiet:
        row("", "", f"jaw {quiet['frames_measured']} frames, {quiet['wakeups_per_minute']:.1f} wake-ups a minute", "jaw attached to the quiet jawd", "steady", quiet["frames_measured"] <= 1)

    head = "| Budget | Target | Measured | How | Load | |\n| --- | --- | --- | --- | --- | --- |"
    return head + "\n" + "\n".join(rows)


if __name__ == "__main__":
    sys.exit(main())
