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
3. **session** — `MADOS_SESSION_OK`: logind reports an active Wayland/X11 user
   session (dev images autologin). `make smoke` requires it.
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
MADOS_BOOT_OK version=0.1.0-dev build=<id> kernel=<release> state=running failed=none
MADOS_SESSION_OK type=wayland class=user desktop=KDE
MADOS_SESSION_NONE reason=timeout
MADOS_SHUTDOWN
```

## Not yet automated

- Audio playback inside the guest (the VM has an Intel HDA device; checking
  PipeWire sees it needs guest command execution, which Fedora's guest-agent
  configuration blocks — intentionally).
- Launching applications inside the session (planned: a dev-only user
  service that launches Konsole, Dolphin, Firefox and mados-settings and
  reports their window creation over the serial marker channel).
- Installer ISO end-to-end installation.
- Full image bit-for-bit reproducibility.
