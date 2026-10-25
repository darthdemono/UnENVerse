#![no_main]
// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.

use libfuzzer_sys::fuzz_target;
use unv_app::envfile::{disambiguate_names, env_name_from_text, parse_env_file, unquote_env_value};

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let names: Vec<String> = parse_env_file(&text).into_iter().map(|v| v.name).collect();
    let _ = disambiguate_names(&names);
    let _ = unquote_env_value(&text);
    // The schema promises every generated name is a legal variable name.
    let name = env_name_from_text(&text);
    assert!(name.starts_with(|c: char| c == '_' || c.is_ascii_uppercase()));
    assert!(name
        .chars()
        .all(|c| c == '_' || c.is_ascii_uppercase() || c.is_ascii_digit()));
});
