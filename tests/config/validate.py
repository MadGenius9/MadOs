#!/usr/bin/env python3
"""Static validation of MadOS configuration and image definition.

Runs without a Fedora host or network. Optional external validators
(systemd-analyze, desktop-file-validate, xmllint, shellcheck) are used when
installed and reported as skipped otherwise.

    python3 tests/config/validate.py [--staging DIR]

With --staging, the staged root tree (scripts/stage-system.py output) is
validated too; `make test` stages a tree first.
"""
from __future__ import annotations

import argparse
import configparser
import json
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import product as productlib  # noqa: E402

failures: list[str] = []
skipped: list[str] = []
passed = 0


def check(cond: bool, what: str) -> None:
    global passed
    if cond:
        passed += 1
    else:
        failures.append(what)


def run(cmd: list[str], what: str, cwd: Path | None = None) -> None:
    tool = cmd[0]
    if not shutil.which(tool):
        skipped.append(f"{what} ({tool} not installed)")
        return
    r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    out = (r.stdout + r.stderr).strip()
    check(r.returncode == 0, f"{what}: {out[:2000]}")


# ------------------------------------------------------------------ product

def validate_product() -> None:
    data = productlib.load()
    errors = productlib.validate(data)
    check(not errors, f"product.toml: {errors}")


def validate_branding_centralized() -> None:
    """The product name must not be hard-coded outside product/ (UI strings,
    unit descriptions, config). Comments are allowed to mention it."""
    name = productlib.load()["product"]["name"]
    pattern = re.compile(rf'"[^"\n]*\b{re.escape(name)}\b[^"\n]*"')
    for path in sorted(ROOT.glob("**/src/**/*.rs")):
        if "target" in path.parts:
            continue
        bad = [
            n
            for n, line in enumerate(path.read_text().splitlines(), 1)
            if not line.strip().startswith("//") and pattern.search(line)
        ]
        check(not bad, f"hard-coded product name in {path.relative_to(ROOT)} lines {bad}")
    for base in ("system", "image"):
        for path in sorted((ROOT / base).rglob("*")):
            if not path.is_file():
                continue
            bad = [
                n
                for n, line in enumerate(path.read_text(errors="replace").splitlines(), 1)
                if not line.strip().startswith(("#", "<!--", "//")) and name in line
            ]
            check(not bad, f"hard-coded product name in {path.relative_to(ROOT)} lines {bad}")


# ------------------------------------------------------------------ image

def validate_containerfile() -> None:
    cf = (ROOT / "image/Containerfile").read_text()
    instructions = [l for l in cf.splitlines() if l and not l.startswith("#") and not l.startswith(" ")]
    check(instructions[-1].strip() == "RUN bootc container lint", "Containerfile must end with RUN bootc container lint")
    froms = [l for l in instructions if l.startswith("FROM ")]
    check(len(froms) == 2, "Containerfile must have a builder and a final stage")
    check(froms[-1].strip() == "FROM ${BASE_REF}", "final stage must be FROM ${BASE_REF}")
    check("selinux=0" not in cf and "setenforce" not in cf, "Containerfile must not disable SELinux")
    # Anaconda sets multi-user.target after a text-mode install unless told
    # otherwise; the installed system must boot to the desktop (CI run #12).
    installer = (ROOT / "image/installer/Containerfile").read_text()
    check("xconfig --startxonboot" in installer, "installer interactive-defaults.ks must set xconfig --startxonboot")
    test_ks = (ROOT / "tests/smoke/iso_install.py").read_text()
    # Anaconda-written files need the installed system's labels (CI run #14).
    include = "%include /usr/share/anaconda/mados-relabel.ks"
    check("COPY relabel.ks /usr/share/anaconda/mados-relabel.ks" in installer and include in installer,
          "installer must ship relabel.ks and include it from interactive-defaults.ks")
    check(include in test_ks, "iso_install.py kickstart must include mados-relabel.ks")
    relabel = (ROOT / "image/installer/relabel.ks").read_text()
    check(relabel.count("%post") == 1 and "%end" in relabel, "relabel.ks must be one %post section")
    # Anaconda copies a selinux= boot option to the installed system (CI run #13).
    for name, text in (("image/installer/Containerfile", installer), ("tests/smoke/iso_install.py", test_ks)):
        code = "\n".join(l for l in text.splitlines() if not l.lstrip().startswith("#"))
        check(not re.search(r"\bselinux=0\b", code), f"{name}: installer must not boot with selinux=0 (use enforcing=0)")
    check("\nxconfig --startxonboot\n" in test_ks, "iso_install.py kickstart must set xconfig --startxonboot")
    env = (ROOT / "image/config.env").read_text()
    keys = dict(
        l.split("=", 1) for l in env.splitlines() if l and not l.startswith("#") and "=" in l
    )
    for k in ("FEDORA_VERSION", "BASE_IMAGE", "BUILDER_IMAGE", "IMAGE_NAME", "IMAGE_BUILDER", "ROOTFS"):
        check(bool(keys.get(k)), f"image/config.env: {k} must be set")
    check(keys.get("FEDORA_VERSION", "").isdigit(), "FEDORA_VERSION must be a number")
    check(keys.get("ROOTFS") in {"xfs", "ext4", "btrfs"}, "ROOTFS must be xfs, ext4 or btrfs")


