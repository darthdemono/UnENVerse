# Phase log 39: 2026-10-08 - Phase 26, outward redaction engine

## Goal

Complete current Phase 26: one exact-value redaction engine behind explicit
`envv shield -- <command>` and `envv scan --exposed <path> --out <file>`.

## Current Progress

Phase 26 is complete and uncommitted.

- `envv-cli/src/shield.rs` builds one Aho-Corasick matcher from every vault value
  the fail-closed CLI redactor treats as private. It uses the existing
  `sha256:<12 hex>` fingerprints for replacements and reports.
- `envv shield -- <command>` runs a child with piped stdout and stderr, shields
  both streams independently, and returns the child's exit code. It keeps the
  final `max_secret_len - 1` bytes between reads, so a secret that crosses a
  pipe read boundary cannot leak a prefix.
- The wrapper refuses NUL-containing or invalid-UTF-8 output rather than
  half-masking binary data. It drains the rest of that pipe before failing so a
  child cannot block on a full pipe. `--json` is refused because the wrapper
  deliberately forwards the child's ordinary output.
- `envv scan --exposed PATH --out REPORT` scans a supplied file, or walks a
  supplied directory without following directory links, in bounded reads with
  the same overlap window. The report contains only `file:line` and a
  fingerprint, never the plaintext.
  It is refused without `--out` even under `--reveal`, and uses the existing
  0600 secret-file writer.
- `out::secret_values` is shared with the matcher. It preserves the existing
  exceptions for deliberately public primary/secondary values and public
  extra-vars, while including history, file payloads and unknown string fields
  that fail-closed redaction masks.

## Detailed changelog

### A single private-value inventory

The new matcher does not maintain a second list of secret fields. Instead,
envv-cli/src/out.rs exposes secret_values(entry), built from the established
fail-closed redaction policy used by redact_entry. The matcher therefore sees:

- known secret slots unless the existing public-slot rule marks the primary or
  secondary value public;
- unknown string fields unless they are known public metadata, preserving
  forward-compatible fail-closed behavior;
- extra variables unless they are public, with cookie entries retaining their
  existing whole-entry treatment;
- version-history values, so a rotated but still materialized secret is not
  forgotten by shield or exposure scanning.

Empty values are discarded. The remaining values are deduplicated through a
BTreeSet before matcher creation, giving deterministic pattern ownership and
avoiding repeated replacement/report work. This is important because EnvVault
has already experienced the opposite policy failure: an older binary knew to
mask api_key but did not know a later TOTP field and printed it clear.

### Exact Aho-Corasick matching and overlap policy

envv-cli/src/shield.rs constructs one Aho-Corasick matcher per command from the
vault values. It does not guess at credential-looking strings, token shapes or
field names in the child output: a finding means a current private vault value
appeared verbatim as a contiguous byte sequence.

Every stored pattern retains only its existing EnvVault fingerprint for output.
The replacement/report marker is sha256 followed by the standard truncated
12-character lower-case hexadecimal digest; plaintext values are never used as
replacement text.

The matcher deliberately asks Aho-Corasick for overlapping hits, then applies a
manual leftmost-longest non-overlap selection. If a vault holds both key and
key-long and the stream contains key-long, the result is one key-long
fingerprint. This prevents a shorter match from leaving a suffix visible or
producing two misleading replacements.

### Stream-safe shield behavior

envv shield loads the vault, splits the raw command after the required --
separator, starts the child with stdout and stderr piped, and shields the two
streams on independent reader threads. A normal numerical child exit status is
returned after both readers finish, and main exits with that status; a child
exit 23 remains exit 23.

The critical boundary rule is implemented by StreamRedactor. It withholds the
last max_secret_len - 1 bytes of every nonfinal read. Those bytes might be a
prefix completed in the next pipe buffer, so they are not emitted until the
next read or final flush. The reader buffer is 16 KiB, but correctness does not
depend on a value fitting inside it.

TextProbe refuses a stream containing NUL or a definitely invalid UTF-8
sequence. It retains incomplete valid UTF-8 at a read boundary so ordinary
Unicode text is not mistaken for binary. When binary is detected, shield keeps
draining the pipe to avoid blocking the child on a full output buffer, emits no
bytes from the binary-producing read onward, and returns an Invalid error.
Global --json is rejected because the wrapper forwards arbitrary child output
and cannot promise EnvVault’s JSON envelope at the same time.

### Exposure scanning and reporting

Engine::scan_path accepts one existing file or directory. Directories are
walked with follow_links(false), and symlink entries encountered during that
walk are not scanned as regular files. A directly supplied symlink to a file is
currently resolved by Path::is_file and therefore scanned; that is distinct
from following links discovered inside a directory tree. Each file is read in
16 KiB chunks using the same maximum-pattern overlap logic, so a value
straddling a file-read boundary is still found. FileScanner counts preceding
newlines and reports a 1-based line number. Final findings are sorted by path,
line and fingerprint for stable output.

