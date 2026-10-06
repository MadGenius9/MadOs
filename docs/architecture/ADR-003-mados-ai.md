# ADR-003: Assistant (mados-ai) architecture and safety model

- **Status:** Accepted (0.1 architecture; providers pluggable)
- **Date:** 2026-10-06

## Context

MadOS will include an assistant that can answer questions about the system
and perform actions ("Turn Bluetooth on", "How much battery is left?",
"Install Discord"). A language model must never become a path to arbitrary
command execution or root access, and cloud/local model providers must be
swappable.

## Decision

```
request text
   │  provider (rules today; local/cloud model later) — interpretation only
   ▼
Intent  (closed Rust enum, strict JSON; unknown intents/fields rejected)
   │  policy: capability registry with fixed risk level per intent
   ▼
Execute (read-only) │ Confirm (state change) │ Unsupported │ Deny
   │                        │ user confirms in UI (single use, same caller, 2 min TTL)
   ▼                        ▼
system APIs over D-Bus: org.mados.System1 (polkit), NetworkManager, BlueZ, logind
```

- **mados-ai runs as the user**, unprivileged, on the session bus. It has no
  capability the user does not already have; privileged operations are
  authorized again by polkit in the owning service, against the user.
- **No command intent executes.** `Intent::RunCommand` exists only so such
  requests are recognised and refused. There is no shell, `exec` or generic
  D-Bus passthrough in the executor.
- **Providers only interpret.** Output from any provider is parsed with
  `Intent::from_provider_json`, which rejects unknown intents, extra fields
  and invalid values (control characters, out-of-range numbers).
- **Every state change needs explicit confirmation** in 0.1, even when polkit
  would allow it silently. Confirmations are single-use, bound to the D-Bus
  caller that asked, and expire after 2 minutes.
- **Honesty:** unimplemented capabilities answer "not available in this
  development build" rather than pretending.

### Capabilities in 0.1

| Capability | Intent | Risk | Implemented via |
|---|---|---|---|
| system.info | SystemInfo | read-only | mados-core |
| power.battery | BatteryStatus | read-only | `/sys/class/power_supply` |
| system.diagnose | DiagnosePerformance | read-only | `/proc` load, memory, top RSS |
| power.off / power.reboot | PowerOff / Reboot | privileged | org.mados.System1 (polkit) |
| network.wifi.set | SetWifi | settings | NetworkManager `WirelessEnabled` |
| bluetooth.set | SetBluetooth | settings | BlueZ `Adapter1.Powered` |
| display.brightness.set | SetBrightness | settings | logind `Session.SetBrightness` |
| sound.volume.set / sound.mute.set | SetVolume / SetMuted | settings | `mados-audio` (user's sound server) |
| updates.check | CheckUpdates | read-only | org.mados.System1 `CheckForUpdate` (polkit updates.check) |
| updates.install | InstallUpdate | privileged | org.mados.System1 `StartUpdate` (confirmation + polkit admin auth); waits for `UpdateJobFinished` |
| bluetooth.connect, files.search, apps.open, apps.install | … | — | **not implemented** |
| system.command | RunCommand | forbidden | always denied |

Wi-Fi, Bluetooth, brightness and update actions go through the shared
`mados_api` clients and `org.mados.System1`; they are tested against mock
NetworkManager/BlueZ/logind services and a simulated bootc on private buses,
and are **unverified on real hardware**.

## Providers

`~/.config/mados/assistant.toml`: `provider = "rules"` (default and only
provider in 0.1). Planned providers: local model (e.g. llama.cpp-compatible
runtime), OpenAI, Anthropic, Gemini. Requirements for adding one:

1. Implement `provider::Provider` returning `Option<Intent>` via
   `Intent::from_provider_json` (structured output / tool schema derived from
   the `Intent` enum).
2. Credentials come from the user's keyring (Secret Service) at runtime —
   never from this repository, the image, or plain-text config.
3. Requests leaving the device require an explicit user opt-in setting, shown
   in Settings → Privacy (M5).

## Consequences

- Natural-language coverage in 0.1 is limited to keyword rules.
- Adding a capability means: new `Intent` variant → capability row → executor
  arm → tests. The compiler enforces exhaustiveness.
