# MadOS architecture overview

MadOS is a desktop operating system for x86_64 UEFI laptops. Version 0.1
reuses Linux and proven open-source infrastructure, and builds MadOS-owned
system services and applications on top. Over time the visible experience
(shell, settings, login, assistant) becomes MadOS-owned. Upstream components
are **not** presented as MadOS technology.

## Layers

```
┌────────────────────────────────────────────────────────────────────┐
│ MadOS applications        mados-settings (GTK 4)   madosctl (CLI)  │  MadOS-owned
│                           mados-ai (assistant service, user)       │
├────────────────────────────────────────────────────────────────────┤
│ MadOS system API (D-Bus)  org.mados.System1   (mados-daemon, root) │  MadOS-owned
│                           org.mados.Assistant1 (mados-ai, user)    │
│ Policy                    polkit actions org.mados.*               │
├────────────────────────────────────────────────────────────────────┤
│ Bootstrap desktop         KDE Plasma 6 (Wayland), SDDM, KWin,      │  upstream
│ (temporary, ADR-002)      Konsole, Dolphin, Firefox                │  (Fedora Kinoite)
├────────────────────────────────────────────────────────────────────┤
│ Platform services         systemd, logind, D-Bus, polkit,          │  upstream
│                           NetworkManager, BlueZ, PipeWire, Flatpak │
│                           SELinux, Mesa, Wayland                   │
├────────────────────────────────────────────────────────────────────┤
│ Image / update            bootc + OSTree (A/B deployments,         │  upstream
│                           rollback), built by podman+image-builder │
├────────────────────────────────────────────────────────────────────┤
│ Boot / kernel             shim + GRUB (Secure Boot capable),       │  upstream
│                           Linux kernel, linux-firmware             │  (Fedora 44)
└────────────────────────────────────────────────────────────────────┘
```

## Upstream dependencies (honest inventory)

| Function | Component | Source | MadOS role |
|---|---|---|---|
| Kernel, firmware, drivers | Linux, linux-firmware, Mesa | Fedora 44 | none (reused) |
| Boot | shim, GRUB2, bootupd | Fedora | none (reused) |
| Image & updates | bootc, OSTree | Fedora | MadOS image is a bootc container |
| Image building | podman, osbuild image-builder | Fedora/osbuild | driven by `scripts/` |
| Init, sessions | systemd, logind | Fedora | MadOS units and markers |
| IPC, authorization | D-Bus (dbus-broker), polkit | Fedora | MadOS interfaces and actions |
| Networking | NetworkManager | Fedora | used by mados-ai (Wi-Fi radio) |
| Bluetooth | BlueZ | Fedora | used by mados-ai (adapter power) |
| Audio | PipeWire, WirePlumber | Fedora | volume/mute via `mados-audio` (PulseAudio-compatible API) |
| Desktop session | KDE Plasma 6, KWin, SDDM | Fedora Kinoite | temporary bootstrap; MadOS defaults applied |
| Terminal | Konsole | KDE | used as-is in 0.1 |
| File manager | Dolphin | KDE | used as-is in 0.1 |
| Browser | Firefox | Mozilla/Fedora | used as-is in 0.1 |
| Apps | Flatpak | Fedora | none yet |
| Security | SELinux (enforcing) | Fedora | kept enabled |

## MadOS components

