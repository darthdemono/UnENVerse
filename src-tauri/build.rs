// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
fn main() {
    // The app binary gets its Windows manifest from tauri-build. The integration
    // tests link the same Tauri runtime without it, so Windows cannot resolve the
    // comctl32 v6 entry points (STATUS_ENTRYPOINT_NOT_FOUND) before a test runs.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
        println!("cargo:rerun-if-changed=windows-test.manifest");
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-tests=/MANIFESTINPUT:{dir}\\windows-test.manifest");
    }
    tauri_build::build()
}
