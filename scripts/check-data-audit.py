#!/usr/bin/env python3
"""Checks that every commit touching jaw's data layer names an approved audit.

    check-data-audit.py [BASE] [HEAD]

Checks the commits in BASE..HEAD (origin/main..HEAD by default). A commit that
touches a path below needs the trailer `Data-Audit: docs/audits/<file>.md`, and
that file must exist at HEAD and end its verdict with `Verdict: approved`.
"""

import re
import subprocess
import sys

DATA_PATHS = (
    "crates/jawd/src/store/",
    "crates/jawd/src/sync",
    "crates/jawd/src/feed/",
    "crates/jawd/src/find",
    "crates/jawd/src/send",
    "crates/jawd/src/engine/",
    "engine/internal/wa/",
    "proto/engine/",
    "crates/jaw-voice/src/record/",
    "crates/jaw-voice/src/lib.rs",
    "crates/jaw-voice/src/tests.rs",
    "crates/jaw-media/src/file.rs",
    "crates/jaw-media/src/prepare.rs",
    "crates/jaw-media/src/preview.rs",
    "crates/jaw-media/src/run.rs",
    "crates/jaw-media/src/job.rs",
)

NO_COMMIT = re.compile(r"^0+$")


def git(*args: str) -> str:
    return subprocess.run(["git", *args], check=True, capture_output=True, text=True).stdout


def exists(rev: str) -> bool:
    return subprocess.run(["git", "cat-file", "-e", f"{rev}^{{commit}}"], capture_output=True).returncode == 0


def commits(base: str, head: str) -> list[str]:
    if NO_COMMIT.match(base) or not exists(base):
        # A new branch, or a base CI can't see: check the head commit alone.
        return [git("rev-parse", head).strip()]
    return git("rev-list", "--reverse", f"{base}..{head}").split()


def touched(commit: str) -> list[str]:
    files = git("diff-tree", "--no-commit-id", "--name-only", "-r", "--root", "-m", commit).split()
    return sorted({f for f in files if f.startswith(DATA_PATHS)})


def audits(commit: str) -> list[str]:
    out = git("log", "-1", "--format=%(trailers:key=Data-Audit,valueonly)", commit)
    return [line.strip() for line in out.splitlines() if line.strip()]


def approved(head: str, path: str) -> bool:
    try:
        note = git("show", f"{head}:{path}")
    except subprocess.CalledProcessError:
        return False
    verdicts = [line.strip() for line in note.splitlines() if line.strip().startswith("Verdict:")]
    return bool(verdicts) and verdicts[-1] == "Verdict: approved"


def main() -> int:
    base = sys.argv[1] if len(sys.argv) > 1 else "origin/main"
    head = sys.argv[2] if len(sys.argv) > 2 else "HEAD"
    failures = []
    for commit in commits(base, head):
        files = touched(commit)
        if not files:
            continue
        subject = git("log", "-1", "--format=%h %s", commit).strip()
        named = audits(commit)
        if not named:
            failures.append(f"{subject[:100]}\n  touches the data layer ({', '.join(files[:4])}) but has no Data-Audit trailer")
            continue
        for path in named:
            if not approved(head, path):
                failures.append(f"{subject[:100]}\n  names {path}, which isn't at {head} or doesn't end in 'Verdict: approved'")
    if failures:
        print("The data layer changed without an approved audit:\n")
        print("\n\n".join(failures))
        print("\nSee .cursor/rules/data-integrity.mdc: have the database expert audit the change, commit its note, and add the trailer.")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
