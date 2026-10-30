#!/usr/bin/env python3
"""Times folders and the roster on `jawd demo`'s made-up account of a million messages in 3,000
chats, through the real socket, as an interface would.

    folders-bench.py [--jawd target/release/jawd] [--folder DIR] [--rate 2000] [--seconds 20]

It builds the account in DIR the first time (jawd demo's history), then serves it twice with a
flood of `rate` messages a second: first as it is, then with 30 folders of yours (ten at the top,
each with two inside it) holding 600 chats and people, three favourites, and a client watching the
tree, a big folder's chats, Unread, the Waiting on me smart folder and two rosters. It reports how
long each folder method and each watch took to answer, how many pushes each window got a second
during the flood, the flood's acks as jawd demo measures them either way, and, once jawd has
stopped, whether every folder's counts are still what they count, read from jaw.db on its own.
Every timing is printed with the machine's load averages. DIR is a temporary folder unless given,
and nothing outside it is touched; only the jawd it starts is stopped.
"""

import argparse
import json
import os
import random
import re
import socket
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# The protocol jawd speaks, as jaw-proto says it, so the bench's hello never falls behind it.
PROTOCOL = int(re.search(r"pub const PROTOCOL: u32 = (\d+);", (ROOT / "crates/jaw-proto/src/lib.rs").read_text()).group(1))


def load() -> str:
    one, five, fifteen = os.getloadavg()
    return f"load {one:.1f} {five:.1f} {fifteen:.1f}"


def pct(values, p):
    if not values:
        return float("nan")
    values = sorted(values)
    return values[min(len(values) - 1, int(round(p / 100 * (len(values) - 1))))]


def said(what, ms):
    return f"{what}: p50 {pct(ms, 50):.2f} ms, p99 {pct(ms, 99):.2f} ms, most {max(ms):.2f} ms over {len(ms)} ({load()})"


def phone(n):
    return f"3519{n:08d}@s.whatsapp.net"


def chat_id(n):
    return f"120363{n:012d}@g.us" if n % 5 == 2 else phone(n)


class Client:
    """One connection: requests answered by id, pushes counted by watch with when they came."""

    def __init__(self, path: Path):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(str(path))
        self.file = self.sock.makefile("rb")
        self.next = 1
        self.waiting = {}
        self.pushes = {}
        self.lock = threading.Lock()
        threading.Thread(target=self.read, daemon=True).start()
        self.call("hello", {"protocol": PROTOCOL, "client": "folders-bench", "version": "0", "focus": True})

    def read(self):
        for line in self.file:
            message = json.loads(line)
            if "id" in message:
                with self.lock:
                    slot = self.waiting.pop(message["id"], None)
                if slot:
                    slot["answer"] = message
                    slot["at"] = time.perf_counter()
                    slot["done"].set()
            else:
                watch = message.get("params", {}).get("watch")
                with self.lock:
                    self.pushes.setdefault(watch, []).append(time.perf_counter())

    def call(self, method, params):
        with self.lock:
            id = self.next
            self.next += 1
            slot = {"done": threading.Event()}
            self.waiting[id] = slot
        began = time.perf_counter()
        self.sock.sendall((json.dumps({"id": id, "method": method, "params": params}) + "\n").encode())
        if not slot["done"].wait(60):
            raise RuntimeError(f"{method} wasn't answered in a minute")
        answer = slot["answer"]
        if "error" in answer:
            raise RuntimeError(f"{method} {params}: {answer['error']}")
        return answer["result"], (slot["at"] - began) * 1000


def demo(jawd, folder, *args):
    command = [str(jawd), "demo", "--folder", str(folder), "--exit", *args]
    return subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)


def listening(folder, process, seconds=1800):
    path = folder / "jaw.sock"
    until = time.time() + seconds
    while time.time() < until:
        if process.poll() is not None:
            raise RuntimeError("jawd demo stopped:\n" + process.stdout.read())
        try:
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
                s.connect(str(path))
            return path
        except OSError:
            time.sleep(0.2)
    raise RuntimeError("jawd demo didn't listen")


