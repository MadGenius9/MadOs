# ADR-004: Transactional updates and rollback

- **Status:** Accepted (strategy); partially implemented in 0.1
- **Date:** 2026-10-06

## Goal

A broken update must never permanently break an installation.

```
running deployment (A) ──bootc upgrade──► new deployment (B) staged
                                            │ reboot
                                            ▼
                          B boots ──fails?──► bootc rollback / pick A in GRUB
                          A is kept as the rollback deployment
```

## Decision

MadOS uses **bootc (OSTree)** deployments (ADR-001):

- The OS (`/usr`) is an immutable image; an update downloads a new container
  image and writes a **new deployment** next to the running one. Nothing in
  the running deployment changes.
- The new deployment becomes the default on the **next boot**; the previous
  one stays installed as the **rollback** deployment and remains selectable
  in the boot menu.
- `/etc` is merged three-way; `/var` (including `/var/home`) is shared and
  not versioned.
- Installed systems follow `UPDATE_IMAGE_REF` (`image/config.env`,
  `ghcr.io/madgenius9/mados`). **No images are published yet**, so `bootc
  upgrade` on 0.1 systems has nothing to fetch.

### In 0.1

| Capability | Status |
|---|---|
| A/B deployments, manual rollback (`sudo bootc rollback`, boot menu) | provided by bootc (upstream) |
| Deployment status in Settings → Updates and `madosctl update-status` | implemented (read-only, via mados-daemon) |
| Install update from Settings | not implemented (CLI only) |
| Automatic rollback when a new deployment fails to boot | not implemented |
| Signed images / signature policy | not implemented |

## Plan

- **M7 (update/rollback):** `org.mados.System1` methods `CheckForUpdates`,
  `StageUpdate`, `Rollback`, each polkit-authorized
  (`org.mados.system.updates.*`, admin auth); Settings UI; progress signals.
- **Automatic rollback:** boot-counting with `greenboot`-style health checks
  — the `mados-boot-report` marker (graphical target reached, no failed
  critical units) is the natural health signal; after N failed boots the
  previous deployment is selected.
- **Signed updates:** sign images with sigstore/cosign in CI and ship a
  `containers-policy.json` requiring signatures for `UPDATE_IMAGE_REF`;
  never accept unsigned images on release builds.
- **Release channels:** `:dev`, `:beta`, `:stable` tags.
