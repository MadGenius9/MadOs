#!/usr/bin/env python3
"""Build the initial ramdisk: a POSIX ustar archive of one or more trees.

    python3 tools/mkinitrd.py build/initrd.tar initrd build/initrd

Later trees override earlier ones for the same path. The archive is
deterministic (sorted entries, zero timestamps and owners) and lists every
directory explicitly, which is what src/fs.c expects (it also copes with
missing directories).
"""
from __future__ import annotations

import io
import sys
import tarfile
from pathlib import Path


def collect(roots: list[Path]) -> dict[str, Path | None]:
    """Maps archive path -> source file (None for directories)."""
    entries: dict[str, Path | None] = {}
    for root in roots:
        if not root.is_dir():
            sys.exit(f"mkinitrd: {root} is not a directory")
        for path in sorted(root.rglob("*")):
            rel = path.relative_to(root).as_posix()
            if path.is_dir():
                entries.setdefault(rel, None)
            elif path.is_file():
                parts = rel.split("/")
                for i in range(1, len(parts)):
                    entries.setdefault("/".join(parts[:i]), None)
                entries[rel] = path
    return entries


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    out = Path(sys.argv[1])
    entries = collect([Path(p) for p in sys.argv[2:]])

    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w", format=tarfile.USTAR_FORMAT) as tar:
        for name in sorted(entries):
            src = entries[name]
            info = tarfile.TarInfo(name)
            info.mtime = 0
            info.uid = info.gid = 0
            info.uname = info.gname = "root"
            if src is None:
                info.type = tarfile.DIRTYPE
                info.mode = 0o755
                tar.addfile(info)
            else:
                data = src.read_bytes()
                info.size = len(data)
                info.mode = 0o755 if name.startswith("bin/") else 0o644
                tar.addfile(info, io.BytesIO(data))
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(buf.getvalue())
    files = sum(1 for v in entries.values() if v is not None)
    print(f"mkinitrd: {out} ({len(buf.getvalue())} bytes, {files} files, {len(entries) - files} directories)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
