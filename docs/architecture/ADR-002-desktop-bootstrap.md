# ADR-002: KDE Plasma as the temporary bootstrap desktop

- **Status:** Accepted (for 0.1–M8)
- **Date:** 2026-10-06

## Context

MadOS will eventually own its shell, launcher, panel, notifications, lock
screen and login experience. Writing a Wayland compositor and a full desktop
before 0.1 would delay a bootable system by years and add risk everywhere.

## Decision

Use **KDE Plasma 6 (Wayland)** as delivered by Fedora Kinoite as the
bootstrap desktop, with SDDM as display manager, and Konsole, Dolphin and
Firefox as terminal, file manager and browser.

MadOS customizes it only through **isolated, additive** mechanisms:

- a MadOS Plasma look-and-feel package (`org.mados.desktop`) and wallpaper
  package, generated from `product/product.toml`;
- system defaults in `/usr/share/mados/xdg`, prepended to `XDG_CONFIG_DIRS`
  by `/etc/xdg/plasma-workspace/env/10-mados-xdg.sh` — no Fedora/KDE file is
  overwritten;
- MadOS applications (`mados-settings`) installed alongside KDE's.

Why Plasma: Fedora ships it as a tested atomic desktop image (Kinoite), it is
Wayland-first, highly configurable without patching, and its components are
replaceable one at a time (the shell is a separate process from the
compositor).

## Consequences

- The visible desktop in 0.1 is recognisably Plasma with MadOS defaults.
  Documentation says so.
- Default wallpaper/look-and-feel application relies on Plasma reading the
  look-and-feel defaults at first login. This is **unverified** until the
  image boots in CI (see README status).

## Migration path

1. M4: MadOS Settings covers categories now delegated to KDE System Settings.
2. M9: a MadOS shell replaces `plasmashell` on top of KWin (or another
   wlroots/Smithay compositor), started by a `mados-session` systemd user
   target instead of Plasma's.
3. Later: MadOS login/lock screen replace SDDM/kscreenlocker; the base image
   moves from Kinoite to `fedora-bootc` plus an explicit package list
   (ADR-001 migration path), dropping unused KDE components.
