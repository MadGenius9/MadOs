//! accounts::list against a mock AccountsService on a private bus.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

mod common;

struct PrivateBus(Child, String);

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start() -> Option<PrivateBus> {
    let mut child = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut line = String::new();
    BufReader::new(child.stdout.take()?).read_line(&mut line).ok()?;
    Some(PrivateBus(child, line.trim().to_string()))
}

#[test]
fn lists_accounts_sorted_with_roles() {
    let Some(bus) = start() else {
        eprintln!("SKIP: dbus-daemon not available");
        return;
    };
    zbus::block_on(async {
        let conn = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        assert!(
            mados_api::accounts::list(&conn).await.is_err(),
            "no AccountsService -> error"
        );
        let _server = common::mock_accounts::serve(&bus.1).await;
        let users = mados_api::accounts::list(&conn).await.unwrap();
        let summary: Vec<(&str, u64, bool)> = users
            .iter()
            .map(|u| (u.user_name.as_str(), u.uid, u.administrator))
            .collect();
        assert_eq!(summary, [("mados", 1000, true), ("guest", 1001, false)]);
        assert_eq!(users[0].real_name, "Dev User");
    });
}