The report format is intentionally narrow:

    path:line: sha256:12-hex-characters

It has no plaintext value, source line, surrounding JSON, provider or owner
metadata. cmd_exposed refuses to run without --out, including under --reveal,
because the locations themselves are sensitive. It uses the existing
fmt::write_secret_file policy, producing a 0600 report, and sends only a
non-secret “Wrote PATH” note to stderr.

### CLI integration

envv-cli/src/main.rs registers Shield with final/raw child arguments and
extends Scan with optional --exposed and --out arguments. Without --exposed,
the existing health scan keeps its severity/JSON behavior. With it, Scan
dispatches to cmd_exposed and requires --out. envv-cli/src/lib.rs exports the
module for the binary and its integration test.

Cargo.toml declares aho-corasick and walkdir directly in envv-cli. They were
already present transitively in Cargo.lock, so the lockfile change records
direct ownership rather than a newly resolved third-party graph.

## The bugs and risks that mattered

### A naïve chunk-by-chunk replacement leaks boundary prefixes

If a secret begins at the end of one pipe read and ends in the next, replacing
within each read independently prints its prefix before the full match exists.
That is a credential disclosure. The retained suffix is the minimal state
needed to make the streaming transformation safe across arbitrary read splits.

### Two redaction inventories would reintroduce a known stale-field leak

Maintaining a shield-specific list of fields would let new vault values be
redacted in list output but escape through a child command or be absent from a
scan. Reusing secret_values makes outward tooling follow the same fail-closed
policy as ordinary CLI redaction.

### An exposure report is sensitive even when it hides values

File paths and exact line numbers reveal where deployable credentials live.
Printing findings to stdout would leak into terminal scrollback, pipelines or
shell capture. File-only output with secret-file permissions is a deliberate
security boundary, not a presentation preference.

### Text-only shielding must reject binary rather than corrupt it

The wrapper is not a binary-safe rewrite engine. Attempting partial text
replacement in arbitrary binary data can expose the secret or silently corrupt
the output. Refusal plus pipe draining is safer and gives the caller an
explicit failure to handle.

## Decisions made

- **Exact vault-value matching, not a generic secret scanner.** The commands
  answer whether a value EnvVault knows escaped; they do not attempt entropy or
  regex-based credential discovery.
- **One Aho-Corasick matcher per invocation.** It is linear over the stream
  rather than repeatedly applying one replacement per secret.
- **Leftmost-longest matching.** A longer secret wins where private values
  overlap.
- **Explicit opt-in commands.** EnvVault never silently intercepts shell output
  or rewrites filesystem content.
- **No stdout exposure report, even under --reveal.** Reveal changes listing
  policy, not the risk posture of a map to sensitive deployment files.
- **No directory-tree symlink traversal.** A scan rooted at a supplied
  directory does not silently escape through links it encounters. A directly
  supplied file symlink is currently an explicit request for its target.

## What Worked

- The already-locked direct dependency set contained Aho-Corasick and WalkDir;
  adding them as direct CLI dependencies required no new lock resolution.
- One private-value extractor prevents `envv list` and the outward tools from
  drifting on new fields.
- Focused tests cover the streaming boundary, overlap choice, file-read
  boundary, binary refusal, report redaction and a real CLI round trip.

## What Didn't Work

- The aggregate `cargo test -p envv-cli` tool invocation outlived the terminal
  capture window, so do not quote it as a captured complete run. The focused
  command below is the recorded gate for this phase.
- Git history, Docker-image layers and node-host scans remain explicitly later
  work. This phase walks only the supplied local file or directory.

## Verification

- `cargo fmt --all -- --check` - passed.
- `git diff --check` - passed.
- `cargo test -p envv-cli shield` - six matcher tests plus the real CLI test
  passed. The CLI test proves stdout/stderr redaction, exit-code preservation,
  `--out` refusal, no plaintext report, exact line number and mode 0600.
- `cargo clippy -p envv-cli --all-targets -- -D warnings` - passed.

## Next Steps

1. Phase 27 is the UI lab. Its initial full run is already scaffolded; use the
   Phase 24 findings as the known baseline and run it against a real browser
   viewport matrix.
2. When committing, keep Phase 24, 25 and 26 changes separated deliberately.
   This shared worktree was already broadly dirty before Phase 25, so a broad
   `git add` would mix phases.

## Notes for whoever picks this up

- Current Phase 26 is the old plan's Phase 29. Phase 25 is documented in
  `the session records/38-2026-10-08-phase-25-route-coverage-and-permission-expres.md`; Phase 24 baseline is `37-2026-10-08-phase-24-and-24-1-24-5-everything-that-can-be.md`.
- Binary means invalid UTF-8 or NUL-containing output. Valid text is shielded
  byte-for-byte regardless of whether a caller later treats it as structured.
- Everything remains uncommitted. Do not reset, broadly stage, commit or push
  this tree.
