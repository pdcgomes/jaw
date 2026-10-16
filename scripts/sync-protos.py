"""Copies the .proto files whatsmeow ships into proto/whatsapp/, from the version engine/go.mod pins,
so jawd parses WhatsApp's protobufs with the definitions the engine was built with.

  python3 scripts/sync-protos.py           # copy them, after changing whatsmeow's version
  python3 scripts/sync-protos.py --check   # fail if proto/whatsapp/ isn't what go.mod pins, as CI does
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ENGINE = ROOT / "engine"
OUT = ROOT / "proto" / "whatsapp"
MODULE = "go.mau.fi/whatsmeow"


def whatsmeow() -> tuple[Path, str]:
    """The pinned whatsmeow's folder in the module cache, downloading it if need be, and its version."""
    subprocess.run(["go", "mod", "download", MODULE], cwd=ENGINE, check=True)
    line = subprocess.run(
        ["go", "list", "-m", "-f", "{{.Dir}} {{.Version}}", MODULE],
        cwd=ENGINE,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    folder, version = line.rsplit(" ", 1)
    return Path(folder), version


def wanted() -> dict[str, bytes]:
    """What proto/whatsapp/ should hold: each .proto file by its path, and VERSION."""
    folder, version = whatsmeow()
    files = {str(p.relative_to(folder / "proto")): p.read_bytes() for p in sorted((folder / "proto").rglob("*.proto"))}
    files["VERSION"] = f"{MODULE} {version}\n".encode()
    return files


def present() -> dict[str, bytes]:
    if not OUT.exists():
        return {}
    return {str(p.relative_to(OUT)): p.read_bytes() for p in sorted(OUT.rglob("*")) if p.is_file()}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="fail if proto/whatsapp/ isn't up to date")
    args = parser.parse_args()
    want, have = wanted(), present()
    if args.check:
        stale = sorted(set(want) ^ set(have) | {p for p in want.keys() & have.keys() if want[p] != have[p]})
        if stale:
            print("proto/whatsapp/ isn't what engine/go.mod pins; run python3 scripts/sync-protos.py:", file=sys.stderr)
            for path in stale:
                print(f"  {path}", file=sys.stderr)
            sys.exit(1)
        print(f"proto/whatsapp/: {len(want) - 1} files, {want['VERSION'].decode().strip()}")
        return
    for path in have.keys() - want.keys():
        (OUT / path).unlink()
    for path, data in want.items():
        target = OUT / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        target.chmod(0o644)
    for folder in sorted((p for p in OUT.rglob("*") if p.is_dir()), reverse=True):
        if not any(folder.iterdir()):
            folder.rmdir()
    print(f"proto/whatsapp/: {len(want) - 1} files, {want['VERSION'].decode().strip()}")


if __name__ == "__main__":
    main()