| Component | 0.1 status | Implementation |
|---|---|---|
| **mados-core** | implemented | `crates/mados-core`: product metadata, system information, logging, stable names |
| **mados-audio** | implemented | `crates/mados-audio`: output devices, volume, mute through the user's sound server; separate crate so root services never link audio libraries |
| **mados-api** | implemented | `crates/mados-api`: D-Bus contracts and client proxies; `network` (NetworkManager), `bluetooth` (BlueZ), `display` (sysfs + logind) and `accounts` (AccountsService) clients; `mock-system-services` example for UI work |
| **mados-daemon** (system service; part of *mados-permissions*) | implemented | `services/mados-daemon`: `org.mados.System1`; polkit-checked power actions via logind; bootc status |
| **mados-permissions** | partial | polkit actions in `system/templates/org.mados.system.policy`, enforced in mados-daemon; assistant policy in `services/mados-ai/src/policy.rs` |
| **mados-ai** | partial (architecture + rule-based provider) | `services/mados-ai`; see [ADR-003](ADR-003-mados-ai.md) |
| **mados-settings** | partial | `apps/mados-settings` (GTK 4); see "Settings" below |
| **mados-about** | implemented | the About page of mados-settings (`mados-settings --page=about`), `madosctl about` |
| **mados-update** | partial | check/install/roll back via mados-daemon + Settings; no published images or automatic rollback yet; see [ADR-004](ADR-004-updates.md) |
| **mados-session** | partial | Plasma session + MadOS defaults (look-and-feel, accent, wallpaper) from `scripts/stage-system.py`; dev autologin |
| **mados-shell** | not started | Plasma is the shell until M9; see [ADR-002](ADR-002-desktop-bootstrap.md) |
| **mados-files** | not started | Dolphin is used |
| **mados-terminal** | not started | Konsole is used |
| **mados-first-run** | not started | planned for M6 (installer) |

There are deliberately no empty directories for components that do not exist yet.

## Repository layout

```
product/            product.toml (single source of truth) + placeholder artwork
crates/             shared Rust libraries (mados-core, mados-api)
services/           long-running services (mados-daemon, mados-ai)
apps/               user-facing programs (mados-settings, madosctl)
system/rootfs/      files copied verbatim into the image (units, D-Bus, polkit…)
system/templates/   files rendered with product values at stage time
system/variants/    per-variant overlays (dev)
image/              Containerfile, config.env, installer/ (ISO)
scripts/            build tooling (stage-system.py, build-*.sh, vm.py, …)
tests/config/       static configuration validation
tests/smoke/        VM smoke test and its self-test
docs/               architecture (ADRs), development, hardware, security
```

## System API

All MadOS GUI applications and the assistant change system state **only**
through D-Bus interfaces of services that own that state. No GUI runs a shell
or a privileged command.

`org.mados.System1` (system bus, `/org/mados/System1`, mados-daemon, root):

| Member | Kind | Authorization |
|---|---|---|
| `GetSystemInfo() → s` (JSON `SystemInfo`) | method | none (read-only, non-sensitive) |
| `GetUpdateStatus() → s` (JSON `UpdateStatus`) | method | none (read-only) |
| `PowerOff()`, `Reboot()` | method | polkit `org.mados.system.power`, subject = calling bus name |
| `CheckForUpdate() → s` (JSON `UpdateStatus` incl. `cached_update`) | method | polkit `org.mados.system.updates.check` |
| `StartUpdate()`, `StartRollback()` | method (background job) | polkit `org.mados.system.updates.apply` (admin) |
| `UpdateJobFinished(s operation, b success, s message)` | signal | — |
| `Busy` (b) | property | none |
| `Version` (s), `ApiLevel` (u) | property | none |

Errors: `org.mados.System1.Error.NotAuthorized`, `…Error.Failed`, `…Error.Busy`
(another update/rollback job is running). `ApiLevel` is 2.

`org.mados.Assistant1` (session bus, `/org/mados/Assistant1`, mados-ai, user):
`Ask(s) → s`, `Confirm(s) → s`, `Cancel(s)`, properties `Provider` and `Local`
(true when requests are processed on the device). Replies are
JSON `AssistantReply` (`crates/mados-api/src/lib.rs`).