def parse_os_release(text: str) -> dict[str, str]:
    out: dict[str, str] = {}
    for line in text.splitlines():
        if not line or line.startswith("#"):
            continue
        k, _, v = line.partition("=")
        vals = shlex.split(v) if v else [""]
        out[k] = vals[0] if vals else ""
    return out


def validate_os_release_merge(staging: Path | None) -> None:
    merge = ROOT / "system/rootfs/usr/libexec/mados/merge-os-release"
    base_text = (ROOT / "tests/config/fixtures/os-release.kinoite").read_text()
    data = productlib.load()
    version = productlib.full_version(data)
    with tempfile.TemporaryDirectory() as td:
        frag = Path(td) / "frag"
        if staging:
            frag.write_text((staging / "usr/lib/mados/os-release.mados").read_text())
        else:
            frag.write_text(f'NAME="{data["product"]["name"]}"\nPRETTY_NAME="x"\nIMAGE_VERSION="{version}"\n')
        r = subprocess.run(["sh", str(merge), str(ROOT / "tests/config/fixtures/os-release.kinoite"), str(frag)],
                           capture_output=True, text=True)
        check(r.returncode == 0, f"merge-os-release failed: {r.stderr}")
        merged_lines = [l for l in r.stdout.splitlines() if l]
        keys = [l.split("=", 1)[0] for l in merged_lines]
        check(len(keys) == len(set(keys)), f"merged os-release has duplicate keys: {keys}")
        merged = parse_os_release(r.stdout)
        base = parse_os_release(base_text)
        for k in ("ID", "VERSION_ID", "VARIANT_ID", "PLATFORM_ID", "CPE_NAME"):
            check(merged.get(k) == base.get(k), f"merge must keep base {k}")
        check(merged.get("NAME") == data["product"]["name"], "merge must set NAME")
        check(merged.get("MADOS_BASE_PRETTY_NAME") == base["PRETTY_NAME"], "merge must record base PRETTY_NAME")
        check(merged.get("IMAGE_VERSION") == version, "merge must set IMAGE_VERSION")


# ------------------------------------------------------------------ system files

def unit_parser(path: Path) -> configparser.ConfigParser:
    cp = configparser.ConfigParser(strict=False, interpolation=None, delimiters=("=",))
    cp.optionxform = str  # case-sensitive keys
    cp.read(path)
    return cp


def validate_units(tree: Path) -> None:
    units = sorted(tree.glob("usr/lib/systemd/system/*.service")) + sorted(tree.glob("usr/lib/systemd/user/*.service"))
    check(len(units) >= 3, "expected at least 3 MadOS units")
    for u in units:
        cp = unit_parser(u)
        rel = u.relative_to(tree)
        check(cp.has_section("Unit") and cp.has_section("Service"), f"{rel}: needs [Unit] and [Service]")
        check(bool(cp.get("Unit", "Description", fallback="")), f"{rel}: Description missing")
        exe = cp.get("Service", "ExecStart", fallback="").split()
        check(bool(exe) and exe[0].startswith("/"), f"{rel}: ExecStart must use an absolute path")
        check(cp.get("Service", "NoNewPrivileges", fallback="") == "yes", f"{rel}: NoNewPrivileges=yes required")
        check(bool(cp.get("Service", "SyslogIdentifier", fallback="")), f"{rel}: SyslogIdentifier required (journald)")
        if "boot-report" in u.name:
            check(cp.get("Service", "ProtectHome", fallback="") != "yes",
                  f"{rel}: ProtectHome=yes hides /run/user, where the session check report lives")
        if exe and tree != ROOT / "system/rootfs":
            check((tree / exe[0].lstrip("/")).exists(), f"{rel}: ExecStart binary {exe[0]} not staged")
    if tree != ROOT / "system/rootfs" and shutil.which("systemd-analyze"):
        # --root resolves ExecStart and unit references inside the staged tree.
        sys_units = [str(u) for u in sorted(tree.glob("usr/lib/systemd/system/*.service"))]
        r = subprocess.run(["systemd-analyze", "verify", "--man=no", f"--root={tree}", *sys_units],
                           capture_output=True, text=True)
        noise = re.compile(r"(Failed to (open|connect|load)|No such file|not found|unknown|Cannot add dependency)", re.I)
        real = [l for l in (r.stdout + r.stderr).splitlines() if l and "mados" in l and not noise.search(l)]
        check(not real, f"systemd-analyze verify: {real}")
    elif tree != ROOT / "system/rootfs":
        skipped.append("systemd-analyze verify (not installed)")


