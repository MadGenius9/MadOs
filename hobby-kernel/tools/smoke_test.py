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

KEY_NAMES = {" ": "spc", "\n": "ret", "-": "minus", ".": "dot", "/": "slash", "&": "shift-7"}


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


class Session:
    """One QEMU boot with a serial log, a monitor socket and step checks."""

    def __init__(self, args: argparse.Namespace, label: str, failures: list[str]) -> None:
        self.args = args
        self.label = label
        self.failures = failures
        self.work = Path(tempfile.mkdtemp(prefix="kernel-smoke-"))
        self.log_path = self.work / "serial.log"
        self.mon_path = str(self.work / "monitor.sock")
        self.mon: Monitor | None = None

    def __enter__(self) -> "Session":
        boot = ["-kernel", self.args.kernel] if self.args.kernel else ["-cdrom", self.args.iso]
        cmd = [self.args.qemu, *boot, "-m", "128M", "-display", "none", "-no-reboot",
               "-serial", f"file:{self.log_path}", "-monitor", f"unix:{self.mon_path},server,nowait"]
        print(f"== {self.label}\n+", " ".join(cmd))
        self.started = time.time()
        self.qemu = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        return self

    def __exit__(self, *exc: object) -> None:
        if self.qemu.poll() is None:
            self.qemu.kill()
        self.qemu.wait()
        print(f"---- serial log ({self.label}) ----")
        print(self.serial().replace("\r", ""))
        if os.environ.get("KEEP_SMOKE_DIR") is None:
            for f in self.work.iterdir():
                f.unlink()
            self.work.rmdir()

    def serial(self) -> str:
        try:
            return self.log_path.read_text(errors="replace")
        except FileNotFoundError:
            return ""

    def wait_for(self, pattern: str, start: int) -> bool:
        deadline = time.time() + self.args.timeout
        while time.time() < deadline:
            if re.search(pattern, self.serial()[start:]):
                return True
            if self.qemu.poll() is not None:
                return False
            time.sleep(0.1)
        return False

    def step(self, name: str, keys: str | None, pattern: str, from_start: bool = False) -> bool:
        start = 0 if from_start else len(self.serial())
        if keys is not None:
            if self.mon is None:
                self.mon = Monitor(self.mon_path)
            self.mon.type(keys)
        if self.wait_for(pattern, start):
            print(f"PASS {name}")
            return True
        print(f"FAIL {name}: expected /{pattern}/")
        self.failures.append(name)
        return False

    def command(self, keys: str) -> str:
        """Types a command and returns its output, up to the next prompt."""
        start = len(self.serial())
        assert self.mon is not None
        self.mon.type(keys)
        if not self.wait_for(r"hobby-kernel> ", start + len(keys)):
            return ""
        return self.serial()[start:]

    def check(self, name: str, ok: bool, detail: str = "") -> None:
        if ok and "\n" in detail:
            detail = ""  # multi-line output is only worth showing on failure
        print(f"{'PASS' if ok else 'FAIL'} {name}{': ' + detail if detail else ''}")
        if not ok:
            self.failures.append(name)

    def boot(self) -> bool:
        if not self.step("boot reaches BOOT_OK", None, r"(?m)^BOOT_OK\r?$"):
            return False
        self.mon = Monitor(self.mon_path)
        return self.step("shell prompt", None, r"hobby-kernel> ", from_start=True)


def frames_free(text: str) -> int | None:
    m = re.search(r"\((\d+)/\d+ frames free\)", text)
    return int(m.group(1)) if m else None


def heap_used(text: str) -> int | None:
    m = re.search(r"(\d+) bytes used", text)
    return int(m.group(1)) if m else None


