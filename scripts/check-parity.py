#!/usr/bin/env python3
"""Checks that the Mac keeps up with the terminal, request by request.

    check-parity.py [--terminal REF] [--mac REF] [--jawd REF] [--table]

Reads which of jawd's requests each interface sends and which of its pushes it reads, from its
source (the working tree, or REF): the terminal's in crates/jaw and crates/jaw-client, the Mac's in
mac/, and which requests jawd serves, in crates/jawd. Every request or push the terminal uses and
the Mac doesn't must be in mac/parity.toml, with the step of the parity plan
(docs/plans/2026-10-27-jaw-mac-parity-plan.md) that brings it, or among those not for the Mac with
why. The check fails on a gap the list doesn't have, on one the Mac has closed that it still holds,
and on a step the plan doesn't have.

--terminal <branch> reads a terminal branch, so what's being built there is queued before it
lands. --table prints every request and push with each side's use of it.
"""

import argparse
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LEDGER = "mac/parity.toml"
PLAN = "docs/plans/2026-10-27-jaw-mac-parity-plan.md"
PROTOCOL = "protocol/README.md"
FIXTURES = "protocol/fixtures/"

SIDES = {
    "terminal": ("crates/jaw/src/", "crates/jaw-client/src/"),
    "mac": ("mac/JawKit/Sources/", "mac/JawUI/Sources/", "mac/JawApp/Sources/"),
    "jawd": ("crates/jawd/src/",),
}
# Tests, benches and the terminal's made-up account (which answers requests as jawd would) aren't
# what an interface sends.
SKIPPED = re.compile(r"(/tests?/|/Tests/|tests?\.rs$|_tests\.rs$|bench|/demo\.rs$|/demo/)")

NAME = r"[a-z_]+(?:\.[a-z_]+)?"
SENDS = {
    "terminal": re.compile(r'(?:\.(?:ask|act|owe|call|request)\(|\bencode\(\s*\w+\s*,)\s*"(' + NAME + ')"'),
    "mac": re.compile(r'(?:\b(?:ask|chatAction|send|request)\(\s*|\bmethod:\s*)"(' + NAME + ')"'),
}
READS = {
    "terminal": re.compile(r'method\s*==\s*"(' + NAME + r')"|"(' + NAME + r')"\)*\s*=>'),
    "mac": re.compile(r'\bcase\s+"(' + NAME + ')"'),
}
# A match arm on the method's name; `method == "x" ||` only routes a request, it doesn't serve it.
SERVES = re.compile(r'"(' + NAME + r')"\s*(?:=>|\|(?!\|))')
STEP = re.compile(r"^### (\d+\.\d+) (.+)$", re.M)


def git(*args: str, data: bytes | None = None) -> bytes:
    return subprocess.run(["git", "-C", str(ROOT), *args], check=True, capture_output=True, input=data).stdout


def sources(ref: str | None, prefixes: tuple[str, ...]) -> dict[str, str]:
    """Each source file under the prefixes, at REF or in the working tree, by path."""
    if ref is None:
        paths = [str(p.relative_to(ROOT)) for prefix in prefixes for p in sorted((ROOT / prefix).rglob("*")) if p.is_file()]
        texts = {path: (ROOT / path).read_text(errors="replace") for path in paths}
    else:
        paths = git("ls-tree", "-r", "--name-only", ref, "--", *prefixes).decode().split()
        texts = dict(zip(paths, batch(ref, paths)))
    return {path: text for path, text in texts.items() if path.endswith((".rs", ".swift")) and not SKIPPED.search(path)}


def batch(ref: str, paths: list[str]) -> list[str]:
    out = git("cat-file", "--batch", data="".join(f"{ref}:{p}\n" for p in paths).encode())
    texts, at = [], 0
    for _ in paths:
        header_end = out.index(b"\n", at)
        size = int(out[at:header_end].split()[2])
        texts.append(out[header_end + 1 : header_end + 1 + size].decode(errors="replace"))
        at = header_end + 1 + size + 1
    return texts


def text(ref: str | None, path: str) -> str:
    if ref is None:
        return (ROOT / path).read_text()
    return git("show", f"{ref}:{path}").decode()


def protocol(refs: set[str | None]) -> tuple[set[str], set[str]]:
    """The requests and pushes the protocol names, at any of the refs read: its Methods and Other
    pushes tables, the windows' pushes, and every fixture."""
    methods, pushes = set(), set()
    for ref in refs:
        readme = text(ref, PROTOCOL)
        methods |= set(re.findall(r"`(" + NAME + ")`", section(readme, "## Methods", first_column=True)))
        pushes |= set(re.findall(r"`(" + NAME + ")`", section(readme, "## Other pushes", first_column=True)))
        pushes |= set(re.findall(r"`(\w+)`", section(readme, "### Pushes for windows").split("each naming")[0]))
        if ref is None:
            fixtures = [p.name for p in (ROOT / FIXTURES).iterdir()]
        else:
            fixtures = git("ls-tree", "--name-only", ref, FIXTURES).decode().split()
        for fixture in fixtures:
            name = Path(fixture).name
            if name.endswith(".request.json"):
                methods.add(name.removesuffix(".request.json"))
            elif name.endswith(".push.json"):
                pushes.add(name.removesuffix(".push.json"))
    return methods, pushes


