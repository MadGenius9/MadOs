# MadOS roadmap

Status legend: **done** (verified), **implemented** (written and tested where
this environment allows; awaiting end-to-end verification), **planned**.

| Milestone | Scope | Status |
|---|---|---|
| **M0 — Repository/bootstrap** | monorepo, CLAUDE.md, ADRs, product metadata, Makefile, CI, tests | done |
| **M1 — Bootable VM image** | bootc Containerfile (Fedora 44 Kinoite base), qcow2 via image-builder, QEMU tooling, smoke test with boot markers, experimental installer ISO | **done (qcow2)**: CI run #2 built the image and qcow2 and the smoke test passed under KVM/UEFI — boot in 57 s, no failed units, Wayland KDE session, network, clean reboot and shutdown. Installer ISO still experimental/unverified |
| **M2 — Branded functional desktop** | MadOS look-and-feel, wallpaper, accent, os-release identity; terminal, files, browser (upstream) | implemented, unverified in a booted image |
| **M3 — MadOS system services** | `mados-daemon` (`org.mados.System1`): system info, update status, polkit-authorized power | implemented; D-Bus/polkit logic tested on a private bus |
| **M4 — Mad Settings** | GTK 4 app: About, Network & Wi-Fi (status + radio), Bluetooth (status + adapter power), Display (outputs + brightness), Storage, Power, Updates (read-only), Assistant; other categories honestly marked; next: Wi-Fi network selection, Bluetooth pairing, sound (needs a PipeWire-facing API), display modes | partial |
| **M5 — Mad AI** | intent → policy → API architecture, rule-based provider, confirmations; next: model providers, privacy settings, more capabilities | partial |
| **M6 — Installer** | Anaconda/bootc ISO proven end-to-end, first-run experience (`mados-first-run`) | planned (ISO build implemented, experimental) |
| **M7 — Update/rollback** | update API + UI, published signed images, automatic rollback on failed boot | partial: check/install/rollback API + Settings UI implemented (tested with fake/simulated bootc); published signed images and automatic rollback planned |
| **M8 — Physical laptop testing** | deliberate testing on spare hardware; Secure Boot | planned — **no physical installs before this milestone** |
| **M9 — Custom shell progression** | MadOS shell/launcher/panel/notifications replacing Plasma components | planned |
| **M10 — Beta** | stable channel, documentation, release process | planned |

## Next steps (in order)

1. ~~Run CI's `image` job~~ — done; run #2 boots and passes the smoke test.
2. Pin the base image digest once a known-good build exists.
3. ~~App-launch smoke checks and audio-device check~~ (implemented; first
   result comes with the first successful VM boot in CI).
4. ~~Settings: Network & Wi-Fi page via NetworkManager D-Bus~~ (status and
   radio switch done; network selection next).
5. Prove `make iso` end-to-end in CI.
