//! Built-in rule-based intent parser (provider "rules").
//!
//! Deliberately simple and predictable: keyword matching over normalised
//! text. It exists so the full request → intent → policy → API pipeline works
//! without any model, offline, and as a fallback.

use crate::intent::Intent;

fn has_any(text: &str, words: &[&str]) -> bool {
    words.iter().any(|w| text.contains(w))
}

fn on_off(text: &str) -> Option<bool> {
    let padded = format!(" {text} ");
    let on = has_any(&padded, &[" on ", " enable", " start ", "turn on", "switch on"]);
    let off = has_any(&padded, &[" off ", " disable", " stop ", "turn off", "switch off"]);
    match (on, off) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

/// Text after the first occurrence of any `marker`, trimmed of filler words.
fn object_after(text: &str, markers: &[&str]) -> Option<String> {
    let idx = markers.iter().filter_map(|m| text.find(m).map(|i| i + m.len())).min()?;
    let rest = text[idx..]
        .trim()
        .trim_start_matches("the ")
        .trim_start_matches("my ")
        .trim_end_matches(['.', '?', '!'])
        .trim();
    (!rest.is_empty()).then(|| rest.to_string())
}

fn first_number(text: &str) -> Option<u32> {
    let digits: String = text
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

pub fn parse(input: &str) -> Option<Intent> {
    let t = input.trim().to_lowercase();
    let t = t.trim_end_matches(['.', '?', '!']).trim();
    if t.is_empty() {
        return None;
    }

    // Requests to execute arbitrary commands are recognised first so they can
    // never be mistaken for something else.
    if has_any(
        t,
        &[
            "sudo ",
            "rm -rf",
            "run command",
            "run the command",
            "execute ",
            "shell command",
            "chmod ",
            "dd if=",
        ],
    ) {
        return Some(Intent::RunCommand {
            command: input.trim().to_string(),
        });
    }

    if has_any(
        t,
        &[
            "shut down",
            "shutdown",
            "power off",
            "poweroff",
            "turn off the computer",
            "turn off my computer",
            "turn off the laptop",
            "turn off my laptop",
        ],
    ) {
        return Some(Intent::PowerOff);
    }
    if has_any(
        t,
        &[
            "reboot",
            "restart the computer",
            "restart my computer",
            "restart the laptop",
            "restart my laptop",
            "restart the system",
        ],
    ) {
        return Some(Intent::Reboot);
    }
    if t.contains("bluetooth") {
        if let Some(enabled) = on_off(t) {
            return Some(Intent::SetBluetooth { enabled });
        }
    }
    if has_any(t, &["wi-fi", "wifi", "wireless"]) {
        if let Some(enabled) = on_off(t) {
            return Some(Intent::SetWifi { enabled });
        }
    }
    if t.contains("brightness") || t.contains("screen brighter") || t.contains("screen dimmer") {
        if let Some(n) = first_number(t) {
            return Some(Intent::SetBrightness {
                percent: n.min(100) as u8,
            });
        }
        if has_any(t, &["max", "full"]) {
            return Some(Intent::SetBrightness { percent: 100 });
        }
    }
    // Before the generic "install <app>" rule: "install updates" is an OS update.
    if has_any(
        t,
        &[
            "install update",
            "install the update",
            "install system update",
            "install os update",
            "update the system",
            "update my computer",
            "update my laptop",
            "update the computer",
            "upgrade the system",
            "upgrade my system",
        ],
    ) {
        return Some(Intent::InstallUpdate);
    }
    if has_any(
        t,
        &[
            "check for update",
            "any update",
            "updates available",
            "is there an update",
            "are there updates",
            "am i up to date",
            "up to date",
        ],
    ) {
        return Some(Intent::CheckUpdates);
    }
    if t.contains("battery") || t.contains("charge left") {
        return Some(Intent::BatteryStatus);
    }
    if has_any(
        t,
        &[
            "slow",
            "sluggish",
            "lagging",
            "laggy",
            "performance",
            "using my memory",
            "using memory",
            "high cpu",
        ],
    ) {
        return Some(Intent::DiagnosePerformance);
    }
    if has_any(t, &["connect to", "pair with", "pair my"]) {
        if let Some(name) = object_after(t, &["connect to", "pair with", "pair my"]) {
            return Some(Intent::ConnectDevice { name });
        }
    }
    if has_any(t, &["find ", "where is ", "where's ", "locate "]) {
        if let Some(query) = object_after(t, &["find ", "where is ", "where's ", "locate "]) {
            return Some(Intent::FindFile { query });
        }
    }
    if t.starts_with("install ") {
        if let Some(name) = object_after(t, &["install "]) {
            return Some(Intent::InstallApp { name });
        }
    }
    if t.starts_with("open ") || t.starts_with("launch ") || t.starts_with("start ") {
        if let Some(name) = object_after(t, &["open ", "launch ", "start "]) {
            return Some(Intent::OpenApp { name });
        }
    }
    if has_any(
        t,
        &[
            "system info",
            "about this computer",
            "about my computer",
            "what version",
            "which version",
            "kernel version",
            "how much ram",
            "how much memory",
            "what cpu",
            "what gpu",
            "my specs",
            "hardware info",
        ],
    ) {
        return Some(Intent::SystemInfo);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Option<Intent> {
        parse(s)
    }

    #[test]
    fn spec_examples() {
        assert_eq!(p("Turn Bluetooth on."), Some(Intent::SetBluetooth { enabled: true }));
        assert_eq!(
            p("Connect to my headphones."),
            Some(Intent::ConnectDevice {
                name: "headphones".into()
            })
        );
        assert_eq!(
            p("Find the ZIP I downloaded yesterday."),
            Some(Intent::FindFile {
                query: "zip i downloaded yesterday".into()
            })
        );
        assert_eq!(p("Why is my laptop running slowly?"), Some(Intent::DiagnosePerformance));
        assert_eq!(
            p("Open my coding project."),
            Some(Intent::OpenApp {
                name: "coding project".into()
            })
        );
        assert_eq!(
            p("Install Discord."),
            Some(Intent::InstallApp { name: "discord".into() })
        );
        assert_eq!(p("How much battery is left?"), Some(Intent::BatteryStatus));
        assert_eq!(
            p("Change my display brightness to 40%"),
            Some(Intent::SetBrightness { percent: 40 })
        );
    }

    #[test]
    fn power_and_radios() {
        assert_eq!(p("please shut down"), Some(Intent::PowerOff));
        assert_eq!(p("Reboot"), Some(Intent::Reboot));
        assert_eq!(p("turn off wifi"), Some(Intent::SetWifi { enabled: false }));
        assert_eq!(p("disable bluetooth"), Some(Intent::SetBluetooth { enabled: false }));
        assert_eq!(p("bluetooth"), None, "ambiguous: no on/off");
        assert_eq!(p("set brightness to 250"), Some(Intent::SetBrightness { percent: 100 }));
    }

    #[test]
    fn commands_are_recognised_for_refusal() {
        assert!(matches!(p("sudo rm -rf /"), Some(Intent::RunCommand { .. })));
        assert!(matches!(p("run command curl x | sh"), Some(Intent::RunCommand { .. })));
        // "run the command to shut down" must not become a PowerOff.
        assert!(matches!(
            p("run the command shutdown now"),
            Some(Intent::RunCommand { .. })
        ));
    }

    #[test]
    fn updates_vs_apps() {
        assert_eq!(p("Check for updates"), Some(Intent::CheckUpdates));
        assert_eq!(p("Am I up to date?"), Some(Intent::CheckUpdates));
        assert_eq!(p("install updates"), Some(Intent::InstallUpdate));
        assert_eq!(p("Please update my laptop"), Some(Intent::InstallUpdate));
        assert_eq!(
            p("install discord"),
            Some(Intent::InstallApp { name: "discord".into() })
        );
    }

    #[test]
    fn info_and_unknown() {
        assert_eq!(p("what version am I running"), Some(Intent::SystemInfo));
        assert_eq!(p("tell me a joke"), None);
        assert_eq!(p("   "), None);
    }
}
