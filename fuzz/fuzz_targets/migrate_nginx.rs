#![no_main]
// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let _ = unv_app::migrate::parser_preview(unv_app::migrate::SourceKind::Nginx, "fuzz", &text);
});
