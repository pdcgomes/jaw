"""Deletes the object files that debug builds leave in Cargo's target folders once no binary names
them (`make prune-builds`).

    prune-builds.py [--dry-run] [--min-age MINUTES] [TARGET...]

On macOS a debug build keeps each codegen unit's debug info in its object file, beside the binary,
and the binary names the object files it was linked from (split-debuginfo=unpacked, the dev
profile's default there). An incremental rebuild writes its objects under new names and nothing
deletes the old ones, so target/debug/deps fills with objects no binary names: in a worktree whose
jawd tests were rebuilt for two days, 115,066 of its 121,292 objects, 31 GB. This deletes those, and
only those. Every object a binary in the folder still names stays, so every binary keeps its files
and lines, and rustc's incremental cache keeps its own copies, so nothing rebuilds.

TARGET defaults to the target folder of every worktree of this repository. A folder is left alone
while cargo, rustc or the linker runs in its checkout, and an object only goes once nothing has
linked or replaced it for --min-age minutes. Elsewhere than macOS the debug info is inside the
binary, and there is nothing to delete.
"""

import argparse
import ctypes
import functools
import os
import re
import struct
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

MH_MAGIC_64 = b"\xcf\xfa\xed\xfe"
LINKED = {2, 6, 8}  # MH_EXECUTE, MH_DYLIB, MH_BUNDLE
LC_SYMTAB = 0x2
N_OSO = 0x66
NOT_LINKED = (".o", ".rlib", ".rmeta", ".d", ".json", ".lock")
# Build scripts are linked from objects in build/, never from deps/.
SKIPPED = {"incremental", ".fingerprint", "build"}
BUILD_TOOLS = {"cargo", "rustc", "rustdoc", "clippy-driver", "cc", "clang", "ld", "dsymutil"}

# proc_pidinfo's PROC_PIDVNODEPATHINFO: the working folder's vnode_info, then its path.
PROC_PIDVNODEPATHINFO = 9
VNODE_INFO_SIZE = 152
MAXPATHLEN = 1024


@functools.lru_cache(maxsize=None)
def real(folder: str) -> str:
    return os.path.realpath(folder)


def resolved(path: str) -> str:
    folder, name = os.path.split(path)
    return os.path.join(real(folder), name)


def named_objects(path: str) -> set:
    """The object files a linked Mach-O binary names for its debug info (its N_OSO entries)."""
    try:
        with open(path, "rb") as f:
            header = f.read(32)
            if len(header) < 32 or header[:4] != MH_MAGIC_64:
                return set()
            filetype, ncmds, sizeofcmds = struct.unpack_from("<III", header, 12)
            if filetype not in LINKED:
                return set()
            commands = f.read(sizeofcmds)
            at = 0
            for _ in range(ncmds):
                cmd, size = struct.unpack_from("<II", commands, at)
                if cmd == LC_SYMTAB:
                    symoff, nsyms, stroff, strsize = struct.unpack_from("<IIII", commands, at + 8)
                    break
                at += size
            else:
                return set()
            f.seek(symoff)
            symbols = f.read(nsyms * 16)
            f.seek(stroff)
            strings = f.read(strsize)
    except OSError:
        return set()
    named = set()
    types = symbols[4::16]
    i = types.find(N_OSO)
    while i >= 0:
        strx = int.from_bytes(symbols[16 * i : 16 * i + 4], "little")
        name = strings[strx : strings.index(b"\0", strx)].decode(errors="replace")
        # An object inside an rlib is named archive(member), and goes with the rlib.
        if not name.endswith(")"):
            named.add(resolved(name))
        i = types.find(N_OSO, i + 1)
    return named


def walk(target: str):
    for root, dirs, files in os.walk(target):
        dirs[:] = [d for d in dirs if d not in SKIPPED]
        yield root, files


@dataclass
class Plan:
    objects: int = 0
    recent: int = 0
    size: int = 0
    stale: list = field(default_factory=list)


def plan(target: str, min_age: float) -> Plan:
    named = set()
    for root, files in walk(target):
        for name in files:
            if not name.endswith(NOT_LINKED):
                named |= named_objects(os.path.join(root, name))
    p, now, inodes = Plan(), time.time(), {}
    for root, _ in walk(target):
        if os.path.basename(root) != "deps":
            continue
        with os.scandir(root) as entries:
            for entry in entries:
                if not entry.name.endswith(".rcgu.o") or not entry.is_file(follow_symlinks=False):
                    continue
                p.objects += 1
                if resolved(entry.path) in named:
                    continue
                st = entry.stat(follow_symlinks=False)
                if now - st.st_ctime < min_age * 60:
                    p.recent += 1
                    continue
                inode = (st.st_dev, st.st_ino)
                p.stale.append((entry.path, inode))
                links, blocks, names = inodes.get(inode, (st.st_nlink, st.st_blocks, 0))
                inodes[inode] = (links, blocks, names + 1)
    # An object linked from rustc's incremental cache too stays on the disk.
    p.size = sum(blocks * 512 for links, blocks, names in inodes.values() if names >= links)
    return p