def finish(process, report):
    out, _ = process.communicate(timeout=1800)
    if process.returncode != 0:
        raise RuntimeError(f"jawd demo failed:\n{out}")
    phases = json.loads(Path(report).read_text()) if Path(report).exists() else {}
    return out, phases


def windows(client, big):
    """Watches each window five times over, timing each watch, and leaves one of each watched."""
    asks = [
        ("the tree", "folders.watch", {}),
        ("a big folder's chats, 100", "chats.watch", {"folder": big, "around": "top", "after": 100}),
        ("Unread, 100", "chats.watch", {"folder": "unread", "around": "top", "after": 100}),
        ("Waiting on me, 100", "chats.watch", {"folder": "smart:6", "around": "top", "after": 100}),
        ("All, 100", "chats.watch", {"folder": "all", "around": "top", "after": 100}),
        ("the roster, 200", "roster.watch", {"folder": "all", "around": "top", "after": 200}),
        ("a big folder's roster, 200", "roster.watch", {"folder": big, "around": "top", "after": 200}),
    ]
    kept = {}
    for name, method, params in asks:
        times = []
        for _ in range(5):
            result, ms = client.call(method, params)
            times.append(ms)
            client.call("unwatch", {"watch": result["watch"]})
        result, ms = client.call(method, params)
        times.append(ms)
        kept[name] = (result["watch"], len(result.get("rows", [])))
        print(said(f"  {name}: watched, {kept[name][1]} rows", times))
    return kept