def validate_xml(tree: Path, staged: bool) -> None:
    for path in sorted(list(tree.rglob("*.conf")) + list(tree.rglob("*.policy"))):
        if not path.read_text(errors="replace").lstrip().startswith("<?xml"):
            continue
        try:
            ET.parse(path)
            check(True, "")
        except ET.ParseError as e:
            check(False, f"{path.relative_to(tree)}: invalid XML: {e}")
    if staged:
        policy = tree / "usr/share/polkit-1/actions/org.mados.system.policy"
        check(policy.exists(), "polkit policy not staged")
        if policy.exists():
            root = ET.parse(policy).getroot()
            ids = {a.get("id") for a in root.findall("action")}
            for action in ("org.mados.system.power", "org.mados.system.updates.check", "org.mados.system.updates.apply"):
                check(action in ids, f"polkit action {action} missing")
            apply = next((a for a in root.findall("action") if a.get("id") == "org.mados.system.updates.apply"), None)
            if apply is not None:
                check(all((apply.findtext(f"defaults/{k}") or "").startswith("auth_admin")
                          for k in ("allow_any", "allow_inactive", "allow_active")),
                      "updates.apply must always require admin authentication")
            for a in root.findall("action"):
                any_ = a.findtext("defaults/allow_any")
                check(any_ != "yes", f"polkit {a.get('id')}: allow_any must not be yes")
            check("@" not in policy.read_text(), "polkit policy has unexpanded template variables")


def validate_dbus_activation(tree: Path) -> None:
    for path in sorted(tree.glob("usr/share/dbus-1/*services/*.service")):
        cp = unit_parser(path)
        rel = path.relative_to(tree)
        name = cp.get("D-BUS Service", "Name", fallback="")
        check(path.stem == name, f"{rel}: file name must match bus name {name}")
        check(bool(cp.get("D-BUS Service", "SystemdService", fallback="")), f"{rel}: SystemdService required")


def validate_staging(tree: Path) -> None:
    info = json.loads((tree / "usr/lib/mados/build-info.json").read_text())
    check(info.get("version") == productlib.full_version(productlib.load()), "build-info version mismatch")
    for key in ("build_id", "git_commit", "build_time", "base_image", "variant"):
        check(bool(info.get(key)), f"build-info.json: {key} empty")
    for d in sorted(tree.glob("usr/share/applications/*.desktop")):
        run(["desktop-file-validate", str(d)], f"desktop-file-validate {d.name}")
    for j in sorted(tree.rglob("metadata.json")):
        try:
            json.loads(j.read_text())
            check(True, "")
        except json.JSONDecodeError as e:
            check(False, f"{j.relative_to(tree)}: {e}")
    kded = tree / "usr/share/mados/xdg/kded5rc"
    check(kded.exists() and "[Module-kded_plasma_welcome]\nautoload=false" in kded.read_text(),
          "KDE Welcome Center autostart must be off (MadOS first-run window replaces it)")
    env_script = tree / "etc/xdg/plasma-workspace/env/10-mados-xdg.sh"
    check(env_script.exists(), "Plasma env script not staged")
    check(not (tree / "etc/xdg/kdeglobals").exists(), "must not overwrite /etc/xdg/kdeglobals (use /usr/share/mados/xdg)")
    run(["sh", "-n", str(env_script)], "env script syntax")
    variant = info.get("variant")
    # Fedora 44 KDE's display manager is Plasma Login Manager; it ignores
    # /etc/sddm.conf.d. Marking Plasma Setup done without a working autologin
    # leaves dev images at the login screen (CI run #6).
    autologin = tree / "etc/plasmalogin.conf.d/50-mados-dev-autologin.conf"
    check(autologin.exists() == (variant == "dev"), "Plasma Login Manager autologin must exist only in dev variant")
    check(not (tree / "etc/sddm.conf.d").exists(), "etc/sddm.conf.d is ignored by Plasma Login Manager (Fedora 44)")
    if autologin.exists():
        conf = autologin.read_text()
        check(re.search(r"^\[Autologin\]$", conf, re.M) is not None, "autologin config lacks [Autologin]")
        check(re.search(r"^User=[a-z_][a-z0-9_-]*$", conf, re.M) is not None, "autologin config lacks a valid User=")
        check(re.search(r"^Session=plasma$", conf, re.M) is not None, "autologin must start the plasma (Wayland) session")
    if (tree / "etc/plasma-setup-done").exists():
        check(autologin.exists(), "plasma-setup-done without autologin leaves the VM at the login screen")
    check((tree / "etc/plasma-setup-done").exists() == (variant == "dev"),
          "Plasma Setup may be marked done only in dev images (release images need it to create the first user)")
    session_check = tree / "usr/lib/systemd/user/mados-session-check.service"
    check(session_check.exists() == (variant == "dev"), "session check unit must exist only in dev variant")
    link = tree / "usr/lib/systemd/user/graphical-session.target.wants/mados-session-check.service"
    check(link.is_symlink() == (variant == "dev"), "session check must be enabled only in dev variant")
    if link.is_symlink():
        check(link.resolve() == session_check.resolve(), "session check enable link must point at the unit")
    creds = [
        str(p.relative_to(tree))
        for p in tree.rglob("*")
        if p.is_file()
        and p.suffix in {".toml", ".json", ".conf", ".desktop", ".service"}
        and re.search(r"(password|secret|api[_-]?key|token)\s*=\s*\S", p.read_text(errors="replace"), re.I)
    ]
    check(not creds, f"possible hard-coded credentials in {creds}")


