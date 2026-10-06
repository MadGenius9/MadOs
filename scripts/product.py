#!/usr/bin/env python3
"""Read MadOS product metadata from product/product.toml.

Usage:
  product.py get <dotted.key>     print one value (e.g. product.name)
  product.py version              print the full version string (0.1.0-dev)
  product.py env                  print shell-safe KEY=VALUE lines
  product.py validate             validate the schema; non-zero exit on error

This is the only way shell/Make tooling should obtain product strings.
"""
from __future__ import annotations

import re
import shlex
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PRODUCT_FILE = ROOT / "product" / "product.toml"

REQUIRED = {
    "product": {"name": str, "full_name": str, "id": str, "tagline": str},
    "version": {"major": int, "minor": int, "patch": int, "pre": str, "codename": str},
    "vendor": {"name": str, "url": str},
    "urls": {"home": str, "bugs": str, "docs": str},
    "branding": {"logo": str, "wallpaper": str, "accent": str, "accent_secondary": str},
}
HEX_COLOUR = re.compile(r"^#[0-9a-fA-F]{6}$")
PRODUCT_ID = re.compile(r"^[a-z0-9][a-z0-9._-]*$")


def load(path: Path = PRODUCT_FILE) -> dict:
    with path.open("rb") as f:
        return tomllib.load(f)


def full_version(data: dict) -> str:
    v = data["version"]
    base = f'{v["major"]}.{v["minor"]}.{v["patch"]}'
    return f'{base}-{v["pre"]}' if v["pre"] else base


def lookup(data: dict, dotted: str):
    if dotted == "version.full":
        return full_version(data)
    node = data
    for part in dotted.split("."):
        if not isinstance(node, dict) or part not in node:
            raise KeyError(dotted)
        node = node[part]
    return node


def validate(data: dict, base: Path = PRODUCT_FILE.parent) -> list[str]:
    errors: list[str] = []
    if data.get("schema") != 1:
        errors.append("schema must be 1")
    for table, fields in REQUIRED.items():
        section = data.get(table)
        if not isinstance(section, dict):
            errors.append(f"missing table [{table}]")
            continue
        for key, typ in fields.items():
            if key not in section:
                errors.append(f"missing {table}.{key}")
            elif not isinstance(section[key], typ) or isinstance(section[key], bool):
                errors.append(f"{table}.{key} must be {typ.__name__}")
    if errors:
        return errors
    if not PRODUCT_ID.match(data["product"]["id"]):
        errors.append("product.id must be lowercase [a-z0-9._-]")
    if data["version"]["pre"] and not re.match(r"^[0-9A-Za-z.-]+$", data["version"]["pre"]):
        errors.append("version.pre must be a semver pre-release identifier")
    for key in ("accent", "accent_secondary"):
        if not HEX_COLOUR.match(data["branding"][key]):
            errors.append(f"branding.{key} must be #RRGGBB")
    for key in ("logo", "wallpaper"):
        if not (base / data["branding"][key]).is_file():
            errors.append(f"branding.{key} file not found: {data['branding'][key]}")
    return errors


def env_lines(data: dict) -> list[str]:
    pairs = {
        "MADOS_PRODUCT_NAME": data["product"]["name"],
        "MADOS_PRODUCT_ID": data["product"]["id"],
        "MADOS_VERSION": full_version(data),
        "MADOS_VENDOR": data["vendor"]["name"],
        "MADOS_HOME_URL": data["urls"]["home"],
        "MADOS_BUG_URL": data["urls"]["bugs"],
        "MADOS_ACCENT": data["branding"]["accent"],
    }
    return [f"{k}={shlex.quote(str(v))}" for k, v in pairs.items()]


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__, file=sys.stderr)
        return 2
    data = load()
    cmd = argv[1]
    if cmd == "get" and len(argv) == 3:
        try:
            print(lookup(data, argv[2]))
        except KeyError:
            print(f"product.py: unknown key {argv[2]!r}", file=sys.stderr)
            return 1
        return 0
    if cmd == "version":
        print(full_version(data))
        return 0
    if cmd == "env":
        print("\n".join(env_lines(data)))
        return 0
    if cmd == "validate":
        errors = validate(data)
        for e in errors:
            print(f"product.toml: {e}", file=sys.stderr)
        if not errors:
            print(f"product.toml OK ({data['product']['name']} {full_version(data)})")
        return 1 if errors else 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
