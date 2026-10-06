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
| Deployment status in Settings → Updates and `madosctl update-status` | implemented (via mados-daemon) |
| Check for updates from Settings (`CheckForUpdate`, bootc `cachedUpdate`) | implemented; polkit `org.mados.system.updates.check` (active user) |
| Install update from Settings (`StartUpdate` → `bootc upgrade`, staged for next boot, never auto-reboots) | implemented; polkit `org.mados.system.updates.apply` (admin auth); verified against a simulated bootc, unverified on a real bootc system |
| Roll back from Settings (`StartRollback` → `bootc rollback`) | implemented; same polkit action; same verification status |
| Automatic rollback when a new deployment fails to boot | not implemented |
| Signed images / signature policy | not implemented |

## Plan

- **M7 (update/rollback):** done in API level 2 — `CheckForUpdate`,
  `StartUpdate`, `StartRollback`, `UpdateJobFinished` signal and `Busy`
  property on `org.mados.System1`; one job at a time; Settings UI. Still
  missing: download progress reporting and published images to update from.
- **Automatic rollback:** boot-counting with `greenboot`-style health checks
  — the `mados-boot-report` marker (graphical target reached, no failed
  critical units) is the natural health signal; after N failed boots the
  previous deployment is selected.
- **Signed updates:** sign images with sigstore/cosign in CI and ship a
  `containers-policy.json` requiring signatures for `UPDATE_IMAGE_REF`;
  never accept unsigned images on release builds.
- **Release channels:** `:dev`, `:beta`, `:stable` tags.
