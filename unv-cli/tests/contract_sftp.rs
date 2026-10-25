// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! SFTP recovery destinations ("Backups", README), against a real OpenSSH server
//! in a container (`atmoz/sftp`); skipped where Docker or the image is missing.
//!
//! * the host key is shown first and pinned only when it is passed back; a
//!   different key is refused naming both;
//! * a package written to the server is listed, restores in a drill, and the
//!   password comes from a vault entry, never the command line.

mod common;
use common::Vault;
use std::process::Command;

struct Container(String);

impl Drop for Container {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "--force", &self.0])
            .output();
    }
}

fn start() -> Option<(Container, u16)> {
    let image = Command::new("docker")
        .args(["image", "inspect", "atmoz/sftp:latest"])
        .output()
        .ok()?;
    if !image.status.success() {
        eprintln!("skipped: atmoz/sftp:latest is not available");
        return None;
    }
    let name = format!(
        "unv-sftp-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let up = Command::new("docker")
        .args([
            "run",
            "--detach",
            "--rm",
            "--name",
            &name,
            "--publish",
            "127.0.0.1::22",
            "atmoz/sftp:latest",
            "unv:fixture-password:1001:1001:upload",
        ])
        .output()
        .ok()?;
    if !up.status.success() {
        return None;
    }
    let c = Container(name.clone());
    for _ in 0..80 {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let out = Command::new("docker")
            .args(["port", &name, "22/tcp"])
            .output()
            .ok()?;
        let line = String::from_utf8_lossy(&out.stdout).to_string();
        if let Some(port) = line
            .lines()
            .next()
            .and_then(|l| l.rsplit(':').next())
            .and_then(|p| p.trim().parse::<u16>().ok())
        {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                std::thread::sleep(std::time::Duration::from_millis(800));
                return Some((c, port));
            }
        }
    }
    None
}

fn add(v: &Vault, port: &str, extra: &[&str]) -> common::Out {
    let mut a = vec![
        "backup",
        "dest",
        "add",
        "sftp",
        "remote",
        "--host",
        "127.0.0.1",
        "--port",
        port,
        "--username",
        "unv",
        "--path",
        "upload",
        "--credential",
        "sftp-login",
    ];
    a.extend_from_slice(extra);
    v.run(&a)
}

#[test]
fn an_sftp_destination_is_pinned_then_carries_a_package_and_a_drill() {
    let Some((_c, port)) = start() else { return };
    let port = port.to_string();
    let v = Vault::new("sftp");
    v.ok(&["entry", "add", "Seed", "--key", "seed-secret-123456"]);
    v.ok(&["entry", "add", "sftp-login", "--key", "fixture-password"]);
    v.ok(&["backup", "recovery-key"]);

    let first = add(&v, &port, &[]);
    assert_eq!(
        first.code,
        10,
        "the key is shown, not trusted: {}",
        first.all()
    );
    let msg = first.all();
    let fp = msg
        .split('`')
        .nth(1)
        .unwrap_or_else(|| panic!("no fingerprint in: {msg}"))
        .to_string();
    assert!(fp.starts_with("SHA256:"), "{fp}");
    assert!(
        v.ok(&["backup", "dest", "ls"])["destinations"]
            .as_array()
            .unwrap()
            .is_empty(),
        "nothing is stored before it is pinned"
    );

    let wrong = add(&v, &port, &["--host-key", "SHA256:not-the-key"]);
    assert_eq!(wrong.code, 10, "{}", wrong.all());
    assert!(
        wrong.all().contains("SHA256:not-the-key") && wrong.all().contains(&fp),
        "names both: {}",
        wrong.all()
    );

    let pinned = add(&v, &port, &["--host-key", &fp]);
    assert_eq!(pinned.code, 0, "{}", pinned.all());
    let ls = v.ok(&["backup", "dest", "ls"]);
    assert_eq!(
        ls["destinations"][0]["transport"]["sftp"]["host_key_fingerprint"],
        fp.as_str(),
        "the pin is stored: {ls}"
    );
    assert!(
        !v.run(&["backup", "dest", "ls"])
            .all()
            .contains("fixture-password"),
        "the password is never stored in the destination"
    );

    let made = v.run(&["backup", "package", "--dest", "remote"]);
    assert_eq!(made.code, 0, "{}", made.all());
    assert_eq!(
        made.json()["data"]["receipt"]["destination_id"],
        "remote",
        "{}",
        made.stdout
    );
    let drill = v.run(&["backup", "drill", "--dest", "remote"]);
    assert_eq!(
        drill.code,
        0,
        "restored from the server and verified: {}",
        drill.all()
    );
    assert_eq!(drill.json()["data"]["run"]["state"], "succeeded");
}

#[test]
fn a_wrong_password_in_the_credential_entry_is_refused() {
    let Some((_c, port)) = start() else { return };
    let port = port.to_string();
    let v = Vault::new("sftpbad");
    v.ok(&["entry", "add", "sftp-login", "--key", "not-the-password"]);
    let first = add(&v, &port, &[]);
    let fp = first.all().split('`').nth(1).unwrap().to_string();
    let o = add(&v, &port, &["--host-key", &fp]);
    // Adding probes the server, so a bad login is caught before anything is stored.
    if o.code == 0 {
        v.ok(&["backup", "recovery-key"]);
        let p = v.run(&["backup", "package", "--dest", "remote"]);
        assert_ne!(p.code, 0, "{}", p.all());
    } else {
        assert!(v.ok(&["backup", "dest", "ls"])["destinations"]
            .as_array()
            .unwrap()
            .is_empty());
    }
}
