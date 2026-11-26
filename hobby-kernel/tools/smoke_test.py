#!/usr/bin/env python3
"""Boot the kernel in QEMU and drive the shell with real key presses.

    python3 tools/smoke_test.py --qemu qemu-system-i386 --kernel build/kernel.elf
    python3 tools/smoke_test.py --qemu qemu-system-i386 --iso build/hobby-kernel.iso

QEMU runs headless. The serial port goes to a log file; keys are typed via
the QEMU monitor's `sendkey`, so they arrive as PS/2 scancodes on IRQ 1.
Each step waits for the expected text on the serial log. A screenshot of
the VGA text screen is saved as PNG (--screenshot). Exit status 0 = pass.
"""
from __future__ import annotations

import argparse
import os
import re
import socket
import struct
import subprocess
import sys
import tempfile
import time
import zlib
from pathlib import Path

KEY_NAMES = {" ": "spc", "\n": "ret", "-": "minus", ".": "dot", "/": "slash"}


def key_for(ch: str) -> str:
    if ch in KEY_NAMES:
        return KEY_NAMES[ch]
    if ch.isupper():
        return "shift-" + ch.lower()
    if ch.isalnum():
        return ch
    raise ValueError(f"no key mapping for {ch!r}")


class Monitor:
    def __init__(self, path: str) -> None:
        deadline = time.time() + 10
        while True:
            try:
                self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                self.sock.connect(path)
                break
            except OSError:
                if time.time() > deadline:
                    raise
                time.sleep(0.1)
        self.sock.settimeout(5)
        self._read_until(b"(qemu)")

    def _read_until(self, marker: bytes) -> bytes:
        data = b""
        while marker not in data:
            chunk = self.sock.recv(4096)
            if not chunk:
                break
            data += chunk
        return data

    def command(self, cmd: str) -> str:
        self.sock.sendall(cmd.encode() + b"\n")
        return self._read_until(b"(qemu)").decode(errors="replace")

    def type(self, text: str) -> None:
        for ch in text:
            self.command(f"sendkey {key_for(ch)}")
            time.sleep(0.03)


def ppm_to_png(ppm: bytes, out: Path) -> None:
    """Convert a binary P6 PPM (what `screendump` writes) to PNG, stdlib only."""
    parts = re.match(rb"P6\s+(\d+)\s+(\d+)\s+(\d+)\s", ppm)
    if not parts:
        raise ValueError("not a P6 PPM")
    width, height = int(parts.group(1)), int(parts.group(2))
    pixels = ppm[parts.end():]
    raw = b"".join(b"\x00" + pixels[y * width * 3:(y + 1) * width * 3] for y in range(height))

    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    out.write_bytes(png)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--qemu", default="qemu-system-i386")
    src = ap.add_mutually_exclusive_group(required=True)
    src.add_argument("--kernel", help="boot the ELF with QEMU's Multiboot1 loader")
    src.add_argument("--iso", help="boot a GRUB ISO (Multiboot2)")
    ap.add_argument("--screenshot", type=Path, help="save the VGA screen as PNG here")
    ap.add_argument("--timeout", type=float, default=30)
    args = ap.parse_args()

    work = Path(tempfile.mkdtemp(prefix="kernel-smoke-"))
    log_path = work / "serial.log"
    mon_path = str(work / "monitor.sock")
    boot = ["-kernel", args.kernel] if args.kernel else ["-cdrom", args.iso]
    cmd = [args.qemu, *boot, "-m", "128M", "-display", "none", "-no-reboot",
           "-serial", f"file:{log_path}", "-monitor", f"unix:{mon_path},server,nowait"]
    print("+", " ".join(cmd))
    qemu = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

    failures: list[str] = []

    def serial() -> str:
        try:
            return log_path.read_text(errors="replace")
        except FileNotFoundError:
            return ""

    def wait_for(pattern: str, start: int, timeout: float = args.timeout) -> bool:
        deadline = time.time() + timeout
        while time.time() < deadline:
            text = serial()[start:]
            if re.search(pattern, text):
                return True
            if qemu.poll() is not None:
                return False
            time.sleep(0.1)
        return False

    def step(name: str, keys: str | None, pattern: str, mon: Monitor | None) -> None:
        start = len(serial())
        if keys is not None and mon is not None:
            mon.type(keys)
        if wait_for(pattern, start):
            print(f"PASS {name}")
        else:
            print(f"FAIL {name}: expected /{pattern}/")
            failures.append(name)

    try:
        step("boot reaches BOOT_OK", None, r"(?m)^BOOT_OK\r?$", None)
        if failures:
            return 1
        mon = Monitor(mon_path)
        step("shell prompt", None, r"hobby-kernel> ", mon)
        step("echo (keyboard IRQ, Shift)", "echo Hello World\n", r"(?m)^Hello World\r?$", mon)
        step("unknown command", "frobnicate\n", r"frobnicate: command not found", mon)
        step("mem (PMM)", "mem\n", r"Physical memory: \d+ KiB usable", mon)
        step("memtest (heap + paging)", "memtest\n", r"memtest: OK", mon)
        step("heap stats", "heap\n", r"Heap: \d+ KiB mapped, 0 bytes used", mon)
        time.sleep(0.5)
        step("uptime (PIT IRQ)", "uptime\n", r"Up [1-9]\d*\.\d\d s|Up 0\.(?!00)\d\d s", mon)
        if args.screenshot:
            ppm = work / "screen.ppm"
            mon.command(f"screendump {ppm}")
            time.sleep(0.5)
            ppm_to_png(ppm.read_bytes(), args.screenshot)
            print(f"saved screenshot to {args.screenshot}")
        step("crash null (page fault handler)", "crash null\n",
             r"Page fault at address 0x00000000[\s\S]*KERNEL PANIC: unhandled page fault", mon)
        mon.command("quit")
    finally:
        if qemu.poll() is None:
            qemu.kill()
        qemu.wait()
        print("---- serial log ----")
        print(serial().replace("\r", ""))
        if os.environ.get("KEEP_SMOKE_DIR") is None:
            for f in work.iterdir():
                f.unlink()
            work.rmdir()

    print("smoke test:", "FAILED " + ", ".join(failures) if failures else "all steps passed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
