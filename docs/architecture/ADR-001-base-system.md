# ADR-001: Base system and image-build strategy

- **Status:** Accepted (for MadOS 0.1)
- **Date:** 2026-10-06
- **Deciders:** MadOS architecture

## Context

MadOS 0.1 must be a *real* bootable x86_64 UEFI operating system image,
produced reproducibly from source, bootable in QEMU/KVM, and installable on
physical hardware later. We do not write a kernel or a desktop stack for 0.1;
we reuse Linux and proven userspace and build MadOS-owned layers on top.

Hard requirements that drive this decision:

1. Modern laptop hardware support (recent kernel, firmware, Mesa, PipeWire,
   NetworkManager, BlueZ, Wayland).
2. A path to **transactional updates with rollback** (see
   [ADR-004](ADR-004-updates.md)).
3. SELinux and Secure Boot compatibility kept intact.
4. A build that is reproducible-ish from a declarative definition in git and
   runnable in CI.
5. Low fragility: 0.1 must actually boot.

Fedora is the chosen foundation (current kernel, Wayland-first desktop stack,
SELinux, signed shim/GRUB for Secure Boot, Flatpak, strong image tooling).
Fedora 44 is the current stable release at the time of writing (released
2026-04-28, supported until roughly June 2027). Fedora 45 is due in October
2026; the release number is a single build variable (`image/config.env`).

## Tooling state (researched 2026-10)

- **bootc** (bootable OCI containers): Fedora publishes bootable base images
  (`quay.io/fedora/fedora-bootc`) and the Fedora Atomic Desktops publish
  bootable desktop images (`quay.io/fedora-ostree-desktops/kinoite` for KDE
  Plasma). bootc uses OSTree underneath, giving A/B deployments and rollback.
  This model is used in production by large downstreams (e.g. Universal Blue
  derivatives) that build `FROM` the Fedora desktop images.
- **image-builder** (osbuild): converts a bootable container into disk images.
  The former standalone `bootc-image-builder` has been **merged into
  `image-builder`**; the container is `ghcr.io/osbuild/image-builder-cli`.
  Disk types (`qcow2`, `raw`, …) are mature. ISOs are built with the
  `bootc-generic-iso` type from a separate *installer* container; the legacy
  `anaconda-iso` type no longer exists in `image-builder`, and upstream
  documents a known issue (`systemd-remount-fs.service` fails on
  Anaconda-installed bootc systems). ISO generation is therefore the
  least mature part of this toolchain.
- **rpm-ostree** compose (treefiles): what Fedora itself historically used to
  build Kinoite; being superseded by container-native builds for derivatives.
- **Kickstart + Lorax/livemedia-creator**: mature live/installer ISO
  production for *package-mode* systems. Fedora spins have largely moved to
  **Kiwi** for live media.
- **Kiwi / mkosi**: capable image builders, but produce package-mode systems;
  transactional updates would have to be built separately.

## Alternatives considered

| Option | Pros | Cons |
|---|---|---|
| **A. bootc container `FROM` Fedora Kinoite + image-builder** | Atomic/transactional from day one (OSTree A/B + rollback); Containerfile is simple and reviewable; desktop/hardware stack is exactly Fedora's tested KDE composition; updates are just "push a new container image"; same artifact feeds qcow2, ISO and future updates | Depends on fairly new tooling (image-builder bootc support, ISO path); image build needs privileged podman; base image is large (~several GB); we inherit Kinoite's package set |
| B. bootc `FROM fedora-bootc` + install KDE ourselves | Full control of package set; smaller | We must curate a desktop composition (firmware, codecs, portals, SDDM, Plasma) ourselves — more ways to fail for 0.1 |
| C. Kickstart + livemedia-creator/Lorax live ISO | Most mature ISO path; classic Fedora approach | Package-mode: no transactional updates or rollback; we'd have to migrate to image mode later anyway; Lorax needs a Fedora host/VM |
| D. Kiwi or mkosi package-mode image | Flexible, well-maintained | Same lack of transactionality as C; second migration later |
| E. rpm-ostree treefile compose | Transactional; what Fedora used | Heavier, less approachable tooling; ecosystem moving to container builds |

## Decision

**Option A.** MadOS is a **bootc (OSTree-backed) bootable container image**
built with `podman build` from `image/Containerfile`, which starts
`FROM quay.io/fedora-ostree-desktops/kinoite:${FEDORA_VERSION}` and layers
MadOS components, configuration and branding on top. Bootable artifacts are
produced from that container by `image-builder`:

- `make disk` → `qcow2` (the primary 0.1 VM artifact; boots straight to the
  installed system).
- `make iso` → `bootc-generic-iso` installer ISO built from
  `image/installer/Containerfile` (Anaconda installs the MadOS container that
  is embedded in the ISO). **Experimental in 0.1** because of the upstream
  maturity notes above.

The KDE Plasma session that Kinoite provides is the **temporary bootstrap
desktop** (see [ADR-002](ADR-002-desktop-bootstrap.md)).

Package-mode and OS identity choices for 0.1:

- `os-release` keeps `ID=fedora` and `ID_LIKE` untouched and sets
  `NAME`/`PRETTY_NAME`/`VARIANT`/`VARIANT_ID`/`IMAGE_ID`/`IMAGE_VERSION` to
  MadOS values generated from `product/product.toml`. Changing `ID` breaks
  tooling that keys off it (image-builder distro detection, Anaconda,
  `%{fedora}`-adjacent assumptions) and is not worth the risk in 0.1. This is
  honest: MadOS 0.1 *is* a Fedora derivative.
- SELinux stays enforcing. Nothing in the build disables it.

## Why

- It is the only option that gives **transactional updates and rollback**
  without a later re-platforming (the stated top safety goal).
- Kinoite is a composition Fedora already tests and ships, so the hardware and
  desktop stack is not ours to break in 0.1.
- The build definition is one Containerfile plus overlay files — easy to
  review, diff and run in CI.
- The same OCI image is the update payload (`bootc upgrade`/`bootc switch`).

## Disadvantages / risks

- **Network and privilege requirements.** The build needs access to
  `quay.io`, Fedora mirrors and `ghcr.io`, and image-builder needs a
  privileged container. The development container this repository was
  bootstrapped in has Fedora infrastructure blocked by egress policy, so the
  image build is executed in GitHub Actions (see
  [building.md](../development/building.md)).
- **ISO path maturity.** `bootc-generic-iso` is newer than Lorax; the qcow2
  path is primary until the ISO is proven in CI and on hardware.
- **Size.** Kinoite-based images are multi-GB.
- **Base image pinning.** Tags float. Reproducible builds require pinning the
  base by digest (`BASE_IMAGE_DIGEST` in `image/config.env`); the digest is
  recorded in `/usr/lib/mados/build-info.json` in every build regardless.
- **Inherited composition.** Kinoite's package set and defaults are not ours;
  removing packages from an OSTree-based image works but must be done with care.

## Migration path

- **To Option B** (own composition): change the `FROM` line to
  `quay.io/fedora/fedora-bootc` and add a package list; overlay files,
  components and image-builder usage are unchanged. Expected around M9 when
  the MadOS shell replaces Plasma.
- **Away from image-builder:** the OCI image itself is tool-agnostic;
  `bootc install to-disk` or a future installer can consume it directly.
- **Away from Fedora:** everything MadOS-owned is packaged as files installed
  into a staging root (`scripts/install-components.sh`); RPM/other packaging
  can be added under `packages/` without changing component code.
