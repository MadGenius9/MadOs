# Testing MadOS

| Layer | Command | Where it runs | What it proves |
|---|---|---|---|
| Lint | `make test-lint` | anywhere | `cargo fmt`, `clippy -D warnings` |
| Unit + integration | `make test-unit` | anywhere | product metadata, sysinfo (fixture trees), bootc status parsing, intent parsing, policy table, confirmation flow, strict provider JSON; **D-Bus integration** on a private `dbus-daemon`: power actions refused without authorization, polkit subject is the caller |
| Static config | `make test-config` | anywhere | product.toml schema, no hard-coded product name, Containerfile ends with `bootc container lint`, os-release merge keeps Fedora compatibility keys, unit hardening (`NoNewPrivileges`, absolute `ExecStart`, `SyslogIdentifier`), `systemd-analyze verify`, XML well-formedness, polkit `allow_any` never `yes`, D-Bus activation files, desktop files, dev-only autologin, no credentials, shellcheck |
| Reproducibility | `sh scripts/repro-check.sh` | anywhere | the staged tree is byte-identical across two builds from the same inputs (`SOURCE_DATE_EPOCH`) |
| Harness self-test | `make test-harness` | needs QEMU + OVMF (KVM optional) | boots a real Linux kernel with a busybox initramfs that emits the MadOS markers; proves the QEMU/UEFI invocation, serial capture, QMP screenshots and shutdown detection |
| Image build | `make image disk` | Fedora network access + root podman | the image builds and passes `bootc container lint` |
| VM smoke | `make smoke` | needs a built disk; KVM strongly recommended | see below |

`make test` runs lint, unit, config and harness layers.

## VM smoke test

`tests/smoke/vm_smoke.py` boots the newest `out/*.qcow2` (through a
copy-on-write overlay) and checks:

1. **boot** — `MADOS_BOOT_OK` on the serial console, written by
   `mados-boot-report.service` *after `graphical.target`*.
2. **units** — `systemctl is-system-running --wait` state and the list of
   failed units carried in the marker; any failed unit fails the test.
   - **selinux** — the marker's `selinux=` field must be `enforcing`.
3. **session** — `MADOS_SESSION_OK`: logind reports an active Wayland/X11 user
   session (dev images autologin). `make smoke` requires it.
   - **apps** — `MADOS_APPS`: the dev-only user unit
     `mados-session-check.service` runs `madosctl session-check` in the
     session; it launches Konsole, Dolphin, Firefox and MadOS Settings and
     records `ok` when each claims its D-Bus name (it reached its main loop),
     `running` when it stayed alive without one (reported as weaker),
     `exited`/`missing`/`failed` otherwise. `make smoke` requires the report.
   - **audio** — same marker: `audio=ok` means the kernel found a sound card
     (`/proc/asound/cards`) and WirePlumber has a default sink
     (`wpctl inspect @DEFAULT_AUDIO_SINK@`).
   - **services** — same marker: `daemon=ok` (org.mados.System1 answered
     `GetSystemInfo` on the real system bus: D-Bus activation, bus policy,
     SELinux and unit hardening all allowed it), `bootc=ok` (real
     `bootc status` through the daemon), `assistant=ok` (org.mados.Assistant1
     answered a read-only request on the session bus).
4. **network** — via qemu-guest-agent: a non-loopback interface has IPv4.
5. **screenshot** — QMP `screendump` to `out/smoke/screen.png` (for humans).
6. **reboot** — guest-agent `guest-shutdown mode=reboot`; requires
   `MADOS_SHUTDOWN` and a second `MADOS_BOOT_OK`.
7. **shutdown** — guest-agent power-off; requires `MADOS_SHUTDOWN` and QEMU
   exiting with status 0.

Missing prerequisites produce **skip**, never **pass**. The result is in
`out/smoke/report.json`; serial and QEMU logs are next to it.

Marker format (contract between `apps/madosctl/src/boot_report.rs` and the
harness):

```
MADOS_BOOT_OK version=0.1.0-dev build=<id> kernel=<release> selinux=enforcing state=running failed=none
MADOS_SESSION_OK type=wayland class=user desktop=KDE
MADOS_SESSION_NONE reason=timeout
MADOS_APPS terminal=ok files=ok browser=ok settings=ok audio=ok daemon=ok bootc=ok assistant=ok
MADOS_APPS_NONE reason=timeout
MADOS_SHUTDOWN
```

## Installer ISO test (`make iso-test`)

`tests/smoke/iso_install.py` extracts the installer kernel/initrd from the
newest `out/*.iso` (the ISO is not modified) and boots it with a tiny extra
disk image labelled `OEMDRV` holding an unattended kickstart (Anaconda loads
`ks.cfg` from such a volume automatically). The kickstart installs the
MadOS image embedded in the ISO onto a scratch disk in `out/iso-install/`
and powers off; the installed disk is then booted through the regular smoke
test. Runs in CI on manual runs with `build_iso`. The shipped ISO contains
no unattended path: without an OEMDRV volume, Anaconda is interactive.

## Not yet automated

- Audible playback (the check proves device + sink, not sound output).
- Window mapping on screen (the app check proves D-Bus registration).
- Full image bit-for-bit reproducibility.
