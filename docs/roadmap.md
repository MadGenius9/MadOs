# MadOS roadmap

Status legend: **done** (verified), **implemented** (written and tested where
this environment allows; awaiting end-to-end verification), **planned**.

| Milestone | Scope | Status |
|---|---|---|
| **M0 — Repository/bootstrap** | monorepo, CLAUDE.md, ADRs, product metadata, Makefile, CI, tests | done |
| **M1 — Bootable VM image** | bootc Containerfile (Fedora 44 Kinoite base), qcow2 via image-builder, QEMU tooling, smoke test with boot markers, experimental installer ISO | container image **builds and passes `bootc container lint` in CI**; qcow2 conversion and VM boot pending (first disk build hit an unsupported blueprint option, fixed) |
| **M2 — Branded functional desktop** | MadOS look-and-feel, wallpaper, accent, os-release identity; terminal, files, browser (upstream) | implemented, unverified in a booted image |
| **M3 — MadOS system services** | `mados-daemon` (`org.mados.System1`): system info, update status, polkit-authorized power | implemented; D-Bus/polkit logic tested on a private bus |
| **M4 — Mad Settings** | GTK 4 app: About, Storage, Power, Updates (read-only), Assistant; other categories honestly marked; next: network, Bluetooth, display, sound pages backed by real APIs | partial |
| **M5 — Mad AI** | intent → policy → API architecture, rule-based provider, confirmations; next: model providers, privacy settings, more capabilities | partial |
| **M6 — Installer** | Anaconda/bootc ISO proven end-to-end, first-run experience (`mados-first-run`) | planned (ISO build implemented, experimental) |
| **M7 — Update/rollback** | update API + UI, published signed images, automatic rollback on failed boot | planned (bootc A/B + manual rollback available) |
| **M8 — Physical laptop testing** | deliberate testing on spare hardware; Secure Boot | planned — **no physical installs before this milestone** |
| **M9 — Custom shell progression** | MadOS shell/launcher/panel/notifications replacing Plasma components | planned |
| **M10 — Beta** | stable channel, documentation, release process | planned |

## Next steps (in order)

1. Run CI's `image` job; fix whatever the first real build/boot reveals.
2. Pin the base image digest once a known-good build exists.
3. ~~App-launch smoke checks and audio-device check~~ (implemented; first
   result comes with the first successful VM boot in CI).
4. Settings: Network & Wi-Fi page via NetworkManager D-Bus.
5. Prove `make iso` end-to-end in CI.
