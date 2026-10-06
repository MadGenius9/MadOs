#!/usr/bin/env python3
"""Stage MadOS components and configuration into a root-filesystem tree.

Used by image/Containerfile (builder stage) and by `make stage` for local
inspection/validation. Everything product-specific is generated from
product/product.toml here, so no other file hard-codes branding.

    stage-system.py --bin-dir target/release --destdir out/staging \
        [--variant dev|release] [--build-id ID] [--git-commit SHA] \
        [--base-image REF] [--dev-user NAME]

The staged tree is copied over the base image ("COPY --from=builder /staging/ /"),
after which /usr/libexec/mados/image-finalize runs inside the image.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import product as productlib  # noqa: E402

BINARIES = {
    # name in bin-dir -> install path
    "madosctl": "usr/bin/madosctl",
    "mados-settings": "usr/bin/mados-settings",
    "mados-first-run": "usr/bin/mados-first-run",
    "mados-ai": "usr/libexec/mados/mados-ai",
    "mados-daemon": "usr/libexec/mados/mados-daemon",
}
DEV_USER_RE = re.compile(r"^[a-z_][a-z0-9_-]{0,31}$")


def write(dest: Path, rel: str, content: str, mode: int = 0o644) -> Path:
    p = dest / rel
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(content, encoding="utf-8")
    p.chmod(mode)
    return p


def copy_tree(src: Path, dest: Path) -> None:
    for path in sorted(src.rglob("*")):
        rel = path.relative_to(src)
        target = dest / rel
        if path.is_dir():
            target.mkdir(parents=True, exist_ok=True)
        else:
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, target)


def osr_quote(value: str) -> str:
    """Quote a value for os-release (shell-compatible double quotes)."""
    escaped = re.sub(r'([\\"`$])', r"\\\1", value)
    return f'"{escaped}"'


def hex_to_rgb(colour: str) -> str:
    c = colour.lstrip("#")
    return ",".join(str(int(c[i : i + 2], 16)) for i in (0, 2, 4))


def render_png(svg: Path, png: Path, width: int, height: int) -> bool:
    tool = shutil.which("rsvg-convert")
    if not tool:
        return False
    png.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run([tool, "-w", str(width), "-h", str(height), "-o", str(png), str(svg)], check=True)
    return True


def stage(args: argparse.Namespace) -> list[str]:
    data = productlib.load()
    errors = productlib.validate(data)
    if errors:
        raise SystemExit("product.toml invalid: " + "; ".join(errors))
    name = data["product"]["name"]
    pid = data["product"]["id"]
    version = productlib.full_version(data)
    vendor = data["vendor"]["name"]
    urls = data["urls"]
    branding = data["branding"]
    assets = ROOT / "product"
    dest: Path = args.destdir
    notes: list[str] = []

    if dest.exists() and any(dest.iterdir()) and not args.force:
        raise SystemExit(f"{dest} is not empty (use --force to stage over it)")
    dest.mkdir(parents=True, exist_ok=True)

    # Binaries.
    for binary, rel in BINARIES.items():
        src = args.bin_dir / binary
        if not src.is_file():
            raise SystemExit(f"missing binary {src} (run `make build` first)")
        target = dest / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, target)
        target.chmod(0o755)

    # Static overlay + variant overlay.
    copy_tree(ROOT / "system" / "rootfs", dest)
    variant_dir = ROOT / "system" / "variants" / args.variant
    if variant_dir.is_dir():
        copy_tree(variant_dir, dest)

    # Product metadata and build info.
    (dest / "usr/lib/mados").mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT / "product" / "product.toml", dest / "usr/lib/mados/product.toml")
    build_info = {
        "build_id": args.build_id,
        "version": version,
        "git_commit": args.git_commit,
        "build_time": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "base_image": args.base_image,
        "variant": args.variant,
    }
    if os.environ.get("SOURCE_DATE_EPOCH"):
        ts = dt.datetime.fromtimestamp(int(os.environ["SOURCE_DATE_EPOCH"]), dt.timezone.utc)
        build_info["build_time"] = ts.isoformat().replace("+00:00", "Z")
    write(dest, "usr/lib/mados/build-info.json", json.dumps(build_info, indent=2) + "\n")

    # os-release fragment (merged into the base file by image-finalize).
    pretty = f"{name} {version}"
    osr = {
        "NAME": name,
        "PRETTY_NAME": pretty,
        "IMAGE_ID": pid,
        "IMAGE_VERSION": version,
        "BUILD_ID": args.build_id,
        "HOME_URL": urls["home"],
        "DOCUMENTATION_URL": urls["docs"],
        "BUG_REPORT_URL": urls["bugs"],
        "SUPPORT_URL": urls["bugs"],
        "LOGO": f"{pid}-logo",
        "MADOS_VERSION": version,
        "MADOS_VARIANT": args.variant,
    }
    write(dest, "usr/lib/mados/os-release.mados", "".join(f"{k}={osr_quote(v)}\n" for k, v in osr.items()))

    # Logos and icons.
    share = dest / "usr/share/mados"
    share.mkdir(parents=True, exist_ok=True)
    shutil.copy2(assets / branding["logo"], share / "logo.svg")
    shutil.copy2(assets / branding["wallpaper"], share / "wallpaper.svg")
    icons = dest / "usr/share/icons/hicolor/scalable/apps"
    icons.mkdir(parents=True, exist_ok=True)
    shutil.copy2(assets / branding["logo"], icons / "org.mados.Settings.svg")
    shutil.copy2(assets / branding["logo"], icons / f"{pid}-logo.svg")

    # Desktop entries.
    write(
        dest,
        "usr/share/applications/org.mados.Settings.desktop",
        f"""[Desktop Entry]
