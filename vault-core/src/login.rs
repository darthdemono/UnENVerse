// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! The login sequence of a bundle (R36): one account made of up to four credentials, in a fixed
//! order, whatever order the members were added in.
//!
//! 1. the password, 2. the 2FA code, 3. the security phrase, 4. a recovery code.
//!
//! This is the one place the order is decided (invariant 10): the CLI, the desktop card and the
//! browser host all call [`login_sequence`]. A step with nothing stored is absent, not empty. The
//! plan names entries and fields and never carries a value.

use crate::type_emit::code_status;
use serde_json::Value;

/// One step of a login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// `password`, `2fa`, `phrase` or `recovery_code`.
    pub step: &'static str,
    pub label: &'static str,
    pub entry_id: String,
    pub field: &'static str,
}

/// What happens to a recovery code once it has been filled into a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminate {
    /// Show "Mark this code as used?" after the fill (the default).
    Ask,
    /// Mark it used straight after the fill.
    Auto,
    /// Leave it for the person.
    Never,
}

impl Terminate {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "ask" => Some(Self::Ask),
            "auto" => Some(Self::Auto),
            "never" => Some(Self::Never),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Auto => "auto",
            Self::Never => "never",
        }
    }
}

/// The bundle's `terminate_codes` setting; `ask` when unset or unreadable.
pub fn terminate_mode(bundle: &Value) -> Terminate {
    bundle
        .get("terminate_codes")
        .and_then(Value::as_str)
        .and_then(Terminate::parse)
        .unwrap_or(Terminate::Ask)
}

fn text<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

fn kind(v: &Value) -> &str {
    match text(v, "secretType") {
        "" => "api_key",
        t => t,
    }
}

/// The ordered steps present in `bundle`, from `entries` (the whole vault's entries).
pub fn login_sequence(entries: &[Value], bundle: &Value) -> Vec<Step> {
    let id = text(bundle, "id");
    if id.is_empty() {
        return Vec::new();
    }
    let members: Vec<&Value> = entries
        .iter()
        .filter(|e| text(e, "bundle_id") == id)
        .collect();
    let by = |ty: &str| members.iter().copied().find(|m| kind(m) == ty);
    let password = by("password").or_else(|| {
        members.iter().copied().find(|m| {
            !text(m, "api_key").is_empty()
                && !matches!(kind(m), "security_phrase" | "recovery_codes")
        })
    });
    let mut out = Vec::new();
    if let Some(p) = password {
        if !text(p, "api_key").is_empty() {
            out.push(Step {
                step: "password",
                label: "Password",
                entry_id: text(p, "id").into(),
                field: "api_key",
            });
        }
    }
    let seed = password
        .filter(|p| !text(p, "totp_secret").is_empty())
        .or_else(|| {
            members
                .iter()
                .copied()
                .find(|m| !text(m, "totp_secret").is_empty())
        });
    if let Some(s) = seed {
        out.push(Step {
            step: "2fa",
            label: "2FA code",
            entry_id: text(s, "id").into(),
            field: "totp_secret",
        });
    }
    if let Some(ph) = by("security_phrase").filter(|m| !text(m, "api_key").is_empty()) {
        out.push(Step {
            step: "phrase",
            label: "Security phrase",
            entry_id: text(ph, "id").into(),
            field: "api_key",
        });
    }
    if let Some(rc) = by("recovery_codes").filter(|m| code_status(m).remaining > 0) {
        out.push(Step {
            step: "recovery_code",
            label: "Recovery code",
            entry_id: text(rc, "id").into(),
            field: "codes",
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn member(id: &str, ty: &str, extra: Value) -> Value {
        let mut m = json!({ "id": id, "bundle_id": "b1", "secretType": ty });
        for (k, v) in extra.as_object().unwrap() {
            m[k] = v.clone();
        }
        m
    }

    fn all() -> Vec<Value> {
        vec![
            member(
                "rc",
                "recovery_codes",
                json!({ "extra_vars": [{ "key": "codes", "value": "aaaa-1111\nbbbb-2222" }] }),
            ),
            member("ph", "security_phrase", json!({ "api_key": "blue cat" })),
            member(
                "pw",
                "password",
                json!({ "api_key": "pw", "totp_secret": "JBSWY3DP" }),
            ),
            json!({ "id": "other", "bundle_id": "b2", "secretType": "password", "api_key": "x" }),
        ]
    }

    #[test]
    fn the_order_is_fixed_whatever_order_the_members_were_added_in() {
        let steps = login_sequence(&all(), &json!({ "id": "b1" }));
        let names: Vec<_> = steps.iter().map(|s| s.step).collect();
        assert_eq!(names, ["password", "2fa", "phrase", "recovery_code"]);
        assert_eq!(steps[0].entry_id, "pw");
        assert_eq!(steps[3].field, "codes");
    }

    #[test]
    fn a_step_with_nothing_stored_is_absent_and_spent_codes_drop_the_last_step() {
        let mut e = all();
        e.remove(1); // no phrase
        e[0]["extra_vars"][0]["value"] = json!("aaaa-1111\tUSED 2026-01-01");
        e[1].as_object_mut().unwrap().remove("totp_secret");
        let names: Vec<_> = login_sequence(&e, &json!({ "id": "b1" }))
            .iter()
            .map(|s| s.step)
            .collect();
        assert_eq!(names, ["password"]);
        assert!(login_sequence(&e, &json!({})).is_empty());
    }

    #[test]
    fn a_login_without_a_password_type_uses_the_first_member_that_holds_a_value() {
        let e = vec![member(
            "t",
            "api_key",
            json!({ "api_key": "tok", "totp_secret": "AAAA" }),
        )];
        let steps = login_sequence(&e, &json!({ "id": "b1" }));
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[1].entry_id, "t");
        // A seed on a different member still gives step 2.
        let e = vec![
            member("p", "password", json!({ "api_key": "pw" })),
            member("o", "api_key", json!({ "totp_secret": "AAAA" })),
        ];
        assert_eq!(login_sequence(&e, &json!({ "id": "b1" }))[1].entry_id, "o");
    }

    #[test]
    fn terminate_modes_parse_and_default_to_ask() {
        assert_eq!(terminate_mode(&json!({})), Terminate::Ask);
        assert_eq!(
            terminate_mode(&json!({ "terminate_codes": "auto" })),
            Terminate::Auto
        );
        assert_eq!(
            terminate_mode(&json!({ "terminate_codes": "never" })).name(),
            "never"
        );
        assert_eq!(
            terminate_mode(&json!({ "terminate_codes": "bogus" })),
            Terminate::Ask
        );
        assert_eq!(Terminate::parse("ask").unwrap().name(), "ask");
        assert!(Terminate::parse("x").is_none());
    }

    #[test]
    fn a_plan_never_carries_a_value() {
        let dump = format!("{:?}", login_sequence(&all(), &json!({ "id": "b1" })));
        for secret in ["JBSWY3DP", "blue cat", "aaaa-1111"] {
            assert!(!dump.contains(secret));
        }
    }
}