def section(markdown: str, heading: str, first_column: bool = False) -> str:
    start = markdown.find(heading + "\n")
    if start < 0:
        return ""
    level = heading.split(" ")[0]
    rest = markdown[start + len(heading) + 1 :]
    end = re.search(r"^#{1," + str(len(level)) + r"} ", rest, re.M)
    body = rest[: end.start()] if end else rest
    if first_column:
        return "\n".join(line.split("|")[1] for line in body.splitlines() if line.startswith("| `"))
    return body


def used(side: str, ref: str | None, methods: set[str], pushes: set[str]) -> tuple[set[str], set[str]]:
    files = sources(ref, SIDES[side])
    sent, read = set(), set()
    for body in files.values():
        sent |= {m for m in SENDS[side].findall(body) if m in methods}
        read |= {a or b for a, b in pair(READS[side].findall(body)) if (a or b) in pushes}
    return sent, read


def pair(found: list) -> list[tuple[str, str]]:
    return [(f, "") if isinstance(f, str) else f for f in found]


def served(ref: str | None, methods: set[str]) -> set[str]:
    files = sources(ref, SIDES["jawd"])
    return {m for body in files.values() if "params" in body for m in SERVES.findall(body) if m in methods}


def gaps(ledger: dict) -> dict[tuple[str, str], tuple[str, str | None]]:
    """The list's gaps by kind and name: each its step, and the branch it comes from when the
    protocol on main doesn't name it yet."""
    out = {}
    for kind, table in (("request", "requests"), ("push", "pushes")):
        for name, entry in ledger.get(table, {}).items():
            out[(kind, name)] = (entry, None) if isinstance(entry, str) else (entry["step"], entry.get("from"))
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    for side in ("terminal", "mac", "jawd"):
        parser.add_argument(f"--{side}", metavar="REF", help=f"read the {side} at REF rather than the working tree")
    parser.add_argument("--table", action="store_true", help="print every request and push with each side's use")
    args = parser.parse_args()

    methods, pushes = protocol({args.terminal, args.mac, args.jawd})
    t_sent, t_read = used("terminal", args.terminal, methods, pushes)
    m_sent, m_read = used("mac", args.mac, methods, pushes)
    sides = {"request": (t_sent, m_sent, methods), "push": (t_read, m_read, pushes)}
    serving = served(args.jawd, methods)
    ledger = tomllib.loads(text(None, LEDGER))
    listed = gaps(ledger)
    not_mac = ledger.get("not_for_the_mac", {})
    steps = dict(STEP.findall(text(None, PLAN)))

    failures, ahead = [], []
    for kind, (terminal, mac, known) in sides.items():
        for name in sorted(terminal - mac):
            if (kind, name) not in listed and name not in not_mac:
                failures.append(f"the terminal {verb(kind)} {name}, which the Mac doesn't: give it a step in {LEDGER}")
    for (kind, name), (step, branch) in sorted(listed.items()):
        terminal, mac, known = sides[kind]
        if step not in steps:
            failures.append(f"{LEDGER} gives {name} step {step}, which {PLAN} doesn't have")
        if name not in known and not branch:
            failures.append(f"{LEDGER} lists the {kind} {name}, which the protocol doesn't name; a branch's says `from`")
        elif name in mac:
            failures.append(f"the Mac {verb(kind)} {name} now: take it out of {LEDGER} (step {step})")
        elif name not in terminal:
            ahead.append(name)

    if args.table:
        print(f"{'':28} {'terminal':9} {'Mac':9} {'jawd':9} step")
        for kind, (terminal, mac, known) in sides.items():
            for name in sorted(known):
                step = listed.get((kind, name), ("not the Mac's" if name in not_mac else "", None))[0]
                serves = ("serves" if name in serving else "") if kind == "request" else ""
                print(f"{kind[0]} {name:26} {mark(name, terminal):9} {mark(name, mac):9} {serves:9} {step}")
        print()

    print(f"{len(listed)} requests and pushes the Mac doesn't use yet, by the step of the parity plan that brings them", end="")
    print(f" ({len(ahead)} before the terminal at this ref uses them):" if ahead else ":")
    by_step: dict[str, list[str]] = {}
    for (kind, name), (step, branch) in listed.items():
        label = name + (" push" if kind == "push" else "") + (f" (from {branch})" if branch and name not in sides[kind][2] else "")
        by_step.setdefault(step, []).append(label)
    for step in sorted(by_step, key=lambda s: tuple(int(p) for p in s.split("."))):
        waiting = sorted(n for (k, n), (s, _) in listed.items() if s == step and k == "request" and n not in serving)
        tail = f"; jawd doesn't serve {', '.join(waiting)} on this ref yet" if waiting else ""
        print(f"  {step} {steps.get(step, '(not in the plan)')}: {', '.join(sorted(by_step[step]))}{tail}")
    mac_only = sorted((sides["request"][1] - sides["request"][0]) | (sides["push"][1] - sides["push"][0]))
    if mac_only:
        print(f"The Mac uses what the terminal doesn't: {', '.join(mac_only)}.")
    for failure in failures:
        print(f"check-parity: {failure}", file=sys.stderr)
    return 1 if failures else 0


def verb(kind: str) -> str:
    return "sends" if kind == "request" else "reads the push"


def mark(name: str, used_by: set[str]) -> str:
    return "uses" if name in used_by else ""


if __name__ == "__main__":
    sys.exit(main())
