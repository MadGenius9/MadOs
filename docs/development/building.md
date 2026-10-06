# Building MadOS

## Requirements

Run `make setup` first. It checks everything below and prints the install
command for your distribution. It installs nothing.

| Purpose | Needs |
|---|---|
| Components (`make build`, `make test`) | Rust (stable), GTK 4 dev files, Python ≥ 3.11, dbus-daemon, shellcheck, rsvg-convert |
| Image (`make image`, `make disk`, `make iso`) | podman (run as root via sudo), openssl, ~50 GiB free disk, network access to `quay.io`, Fedora mirrors and `ghcr.io` |
| VMs (`make vm`, `make smoke`) | qemu-system-x86_64, qemu-img, OVMF (UEFI firmware), ideally `/dev/kvm` access |

A Fedora workstation or a Fedora VM is the most natural build host; Ubuntu
24.04 works too (it is what CI uses).

## Steps

```sh
make setup        # check prerequisites
make build        # compile MadOS components (debug)
make test         # lint, unit/integration tests, config validation, harness self-test
make image        # podman build -> localhost/mados-dev:<version>     (sudo)
make disk         # image-builder -> out/mados-<version>-<build>.qcow2 (sudo)
make vm           # boot it in QEMU (UEFI); changes go to out/vm/ overlay
make smoke        # automated boot/session/network/reboot/shutdown test
make iso          # EXPERIMENTAL installer ISO -> out/*-installer.iso   (sudo)
make vm-iso       # boot the ISO with a scratch target disk in out/vm/
make clean
```

`MADOS_VARIANT=release make image` builds without development conveniences
(no autologin, no serial console arguments).

### What `make image` does

1. `podman build -f image/Containerfile .` (build args from
   `image/config.env` and `product/product.toml`).
2. Builder stage (`quay.io/fedora/fedora:44`): compiles the Rust workspace
   with `--locked`, then `scripts/stage-system.py` assembles `/staging`: the
   binaries, `system/rootfs`, the variant overlay and all generated branded
   files.
3. Final stage (`FROM quay.io/fedora-ostree-desktops/kinoite:44`): installs
   Konsole, Dolphin, Firefox and qemu-guest-agent, copies `/staging`, runs
   `/usr/libexec/mados/image-finalize` (os-release merge, unit presets) and
   finally `bootc container lint`.

### What `make disk` does

Generates `out/build/disk/blueprint.toml` with the development user `mados`
(group `wheel`). The password is `$MADOS_DEV_PASSWORD` or a random one written
to `out/dev-credentials.txt` (mode 0600, git-ignored); `MADOS_DEV_SSH_PUBKEY`
adds an SSH key. It then runs `ghcr.io/osbuild/image-builder-cli` privileged
to produce a qcow2 and writes a `.sha256` next to it.

### Development image conveniences (dev variant only)

- SDDM autologin of user `mados` into Plasma (Wayland) — so smoke tests can
  verify the session.
- Kernel arguments `console=tty0 console=ttyS0,115200n8` — boot markers on
  the serial port.

Both are **absent from release builds** and are checked by
`tests/config/validate.py`.

### The installer ISO (experimental)

`make iso` builds an Anaconda installer environment
(`image/installer/Containerfile`, adapted from osbuild's documentation) and
turns it into a `bootc-generic-iso` with the MadOS image embedded as an
offline payload. Known upstream issue: `systemd-remount-fs.service` fails on
Anaconda-installed bootc systems. The qcow2 path is the primary 0.1 artifact.

## Developing the Settings UI without a full system

`mados-api` ships a mock NetworkManager. On any machine with `dbus-daemon`:

```sh
addr=$(dbus-daemon --session --print-address=1 --fork)
DBUS_SYSTEM_BUS_ADDRESS=$addr cargo run -p mados-api --example mock-networkmanager &
DBUS_SYSTEM_BUS_ADDRESS=$addr cargo run -p mados-settings -- --page=network
```

`MOCK_NM_DENY=1` makes the mock refuse Wi-Fi changes, like polkit would.

## Host safety

- No script writes to a host block device. VMs write only to files in
  `out/vm/` (a copy-on-write overlay over the image, or a scratch target
  disk); `scripts/vm.py` refuses block-device paths.
- `dd`, `wipefs`, `mkfs`, `fdisk` and `parted` are never run against host
  disks. image-builder creates partitions inside its own image files in a
  privileged container.
- Image steps run podman as root (via `sudo`, announced before it happens),
  because image-builder needs a privileged container and root's image store.

## Building in this repository's cloud development environment

The environment in which MadOS 0.1 was bootstrapped (Ubuntu 24.04 container)
**cannot build the image**: its network policy denies `quay.io`,
`registry.fedoraproject.org` and all Fedora mirrors (HTTP 403), and it has no
`/dev/kvm`. Running `make image` there fails at the first `FROM` pull
(`pinging container registry quay.io: … Forbidden`). Everything else
(`make build`, `make test` including the QEMU harness self-test under TCG)
runs there.

To build there, allow `quay.io`, `cdn.quay.io`/`*.quay.io`,
`registry.fedoraproject.org`, `mirrors.fedoraproject.org`,
`dl.fedoraproject.org` and `download.fedoraproject.org` (plus the mirrors it
redirects to) in the environment's network settings. GitHub Actions
(`.github/workflows/ci.yml`, job `image`) builds and boots the image with
KVM and is the reference build path.