def remove(stale: list):
    for path, inode in stale:
        try:
            st = os.lstat(path)
        except FileNotFoundError:
            continue
        # A build since the plan may have put another object under the same name.
        if (st.st_dev, st.st_ino) == inode:
            os.unlink(path)


@functools.lru_cache(maxsize=None)
def libproc():
    lib = ctypes.CDLL("/usr/lib/libproc.dylib")
    lib.proc_pidinfo.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_uint64, ctypes.c_void_p, ctypes.c_int]
    lib.proc_pidinfo.restype = ctypes.c_int
    return lib


def working_folder(pid: int) -> str:
    size = 2 * (VNODE_INFO_SIZE + MAXPATHLEN)
    buf = ctypes.create_string_buffer(size)
    if libproc().proc_pidinfo(pid, PROC_PIDVNODEPATHINFO, 0, buf, size) <= 0:
        return ""
    return buf.raw[VNODE_INFO_SIZE : VNODE_INFO_SIZE + MAXPATHLEN].split(b"\0", 1)[0].decode(errors="replace")


def processes() -> dict:
    out = subprocess.run(["ps", "-axo", "pid=,ppid=,args="], check=True, capture_output=True, text=True).stdout
    procs = {}
    for line in out.splitlines():
        fields = line.split(None, 2)
        if len(fields) == 3:
            procs[int(fields[0])] = (int(fields[1]), fields[2])
    return procs


def busy(checkout: str) -> str:
    """What builds in the checkout, if anything: a build tool working in it, or one that it or a
    shell above it names."""
    home = str(Path.home())
    forms = [checkout] + ([checkout.replace(home, "~", 1)] if checkout.startswith(home + "/") else [])
    names = re.compile("|".join(re.escape(f) + r"(?=[/\s'\";&|)]|$)" for f in forms))
    procs = processes()
    mine, pid = set(), os.getpid()
    while pid in procs and pid not in mine:
        mine.add(pid)
        pid = procs[pid][0]
    for pid, (_, args) in procs.items():
        tool = os.path.basename(args.split(None, 1)[0])
        if pid in mine or tool not in BUILD_TOOLS:
            continue
        cwd = working_folder(pid)
        if cwd == checkout or cwd.startswith(checkout + "/"):
            return f"{tool} (pid {pid}) runs in it"
        up, seen = pid, set()
        while up in procs and up not in seen and up > 1:
            seen.add(up)
            parent, command = procs[up]
            if names.search(command):
                return f"{tool} (pid {pid}) runs for it"
            up = parent
    return ""


def worktree_targets() -> list:
    out = subprocess.run(
        ["git", "-C", str(ROOT), "worktree", "list", "--porcelain"], check=True, capture_output=True, text=True
    ).stdout
    checkouts = [line[len("worktree ") :] for line in out.splitlines() if line.startswith("worktree ")]
    return [os.path.join(c, "target") for c in checkouts if os.path.isdir(os.path.join(c, "target"))]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("targets", nargs="*", metavar="TARGET", help="Cargo target folders (every worktree's by default)")
    ap.add_argument("--dry-run", action="store_true", help="count what would go, delete nothing")
    ap.add_argument(
        "--min-age", type=float, default=60, metavar="MINUTES", help="how long an object goes untouched before it's deleted"
    )
    args = ap.parse_args()
    if sys.platform != "darwin":
        print("Nothing to delete: only macOS keeps debug info in object files beside the binary.")
        return 0

    done = "to delete" if args.dry_run else "deleted"
    total = 0
    for target in [os.path.realpath(t) for t in args.targets] or worktree_targets():
        checkout = os.path.dirname(target)
        label = os.path.basename(checkout) if os.path.basename(target) == "target" else target
        why = busy(checkout)
        if why:
            print(f"{label}: left alone, {why}")
            continue
        p = plan(target, args.min_age)
        if not args.dry_run:
            why = busy(checkout)
            if why:
                print(f"{label}: left alone, {why}")
                continue
            remove(p.stale)
        line = f"{label}: {len(p.stale):,} of {p.objects:,} objects named by no binary, {p.size / 1e9:.1f} GB {done}"
        if p.recent:
            line += f"; {p.recent:,} more touched in the last {args.min_age:g} minutes"
        print(line, flush=True)
        total += p.size
    print(f"{total / 1e9:.1f} GB {done} in all")
    return 0


if __name__ == "__main__":
    sys.exit(main())
