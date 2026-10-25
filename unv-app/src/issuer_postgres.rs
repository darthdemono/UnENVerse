// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! The PostgreSQL connector for the rotation engine: two roles take turns.
//!
//! The application connects as `app_a` or `app_b`, each with its own password. A rotation sets a new
//! password on the role that is *not* in use (`set_value_client_generated`), checks that it can log
//! in, lets the engine deploy it, and only then clears the password of the old role. Both roles
//! stay valid until the end, so a consumer that is slow to switch is never locked out.
//!
//! The password is sent to the server already hashed: the SCRAM-SHA-256 verifier is computed here,
//! so the plaintext appears in no SQL statement, no server log and no `pg_stat_activity` row. The
//! SQL goes to the user's own `psql` on stdin, never as an argument, and the admin password reaches
//! it through its environment only.
//!
//! `VALID UNTIL` is not used: it limits password logins only and says nothing about other methods.

use crate::tool_exec::{self, ToolError, ToolSpec};
use base64::Engine as _;
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::time::Duration;
use vault_core::rotation::{Class, Credential, IssuerConnector, IssuerError, Reconcile, StepKind};

const ITERATIONS: u32 = 4096;

pub struct Postgres {
    pub psql: String,
    pub host: String,
    pub port: String,
    pub database: String,
    pub admin_user: String,
    pub admin_password: String,
    /// The two role names that take turns.
    pub roles: (String, String),
    /// How long one `psql` call may take before it is killed and counted as lost.
    pub timeout: Duration,
}

/// The default for [`Postgres::timeout`].
pub const PSQL_TIMEOUT: Duration = Duration::from_secs(30);

/// The SCRAM-SHA-256 verifier PostgreSQL stores for a password, built from a given salt.
pub fn scram_verifier(password: &str, salt: &[u8]) -> String {
    let salted = pbkdf2::pbkdf2_hmac_array::<Sha256, 32>(password.as_bytes(), salt, ITERATIONS);
    let mac = |key: &[u8], msg: &[u8]| {
        let mut m = <Hmac<Sha256> as KeyInit>::new_from_slice(key).expect("any key length");
        m.update(msg);
        m.finalize().into_bytes()
    };
    let stored = Sha256::digest(mac(&salted, b"Client Key"));
    let server = mac(&salted, b"Server Key");
    let b64 = |b: &[u8]| base64::engine::general_purpose::STANDARD.encode(b);
    format!(
        "SCRAM-SHA-256${ITERATIONS}:{}${}:{}",
        b64(salt),
        b64(&stored),
        b64(&server)
    )
}

fn valid_role(name: &str) -> Result<(), IssuerError> {
    if !name.is_empty()
        && name.len() <= 63
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        Ok(())
    } else {
        Err(IssuerError::Rejected(format!(
            "'{name}' is not a plain role name (letters, digits and underscore)"
        )))
    }
}

impl Postgres {
    fn psql(&self, user: &str, password: &str, sql: &str) -> Result<(), ToolError> {
        let mut spec = ToolSpec::new(
            &self.psql,
            &[
                "-X",
                "-q",
                "-w",
                "-v",
                "ON_ERROR_STOP=1",
                "-h",
                &self.host,
                "-p",
                &self.port,
                "-U",
                user,
                "-d",
                &self.database,
                "-f",
                "-",
            ],
        )
        .timeout(self.timeout);
        spec.stdin = Some(sql.as_bytes().to_vec());
        spec.set_env.push(("PGPASSWORD".into(), password.into()));
        spec.set_env.push(("PGCONNECT_TIMEOUT".into(), "10".into()));
        tool_exec::run(&spec).map(|_| ())
    }

    fn admin(&self, sql: &str) -> Result<(), IssuerError> {
        match self.psql(&self.admin_user, &self.admin_password, sql) {
            Ok(()) => Ok(()),
            // psql ran and the server said no: nothing changed.
            Err(ToolError::Failed { stderr, .. }) => Err(IssuerError::Rejected(stderr)),
            // A timeout or an unreadable exit: the statement may have run.
            Err(ToolError::Missing(m)) => Err(IssuerError::Rejected(m)),
            Err(e) => Err(IssuerError::Lost(format!("{e:?}"))),
        }
    }
}

impl IssuerConnector for Postgres {
    fn capabilities(&self) -> Vec<Class> {
        vec![Class::SetValueClientGenerated]
    }

    fn set_target(&self, current_id: &str, _run_name: &str) -> String {
        if current_id == self.roles.0 {
            self.roles.1.clone()
        } else {
            self.roles.0.clone()
        }
    }

