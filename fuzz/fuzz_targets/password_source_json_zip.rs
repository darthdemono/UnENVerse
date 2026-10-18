#![no_main]
// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    unv_app::password_sources::fuzz_json_zip(data);
});
