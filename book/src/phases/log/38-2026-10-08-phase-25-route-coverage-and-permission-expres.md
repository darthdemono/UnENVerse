# Phase log 38: 2026-10-08 - Phase 25, route coverage and permission-expression parity

## Goal

Complete current Phase 25: replace shallow server-route coverage with real-router
tests and make the TypeScript permission-expression evaluator mechanically agree
with `vault-core`.

## Current Progress

Phase 25 is complete, but remains uncommitted with the rest of the Phase 24
worktree.

- `envv-server/src/lib.rs` now exercises the actual `build_router` path across
  more than thirty requests: public metadata, owner and sub-user auth, locked
  vault responses, read and write RBAC, token lifecycle, user/class lifecycle
  and revocation.
- `tests/fixtures/parity/permex.json` is the shared truth table for permission
  expressions. `vault-core/src/permex.rs` and `tests/permex.test.ts` both read
  it, so a semantic disagreement fails either suite.
- The TypeScript evaluator gained the core's composition primitives
  (`combine`, `anyOf`, `requireAll`). The permission editor preview now shows
  the effective class-plus-user scope, including strict-write narrowing.
- The server's user/class JSON now returns `class_id` and `strict_write`.
  The UI had fields for those values but could not receive them, so its former
  preview could not model the server's effective access rule.

## Detailed changelog

### Router test harness and public/private boundary

envv-server/src/lib.rs now has a request helper that starts from the assembled
build_router result rather than directly calling handlers. It uses Tower’s
oneshot service path and attaches the same ConnectInfo socket address extension
the production service adds. That means the tests cover route selection, method
dispatch, layers, authorization-header extraction, state transitions and
handler behavior in one request path without needing a TCP port.

The startup-only build_router test is deliberate. Axum detects duplicate route
paths when a router is assembled, so a binary can compile successfully and still
panic before listening. The test keeps that failure in the normal server suite.

The core HTTP sequence establishes this contract:

- status and health remain publicly reachable; health intentionally omits
  secrets_stored while stats exposes it.
- the OpenAPI document remains public.
- anonymous callers receive 401 for vault, filtered-entry, expiry, audit and
  ping paths.
- POST on the read-only vault route returns 405.
- an owner vault response includes an ETag, provider filtering finds the seeded
  entry, and an unmatched filter is an empty result rather than an error.
- deleting the owner unlock/session state is global: the earlier bearer token
  can no longer read the vault, and the public status endpoint reports locked.

The test suite also drives the calendar-feed and UID-registry surfaces through
the assembled router. This preserves the Phase 24.3/24.4 contracts around
metadata-only feeds, revocation, locked/authenticated responses, registry
opt-in, duplicate detection, rate limiting and request-id error envelopes.

### User/class lifecycle coverage

The RBAC router test creates a normal user, authenticates it, and proves that a
valid low-privilege token does not obtain management access. It checks that the
user receives 403 from users, classes and audit endpoints and receives no vault
entries before a permission is assigned.

The owner then creates an Infra readers class, assigns its type:api_key read
rule, assigns the class to the user, and adds the user’s project:* read rule.
The route test reads those expressions back over their GET endpoints and proves
the still-valid user session sees the intended entry only after that effective
intersection exists. The rest of the
same flow covers token creation/list/revocation, user rename, password change,
class update, user deletion and class deletion. That is intentionally a
lifecycle rather than a collection of mocks: the value is in proving the
routes and their authorization gates compose.

### One shared permission-expression contract

tests/fixtures/parity/permex.json is now the cross-language truth table. It
defines Universal, Acme and Acme/Web projects, three representative entries,
and four composition cases:

1. a class restriction intersects the individual rule;
2. strict write narrows a top-level OR;
3. strict write preserves an authored nested OR under an AND;
4. a named project does not match an entry that belongs only to Universal.

vault-core/src/permex.rs embeds and evaluates this fixture with the enforcing
parser/evaluator. tests/permex.test.ts reads the same file and evaluates the
client mirror. Both parse optional class and individual source, combine them,
apply strict-write conversion when requested, and compare every entry against
the fixture’s expected Boolean vector.

This fixture deliberately covers composition, not every lexical rule. Existing
Rust and TypeScript unit tests still own precedence, aliases, quoted strings,
wildcards, malformed expressions and individual predicate behavior.

### Client composition and truthful preview

src/ts/permex.ts gained the same three composition operations that core uses:

- combine uses class AND individual when both are present, returns the one rule
  when only one is present, and returns no grant when neither exists.