# image-builder data/distrodefs/bootc-generic/imagetypes.yaml (supported_options_disk)
BOOTC_DISK_CUSTOMIZATIONS = {
    "bootloader", "directories", "disk", "files", "group", "ignition", "kernel", "user", "sshd",
}


def validate_blueprint() -> None:
    """The disk blueprint may only use customizations image-builder supports
    for bootc disk images (an unsupported one fails the build late, in CI)."""
    import os
    import tomllib

    env = dict(os.environ, MADOS_BLUEPRINT_ONLY="1", MADOS_DEV_PASSWORD="validation-only")
    env.pop("MADOS_DEV_SSH_PUBKEY", None)
    r = subprocess.run(["sh", str(ROOT / "scripts/build-disk.sh")], env=env, capture_output=True, text=True)
    if r.returncode != 0 and "not found" in r.stderr:
        skipped.append(f"blueprint check ({r.stderr.strip()})")
        return
    check(r.returncode == 0, f"build-disk.sh blueprint-only failed: {r.stderr.strip()}")
    bp_path = ROOT / "out/build/disk/blueprint.toml"
    if r.returncode != 0 or not bp_path.exists():
        return
    bp = tomllib.loads(bp_path.read_text())
    used = set(bp.get("customizations", {}))
    check(used <= BOOTC_DISK_CUSTOMIZATIONS, f"blueprint uses unsupported customizations: {sorted(used - BOOTC_DISK_CUSTOMIZATIONS)}")
    kernel = bp.get("customizations", {}).get("kernel", {})
    check(set(kernel) <= {"append"}, "only customizations.kernel.append is supported for bootc disks")
    users = bp.get("customizations", {}).get("user", [])
    check(any(u.get("name") == "mados" for u in users), "blueprint must define the development user")
    check(not (ROOT / "out/dev-credentials.txt").exists() or "validation-only" not in (ROOT / "out/dev-credentials.txt").read_text(),
          "blueprint-only mode must not write credentials")


def validate_shell() -> None:
    scripts = [str(p) for p in sorted(ROOT.glob("scripts/*.sh"))] + [
        str(ROOT / "system/rootfs/usr/libexec/mados/merge-os-release"),
        str(ROOT / "system/rootfs/usr/libexec/mados/image-finalize"),
        str(ROOT / "system/rootfs/usr/libexec/mados/is-intel-cpu"),
    ]
    scripts += [str(p) for p in sorted(ROOT.glob("tests/**/*.sh"))]
    run(["shellcheck", "-x", *scripts], "shellcheck", cwd=ROOT)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--staging", type=Path)
    args = ap.parse_args()
    validate_product()
    validate_branding_centralized()
    validate_containerfile()
    validate_blueprint()
    validate_os_release_merge(args.staging)
    validate_units(ROOT / "system/rootfs")
    validate_xml(ROOT / "system", staged=False)
    validate_shell()
    if args.staging:
        validate_units(args.staging)
        validate_xml(args.staging, staged=True)
        validate_dbus_activation(args.staging)
        validate_staging(args.staging)
    for s in skipped:
        print(f"SKIP {s}")
    for f in failures:
        print(f"FAIL {f}")
    print(f"config validation: {passed} checks passed, {len(failures)} failed, {len(skipped)} skipped")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