    fn verify(&mut self, c: &Credential) -> Result<(), IssuerError> {
        valid_role(&c.id)?;
        match self.psql(&c.id, &c.value, "SELECT 1;") {
            Ok(()) => Ok(()),
            Err(ToolError::Failed { stderr, .. }) => Err(IssuerError::Rejected(stderr)),
            Err(e) => Err(IssuerError::Lost(format!("{e:?}"))),
        }
    }

    fn create(&mut self, _name: &str) -> Result<Credential, IssuerError> {
        Err(IssuerError::Rejected(
            "PostgreSQL rotation sets a password on the other role; it creates nothing".into(),
        ))
    }

    fn set_value(&mut self, role: &str, value: &str) -> Result<(), IssuerError> {
        valid_role(role)?;
        let salt: [u8; 16] = rand::random();
        let verifier = scram_verifier(value, &salt);
        self.admin(&format!(
            "ALTER ROLE \"{role}\" WITH LOGIN PASSWORD '{verifier}';\n"
        ))
    }

    fn roll(&mut self, _current: &Credential) -> Result<Credential, IssuerError> {
        Err(IssuerError::Rejected(
            "PostgreSQL has no provider-generated passwords".into(),
        ))
    }

    fn revoke(&mut self, c: &Credential) -> Result<(), IssuerError> {
        valid_role(&c.id)?;
        self.admin(&format!("ALTER ROLE \"{}\" WITH PASSWORD NULL;\n", c.id))
    }