def process_steps(vm: Session) -> None:
    """Phase 8-9: scheduler, kernel threads, user processes, system calls."""
    heap_before = heap_used(vm.command("heap\n"))

    out = vm.command("ps\n")
    vm.check("ps lists shell and idle", bool(re.search(r"0\s+running\s+kernel\s+\d+\s+shell", out))
             and bool(re.search(r"\s1\s+\w+\s+kernel\s+\d+\s+idle", out)), out.strip())

    vm.step("kernel thread runs while the shell stays responsive", "spawn ticker\necho shell still works\n",
            r"started kernel thread \d+ \(ticker\)[\s\S]*shell still works[\s\S]*ticker: done")

    vm.step("user program runs in ring 3 and exits", "run hello\n",
            r"Hello from user mode \(ring 3\)! My pid is \d+\r?\n\[process \d+ \(hello\) exited with code 0\]")

    first = frames_free(vm.command("mem\n"))
    vm.command("run hello\n")
    second = frames_free(vm.command("mem\n"))
    vm.check("process memory is freed after exit (no frame leak)", first is not None and first == second,
             f"frames free {first} -> {second}")

    vm.step("write() refuses a kernel pointer", "run badptr\n", r"refused \(EFAULT\)")
    vm.step("ring-3 write to kernel memory kills only the process", None,
            r"\(badptr\) killed: Page fault at 0x00100000 \(protection violation, write\)", from_start=True)
    vm.step("privileged instruction in ring 3 kills only the process", "run priv\n",
            r"\(priv\) killed: General protection fault")

    vm.step("two background processes run concurrently", "run counter &\nrun counter &\n",
            r"counter (\d): 1[\s\S]*counter (?!\1)\d: 1[\s\S]*counter \d: 5[\s\S]*exited with code 0"
            r"[\s\S]*exited with code 0")

    out = vm.command("run spin &\n")
    m = re.search(r"started process (\d+) \(spin\)", out)
    vm.check("background spin starts", m is not None, out.strip())
    if m:
        pid = m.group(1)
        time.sleep(1.0)
        out = vm.command("ps\n")
        sm = re.search(rf"\s{pid}\s+(running|ready)\s+user\s+(\d+)\s+spin", out)
        vm.check("ring-3 busy loop is preempted (shell answers, spin got CPU time)",
                 sm is not None and int(sm.group(2)) > 0, out.strip())
        vm.step("kill PID", f"kill {pid}\n", rf"\[process {pid} \(spin\) killed\]")
        out = vm.command("ps\n")
        vm.check("killed process is gone from ps", "spin" not in out, out.strip())

    start = len(vm.serial())
    assert vm.mon is not None
    vm.mon.type("run spin\n")
    time.sleep(0.7)
    vm.mon.command("sendkey ctrl-c")
    vm.check("Ctrl+C stops a foreground program",
             vm.wait_for(r"\^C\r?\n\[process \d+ \(spin\) killed\][\s\S]*hobby-kernel> ", start))

    time.sleep(0.3)
    after = heap_used(vm.command("heap\n"))
    vm.check("finished tasks are reaped (heap back to its starting use)",
             heap_before is not None and heap_before == after, f"{heap_before} -> {after} bytes used")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--qemu", default="qemu-system-i386")
    src = ap.add_mutually_exclusive_group(required=True)
    src.add_argument("--kernel", help="boot the ELF with QEMU's Multiboot1 loader")
    src.add_argument("--iso", help="boot a GRUB ISO (Multiboot2)")
    ap.add_argument("--screenshot", type=Path, help="save the VGA screen as PNG here")
    ap.add_argument("--timeout", type=float, default=30)
    args = ap.parse_args()
    failures: list[str] = []

    with Session(args, "shell session", failures) as vm:
        if vm.boot():
            vm.step("echo (keyboard IRQ, Shift)", "echo Hello World\n", r"(?m)^Hello World\r?$")
            vm.step("unknown command", "frobnicate\n", r"frobnicate: command not found")
            vm.step("mem (PMM)", "mem\n", r"Physical memory: \d+ KiB usable")
            vm.step("memtest (heap + paging)", "memtest\n", r"memtest: OK")
            vm.step("heap stats", "heap\n", r"Heap: \d+ KiB mapped, \d+ bytes used")
            time.sleep(0.5)
            start = len(vm.serial())
            vm.step("uptime (PIT IRQ)", "uptime\n", r"Up [1-9]\d*\.\d\d s|Up 0\.(?!00)\d\d s")
            elapsed = time.time() - vm.started
            m = re.search(r"Up (\d+)\.(\d\d) s", vm.serial()[start:])
            if m:
                uptime = int(m.group(1)) + int(m.group(2)) / 100
                # The kernel booted after QEMU started, so uptime <= wall time.
                if 0.5 * elapsed - 1 <= uptime <= elapsed + 1:
                    print(f"PASS timer rate (uptime {uptime:.2f} s, wall clock {elapsed:.2f} s)")
                else:
                    print(f"FAIL timer rate: uptime {uptime:.2f} s vs wall clock {elapsed:.2f} s")
                    failures.append("timer rate")
            if args.screenshot and vm.mon:
                ppm = vm.work / "screen.ppm"
                vm.mon.command(f"screendump {ppm}")
                time.sleep(0.5)
                ppm_to_png(ppm.read_bytes(), args.screenshot)
                print(f"saved screenshot to {args.screenshot}")
            vm.step("crash null (page fault handler)", "crash null\n",
                    r"Page fault at address 0x00000000[\s\S]*KERNEL PANIC: unhandled page fault")

    with Session(args, "exception session", failures) as vm:
        if vm.boot():
            vm.step("crash div (CPU exception handler)", "crash div\n",
                    r"CPU exception 0: Divide error[\s\S]*eip=[0-9a-f]{8}[\s\S]*KERNEL PANIC: unhandled CPU exception")

    with Session(args, "process session", failures) as vm:
        if vm.boot():
            process_steps(vm)

    print("smoke test:", "FAILED " + ", ".join(failures) if failures else "all steps passed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
