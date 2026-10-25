# Phase log 77: Website identity, TOTP audit and updater phase plan

The website now uses the packaged application icon and describes controlled secret access for AI integrations; TOTP removal and encrypted Ente export handling are repaired; signed desktop and CLI updates are next.

## Website identity, TOTP audit and updater phase plan

## Goal

Make the public website look like UnENVerse and explain why the product matters: a powerful local vault that gives controlled credentials to scripts, services and AI integrations. Audit the existing two-factor implementation and Ente support, fix any concrete shortcomings, write the required session handoff, and reorder the roadmap so desktop and CLI updating is the next project phase.

The phase change was planning only. R26 remains the roadmap identifier, but is now project phase 54 and takes priority over R10. No updater implementation started in this session.

## Current Progress

Complete, uncommitted work based on `e97c78c`.

`scripts/build-site.mjs` now copies the canonical packaged icon from `src-tauri/icons/128x128.png` to the generated `/assets/app-icon.png`. The shared site header uses that image with an explicit decorative empty `alt`, and the landing page plus all five subpages use it as their PNG favicon. This keeps the website tied to the shipped application asset without maintaining a second icon file.

The landing page now leads with the practical strength of the product: AI agents, scripts and homelabs receive only the credential they need through `unv`, while routine output stays redacted. The hero, metadata and capability cards explain agent boundaries, broad migration support, configuration generation and approved node delivery. The old claim about proving where a credential went is gone.

`scripts/check-site.mjs` now accepts an explicitly decorative empty `alt` attribute while still rejecting an image with no `alt` attribute. `tests/site-build.test.ts` proves the generated icon is byte-for-byte the packaged app icon, accepts only the declared generated asset, and asserts the shared header shape.

The two-factor audit found two real issues. `unv totp rm` previously removed a seed but could leave HOTP counter and kind fields behind. `unv-app/src/totp_cmd.rs` now clears all six TOTP fields as one operation, with a regression test for counter-based codes. `vault-core/src/totp_import.rs` recognizes Ente's encrypted-export JSON shape before the generic JSON parser and names Ente Auth plus the safe path: decrypt it with the Ente CLI into a plain-text `otpauth://` list. The README now correctly says that HOTP counters survive import and export.

Plain-text Ente migration was already implemented through the shared Rust TOTP importer. The Ente alias parses newline-separated `otpauth://` records, including the `codeDisplay` parameter, and desktop and CLI both call that same parser. Encrypted exports are deliberately not decrypted in UnENVerse because that would require taking an Ente export password and reproducing Ente's container handling. The actionable failure message prevents the old confusing generic parse error.

The roadmap records now promote D1/R26 as project phase 54 before R10. `the session records/roadmap/phases/26_distribution_channels.md` specifies a signed Tauri desktop updater and a separately signed standalone `unv` updater, with explicit user action, signature and checksum verification, atomic replacement and rollback. Package-manager channels are deferred to R26.1. The phase forbids silent installation, background updates and downgrade paths. Key generation, release publishing and CI secret setup remain maintainer-authorized actions.

## Verification

- `cargo test -p vault-core totp_import` passed 31 tests, including plain Ente and encrypted-export detection.
- `cargo test -p unv-app totp_cmd::tests` passed the HOTP removal regression test.
- `npx vitest run tests/site-build.test.ts` passed 18 tests.
- `npm run site:build` generated six pages, including the copied app icon.
- `npm run site:check` passed: `site checks ok: 6 pages x 2 viewports`.
- A local browser preview of the generated home page showed the native purple application icon in the header and the revised hero copy with no layout defect. The local server was stopped after inspection.
- The first site check reported 12 findings because the checker mistook the intentionally empty decorative `alt` for a missing attribute. The checker was corrected, then the build and check passed.
- Final `npm run preflight` passed. It covered Cargo formatting, clippy, the full Rust workspace, version check, Prettier, ESLint, TypeScript, 92 Vitest files and 1,564 tests, then the production Vite build. The run emitted existing Vite chunking notices and Cargo's `proc-macro-error2` future-incompatibility warning, neither of which failed the gate.
- `git diff --check` passed after the final formatting pass.

## The bugs that mattered

