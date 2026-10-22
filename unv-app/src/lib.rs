// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Shared application engine for the CLI, desktop app and optional server.
//!
//! This is the former library half of `unv-cli`. It owns the reusable vault
//! operations while `unv-cli` stays responsible for argument parsing and
//! terminal output wiring.

pub mod access;
pub mod agentio;
pub mod authreq;
pub mod backup;
pub mod blast_cmd;
pub mod bundle_cmd;
pub mod catalogue_cmd;
pub mod check_cmd;
pub mod chunks;
pub mod context;
pub mod cookies;
pub mod cxf_cmd;
pub mod data;
pub mod destinations;
pub mod diff_cmd;
pub mod doctor;
pub mod emit_cmd;
pub mod enrich;
pub mod entries;
pub mod envfile;
pub mod error;
pub mod exec;
pub mod exporters;
pub mod exposure;
pub mod feed_cmd;
pub mod filecred;
pub mod fmt;
pub mod gen;
pub mod git_sources;
pub mod git_sync;
pub mod history;
pub mod history_cmd;
pub mod import_vaults;
pub mod issuer_cloudflare;
pub mod matlog;
pub mod migrate;
pub mod node_agent;
pub mod node_cmd;
pub mod node_install;
pub mod node_listen;
pub mod oauth_cmd;
pub mod out;
pub mod password_sources;
pub mod pool;
pub mod profile;
pub mod projects;
pub mod ratelimit;
pub mod recovery_ops;
pub mod refs;
pub mod render;
pub mod reset_cmd;
pub mod scan;
#[cfg(feature = "aws")]
pub mod service_aws;
pub mod service_sources;
pub mod session;
pub mod session_cmd;
pub mod shield;
pub mod starters;
pub mod template_cmd;
pub mod tls;
pub mod tool_exec;
pub mod totp_cmd;
pub mod uid_cmd;
pub mod update_cmd;
pub mod users_cmd;
