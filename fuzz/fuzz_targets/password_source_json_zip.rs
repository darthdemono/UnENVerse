#![no_main]
// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    unv_app::password_sources::fuzz_json_zip(data);
});