def drift(db_path):
    """Whether each folder's counts are what they count, read from jaw.db on its own, as the
    background check derives them."""
    db = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    wrong = db.execute(
        """WITH reach (folder, chat) AS (
            SELECT p.ancestor, i.chat FROM folder_items i JOIN folder_paths p ON p.folder = i.folder WHERE i.chat IS NOT NULL
            UNION ALL
            SELECT p.ancestor, c.n FROM folder_items i JOIN folder_paths p ON p.folder = i.folder
                JOIN chats c ON c.person = i.person WHERE i.person IS NOT NULL),
        counted (folder, chats, unread, mentions) AS (
            SELECT r.folder, count(DISTINCT r.chat), (SELECT coalesce(sum(c.unread), 0) FROM chats c
                WHERE c.n IN (SELECT chat FROM reach x WHERE x.folder = r.folder)),
                (SELECT coalesce(sum(c.mentions), 0) FROM chats c WHERE c.n IN (SELECT chat FROM reach x WHERE x.folder = r.folder))
            FROM reach r GROUP BY r.folder)
        SELECT count(*) FROM counted d JOIN folder_counts k ON k.folder = d.folder
        WHERE (k.chats, k.unread, k.mentions) IS NOT (d.chats, d.unread, d.mentions)"""
    ).fetchone()[0]
    builtins = db.execute(
        """SELECT count(*) FROM folders f JOIN folder_counts k ON k.folder = f.n WHERE f.builtin IS NOT NULL
        AND (k.chats, k.unread, k.mentions) IS NOT (SELECT count(c.n), coalesce(sum(c.unread), 0), coalesce(sum(c.mentions), 0)
            FROM chats c WHERE CASE f.builtin WHEN 'all' THEN c.archived = 0
                WHEN 'unread' THEN c.archived = 0 AND (c.unread > 0 OR c.marked_unread <> 0)
                WHEN 'groups' THEN c.archived = 0 AND c.kind IN ('group', 'announcement')
                WHEN 'communities' THEN c.archived = 0 AND (c.kind = 'community' OR c.community IS NOT NULL)
                ELSE c.archived <> 0 END)"""
    ).fetchone()[0]
    return wrong, builtins


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--jawd", default=str(ROOT / "target/release/jawd"))
    parser.add_argument("--folder")
    parser.add_argument("--rate", type=int, default=2000)
    parser.add_argument("--seconds", type=float, default=20)
    args = parser.parse_args()
    folder = Path(args.folder) if args.folder else Path(tempfile.mkdtemp(prefix="jaw-folders-bench-"))
    folder.mkdir(parents=True, exist_ok=True)
    jawd = Path(args.jawd)
    print(f"jawd demo in {folder} ({load()})")
    if not (folder / "jaw.db").exists():
        began = time.time()
        process = demo(jawd, folder, "--report", str(folder / "history.json"))
        out, _ = finish(process, folder / "history.json")
        print(f"the account built in {time.time() - began:.0f} s ({load()})")

    flood = ["--flood", str(args.rate), "--seconds", str(args.seconds)]
    # As it is: no folders of yours, nobody else watching.
    report = folder / "bare.json"
    report.unlink(missing_ok=True)
    process = demo(jawd, folder, *flood, "--after", "3", "--report", str(report))
    out, phases = finish(process, report)
    bare = phases.get("flood", {})
    print(f"flood without folders: {bare.get('achieved', 0):.0f} a second, acks {bare.get('ack')}, ({load()})")

    # With folders, favourites and windows on them.
    report = folder / "folders.json"
    report.unlink(missing_ok=True)
    process = demo(jawd, folder, *flood, "--after", "60", "--report", str(report))
    path = listening(folder, process)
    client = Client(path)
    rng = random.Random(6)
    run = time.strftime("%H%M%S")
    made, filed = [], []
    tops = []
    for t in range(10):
        result, ms = client.call("folder.create", {"name": f"Bench {t} at {run}"})
        tops.append(result["folder"]["id"])
        made.append(ms)
        for s in range(2):
            result, ms = client.call("folder.create", {"name": f"Inside {s}", "parent": tops[-1]})
            made.append(ms)
            tops.append(result["folder"]["id"])
    print(said("folder.create", made))
    folders = tops
    big = folders[0]
    for i in range(600):
        n = rng.randrange(3000)
        # The big folder and its two inside take a third of everything, some of it twice.
        target = folders[rng.randrange(3)] if i % 3 == 0 else rng.choice(folders)
        try:
            _, ms = client.call("folder.add", {"folder": target, "item": chat_id(n)})
            filed.append(ms)
        except RuntimeError:
            pass
    print(said("folder.add", filed))
    for slot, target in [(1, big), (2, folders[3]), (3, chat_id(1))]:
        client.call("favourite.set", {"slot": slot, "target": target})
    moved = []
    for _ in range(10):
        _, ms = client.call("folder.move", {"folder": folders[3], "parent": folders[0]})
        moved.append(ms)
        _, ms = client.call("folder.move", {"folder": folders[3], "parent": None})
        moved.append(ms)
    print(said("folder.move, a folder and what's in it", moved))
    tree, _ = client.call("folders.watch", {})
    biggest = max((row for row in tree["rows"] if row["id"] == big), key=lambda row: row["count"])
    print(f"  the big folder holds {biggest['count']} chats and people, {biggest['unread']} unread")
    kept = windows(client, big)
    before = {name: len(client.pushes.get(watch, [])) for name, (watch, _) in kept.items()}
    began = time.perf_counter()
    out, phases = finish(process, report)
    took = time.perf_counter() - began
    filled = phases.get("flood", {})
    print(f"flood with folders watched: {filled.get('achieved', 0):.0f} a second, acks {filled.get('ack')}, ({load()})")
    span = max(1, round(filled.get("seconds", args.seconds)))
    for name, (watch, _) in kept.items():
        times = client.pushes.get(watch, [])[before[name] :]
        bins = [0] * (int(took) + 2)
        for t in times:
            bins[min(len(bins) - 1, max(0, int(t - began)))] += 1
        busiest = max(sum(bins[i : i + span]) for i in range(max(1, len(bins) - span + 1)))
        print(
            f"  {name}: {busiest / span:.1f} pushes a second over the flood's {span} s, "
            f"at most {max(bins)} in a second, {len(times)} in all"
        )
    wrong, builtins = drift(folder / "jaw.db")
    print(f"once stopped: {wrong} of your folders' and {builtins} built-in folders' counts differ from what they count")
    print(json.dumps({"bare": bare, "folders": filled, "drift": [wrong, builtins]}, indent=1))


if __name__ == "__main__":
    sys.exit(main())