Service boundaries for other areas (planned; until then the owning upstream
service's own D-Bus API is used directly, which already enforces polkit):

| Area | Owner today | MadOS API plan |
|---|---|---|
| Wi-Fi, networking | NetworkManager D-Bus | **done for status + radio**: `mados_api::network` is the single client used by Settings and mados-ai (NM polkit applies to the user) |
| Bluetooth | BlueZ D-Bus | **done for status + adapter power**: `mados_api::bluetooth`, shared by Settings and mados-ai |
| Audio | PipeWire (`pipewire-pulse`) | **done for output volume/mute**: `mados-audio` uses PipeWire's PulseAudio-compatible client API (libpulse) as the user; output selection and input later |
| Displays, brightness | sysfs DRM + logind `SetBrightness` | **done for outputs + brightness**: `mados_api::display`, shared by Settings and mados-ai; modes/arrangement belong to the compositor (KWin), later |
| Power, reboot, shutdown | **org.mados.System1** → logind | done |
| Updates | **org.mados.System1** → bootc | **done**: status, check, stage, rollback (progress reporting later) |
| Storage info | mados-core (unprivileged statvfs) | done |
| System info | mados-core / org.mados.System1 | done |
| User session | logind | — |
| User accounts | AccountsService D-Bus | **read-only list done**: `mados_api::accounts`; changes later (AccountsService enforces polkit) |

Complex results are JSON strings with a `schema` field so the wire format can
evolve without changing D-Bus signatures.

## Naming

User-visible strings come from `product/product.toml` only (enforced by
`tests/config/validate.py`). Technical identifiers — crate and binary names,
D-Bus names (`org.mados.*`), polkit action ids, paths like `/usr/lib/mados` —
use the codename and are **stable API**; renaming the product does not rename
them. A future rename of technical identifiers would be a deliberate,
versioned API migration.

## Settings

Categories (sidebar order): About, Network & Wi-Fi, Bluetooth, Display,
Sound, Power, Storage, Users, Applications, Updates, Assistant, Privacy.

Implemented with real data/actions in 0.1: **About**, **Network & Wi-Fi**
(NetworkManager status, devices, addresses; Wi-Fi radio switch authorized by
NetworkManager's polkit; selecting networks still delegated to KDE),
**Bluetooth** (BlueZ adapter state, paired/connected devices, adapter power;
pairing still delegated to KDE), **Display** (connected outputs and preferred
modes from sysfs; backlight slider through logind `SetBrightness`; modes and
arrangement still delegated to KDE), **Sound** (output devices, volume, mute
via `mados-audio`; device choice and input still delegated to KDE),
**Users** (read-only list from AccountsService with administrator/standard
roles; account changes still delegated to KDE), **Applications** (installed
apps from desktop entries: system image, system and user Flatpaks;
install/remove opens Discover),
**Storage**, **Power** (restart/shut down through mados-daemon), **Updates**
(status, check, install, roll back, restart — through mados-daemon),
**Assistant**, **Privacy** (assistant provider and where requests are
processed, read from the running assistant). Every category now shows real data. Within each page, what MadOS Settings
cannot do yet is stated explicitly and, where KDE has a module, an "Open in
KDE System Settings" button (a real, working control) is offered. No control
pretends to change state.

## Logging

MadOS services log to stderr; under systemd, journald stores them with the
unit's `SyslogIdentifier` (`mados-daemon`, `mados-ai`, `mados-boot-report`)
and a priority parsed from `sd-daemon` prefixes (`mados_core::log`). Services
log state changes and failures, not routine reads. Use
`journalctl -t mados-daemon` etc. `MADOS_DEBUG=1` enables debug lines.

## Boot markers

`mados-boot-report.service` (after `graphical.target`) writes
`MADOS_BOOT_OK …`, `MADOS_SESSION_OK|NONE …` and, at shutdown,
`MADOS_SHUTDOWN` to the journal and console. Development images mirror the
console to the serial port so tests detect a successful boot from a systemd
target rather than from screenshots. See [testing.md](../development/testing.md).

## Decisions

- [ADR-001](ADR-001-base-system.md) — Base system and image build (Fedora Kinoite + bootc + image-builder)
- [ADR-002](ADR-002-desktop-bootstrap.md) — KDE Plasma as temporary bootstrap desktop
- [ADR-003](ADR-003-mados-ai.md) — Assistant architecture and safety model
- [ADR-004](ADR-004-updates.md) — Transactional updates and rollback
- [ADR-005](ADR-005-ui-toolkit.md) — GTK 4 + Rust for MadOS system applications
