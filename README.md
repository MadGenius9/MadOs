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
| Bootable container image (bootc, Fedora 44 Kinoite base) | **builds in CI** (GitHub Actions job `image`): components compile against Fedora 44, `bootc container lint` passes. Cannot be built in the bootstrap dev environment (Fedora servers blocked) |
| qcow2 disk image via image-builder | **builds in CI** |
| **SELinux enforcing, MadOS identity at boot** | **verified in CI run #4**: `selinux=enforcing`, no failed units, systemd banner "Welcome to MadOS 0.1.0-dev!" (os-release branding), Wayland KDE session. The run failed later only because the boot report's sandbox (`ProtectHome=yes`) hid the dev session check's report in `/run/user`; fixed, re-running |
| **Boots in QEMU/KVM (UEFI)** | **verified in CI run #2**: `MADOS_BOOT_OK` 57 s after power-on (version 0.1.0-dev, Fedora kernel 7.2.8-200.fc44), `graphical.target` reached with **no failed units**, active **Wayland KDE session**, network up (DHCP 10.0.2.15), clean **reboot** to a second successful boot, clean **shutdown** (QEMU exit 0) |
| Installer ISO (`bootc-generic-iso`) | written, with an unattended install test (`make iso-test`); **experimental**, not yet built |
| MadOS components (Rust) | 57 unit/integration tests: D-Bus policy tests on a private bus; mock NetworkManager/BlueZ/AccountsService/logind; fake and simulated bootc; real PulseAudio server |
| MadOS Settings (GTK 4) | every category has a real page (About, Network & Wi-Fi, Bluetooth, Display, Sound, Power, Storage, Users, Applications, Updates, Assistant, Privacy); verified headless against mocks/real test servers, not yet inside the VM |
| VM tooling + smoke test | verified: harness self-test (real kernel, TCG) and the MadOS image (KVM) in CI |
| Physical hardware | **untested — do not install** |

### What works (verified)

- **The MadOS image builds and boots** (GitHub Actions run #2, QEMU/KVM,
  UEFI): graphical target with no failed units, Wayland KDE session, network,
  clean reboot and shutdown — detected by MadOS's own boot markers, not
  screenshots.
- `make build`, `make test`: fmt/clippy clean, 57 Rust tests, 156 static
  configuration checks, reproducibility check of generated files, QEMU
  harness self-test.
- `madosctl about` / Settings → About: real version, kernel, CPU, memory,
  GPU, storage, hostname, architecture, session, build ID (gracefully
  "Unavailable" when absent).
- Assistant: "How much battery is left?", "Why is my laptop running
  slowly?", "Check for updates" answered from real data; "Turn Bluetooth on",
  "Set the volume to 30%", "Install updates" ask for confirmation first;
  "sudo rm -rf /" is refused. Settings → Assistant → mados-ai →
  org.mados.System1 verified end to end with a simulated bootc.
- `org.mados.System1`: power and update actions are polkit-gated against the
  caller; update/rollback jobs run one at a time and report completion
  (private-bus tests, fake `bootc` executable).
- Settings pages driven headless: Network & Wi-Fi and Bluetooth (mock
  NetworkManager/BlueZ; switches always show the service's real state,
  refusals shown as "Not authorized."), Sound (real PulseAudio test server,
  cross-checked with `pactl`), Display (sysfs; brightness via logind,
  mock-tested), Users (mock AccountsService), Applications (desktop entries),
  Updates (check → install → pending restart, simulated bootc), Privacy
  (read from the running assistant).
- First-run welcome window: shown once per user, then never again.

### Implemented but unverified

On the booted image: launching Konsole/Dolphin/Firefox/Settings, audio
device detection and the MadOS services check (run #4 could not read their
report; fixed),
MadOS branding in Plasma (screenshot captured, not yet reviewed), and all
MadOS services against the *real* polkit, logind, NetworkManager, BlueZ,
AccountsService, PipeWire and bootc. The installer ISO.

### Not implemented yet

Account setup during install, automatic rollback, published and signed
update images (so "Check for Updates" has nothing to find yet), choosing a
Wi-Fi network, pairing Bluetooth devices, choosing sound devices, display
modes and adding users in MadOS Settings (each page says so and opens the
KDE module), assistant model providers, MadOS shell, file manager and
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