Type=Application
Name={name} Settings
Comment=Configure {name}
Exec=mados-settings
Icon=org.mados.Settings
Categories=Settings;System;
Keywords=settings;system;about;power;updates;assistant;storage;
StartupNotify=true
""",
    )
    write(
        dest,
        "usr/share/applications/org.mados.About.desktop",
        f"""[Desktop Entry]
Type=Application
Name=About {name}
Comment=Version and hardware information
Exec=mados-settings --page=about
Icon={pid}-logo
Categories=System;
StartupNotify=true
""",
    )

    # First-run welcome: once per user at login (XDG autostart), and from the menu.
    write(
        dest,
        "etc/xdg/autostart/org.mados.FirstRun.desktop",
        f"""[Desktop Entry]
Type=Application
Name=Welcome to {name}
Exec=mados-first-run --autostart
Icon={pid}-logo
NoDisplay=true
X-KDE-autostart-phase=2
""",
    )
    write(
        dest,
        "usr/share/applications/org.mados.FirstRun.desktop",
        f"""[Desktop Entry]
Type=Application
Name=Welcome to {name}
Comment=Introduction to {name}
Exec=mados-first-run
Icon={pid}-logo
Categories=System;
""",
    )

    # polkit policy.
    policy = (ROOT / "system/templates/org.mados.system.policy").read_text()
    policy = policy.replace("@VENDOR@", vendor).replace("@VENDOR_URL@", data["vendor"]["url"])
    write(dest, "usr/share/polkit-1/actions/org.mados.system.policy", policy)

    # Wallpaper package (Plasma) and look-and-feel defaults.
    wp = f"usr/share/wallpapers/{name}"
    wp_meta = {
        "KPlugin": {
            "Id": name,
            "Name": name,
            "Authors": [{"Name": vendor}],
            "License": "CC0-1.0",
        }
    }
    write(dest, f"{wp}/metadata.json", json.dumps(wp_meta, indent=2) + "\n")
    wall_svg = assets / branding["wallpaper"]
    if render_png(wall_svg, dest / wp / "contents/images/3840x2160.png", 3840, 2160):
        render_png(wall_svg, dest / wp / "contents/screenshot.png", 400, 225)
    else:
        (dest / wp / "contents/images").mkdir(parents=True, exist_ok=True)
        shutil.copy2(wall_svg, dest / wp / "contents/images/3840x2160.svg")
        notes.append("rsvg-convert not found: wallpaper staged as SVG")

    lnf_id = "org.mados.desktop"
    lnf = f"usr/share/plasma/look-and-feel/{lnf_id}"
    lnf_meta = {
        "KPackageStructure": "Plasma/LookAndFeel",
        "KPlugin": {
            "Id": lnf_id,
            "Name": name,
            "Description": f"{name} defaults",
            "Authors": [{"Name": vendor}],
            "License": "Apache-2.0",
            "Version": version,
            "Website": urls["home"],
        },
    }
    write(dest, f"{lnf}/metadata.json", json.dumps(lnf_meta, indent=2) + "\n")
    accent = hex_to_rgb(branding["accent"])
    write(
        dest,
        f"{lnf}/contents/defaults",
        f"""[kdeglobals][KDE]
