# MadOS

> **MadOS** is a working codename. Version **0.1.0-dev**: early development.
> Not for daily use. Not for physical hardware yet.

MadOS is a desktop operating system for x86_64 UEFI laptops. It is built on
the Linux kernel and Fedora's proven infrastructure (systemd, Wayland, Mesa,
PipeWire, NetworkManager, BlueZ, SELinux, bootc/OSTree), with MadOS-owned
system services and applications on top. The long-term goal is an OS with its
own shell, settings, assistant and update experience. In 0.1 the desktop is
KDE Plasma (Fedora Kinoite) as a temporary bootstrap, and this README says so.

## Current status

| Area | Status |
|---|---|
| Bootable container image (bootc, Fedora 44 Kinoite base) | **builds in CI** (GitHub Actions job `image`, run #1): components compile against Fedora 44, os-release merge yields `PRETTY_NAME="MadOS 0.1.0-dev"`, boot-report unit enabled, `bootc container lint` passes (13 checks). Cannot be built in the bootstrap dev environment (Fedora servers blocked) |
| qcow2 disk image via image-builder | **builds in CI** (run #2) |
| **Boots in QEMU/KVM (UEFI)** | **verified in CI run #2**: `MADOS_BOOT_OK` 57 s after power-on (version 0.1.0-dev, Fedora kernel 7.2.8-200.fc44), `graphical.target` reached with **no failed units**, active **Wayland KDE session**, network up (DHCP 10.0.2.15), clean **reboot** to a second successful boot, clean **shutdown** (QEMU exit 0) |
| Installer ISO (`bootc-generic-iso`) | written; **experimental**, unverified |
| MadOS components (Rust) | built and tested: 31 unit/integration tests incl. D-Bus policy tests on a real (private) bus |
| MadOS Settings (GTK 4) | runs; verified headless on Ubuntu (About, Assistant pages rendered with real data) |
| VM tooling + smoke test | harness verified by booting a real Linux kernel under QEMU/UEFI (TCG); full MadOS smoke test pending first image build |
| Physical hardware | **untested — do not install** |

### What works (verified)

- **The MadOS image builds and boots** (GitHub Actions run #2, QEMU/KVM,
  UEFI): graphical target with no failed units, Wayland KDE session, network,
  clean reboot and shutdown — detected by MadOS's own boot markers, not
  screenshots.
- `make build`, `make test`: Rust workspace, fmt/clippy clean, unit and D-Bus
  integration tests, 127 static configuration checks, reproducibility check of
  generated files, QEMU harness self-test.
- `madosctl about` / Settings → About: real version, kernel, CPU, memory,
  GPU, storage, hostname, architecture, session, build ID (gracefully
  "Unavailable" when absent).
- Assistant pipeline: "How much battery is left?", "Why is my laptop running
  slowly?", "What version am I running?" answered from real system data;
  "Turn Bluetooth on" asks for confirmation; "sudo rm -rf /" is refused.
- `org.mados.System1` refuses power actions for unauthorized callers and
  checks polkit against the caller (tested with mocks on a private bus).
- Settings → Network & Wi-Fi against a mock NetworkManager on a private bus:
  shows state, devices, connection and IPv4 address; the Wi-Fi switch changes
  the radio, and a refusal leaves the switch showing the real state with
  "Not authorized." (verified headless; real NetworkManager unverified).
- Settings → Bluetooth against a mock BlueZ: adapter, paired/connected
  devices, adapter power switch (verified headless; real BlueZ unverified).
- Updates: `org.mados.System1` check/install/rollback jobs (polkit-gated,
  one at a time, completion signal) tested on a private bus and against a
  fake `bootc` executable; Settings → Updates driven end to end against the
  real service code with a simulated bootc (real bootc system unverified).
- Display brightness through logind `SetBrightness` against a mock logind;
  Settings → Display shows connectors/preferred modes from sysfs and hides
  the brightness slider when there is no backlight (real hardware unverified).

### Implemented but unverified

On the booted image: launching Konsole/Dolphin/Firefox/Settings and audio
device detection (smoke checks added after run #2; first result in the next
CI run), MadOS branding in Plasma (screenshot captured but not yet
reviewed), SELinux mode, mados-daemon/mados-ai against the real
polkit/logind/NetworkManager/BlueZ/bootc, the installer ISO. The Wi-Fi/Bluetooth/brightness assistant
actions are implemented against NetworkManager/BlueZ/logind D-Bus APIs but
untested.

### Not implemented yet

Installer polish and first-run, automatic rollback, published and signed
update images (so "Check for Updates" has nothing to find yet), most Settings categories (they say so and open the
KDE module instead), choosing a Wi-Fi network or pairing Bluetooth devices in MadOS Settings, assistant model providers, MadOS shell, file manager,
terminal (Dolphin and Konsole are used).

## Build

```sh
make setup     # checks prerequisites; installs nothing
make build     # MadOS components
make test      # lint + tests + config validation + harness self-test
make image     # bootable container image (root podman; needs Fedora network access)
make disk      # out/mados-<version>-<build>.qcow2
make iso       # EXPERIMENTAL installer ISO
```

Details, requirements and what each step does: [docs/development/building.md](docs/development/building.md).

## Boot it in a VM

```sh
make vm        # newest out/*.qcow2 in QEMU with UEFI (KVM if available)
make smoke     # automated boot/session/network/reboot/shutdown test
make vm-iso    # boot the installer ISO against a scratch disk in out/vm/
```

The VM never writes to the built image (copy-on-write overlay in `out/vm/`).
Development images log in automatically as `mados`; the password for
`sudo` is in `out/dev-credentials.txt`.

## Warnings

- **VM only.** Do not install MadOS on a physical machine, and do not point
  any MadOS tool at a real disk. Physical hardware testing is milestone M8.
- Development images have **autologin** and a serial console enabled
  ([docs/security.md](docs/security.md)). Never expose them to untrusted
  networks.
- No update images are published; `bootc upgrade` on an installed 0.1 system
  has nothing to fetch.

## Roadmap

M0 repository ✔ · M1 bootable VM image ✔ (qcow2; ISO experimental) ·
M2 branded desktop · M3 system services · M4 Settings · M5 assistant ·
M6 installer · M7 update/rollback · M8 physical hardware · M9 custom shell ·
M10 beta. Details: [docs/roadmap.md](docs/roadmap.md).

## Documentation

- [Architecture overview](docs/architecture/overview.md) and ADRs in `docs/architecture/`
- [Building](docs/development/building.md) · [Testing](docs/development/testing.md)
- [Hardware support](docs/hardware/support.md) · [Security](docs/security.md)
- [CLAUDE.md](CLAUDE.md) — guide for AI-assisted development sessions

## Upstream

MadOS 0.1 is a Fedora derivative. Linux, Fedora, KDE Plasma, Firefox, bootc,
osbuild and the other components listed in the
[architecture overview](docs/architecture/overview.md#upstream-dependencies-honest-inventory)
are developed by their respective projects and are not MadOS technology.

## License

MadOS source code: Apache-2.0 ([LICENSE](LICENSE)). Placeholder artwork in
`product/assets/`: CC0. Upstream components keep their own licenses.
