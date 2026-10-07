#!/usr/bin/env python3
"""End-to-end installer ISO test (unattended install, then boot the result).

1. Extracts the installer kernel/initrd and the volume label from the ISO
   (the ISO itself is not modified).
2. Builds a tiny disk image labelled OEMDRV that contains an unattended
   kickstart. Anaconda loads `ks.cfg` from an OEMDRV volume automatically;
   real users never have one attached, so the shipped ISO has no
   auto-install path.
3. Boots the installer in QEMU (UEFI) against a scratch target disk in the
   work directory; the kickstart installs the MadOS image embedded in the
   ISO and powers off.
4. Boots the installed disk with tests/smoke/vm_smoke.py.

Only files under the work directory are written. Requires xorriso,
mkfs.vfat (dosfstools) and mcopy (mtools) in addition to QEMU/OVMF.

    iso_install.py [--iso out/x.iso] [--payload REF] [--target-ref REF]
"""
from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "tests" / "smoke"))
import vm  # noqa: E402

KICKSTART = """\
# Unattended install for automated testing ONLY (tests/smoke/iso_install.py).
text
lang en_US.UTF-8
keyboard us
timezone UTC --utc
ignoredisk --only-use=vda
zerombr
clearpart --all --initlabel --disklabel=gpt --drives=vda
reqpart --add-boot
part / --grow --fstype=xfs --ondisk=vda
rootpw --lock
user --name=mados --groups=wheel --lock
# A text-mode install otherwise sets the installed system's default target
# to multi-user (text console; CI run #12).
xconfig --startxonboot
bootc --source-imgref containers-storage:{payload} --target-imgref {target}
poweroff
# Shipped in the installer image (image/installer/relabel.ks).
%include /usr/share/anaconda/mados-relabel.ks
"""


def need(tool: str) -> str:
    path = shutil.which(tool)
    if not path:
        raise SystemExit(f"iso_install: '{tool}' is required")
    return path


def iso_label(iso: Path) -> str:
    out = subprocess.run([need("xorriso"), "-indev", str(iso), "-pvd_info"], capture_output=True, text=True)
    m = re.search(r"^Volume Id\s*:\s*(\S+)", out.stdout + out.stderr, re.M)
    if not m:
        raise SystemExit("iso_install: cannot read the ISO volume label")
    return m.group(1)


def extract(iso: Path, inside: str, dest: Path) -> Path:
    subprocess.run(
        [need("xorriso"), "-osirrox", "on", "-indev", str(iso), "-extract", inside, str(dest)],
        check=True,
        capture_output=True,
    )
    return dest


def oemdrv_image(work: Path, kickstart: str) -> Path:
    img = work / "oemdrv.img"
    ks = work / "ks.cfg"
    ks.write_text(kickstart)
    img.unlink(missing_ok=True)
    # A 16 MiB image FILE in the work directory; never a block device.
    with img.open("wb") as f:
        f.truncate(16 * 1024 * 1024)
    subprocess.run([need("mkfs.vfat"), "-n", "OEMDRV", str(img)], check=True, capture_output=True)
    subprocess.run([need("mcopy"), "-i", str(img), str(ks), "::ks.cfg"], check=True)
    return img


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--iso", help="installer ISO (default: newest out/*.iso)")
    ap.add_argument("--payload", help="image reference inside the ISO (default: out/image-ref)")
    ap.add_argument("--target-ref", default="ghcr.io/madgenius9/mados:dev")
    ap.add_argument("--workdir", default=str(vm.OUT / "iso-install"))
    ap.add_argument("--install-timeout", type=int, default=3600)
    ns = ap.parse_args(argv)

    iso = Path(ns.iso) if ns.iso else vm.latest("*.iso")
    if not iso or not iso.exists():
        print("iso_install: no ISO; run `make iso` first", file=sys.stderr)
        return 2
    payload = ns.payload or (vm.OUT / "image-ref").read_text().strip()
    work = Path(ns.workdir)
    if work.exists():
        shutil.rmtree(work)
    work.mkdir(parents=True)

    label = iso_label(iso)
    kernel = extract(iso, "/images/pxeboot/vmlinuz", work / "vmlinuz")
    initrd = extract(iso, "/images/pxeboot/initrd.img", work / "initrd.img")
    oemdrv = oemdrv_image(work, KICKSTART.format(payload=payload, target=ns.target_ref))
    print(f"iso_install: ISO {iso.name} label={label} payload={payload}", flush=True)

    cfg = vm.VmConfig(
        workdir=work,
        iso=iso,
        kernel=kernel,
        initrd=initrd,
        # Same as the ISO's boot entry, plus a serial console for the log.
        append=f"inst.stage2=hd:LABEL={label} inst.text console=tty0 console=ttyS0,115200n8 enforcing=0",
        memory_mb=4096,
        cpus=4,
        audio="none",
        fresh=True,
        persist=True,
        extra=["-drive", f"file={oemdrv},if=virtio,format=raw,readonly=on"],
    )
    cmd = vm.build_command(cfg)
    print("qemu: " + " ".join(cmd), flush=True)
    started = time.monotonic()
    with (work / "qemu.log").open("w") as log:
        proc = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL)
        try:
            rc = proc.wait(timeout=ns.install_timeout)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
            print(f"[FAIL] install: not finished after {ns.install_timeout}s (see {cfg.serial_log})")
            return 1
    serial = cfg.serial_log.read_text(errors="replace") if cfg.serial_log.exists() else ""
    failed = re.search(r"(Traceback|An unknown error has occur|kickstart.*error|The following error occurred)", serial, re.I)
    if rc != 0 or failed:
        detail = failed.group(0) if failed else f"qemu exit {rc}"
        print(f"[FAIL] install: {detail} (see {cfg.serial_log})")
        return 1
    print(f"[PASS] install: unattended install finished in {time.monotonic() - started:.0f}s")

    # Boot the installed system and run the regular smoke test against it.
    import vm_smoke  # noqa: E402

    target = work / "install-target.qcow2"
    return vm_smoke.main(["--disk", str(target), "--workdir", str(work / "boot"), "--require-session"])


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
