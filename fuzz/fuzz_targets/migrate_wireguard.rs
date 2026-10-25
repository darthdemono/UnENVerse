#![no_main]
// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let _ = unv_app::migrate::parser_preview(unv_app::migrate::SourceKind::WireGuard, "fuzz", &text);
});