- anyOf models the server’s read OR write rule because write permission implies
  read visibility.
- requireAll applies strict-write narrowing without changing the expression a
  user wrote.

src/ts/perm-editor.ts now applies class expressions to each individual read and
write preview before counting entries. Its separate effective-read summary
combines effective read OR effective write. The class preview has its own
strict-write context, so the UI does not confuse an inherited ceiling with an
individual grant.

The data needed for that calculation already existed in core but was absent
from the serialized user/class responses. vault-core/src/users.rs now exposes
class_id and strict_write for a user, and strict_write for a class; matching
optional fields were added to src/ts/types.ts. When opening a user,
src/ts/users.ts loads the assigned class’s expressions and passes them to the
editor. Strict write is true when either the user or assigned class enables it.
If the supplementary class-expression lookup fails, the panel stays usable but
does not fabricate a class rule.

## The bugs that mattered

### Router coverage had skipped the code that joins routes to enforcement

Direct handler tests can all pass while a duplicate route prevents startup, a
wrong HTTP method reaches nothing, middleware changes a response, or a real
authorization header is extracted differently. The server did not need a new
HTTP implementation; it needed tests that exercised the implementation it
already deploys. The in-process router tests are that safety net.

### The UI could make an administrator believe a rule was broader than the server allowed

This was not an authorization bypass. vault-core remained authoritative and
enforced class AND individual expressions plus strict-write behavior. The
administrative preview, however, evaluated only the rule typed in the form
because class_id and strict_write did not reach it. An owner could see “matches
N entries” and not see the inherited class ceiling or write narrowing. The
serialized-policy gap, not the parser, was the root cause.

### Parallel language tests did not prove their shared semantics

Both implementations had substantial unit coverage, but no one test vector
would fail if their class composition or strict-write behavior diverged. The
fixture is intentionally small, reviewable and shared so a semantic mismatch is
now visible in either suite.

## Decisions made

- **The real router is tested in process.** That covers Axum routing and layers
  without introducing port allocation or network timing into every test.
- **A class is a ceiling.** Class and individual expressions intersect, so an
  individual rule cannot undo a class exclusion.
- **Write implies read; strict write affects writes only.** The preview mirrors
  both rules instead of applying a blanket restriction to read visibility.
- **The browser remains advisory.** Server-side Rust re-evaluates all
  expressions for enforcement; no preview result is trusted for access.
- **The parity fixture is semantic rather than a second grammar.** Existing
  parser tests remain the source for precise syntax diagnostics.

## What Worked

- Reusing the existing router test harness covered middleware, auth extraction,
  lock state and route wiring together instead of adding handler-only tests.
- A small JSON fixture is a more durable parity contract than parallel hand-made
  assertions in Rust and TypeScript.
- The server's effective permission semantics were already concentrated enough
  to mirror in the UI preview without adding a second rule language.

## What Didn't Work

- `npm run check` is not recorded as passing: its watchdog stopped the test
  runner after 30 seconds without a test failure. Run the individual suites or
  repair the watchdog before treating that aggregate command as a gate.
- There is no safe noninteractive Phase-25-only commit command while the
  worktree also contains the large, pre-existing uncommitted Phase 24 changes.
  Do not stage the whole tree to make one.

## Verification

- `cargo test -p vault-core` - 269 passed.
- `cargo test -p envv-server` - 32 passed.
- `npm test -- --run tests/perm-editor.test.ts tests/permex.test.ts tests/users.test.ts` - 74 passed.
- `npm run typecheck`, `cargo fmt --check`, `npm run format:check`, and
  `git diff --check` - passed.
- An earlier `cargo test --workspace` passed before the final UI-context
  correction; the focused Rust suites above were rerun after it.

## Next Steps

1. Current Phase 26 is independent and in progress: implement the shared
   Aho-Corasick redaction engine behind explicit `envv shield -- <command>` and
   `envv scan --exposed <path> --out <file>`.
2. Keep `shield` explicit, preserve child exit status, withhold the trailing
   `max_secret_len - 1` bytes across reads, and never emit a partial secret.
3. Keep exposure reports out of stdout. They name sensitive locations and must
   be written with `--out` using the existing secret-file policy.

## Notes for whoever picks this up

- Phase 25 is the current-number route-test/`permex` phase. Older handoffs call
  that work Phase 26; current Phase 26 is the outward redaction engine.
- The shared tree is deliberately dirty. Preserve all Phase 24 and Phase 25
  changes; do not reset, stage broadly, commit or push.