1. **High: removing an HOTP seed could leave its state behind.** A user who removed a counter-based factor could retain the counter and kind metadata on the entry. A later seed could inherit misleading state. The removal command now clears the seed and every associated TOTP parameter together.
2. **Medium: encrypted Ente exports looked like malformed generic JSON.** The importer did not identify an Ente encrypted export, so a user received an unhelpful parse failure and no safe migration path. It now identifies the vendor and directs the user to the official decrypt-to-`otpauth` workflow.
3. **Medium: the website understated the product.** The lead positioned a niche audit feature as the reason to use UnENVerse, obscuring its stronger role as controlled secret infrastructure for agents, automation and homelabs. The page now leads with that value.
4. **Low: valid decorative branding failed the website checker.** The checker rejected `alt=""` even though the attribute was deliberately declared. That made a semantic accessibility choice look like a broken site. It now distinguishes a missing attribute from an empty decorative value.

## Decisions made

- Copy the native 128px application icon only during the static-site build. The site does not own a duplicate brand asset, so its visible icon follows the packaged application.
- Describe AI integrations in terms of a strict credential boundary: one secret reaches the child process through `unv`, and routine output remains redacted. Do not imply that an agent has unrestricted vault access.
- Keep encrypted Ente processing outside UnENVerse. The official Ente route produces the plain `otpauth://` list the shared importer already verifies, and avoids adding another password-bearing decryption path to the product.
- Clear the complete TOTP field set in one command helper. This keeps removal correct for TOTP, HOTP and Steam-related parameters rather than relying on a stale partial list.
- Promote R26 without renumbering its research identity. It is now project phase 54, immediately after R09, while R26.1 retains package-manager manifests as a later delivery.
- Require explicit user confirmation for both desktop and standalone CLI updates. The design uses signatures, checksums, platform-specific artifacts, atomic replacement and rollback, but excludes automatic background installation and downgrade.

## What Worked / What Didn't Work

The shared Rust TOTP implementation made the audit tractable: format detection, import and CLI behavior already had focused test seams. Ente's documented encrypted JSON fields also allowed a fail-closed detector without retaining an export file or password.

The initial website check failed because its image rule used the element's `alt` property rather than whether the attribute existed. That exposed a checker defect, not an inaccessible page. Correcting the condition and adding a header regression test resolved it.

No sanitized real Ente export was available. The plain and encrypted paths are covered against Ente's published shape and the importer contract, but not yet against a captured vendor export fixture. The product must keep that distinction honest.

No permanent instruction-file update was needed: this session adds neither a broader engineering rule nor an ADR-level product decision beyond the roadmap phase specification.

## Next Steps

1. Start R26 as project phase 54. First establish signed release metadata, separate desktop and CLI update keys, static manifest schema and negative verification fixtures before wiring any UI or command behavior.
2. Implement the desktop path with Tauri's updater primitives, an opt-in check, release notes and an explicit install action. Preserve platform rules: visible Windows installer, AppImage only on Linux, and native package managers outside the self-updater.
3. Implement `unv update check` and `unv update`: signed manifest verification under a distinct update key, checksum verification, atomic replacement, post-replace version probe, rollback and a refusal for package-manager-managed installations.
4. The maintainer must authorize creating update keys, storing CI secrets and publishing any real manifest or release artifact. Do not generate, upload or rotate production signing material during ordinary implementation work.
5. If a lawful sanitized Ente export becomes available, add it as a secret-free fixture and confirm compatibility with the current plain and encrypted detection paths.

## Notes for whoever picks this up

- `Format::Ente` is an alias for the universal `otpauth` list, not a separate storage format. Keep CLI and desktop on the same Rust importer.
- `clear_totp_fields` intentionally includes `totp_counter` and `totp_kind`. Any future TOTP field must be considered at creation, export and removal together.
- The site checker must allow `alt=""` only when the attribute is present. Do not weaken it to ignore images without `alt`.
- R26 must use a dedicated update verification key, not the current catalogue or node-signing keys. Never commit a private key or put a release credential in a source file.
- The generated `site/` directory and Rust `target/` directory came from this session's verification and are ignored. They were removed after the final checks. Handoff and roadmap files are also ignored local records; the fourteen implementation files in `git status` are the intended commit candidates.
