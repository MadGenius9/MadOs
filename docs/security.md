# MadOS security model

## Principles

1. **Least privilege.** Only `mados-daemon` runs as root, and only because
   it reads deployment state and calls logind after its own authorization
   check. `mados-ai` and `mados-settings` run as the user.
2. **No shell from the UI or the assistant.** Applications call D-Bus
   methods on the service that owns the state; services run external
   programs only by absolute path with fixed arguments (today: `bootc status`).
3. **Authorize the caller, not the service.** `mados-daemon` checks polkit
   with the *calling* D-Bus peer as subject (`system-bus-name`), so polkit
   evaluates the user's identity and session. Tested in
   `services/mados-daemon/tests/dbus_policy.rs`.
4. **The assistant has no privileges of its own.** See
   [ADR-003](architecture/ADR-003-mados-ai.md): closed intent set, explicit
   confirmation for every change, no command execution path.
5. **Keep platform security on.** SELinux stays enforcing; nothing in the
   image build disables it. Secure Boot compatibility is preserved by using
   Fedora's signed shim/GRUB/kernel unchanged.
6. **No secrets in the repository or image.** No passwords, API keys, tokens
   or private keys are committed or baked in. `tests/config/validate.py`
   scans the staged image tree for credential-looking assignments.

## polkit actions

| Action | Default (any / inactive / active) | Used by |
|---|---|---|
| `org.mados.system.power` | auth_admin_keep / auth_admin_keep / yes | `PowerOff`, `Reboot` (mirrors logind's own defaults) |

| `org.mados.system.updates.check` | auth_admin_keep / auth_admin_keep / yes | `CheckForUpdate` (fetches metadata only) |
| `org.mados.system.updates.apply` | auth_admin / auth_admin / auth_admin_keep | `StartUpdate`, `StartRollback` — validation enforces admin authentication for every case |

## Service hardening

`mados-daemon.service`: `NoNewPrivileges`, `ProtectHome`, `PrivateTmp`,
kernel tunables/modules/logs protection, clock/hostname/control-group
protection, `RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK`,
`MemoryDenyWriteExecute`, `SystemCallArchitectures=native`. Its D-Bus policy
lets only root own `org.mados.System1`.

**Deliberately not set:** `PrivateNetwork=` and `ProtectSystem=`. The daemon
runs `bootc upgrade`/`rollback` (after polkit admin authorization), and
bootc must reach the image registry and write `/sysroot` (OSTree
repository) and `/boot` (boot entries). These were set in the first 0.1
draft, which would have broken updates on a real system. Planned (M7): move
the bootc mutations into a separate, socket- or D-Bus-activated helper unit
so the long-running daemon can be sandboxed again.

## Development-only weakenings (prominently documented)

These exist **only in `dev` variant images** (the default for `make image`)
and are verified absent from `release` builds by `tests/config/validate.py`:

| What | Why | Where |
|---|---|---|
| Plasma Login Manager **autologin** of user `mados` | VM smoke test must reach a graphical session without typing a password | `system/templates/plasmalogin-dev-autologin.conf` → `/etc/plasmalogin.conf.d/` |
| Serial console kernel arguments | boot markers for automated tests | `system/variants/dev/usr/lib/bootc/kargs.d/` |
| `/etc/plasma-setup-done` | dev disks get their user at build time, so KDE's first-boot wizard (which creates the first account on release images) is marked done | `system/variants/dev/etc/` |
| `mados-session-check` user unit | launches the default apps once per login and checks audio for the smoke test; runs as the user, starts only fixed program paths | `system/variants/dev/usr/lib/systemd/user/` |
| Development user `mados` in `wheel` with a per-build password | log in to VMs | created by `scripts/build-disk.sh` at disk-build time; password is random unless `MADOS_DEV_PASSWORD` is set, stored only in git-ignored `out/dev-credentials.txt` |
| `qemu-guest-agent` installed | clean reboot/shutdown and network checks from tests | installed in all images; activates only when a virtio guest-agent port exists; Fedora's default config blocks `guest-exec` and file RPCs |

**Installer ISO (experimental):** the installer's *live environment* boots
with SELinux **permissive** (`enforcing=0`). It affects only the installer
environment; the installed system boots with SELinux enforcing, which the
ISO install test checks. Upstream image-builder's recipe uses `selinux=0`
instead, but Anaconda carries a `selinux=` boot option over to the installed
system: CI run #13 installed a system with SELinux **disabled**, which the
smoke test caught. Never use `selinux=0` on the installer command line
(`tests/config/validate.py` enforces this). At the end of the install,
`image/installer/relabel.ks` relabels `/etc` and `/var/home` of the installed
system with that system's own policy, because files Anaconda writes there
could carry labels it does not expect (CI run #14: `rpm-ostreed.service`
failed with SELinux enforcing). Revisit (enforcing installer
environment) when the ISO path matures.

**Never use dev images on real hardware or untrusted networks.**

## Long-term requirements

- Signed update images and a signature-enforcing container policy (ADR-004).
- Secure Boot verified on physical hardware (M8).
- Measured boot / TPM-bound disk encryption (post-M8).
- Assistant cloud providers only with explicit opt-in; credentials from the
  user's keyring.

## Reporting

Report security issues privately to the maintainers (see
`product/product.toml` → `urls.bugs` for the project location) rather than in
public issues.