widgetStyle=Breeze

[kdeglobals][General]
ColorScheme=BreezeDark
AccentColor={accent}

[plasmarc][Theme]
name=default

[Wallpaper]
Image={name}
""",
    )
    # MadOS desktop defaults live in their own XDG config dir under /usr
    # (immutable, updated with the image) instead of overwriting files that
    # Fedora ships in /etc/xdg. Plasma sources the env script at session start.
    write(
        dest,
        "usr/share/mados/xdg/kdeglobals",
        f"""# Generated from product/product.toml. Users can override in ~/.config.
[KDE]
LookAndFeelPackage={lnf_id}

[General]
AccentColor={accent}
""",
    )
    write(
        dest,
        "etc/xdg/plasma-workspace/env/10-mados-xdg.sh",
        """# Generated by scripts/stage-system.py: put the MadOS defaults first in the
# system configuration search path (user configuration still wins).
case ":${XDG_CONFIG_DIRS:-}:" in
    *:/usr/share/mados/xdg:*) ;;
    *) export XDG_CONFIG_DIRS="/usr/share/mados/xdg:${XDG_CONFIG_DIRS:-/etc/xdg}" ;;
esac
""",
    )

    # Development-only autologin.
    if args.variant == "dev":
        tmpl = (ROOT / "system/templates/plasmalogin-dev-autologin.conf").read_text()
        write(dest, "etc/plasmalogin.conf.d/50-mados-dev-autologin.conf", tmpl.replace("@DEV_USER@", args.dev_user))
        notes.append(f"dev variant: Plasma Login Manager autologin for user {args.dev_user!r}")
        # Enable the dev session check for every user (global user unit).
        wants = dest / "usr/lib/systemd/user/graphical-session.target.wants"
        wants.mkdir(parents=True, exist_ok=True)
        link = wants / "mados-session-check.service"
        if not link.is_symlink():
            link.symlink_to("../mados-session-check.service")
        notes.append("dev variant: mados-session-check.service enabled")
    return notes


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--bin-dir", type=Path, required=True)
    ap.add_argument("--destdir", type=Path, required=True)
    ap.add_argument("--variant", choices=["dev", "release"], default="dev")
    ap.add_argument("--build-id", default="local")
    ap.add_argument("--git-commit", default="unknown")
    ap.add_argument("--base-image", default="unknown")
    ap.add_argument("--dev-user", default="mados")
    ap.add_argument("--force", action="store_true", help="allow staging into a non-empty directory")
    args = ap.parse_args(argv)
    if not DEV_USER_RE.match(args.dev_user):
        ap.error("--dev-user must be a valid lowercase user name")
    for value in (args.build_id, args.git_commit, args.base_image):
        if any(c in value for c in "\n\r\"\\`$"):
            ap.error("build metadata must not contain quotes, backslashes, $ or newlines")
    for note in stage(args):
        print(f"stage-system: {note}")
    print(f"stage-system: staged {args.variant} tree in {args.destdir}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
