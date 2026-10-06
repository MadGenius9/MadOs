#!/usr/bin/env python3
"""QEMU tooling for MadOS development VMs (UEFI, x86_64).

Library used by tests/smoke/vm_smoke.py, and a CLI used by `make vm`:

    vm.py run [--disk FILE | --iso FILE] [--headless] [--persist]
              [--memory MB] [--cpus N] [--fresh]

Safety: the VM only ever writes to files under out/vm/ (a copy-on-write
overlay over the built image, or a scratch install-target disk). It never
touches host block devices.
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import socket
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "out"

# (code, vars) pairs, most preferred first. Paths differ per distribution.
OVMF_CANDIDATES = [
    ("/usr/share/edk2/ovmf/OVMF_CODE.fd", "/usr/share/edk2/ovmf/OVMF_VARS.fd"),  # Fedora
    ("/usr/share/OVMF/OVMF_CODE_4M.fd", "/usr/share/OVMF/OVMF_VARS_4M.fd"),  # Debian/Ubuntu
    ("/usr/share/OVMF/OVMF_CODE.fd", "/usr/share/OVMF/OVMF_VARS.fd"),  # older Debian
    ("/usr/share/edk2/x64/OVMF_CODE.4m.fd", "/usr/share/edk2/x64/OVMF_VARS.4m.fd"),  # Arch
    ("/usr/share/qemu/ovmf-x86_64-code.bin", "/usr/share/qemu/ovmf-x86_64-vars.bin"),  # openSUSE
]


def log(msg: str) -> None:
    print(f"vm: {msg}", file=sys.stderr, flush=True)


def find_ovmf() -> tuple[Path, Path] | None:
    env_code, env_vars = os.environ.get("MADOS_OVMF_CODE"), os.environ.get("MADOS_OVMF_VARS")
    if env_code and env_vars:
        return Path(env_code), Path(env_vars)
    for code, vars_ in OVMF_CANDIDATES:
        if Path(code).is_file() and Path(vars_).is_file():
            return Path(code), Path(vars_)
    return None


def kvm_usable() -> bool:
    return os.path.exists("/dev/kvm") and os.access("/dev/kvm", os.R_OK | os.W_OK)


def latest(pattern: str) -> Path | None:
    files = sorted(OUT.glob(pattern), key=lambda p: p.stat().st_mtime, reverse=True)
    return files[0] if files else None


def is_host_block_device(path: Path) -> bool:
    try:
        import stat

        return stat.S_ISBLK(path.resolve().stat().st_mode)
    except FileNotFoundError:
        return False


@dataclass
class VmConfig:
    workdir: Path
    disk: Path | None = None  # base image (read-only; an overlay is used)
    iso: Path | None = None  # installer ISO (boots with a scratch target disk)
    kernel: Path | None = None  # direct kernel boot (harness self-test)
    initrd: Path | None = None
    append: str = ""
    memory_mb: int = 4096
    cpus: int = 4
    headless: bool = True
    persist: bool = False  # keep the overlay between runs
    fresh: bool = False  # discard an existing persistent overlay
    accel: str = "auto"  # auto | kvm | tcg
    audio: str = "auto"  # auto | none | pa | pipewire
    serial_to_stdio: bool = False
    extra: list[str] = field(default_factory=list)

    @property
    def serial_log(self) -> Path:
        return self.workdir / "serial.log"

    @property
    def qmp_sock(self) -> Path:
        return self.workdir / "qmp.sock"

    @property
    def qga_sock(self) -> Path:
        return self.workdir / "qga.sock"


def prepare_disks(cfg: VmConfig) -> list[str]:
    """Creates overlay/scratch disks inside cfg.workdir; returns QEMU args."""
    args: list[str] = []
    qemu_img = shutil.which("qemu-img")
    if cfg.disk:
        if is_host_block_device(cfg.disk):
            raise SystemExit(f"refusing to use host block device {cfg.disk}; VM images only")
        if not qemu_img:
            raise SystemExit("qemu-img not found (install QEMU tools; see `make setup`)")
        overlay = cfg.workdir / "overlay.qcow2"
        if overlay.exists() and (cfg.fresh or not cfg.persist):
            overlay.unlink()
        if not overlay.exists():
            fmt = "qcow2" if cfg.disk.suffix == ".qcow2" else "raw"
            subprocess.run(
                [qemu_img, "create", "-q", "-f", "qcow2", "-b", str(cfg.disk.resolve()), "-F", fmt, str(overlay)],
                check=True,
            )
        args += ["-drive", f"file={overlay},if=virtio,format=qcow2,cache=unsafe,discard=unmap"]
    if cfg.iso:
        target = cfg.workdir / "install-target.qcow2"
        if target.exists() and cfg.fresh:
            target.unlink()
        if not target.exists():
            if not qemu_img:
                raise SystemExit("qemu-img not found")
            subprocess.run([qemu_img, "create", "-q", "-f", "qcow2", str(target), "64G"], check=True)
        args += ["-drive", f"file={target},if=virtio,format=qcow2"]
        args += ["-drive", f"file={cfg.iso.resolve()},media=cdrom,readonly=on"]
    return args


def audio_args(choice: str) -> list[str]:
    if choice == "auto":
        uid = os.getuid()
        if os.environ.get("PIPEWIRE_RUNTIME_DIR") or Path(f"/run/user/{uid}/pipewire-0").exists():
            choice = "pipewire"
        elif os.environ.get("PULSE_SERVER") or Path(f"/run/user/{uid}/pulse/native").exists():
            choice = "pa"
        else:
            choice = "none"
    # The HDA device is always present so the guest detects audio hardware;
    # the backend decides whether sound reaches the host.
    return ["-audiodev", f"{choice},id=snd0", "-device", "intel-hda", "-device", "hda-duplex,audiodev=snd0"]


def build_command(cfg: VmConfig) -> list[str]:
    qemu = shutil.which("qemu-system-x86_64")
    if not qemu:
        raise SystemExit("qemu-system-x86_64 not found (see `make setup`)")
    fw = find_ovmf()
    if not fw:
        raise SystemExit("UEFI firmware (OVMF) not found; install edk2-ovmf / ovmf, or set MADOS_OVMF_CODE/VARS")
    cfg.workdir.mkdir(parents=True, exist_ok=True)
    vars_copy = cfg.workdir / "OVMF_VARS.fd"
    if not vars_copy.exists() or cfg.fresh or not cfg.persist:
        shutil.copyfile(fw[1], vars_copy)

    accel = cfg.accel
    if accel == "auto":
        accel = "kvm" if kvm_usable() else "tcg"
    if accel == "kvm" and not kvm_usable():
        raise SystemExit("KVM requested but /dev/kvm is not accessible")
    if accel == "tcg":
        log("KVM unavailable: using software emulation (TCG); boot will be slow")

    for sock in (cfg.qmp_sock, cfg.qga_sock):
        if sock.exists():
            sock.unlink()

    cmd = [
        qemu,
        "-name", "mados-dev",
        "-machine", f"q35,accel={accel}" + (",smm=on" if accel == "kvm" else ""),
        "-cpu", "host" if accel == "kvm" else "max",
        "-smp", str(cfg.cpus),
        "-m", str(cfg.memory_mb),
        "-drive", f"if=pflash,format=raw,unit=0,readonly=on,file={fw[0]}",
        "-drive", f"if=pflash,format=raw,unit=1,file={vars_copy}",
        "-nic", "user,model=virtio-net-pci",
        "-device", "virtio-rng-pci",
        "-device", "qemu-xhci",
        "-device", "usb-tablet",
        "-qmp", f"unix:{cfg.qmp_sock},server=on,wait=off",
        "-chardev", f"socket,path={cfg.qga_sock},server=on,wait=off,id=qga0",
        "-device", "virtio-serial",
        "-device", "virtserialport,chardev=qga0,name=org.qemu.guest_agent.0",
    ]
    cmd += audio_args(cfg.audio)
    if cfg.headless:
        cmd += ["-display", "none", "-device", "virtio-vga"]
    else:
        cmd += ["-device", "virtio-vga", "-display", "gtk" if os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY") else "none"]
    cmd += ["-serial", "mon:stdio" if cfg.serial_to_stdio else f"file:{cfg.serial_log}"]
    cmd += prepare_disks(cfg)
    if cfg.kernel:
        cmd += ["-kernel", str(cfg.kernel)]
        if cfg.initrd:
            cmd += ["-initrd", str(cfg.initrd)]
        if cfg.append:
            cmd += ["-append", cfg.append]
    cmd += cfg.extra
    return cmd


# --------------------------------------------------------------- QMP / QGA

class JsonSocket:
    """Line-oriented JSON over a UNIX socket (QMP and the guest agent)."""

    def __init__(self, path: Path, timeout: float = 30.0):
        deadline = time.monotonic() + timeout
        last: Exception | None = None
        while time.monotonic() < deadline:
            try:
                self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                self.sock.settimeout(timeout)
                self.sock.connect(str(path))
                break
            except OSError as e:
                last = e
                time.sleep(0.2)
        else:
            raise TimeoutError(f"cannot connect to {path}: {last}")
        self.buf = b""

    def send(self, obj: dict) -> None:
        self.sock.sendall(json.dumps(obj).encode() + b"\n")

    def recv(self, timeout: float | None = None) -> dict:
        if timeout is not None:
            self.sock.settimeout(timeout)
        while b"\n" not in self.buf:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise ConnectionError("socket closed")
            self.buf += chunk
        line, self.buf = self.buf.split(b"\n", 1)
        line = line.strip().lstrip(b"\xff")
        return json.loads(line) if line else self.recv(timeout)

    def close(self) -> None:
        try:
            self.sock.close()
        except OSError:
            pass


class Qmp(JsonSocket):
    def __init__(self, path: Path, timeout: float = 30.0):
        super().__init__(path, timeout)
        self.recv()  # greeting
        self.execute("qmp_capabilities")

    def execute(self, command: str, **arguments) -> dict:
        msg: dict = {"execute": command}
        if arguments:
            msg["arguments"] = arguments
        self.send(msg)
        while True:
            reply = self.recv()
            if "event" in reply:
                continue
            if "error" in reply:
                raise RuntimeError(f"QMP {command}: {reply['error']}")
            return reply.get("return", {})


class GuestAgent(JsonSocket):
    """qemu-guest-agent client. Only RPCs Fedora allows by default are used."""

    def sync(self, timeout: float = 10.0) -> bool:
        token = int(time.time() * 1000) % 2**31
        try:
            self.send({"execute": "guest-sync-delimited", "arguments": {"id": token}})
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                reply = self.recv(timeout=timeout)
                if reply.get("return") == token:
                    return True
        except (OSError, ConnectionError, TimeoutError, json.JSONDecodeError):
            return False
        return False

    def execute(self, command: str, expect_reply: bool = True, **arguments) -> dict:
        msg: dict = {"execute": command}
        if arguments:
            msg["arguments"] = arguments
        self.send(msg)
        if not expect_reply:
            return {}
        reply = self.recv(timeout=30)
        if "error" in reply:
            raise RuntimeError(f"QGA {command}: {reply['error']}")
        return reply.get("return", {})


# --------------------------------------------------------------- CLI

def cmd_run(ns: argparse.Namespace) -> int:
    disk = Path(ns.disk) if ns.disk else None
    iso = Path(ns.iso) if ns.iso else None
    if not disk and not iso:
        disk = latest("*.qcow2")
        if not disk:
            log("no disk image in out/; build one with `make disk` (or pass --disk/--iso)")
            return 1
    cfg = VmConfig(
        workdir=OUT / "vm",
        disk=disk,
        iso=iso,
        memory_mb=ns.memory,
        cpus=ns.cpus,
        headless=ns.headless,
        persist=ns.persist,
        fresh=ns.fresh,
        serial_to_stdio=ns.headless,
    )
    cmd = build_command(cfg)
    log(f"booting {'ISO ' + str(iso) if iso else disk}")
    log("changes go to out/vm/ (" + ("kept between runs" if ns.persist else "discarded on next run") + ")")
    if not ns.headless:
        log(f"serial console: {cfg.serial_log}")
    return subprocess.call(cmd)


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run", help="boot a MadOS image")
    g = r.add_mutually_exclusive_group()
    g.add_argument("--disk", help="qcow2/raw image (default: newest out/*.qcow2)")
    g.add_argument("--iso", help="installer ISO; installs to a scratch disk in out/vm/")
    r.add_argument("--headless", action="store_true", help="no window; serial console on stdio")
    r.add_argument("--persist", action="store_true", help="keep VM changes between runs")
    r.add_argument("--fresh", action="store_true", help="discard previous VM state")
    r.add_argument("--memory", type=int, default=4096)
    r.add_argument("--cpus", type=int, default=4)
    ns = ap.parse_args(argv)
    return cmd_run(ns)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
