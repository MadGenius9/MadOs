#!/usr/bin/env python3
"""MadOS VM smoke test.

Boots an image in QEMU (UEFI) and checks, in order:

  1. boot       MADOS_BOOT_OK marker on the serial console (written by
                mados-boot-report.service after graphical.target)
  2. units      no failed systemd units (from the marker's failed= field)
     selinux    SELinux is enforcing (from the marker's selinux= field)
  3. session    MADOS_SESSION_OK: an active Wayland/X11 user session of the
                development user (--expect-user)
     apps       MADOS_APPS (dev images): terminal, file manager, browser and
                settings each started and claimed their D-Bus name
     audio      MADOS_APPS audio=ok: sound card + PipeWire default sink
     welcome    MADOS_APPS first_run=running kde_welcome=absent: the MadOS
                first-run window opened at first login, KDE's Welcome Center not
     defaults   MADOS_APPS defaults=ok: in plasmashell's XDG_CONFIG_DIRS the MadOS
                defaults (look-and-feel, accent, kded) precede /etc/xdg; only
                Plasma's own ~/.config/kdedefaults may come first
     services   MADOS_APPS daemon/bootc/assistant=ok: MadOS's own services
                answer on the real system and session buses
  4. network    a non-loopback interface with an IPv4 address (guest agent)
  5. screenshot QMP screendump of the display (artifact for humans)
  6. reboot     clean reboot via the guest agent; a second MADOS_BOOT_OK
  7. shutdown   clean power-off; MADOS_SHUTDOWN marker and QEMU exit

Steps whose prerequisites are absent are reported as "skip", never "pass".
Results are written to <workdir>/report.json.

    vm_smoke.py --disk out/mados.qcow2 [--require-session] [--timeout SECS]
    vm_smoke.py --kernel K --initrd I --append A   (harness self-test)
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import vm  # noqa: E402

BOOT_RE = re.compile(r"MADOS_BOOT_OK (.*)")
SESSION_RE = re.compile(r"MADOS_SESSION_(OK|NONE) ?(.*)")
APPS_RE = re.compile(r"MADOS_APPS(_NONE)? (.*)")
APP_KEYS = ("terminal", "files", "browser", "settings")


# CSI (colours, cursor), OSC (terminal titles, "ESC ] ... BEL|ESC \") and two-byte escapes.
ANSI_RE = re.compile(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07\x1b]*(?:\x07|\x1b\\)?|[@-Z\\-_])")
PROBLEM_RE = re.compile(
    r"FAILED|Failed to|Timed out|Dependency failed|[Ee]mergency mode|[Rr]escue mode|A start job is running|"
    r"Cannot open|Kernel panic|not found|\b[Ee]rror:"
)


def fields(s: str) -> dict[str, str]:
    return dict(kv.split("=", 1) for kv in s.split() if "=" in kv)


def boot_diagnostics(text: str, problems: int = 25, tail: int = 30) -> list[str]:
    """Readable excerpt of a serial log for a boot that never reported:
    lines that look like problems, then the last lines (ANSI codes removed)."""
    lines = [ANSI_RE.sub("", l).replace("\r", "").strip() for l in text.splitlines()]
    lines = [l[:200] for l in lines if l]
    found = [l for l in lines if PROBLEM_RE.search(l)][-problems:]
    return [f"problem: {l}" for l in found] + [f"last:    {l}" for l in lines[-tail:]]


def screenshot(cfg: vm.VmConfig, workdir: Path, name: str = "screen") -> Path:
    qmp = vm.Qmp(cfg.qmp_sock)
    shot = workdir / f"{name}.png"
    try:
        qmp.execute("screendump", filename=str(shot), format="png")
    except RuntimeError:
        shot = workdir / f"{name}.ppm"
        qmp.execute("screendump", filename=str(shot))
    return shot


class Serial:
    """Follows the serial log file written by QEMU."""

    def __init__(self, path: Path):
        self.path = path
        self.offset = 0
        self.text = ""

    def poll(self) -> str:
        if not self.path.exists():
            return ""
        with self.path.open("rb") as f:
            f.seek(self.offset)
            data = f.read()
        self.offset += len(data)
        new = data.decode("utf-8", errors="replace")
        self.text += new
        return new

    def wait_for(self, pattern: re.Pattern[str], timeout: float, proc: subprocess.Popen, start: int = 0) -> re.Match[str] | None:
        """Waits for `pattern` in output after character offset `start`."""
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            self.poll()
            m = pattern.search(self.text, start)
            if m:
                return m
            if proc.poll() is not None:
                self.poll()
                return pattern.search(self.text, start)
            time.sleep(1)
        return None


class Report:
    def __init__(self) -> None:
        self.steps: list[dict] = []

    def add(self, name: str, status: str, detail: str = "") -> None:
        self.steps.append({"step": name, "status": status, "detail": detail})
        print(f"[{status.upper():4}] {name}: {detail}", flush=True)

    @property
    def failed(self) -> bool:
        return any(s["status"] == "fail" for s in self.steps)


def guest_agent(cfg: vm.VmConfig, timeout: float) -> vm.GuestAgent | None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            ga = vm.GuestAgent(cfg.qga_sock, timeout=5)
            if ga.sync(timeout=5):
                return ga
            ga.close()
        except (OSError, TimeoutError, ConnectionError):
            pass
        time.sleep(3)
    return None


def run(ns: argparse.Namespace) -> int:
    workdir = Path(ns.workdir)
    workdir.mkdir(parents=True, exist_ok=True)
    cfg = vm.VmConfig(
        workdir=workdir,
        disk=Path(ns.disk) if ns.disk else None,
        kernel=Path(ns.kernel) if ns.kernel else None,
        initrd=Path(ns.initrd) if ns.initrd else None,
        append=ns.append or "",
        memory_mb=ns.memory,
        cpus=ns.cpus,
        accel=ns.accel,
        audio="none",
    )
    if cfg.serial_log.exists():
        cfg.serial_log.unlink()
    cmd = vm.build_command(cfg)
    accel = "kvm" if "accel=kvm" in " ".join(cmd) else "tcg"
    timeout = ns.timeout or (900 if accel == "kvm" else 3600)
    print("qemu: " + " ".join(cmd), flush=True)
    report = Report()
    serial = Serial(cfg.serial_log)
    qemu_log = (workdir / "qemu.log").open("w")
    proc = subprocess.Popen(cmd, stdout=qemu_log, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL)
    started = time.monotonic()
    try:
        # 1-2. boot + failed units
        m = serial.wait_for(BOOT_RE, timeout, proc)
        if not m:
            reason = f"QEMU exited with {proc.returncode}" if proc.poll() is not None else f"no marker within {timeout}s"
            if proc.poll() is None:
                try:
                    reason += f"; screen: {screenshot(cfg, workdir)}"
                except (OSError, TimeoutError, RuntimeError, ConnectionError) as e:
                    reason += f"; no screenshot ({e})"
            serial.poll()
            diag = boot_diagnostics(serial.text)
            report.add("boot", "fail", reason + "".join(f"\n         {l}" for l in diag))
            return finish(report, workdir, proc, serial)
        boot = fields(m.group(1))
        report.add("boot", "pass", f"{time.monotonic() - started:.0f}s, version={boot.get('version')} kernel={boot.get('kernel')}")
        failed_units = boot.get("failed", "unknown")
        # boot-report prints failed units' log lines just before MADOS_BOOT_OK.
        unit_logs = re.findall(r"MADOS_UNIT_LOG (.*)", serial.text[: m.end()])
        report.add("units", "pass" if failed_units == "none" else "fail",
                   f"state={boot.get('state')} failed={failed_units}"
                   + ("".join(f"\n         {l}" for l in unit_logs[:10]) if unit_logs else ""))
        selinux = boot.get("selinux")
        if selinux is None:
            report.add("selinux", "skip", "marker has no selinux field (older image)")
        else:
            # Denials are shown for diagnosis; they fail the test only through
            # their effects (failed units, missing session, ...).
            avcs = re.findall(r"MADOS_AVC (.*)", serial.text[: m.end()])
            report.add("selinux", "pass" if selinux == "enforcing" else "fail",
                       f"selinux={selinux} denials={len(avcs)}{'+' if len(avcs) >= 8 else ''}"
                       + "".join(f"\n         {a[:200]}" for a in avcs))
        boot_end = m.end()

        # 3. graphical session
        m = serial.wait_for(SESSION_RE, ns.session_timeout, proc, boot_end)
        session_ok = bool(m and m.group(1) == "OK")
        user = fields(m.group(2)).get("user") if session_ok else None
        if session_ok and ns.expect_user and user is not None and user != ns.expect_user:
            # e.g. Plasma Setup's own "plasma-setup" wizard session instead of
            # the development user's desktop (CI runs #2-#5).
            report.add("session", "fail", f"{m.group(2)} (expected user={ns.expect_user})")
            session_ok = False
        elif session_ok:
            report.add("session", "pass", m.group(2))
        else:
            detail = m.group(2) if m else "no session marker"
            report.add("session", "fail" if ns.require_session else "skip", detail)

        # 5. screenshot — early, before the screen locker or DPMS blank it.
        if session_ok:
            time.sleep(ns.settle)
        try:
            report.add("screenshot", "pass", str(screenshot(cfg, workdir)))
        except (OSError, TimeoutError, RuntimeError, ConnectionError) as e:
            report.add("screenshot", "skip", str(e))


        # apps + audio (relayed from the dev-only session check)
        m = serial.wait_for(APPS_RE, ns.apps_timeout, proc, boot_end) if session_ok else None
        if m and not m.group(1):
            res = fields(m.group(2))
            # "running" = alive at the deadline but no D-Bus name: weaker, reported as such.
            bad = [k for k in APP_KEYS if res.get(k) not in ("ok", "running")]
            weak = [k for k in APP_KEYS if res.get(k) == "running"]
            detail = " ".join(f"{k}={res.get(k, 'absent')}" for k in APP_KEYS)
            if weak:
                detail += f" (no D-Bus name: {','.join(weak)})"
            report.add("apps", "fail" if bad else "pass", detail)
            audio = res.get("audio", "unknown")
            report.add("audio", {"ok": "pass", "unknown": "skip"}.get(audio, "fail"), f"audio={audio}")
            welcome = {k: res.get(k) for k in ("first_run", "kde_welcome")}
            if all(v is None for v in welcome.values()):
                report.add("welcome", "skip", "image predates the welcome check")
            else:
                ok = welcome == {"first_run": "running", "kde_welcome": "absent"}
                report.add("welcome", "pass" if ok else "fail", " ".join(f"{k}={v}" for k, v in welcome.items()))
            defaults = res.get("defaults")
            if defaults is None:
                report.add("defaults", "skip", "image predates the defaults check")
            else:
                report.add("defaults", "pass" if defaults == "ok" else "fail", f"defaults={defaults}")
            svc = {k: res.get(k) for k in ("daemon", "bootc", "assistant")}
            if all(v is None for v in svc.values()):
                report.add("services", "skip", "image predates the services check")
            else:
                ok = all(v == "ok" for v in svc.values())
                report.add("services", "pass" if ok else "fail", " ".join(f"{k}={v}" for k, v in svc.items()))
        else:
            why = "no graphical session" if not session_ok else (m.group(2) if m else "no MADOS_APPS marker")
            report.add("apps", "fail" if ns.require_apps else "skip", why)
            report.add("audio", "skip", why)
            report.add("welcome", "skip", why)
            report.add("defaults", "skip", why)
            report.add("services", "skip", why)

        # 4. network via guest agent
        ga = guest_agent(cfg, ns.agent_timeout)
        if ga:
            ifaces = ga.execute("guest-network-get-interfaces")
            addrs = [
                f"{i['name']}={a['ip-address']}"
                for i in ifaces
                if i.get("name") != "lo"
                for a in i.get("ip-addresses", [])
                if a.get("ip-address-type") == "ipv4"
            ]
            report.add("network", "pass" if addrs else "fail", ", ".join(addrs) or "no IPv4 address")
        else:
            report.add("network", "skip", "guest agent not reachable")

        # 6. reboot
        if ns.reboot and ga:
            mark = len(serial.text)
            ga.execute("guest-shutdown", expect_reply=False, mode="reboot")
            ga.close()
            down = serial.wait_for(re.compile("MADOS_SHUTDOWN"), 300, proc, mark)
            m = serial.wait_for(BOOT_RE, timeout, proc, mark)
            if m:
                report.add("reboot", "pass", "shutdown marker seen" if down else "rebooted (no shutdown marker)")
            else:
                report.add("reboot", "fail", "no MADOS_BOOT_OK after reboot")
                return finish(report, workdir, proc, serial)
            ga = guest_agent(cfg, ns.agent_timeout)
        elif ns.reboot:
            report.add("reboot", "skip", "needs the guest agent")

        # 7. shutdown
        mark = len(serial.text)
        how = "guest-initiated"
        if proc.poll() is None:
            # The guest may power itself off between our checks; a vanished
            # socket then means "already down", not a harness error.
            try:
                if ga:
                    ga.execute("guest-shutdown", expect_reply=False, mode="powerdown")
                    how = "guest agent"
                else:
                    vm.Qmp(cfg.qmp_sock).execute("system_powerdown")
                    how = "ACPI power button"
            except (OSError, ConnectionError, RuntimeError, TimeoutError):
                pass
        try:
            proc.wait(timeout=ns.shutdown_timeout)
            serial.poll()
            clean = "MADOS_SHUTDOWN" in serial.text[mark:] or "MADOS_SHUTDOWN" in serial.text
            report.add("shutdown", "pass" if clean and proc.returncode == 0 else "fail",
                       f"via {how}; qemu exit={proc.returncode}; shutdown marker={'yes' if clean else 'no'}")
        except subprocess.TimeoutExpired:
            report.add("shutdown", "fail", f"VM still running {ns.shutdown_timeout}s after {how} request")
        return finish(report, workdir, proc, serial)
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        qemu_log.close()


def finish(report: Report, workdir: Path, proc: subprocess.Popen, serial: Serial) -> int:
    if proc.poll() is None:
        proc.kill()
        proc.wait()
    serial.poll()
    (workdir / "report.json").write_text(json.dumps({"steps": report.steps, "failed": report.failed}, indent=2) + "\n")
    print(f"smoke: {'FAILED' if report.failed else 'PASSED'} (report: {workdir / 'report.json'}, serial: {serial.path})")
    return 1 if report.failed else 0


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--disk", help="image to boot (default: newest out/*.qcow2)")
    ap.add_argument("--kernel")
    ap.add_argument("--initrd")
    ap.add_argument("--append")
    ap.add_argument("--workdir", default=str(vm.OUT / "smoke"))
    ap.add_argument("--accel", choices=["auto", "kvm", "tcg"], default="auto")
    ap.add_argument("--memory", type=int, default=4096)
    ap.add_argument("--cpus", type=int, default=4)
    ap.add_argument("--timeout", type=int, default=0, help="boot timeout (default 900s KVM / 3600s TCG)")
    ap.add_argument("--session-timeout", type=int, default=300)
    ap.add_argument("--agent-timeout", type=int, default=120)
    ap.add_argument("--shutdown-timeout", type=int, default=300)
    ap.add_argument("--require-session", action="store_true", help="fail if no graphical session")
    ap.add_argument("--expect-user", default="mados",
                    help="user the graphical session must belong to (dev disk user, scripts/build-disk.sh); '' = any")
    ap.add_argument("--require-apps", action="store_true", help="fail if the dev session check does not report")
    ap.add_argument("--settle", type=int, default=20, help="seconds to let the desktop draw before the screenshot")
    ap.add_argument("--apps-timeout", type=int, default=600)
    ap.add_argument("--no-reboot", dest="reboot", action="store_false")
    ns = ap.parse_args(argv)
    if not ns.disk and not ns.kernel:
        newest = vm.latest("*.qcow2")
        if not newest:
            print("smoke: no image; run `make disk` first or pass --disk", file=sys.stderr)
            return 2
        ns.disk = str(newest)
    return run(ns)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