    /// Both writes are idempotent (the same password, or no password), so a lost answer is
    /// repeated: "not done" is always safe to act on.
    fn reconcile(&mut self, step: &StepKind, _name: &str) -> Result<Reconcile, IssuerError> {
        Ok(match step {
            StepKind::SetValue | StepKind::Revoke => Reconcile::NotDone,
            StepKind::Create | StepKind::Roll => Reconcile::Unknown,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_verifier_has_the_shape_and_checks_postgres_expects() {
        let password = vault_core::new_uuid();
        let salt: [u8; 16] = rand::random();
        let other_salt: [u8; 16] = rand::random();
        let v = scram_verifier(&password, &salt);
        let parts: Vec<&str> = v.split(['$', ':']).collect();
        assert_eq!(parts[0], "SCRAM-SHA-256");
        assert_eq!(parts[1], "4096");
        let b64 = base64::engine::general_purpose::STANDARD;
        assert_eq!(b64.decode(parts[2]).unwrap(), salt);
        assert_eq!(b64.decode(parts[3]).unwrap().len(), 32);
        assert_eq!(b64.decode(parts[4]).unwrap().len(), 32);
        assert!(!v.contains(&password));
        assert_ne!(
            v,
            scram_verifier(&password, &other_salt),
            "the salt is part of it"
        );
        assert_ne!(v, scram_verifier(&format!("{password}x"), &salt));
    }

    #[test]
    fn a_role_name_cannot_carry_sql() {
        for bad in ["", "a\"b", "a;drop", "x y", &"a".repeat(64)] {
            assert!(valid_role(bad).is_err(), "{bad}");
        }
        assert!(valid_role("app_a").is_ok());
    }

    #[test]
    fn the_other_role_takes_the_new_password() {
        let pg = Postgres {
            psql: "psql".into(),
            host: "h".into(),
            port: "5432".into(),
            database: "d".into(),
            admin_user: "a".into(),
            admin_password: vault_core::new_uuid(),
            roles: ("app_a".into(), "app_b".into()),
            timeout: PSQL_TIMEOUT,
        };
        assert_eq!(pg.set_target("app_a", "run"), "app_b");
        assert_eq!(pg.set_target("app_b", "run"), "app_a");
        assert_eq!(pg.set_target("something_else", "run"), "app_a");
    }

    #[test]
    fn a_missing_psql_is_a_refusal_and_never_a_lost_answer() {
        let mut pg = Postgres {
            psql: "unv-no-such-psql".into(),
            host: "h".into(),
            port: "5432".into(),
            database: "d".into(),
            admin_user: "a".into(),
            admin_password: vault_core::new_uuid(),
            roles: ("app_a".into(), "app_b".into()),
            timeout: PSQL_TIMEOUT,
        };
        assert!(matches!(
            pg.set_value("app_b", &vault_core::new_uuid()),
            Err(IssuerError::Rejected(_))
        ));
        assert_eq!(
            pg.reconcile(&StepKind::SetValue, "app_b"),
            Ok(Reconcile::NotDone)
        );
    }

    /// A `psql` stand-in that records what it was given and behaves as told, so the connector's
    /// handling of arguments, environment, input, failure and silence is tested without a server.
    #[cfg(unix)]
    mod fake_psql {
        use super::*;
        use std::os::unix::fs::PermissionsExt;
        use std::path::{Path, PathBuf};

        pub struct Fake {
            pub dir: PathBuf,
            pub pg: Postgres,
            pub admin_password: String,
        }

        impl Fake {
            /// `body` runs after the recording; it decides the exit code and output.
            pub fn new(tag: &str, body: &str) -> Self {
                let dir = std::env::temp_dir()
                    .join(format!("unv-fakepsql-{tag}-{}", vault_core::new_uuid()));
                std::fs::create_dir_all(&dir).unwrap();
                let program = dir.join("psql");
                let script = format!(
                    "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{d}/argv'\ncat > '{d}/stdin'\nprintf '%s' \"$PGPASSWORD\" > '{d}/password'\nenv | grep -c '^UNV_' > '{d}/unv_env' || true\n{body}\n",
                    d = dir.display()
                );
                std::fs::write(&program, script).unwrap();
                std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
                let admin_password = vault_core::new_uuid();
                let pg = Postgres {
                    psql: program.to_string_lossy().into_owned(),
                    host: "db.internal".into(),
                    port: "6432".into(),
                    database: "appdb".into(),
                    admin_user: "adminuser".into(),
                    admin_password: admin_password.clone(),
                    roles: ("app_a".into(), "app_b".into()),
                    timeout: Duration::from_secs(5),
                };
                Self {
                    dir,
                    pg,
                    admin_password,
                }
            }
            pub fn read(&self, name: &str) -> String {
                std::fs::read_to_string(self.dir.join(name)).unwrap_or_default()
            }
        }

        impl Drop for Fake {
            fn drop(&mut self) {
                let _: &Path = &self.dir;
                let _ = std::fs::remove_dir_all(&self.dir);
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn setting_a_password_sends_a_verifier_through_stdin_as_the_admin_and_never_the_plaintext() {
        let mut f = fake_psql::Fake::new("set", "exit 0");
        let plaintext = vault_core::new_uuid();
        f.pg.set_value("app_b", &plaintext).unwrap();
        let sql = f.read("stdin");
        assert!(
            sql.starts_with("ALTER ROLE \"app_b\" WITH LOGIN PASSWORD 'SCRAM-SHA-256$4096:"),
            "{sql}"
        );
        assert!(
            !sql.contains(&plaintext),
            "the plaintext must not reach psql"
        );
        let argv = f.read("argv");
        for want in [
            "-h",
            "db.internal",
            "-p",
            "6432",
            "-U",
            "adminuser",
            "-d",
            "appdb",
            "-w",
            "ON_ERROR_STOP=1",
        ] {
            assert!(argv.lines().any(|l| l == want), "{want} in {argv}");
        }
        assert!(
            !argv.contains(&plaintext) && !argv.contains(&f.admin_password),
            "no secret in argv"
        );
        assert_eq!(
            f.read("password"),
            f.admin_password,
            "the admin password travels by environment"
        );
        assert_eq!(
            f.read("unv_env").trim(),
            "0",
            "no UNV_ variable leaks into psql"
        );
    }

    #[cfg(unix)]
    #[test]
    fn verifying_logs_in_as_the_role_with_its_own_password() {
        let mut f = fake_psql::Fake::new("verify", "exit 0");
        let value = vault_core::new_uuid();
        f.pg.verify(&Credential {
            id: "app_b".into(),
            value: value.clone(),
        })
        .unwrap();
        assert!(f
            .read("argv")
            .lines()
            .zip(f.read("argv").lines().skip(1))
            .any(|(a, b)| a == "-U" && b == "app_b"));
        assert_eq!(f.read("password"), value);
        assert_eq!(f.read("stdin").trim(), "SELECT 1;");
    }

    #[cfg(unix)]
    #[test]
    fn revoking_clears_the_old_roles_password_and_nothing_else() {
        let mut f = fake_psql::Fake::new("revoke", "exit 0");
        f.pg.revoke(&Credential {
            id: "app_a".into(),
            value: String::new(),
        })
        .unwrap();
        assert_eq!(
            f.read("stdin").trim(),
            "ALTER ROLE \"app_a\" WITH PASSWORD NULL;"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_server_that_says_no_is_a_refusal_with_its_message_and_a_silent_one_is_a_lost_answer() {
        let mut f = fake_psql::Fake::new("no", "echo 'role \"app_b\" does not exist' >&2; exit 2");
        match f.pg.set_value("app_b", &vault_core::new_uuid()) {
            Err(IssuerError::Rejected(m)) => assert!(m.contains("does not exist"), "{m}"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            f.pg.verify(&Credential {
                id: "app_b".into(),
                value: vault_core::new_uuid()
            }),
            Err(IssuerError::Rejected(_))
        ));
        assert!(matches!(
            f.pg.revoke(&Credential {
                id: "app_a".into(),
                value: String::new()
            }),
            Err(IssuerError::Rejected(_))
        ));

        let mut slow = fake_psql::Fake::new("slow", "sleep 5");
        slow.pg.timeout = Duration::from_millis(300);
        assert!(matches!(
            slow.pg.set_value("app_b", &vault_core::new_uuid()),
            Err(IssuerError::Lost(_))
        ));
        assert!(matches!(
            slow.pg.verify(&Credential {
                id: "app_b".into(),
                value: vault_core::new_uuid()
            }),
            Err(IssuerError::Lost(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_role_name_that_could_carry_sql_never_reaches_psql() {
        let mut f = fake_psql::Fake::new("inject", "exit 0");
        assert!(f
            .pg
            .set_value("app\"; DROP ROLE x; --", &vault_core::new_uuid())
            .is_err());
        assert!(f
            .pg
            .revoke(&Credential {
                id: "a b".into(),
                value: String::new()
            })
            .is_err());
        assert!(f
            .pg
            .verify(&Credential {
                id: "".into(),
                value: vault_core::new_uuid()
            })
            .is_err());
        assert_eq!(f.read("argv"), "", "psql was never started");
    }

    #[test]
    fn creating_or_rolling_is_refused_and_reconcile_knows_what_is_safe_to_repeat() {
        let mut pg = Postgres {
            psql: "psql".into(),
            host: "h".into(),
            port: "1".into(),
            database: "d".into(),
            admin_user: "a".into(),
            admin_password: vault_core::new_uuid(),
            roles: ("app_a".into(), "app_b".into()),
            timeout: PSQL_TIMEOUT,
        };
        assert!(matches!(pg.create("n"), Err(IssuerError::Rejected(_))));
        assert!(matches!(
            pg.roll(&Credential {
                id: "app_a".into(),
                value: String::new()
            }),
            Err(IssuerError::Rejected(_))
        ));
        assert_eq!(pg.capabilities(), vec![Class::SetValueClientGenerated]);
        assert_eq!(
            pg.reconcile(&StepKind::Revoke, "app_a"),
            Ok(Reconcile::NotDone)
        );
        assert_eq!(pg.reconcile(&StepKind::Create, "x"), Ok(Reconcile::Unknown));
        assert_eq!(pg.reconcile(&StepKind::Roll, "x"), Ok(Reconcile::Unknown));
    }

    /// Against a real server. Set `UNV_TEST_PSQL` (a `psql` or a wrapper), `UNV_TEST_PG_HOST` and
    /// `UNV_TEST_PG_ADMIN_PASSWORD`; the server needs login roles `app_a` (password `UNV_TEST_PG_A`)
    /// and `app_b`. Ignored otherwise.
    #[test]
    #[ignore = "needs a PostgreSQL server (see the doc comment)"]
    fn two_roles_take_turns_against_a_real_server() {
        use vault_core::rotation::*;
        let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k} is not set"));
        let mut pg = Postgres {
            psql: var("UNV_TEST_PSQL"),
            host: var("UNV_TEST_PG_HOST"),
            port: "5432".into(),
            database: "postgres".into(),
            admin_user: "postgres".into(),
            admin_password: var("UNV_TEST_PG_ADMIN_PASSWORD"),
            roles: ("app_a".into(), "app_b".into()),
            timeout: PSQL_TIMEOUT,
        };
        let old = Credential {
            id: "app_a".into(),
            value: var("UNV_TEST_PG_A"),
        };
        pg.verify(&old).unwrap();
        let fresh = format!("fresh-{}", vault_core::new_uuid());
        let target = pg.set_target(&old.id, "run");
        pg.set_value(&target, &fresh).unwrap();
        pg.verify(&Credential {
            id: target.clone(),
            value: fresh.clone(),
        })
        .unwrap();
        pg.verify(&old).unwrap();
        assert!(
            pg.verify(&Credential {
                id: target.clone(),
                value: vault_core::new_uuid()
            })
            .is_err(),
            "a wrong password is refused"
        );
        pg.revoke(&old).unwrap();
        assert!(
            pg.verify(&old).is_err(),
            "the old role can no longer log in"
        );
        pg.verify(&Credential {
            id: target,
            value: fresh,
        })
        .unwrap();
    }
}
