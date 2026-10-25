# Phase log 67: roadmap W1/R22 (project phase 44), the UnENVerse product website

Date 2026-10-10. Start commit `942a443` (0.42.4); tree had only the maintainer's `.gitignore` change. Nothing committed, staged or pushed; nothing deployed. Version unchanged. Mapping R22 to 44 is in STATUS.md and PHASES.md (next free number is 45).

## Goal

Replace the API-reference index that `docs.yml` wrote inline at the Pages root with a product site (landing, download, quickstart, security, guides) that makes no claim without a shipped release behind it, keeps the guide and API references under it, and is checked by tests, a browser pass and a link check. Mid-session the maintainer added: enrich the README screenshots, because the demo cards were bare compared with the real vault on the Docker server.

## Re-verified research claims (report 03 vs the code and the network)

- Root page was an inline heredoc in `docs.yml`: true (now removed; the old text is recoverable from git, `git show 942a443:.github/workflows/docs.yml`).
- Three sites in scope: only A (UnENVerse) was built; B and C live in other repositories and need the maintainer's authorisation, so they were not touched (that is W2/R23).
- **Live audit still impossible from this machine:** `unenverse.darthdemono.com` does not resolve (`curl: (6) Couldn't resolve host`). `https://darthdemono.github.io/UnENVerse/` answers `301` to `http://unenverse.darthdemono.com/`. `gh api repos/darthdemono/UnENVerse/pages` says `cname: unenverse.darthdemono.com, https_enforced: false`. So the live "before" screenshots could not be taken; "before" is the old heredoc rendered locally (`shots/site/before/`).
- Catalogue URL: `https://darthdemono.github.io/EnvVault/catalogue/catalogue.json` returns 404 (verified). Left for R21: whether the catalogue exists at the new location depends on the signing secret and a live deployment I cannot observe, so I did not guess a replacement.
- README "card-expanded.png shows no expansion": true, and the same defect lived in `tools/demo/shots_demo.py` (it clicked the card body; expansion is the `.card-chevron` button). Fixed.
- Report said the release body carries the cosign recipe: true, reused on the download page. Release 0.42.4 has Linux (AppImage, deb, rpm) and Windows (NSIS) desktop files, CLI archives for both, a `.vsix`, `SHA256SUMS` and a `.sig`/`.pem` for every asset. No macOS build, no winget or Scoop (so the site does not mention them).
- "No telemetry": verified by grep (no analytics endpoints in src, src-tauri, vault-core, unv-cli, unv-server; `telemetry.rs` is local stderr logging). Worded on the site as "no account and no analytics".

## Findings that were not in the plan

1. **R01 regression, caught here:** `tools/viewer` and `tools/demo` failed to link (`undefined symbol: sqlite3_prepare_v3`) because the viewer image is Debian 12 whose `libsqlcipher0` is 3.4.1 (SQLite 3.15) and rusqlite 0.40 needs 3.20+. Ubuntu runners (22.04/24.04) and Fedora are new enough, so CI was not hit; developer containers were. Fix: both `run.sh` scripts build with `--features vault-core/bundled`; documented in README (From source), `.claude/rules/desktop-platform.md` and `.claude/rules/ci-release.md`.
2. **The published guide had four blank pages.** `book/src/SUMMARY.md` linked `adr/0011-unv-...`, `0098-unv-...`, `0117-unv-...`, `0118-unv-...` but the tracked files are still named `...-envv-...`. mdBook does not fail on a missing chapter: it creates an empty file and renders a blank page (it created four untracked stubs in my tree at 19:36; I deleted them). SUMMARY now points at the real files. Test `the guide index links only files that exist` guards it (verified by putting an old link back: it fails).
3. **GitHub private vulnerability reporting is OFF** (`gh api repos/darthdemono/UnENVerse/private-vulnerability-reporting` gives `enabled: false`) and the profile has no public email, yet `SECURITY.md` promises both routes. The security page therefore names the Security tab only "when it is offered there". Needs the maintainer's switch (see Next steps).
4. `tools/demo` could not show two history snapshots or a real diff: a Compose chunk keeps `${VAR}` (invariant 5 exception), so rotating the Postgres entry changed nothing. The seed now references a `Wireguard` entry from the Home VPN interface, snapshots, rotates it and snapshots again; the history screenshot shows `-PrivateKey = sha256:f486fc5d2198 / +PrivateKey = sha256:7ac71a90ef1b`.

## Changes, file by file

- `website/` (new, hand-written, no framework): `index.html`, `download/`, `quickstart/`, `security/`, `guides/`, `reference/` (landing for the generated references), redirect stubs `book/`, `ts/`, `rust/` (noindex, meta refresh and canonical), `partials/{header,footer}.html`, `styles.css` ("Vault ledger" tokens, automatic dark theme, visible focus, skip link, reduced motion), `assets/` (favicon.svg, og.png 1200x630, self-hosted Syne 600/700 and JetBrains Mono 400/500 with the OFL text, `screens/*.webp` plus `manifest.json` of dimensions), `robots.txt`. Placeholders `{{VERSION}} {{RELEASE_DATE}} {{RELEASE_URL}} {{SITE_URL}} {{JSONLD}} {{DESKTOP_TABLE}} {{CLI_TABLE}} {{HEADER}} {{FOOTER}} {{CUR_x}} {{W_x}} {{H_x}}`.
- `scripts/build-site.mjs`: copies `website/` to the output, stamps version, date and the download tables from the **latest published release** (GitHub API, or `--release r.json --sums SHA256SUMS` offline), fills partials, marks the current nav item, writes `sitemap.xml` (skips noindex stubs), throws on any unfilled placeholder, fails loudly when the release cannot be read. Pure helpers exported and tested.
- `scripts/check-site.mjs`: serves the assembled site, Playwright at 1440 and 390: no horizontal scroll, exactly one `h1` and one `main#main`, title, description, https canonical, `lang`, every link/button named, every image with alt/width/height, first Tab stop is the skip link with a visible ring, next stop has a ring, no console error, no HTTP 400+.
- `scripts/site-shots.sh`: raw PNGs to 1600px WebP and the dimension manifest (ImageMagick).
- `tests/site-build.test.ts` (15 tests): sums parsing, table (asset filtering, escaping, sizes), JSON-LD (true fields only, cannot close the script tag), sitemap, full assemble (placeholders, nav marker, stubs excluded), failure on an unfilled placeholder, WCAG contrast of the tokens in light and dark (ink 7:1, muted/links/buttons/warn/code 4.5:1, focus ring 3:1), guide index links exist.
- `.github/workflows/docs.yml`: runs `build-site.mjs` first (it wipes `site/`), then `mdbook build book --dest-dir ../site/docs`, copies Doxygen to `site/reference/ts` and rustdoc to `site/reference/rust`, requires `reference/rust/vault_core/index.html`; keeps the catalogue signing step; adds "Browser checks" (`npx playwright install --with-deps chromium`, `check-site.mjs`) and an offline lychee check pinned by SHA (`lycheeverse/lychee-action` v2.9.0 `e7477775783e…`, `--offline --include-fragments --index-files index.html --exclude-path '/(docs|reference/ts|reference/rust)/'`). Header comment lists the new layout.
- `package.json`: `site:build`, `site:check`. `.prettierignore`: `website/`.
- `README.md`: nav line (Website, Download, Quickstart, Security, Docs), the Tour section restored with the new screenshots and a pointer to the site, a sentence on the minimum system SQLCipher. `Screenshots/*.png`: 16 enriched images replaced (and `card-expanded.png` now really is expanded).
- `tools/demo/seed.py`: every card now carries description, details, account, environment, tags, categories, API URL, version, scopes, rate limits, callback URLs, env prefixes, rotation and expiry, shaped after which fields the real vault fills (I counted populated field names and counts on the maintainer's server, read-only, redacted, no values or names copied); TMDB v3/v4 bundle; `Wireguard` entry referenced by the VPN interface; two history snapshots with a rotation between. `tools/demo/shots_demo.py`: expands the first card through its chevron and asserts it, drives the history pane (Home VPN, wireguard, Diff newest two) and prints the pane's own message on failure, blurs focus and parks the pointer on blank header space before each shot (an "Edit" tooltip was baked into two screenshots).
- `tools/demo/run.sh`, `tools/viewer/run.sh`: bundled SQLCipher (finding 1).
- `book/src/SUMMARY.md`: four ADR links (finding 2). `.claude/rules/ci-release.md`, `desktop-platform.md`: conventions.

## Verification (what ran)

- `npm run check`: eslint (max-warnings 0), tsc, prettier, Vitest 89 files / 1515 tests (was 1501; +14 site tests, then +1 guide-index test = 15 in the new file).
- Full local assemble exactly as the workflow does it (build-site, mdBook 0.4.52, Doxygen via `npm run docs`, `cargo doc --workspace --no-deps --locked`): `check-site` "6 pages x 2 viewports" ok; lychee offline on the product pages 148 links, 0 errors. Controls: a copy with a second `h1`, a 2000px div, an unnamed link, an image without alt and `outline: none` fails the checker with 12 findings; a broken link and a broken fragment fail lychee; a muted colour change fails the contrast test; an old SUMMARY link fails the guide-index test.
- External links, once by hand (`lychee` online): everything resolves except GitHub `blob/main/...` pages answering `503` under rate limiting; `SECURITY.md` and `CHANGELOG.md` confirmed on `main` through the contents API. External links are deliberately not gated in CI.
- Quickstart executed verbatim from the page (commands extracted from the HTML by script) in a clean `debian:bookworm-slim` container with the real `unv-0.42.4-linux-x86_64.tar.gz` (checksum verified against the release `SHA256SUMS` first): version, init, import, project chunk, env (fingerprints), `env --out`, `SAME`, backup export, restore into a second vault under a different master password, `doctor` all ok (audit chain intact across 2 rows).
- Screenshots: `tools/demo/run.sh` seven runs (build in the viewer container is cached after the first); failures along the way and their causes are above. `shots/site/before/` (old index, 1440 and 390) and `shots/site/after/` (every page, both widths) exist locally (gitignored).
- Landing sentences checked against shipped code (`unv --help` has `check`, `history`, `blast-radius`, `node`, `import-vault`, `cxf`, `totp`, `bundle`, `backup`, `doctor`, `audit`; README sections for each; release 0.42.4 contains all of them): imports of `.env`, Bitwarden, 1Password, Proton, Nextcloud `config.php`, Aegis/2FAS/andOTP/Ente/Google Authenticator (README says implemented from the published formats; the site says "import ... exports", never "verified"); eleven project types and `${refs}` rendering; config checker; per-save snapshots with masked secrets; hash-chained audit log; blast radius with only live values; node agents with approval bound to the exact bytes and device-signed approval; encrypted `.vaultbak`; Argon2id 64 MiB x3; redaction fail-closed; keyless Sigstore signatures; owner-only files on Linux, "not enforceable" on Windows. Claims deliberately absent: macOS, winget/Scoop, scheduled backups, KeePass, SOPS, Home Assistant, anything from R05 onwards.

## Decisions (with rejected alternatives)

- Plain HTML plus one CSS file and a ~200-line Node script, no framework or build tool (non-goal in the plan); partials by token replacement rather than a templating library.
- Version and downloads from the latest release, not `main`: a site that advertises an unpublished build is the failure to avoid. Rejected a JSON written by `build.yml` (couples two workflows).
- Self-hosted fonts (OFL) instead of Google Fonts: no third-party request from a security product's site.
- Link check gate is offline and limited to the product pages: the generated guide and references carry thousands of links (1,378 errors in a full run, mostly Doxygen's README main page with relative image paths and rustdoc source anchors) that this check cannot judge; external links are not gated so a third party's outage cannot block a deploy.
- `https://` canonical URLs and README links even though the Pages setting does not enforce HTTPS: GitHub provisions the certificate for a verified custom domain regardless of the enforcement checkbox, and the repo homepage field is the only `http://` reference. Unverifiable from here (DNS), see Next steps.
- Left `Screenshots/` file names unchanged so every existing README and Doxygen reference keeps working.

## What did not run, and why

- Live site "before" and any live HTTPS, redirect, Lighthouse or real-device check: the domain does not resolve from this network.
- The new workflow steps on GitHub's runners (they need a push). Every command in them was run locally; `npx playwright install --with-deps chromium` was not (Chromium was already installed here).
- Windows rendering of the site (none needed: static files).

## Next steps

Only the maintainer can do these: (1) Settings, Code security, enable **Private vulnerability reporting** (otherwise the policy's reporting route does not exist) before pushing the site; (2) Settings, Pages, confirm the custom domain verifies and tick **Enforce HTTPS** (DNS is not resolvable from my network); (3) push to deploy; the first run should be watched because the new browser and link steps have not run on a runner. Then R21 for the catalogue URL, W2/R23 for the sibling sites (other repositories, needs authorisation), W3/R24 for generated CLI reference and migration tables.

## Traps for the next agent

- `UNV_SERVER_URL` is set in the maintainer's shell and points at the real server on `localhost:8743`; always `env -u UNV_SERVER_URL` (or set your own) before any `unv` command. I used it read-only once, on request, to count populated metadata field names.
- mdBook creates empty chapter files for missing links instead of failing; check `git status book` after a build.
- `lychee --include-fragments` needs `--index-files index.html` or a `/dir/#frag` link looks like a missing fragment.
- `scripts/build-site.mjs` deletes its output folder; never point `--out` at anything you care about.
- The demo seed's `Wireguard` entry name is referenced as `${Wireguard}` by the VPN chunk; renaming one breaks the history screenshot silently (the scenario prints the pane's message).
