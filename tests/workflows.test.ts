// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
// Static guards for the CI/release workflows (roadmap R02). GitHub Actions cannot
// run on a laptop, so what can be proven here is the shape that makes the
// guarantees true: a release needs the CI gate, Docker tags move only after the
// smoke test, every new validator carries a control that must fail, and nothing
// third-party is referenced by a mutable tag.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { parse as parseYaml } from 'yaml';
import { describe, expect, it } from 'vitest';

const root = path.resolve(__dirname, '..');
type Step = {
  name?: string;
  uses?: string;
  run?: string;
  with?: Record<string, unknown>;
  if?: string;
};
type Job = {
  needs?: string | string[];
  uses?: string;
  steps?: Step[];
  if?: string;
  with?: Record<string, unknown>;
  strategy?: { matrix: { include: { label?: string; arch?: string }[] } };
};
type Workflow = {
  on: Record<string, unknown>;
  jobs: Record<string, Job>;
  concurrency?: Record<string, unknown>;
};

function load(name: string): Workflow {
  return parseYaml(fs.readFileSync(path.join(root, '.github/workflows', name), 'utf8')) as Workflow;
}
const needs = (j: Job): string[] => (Array.isArray(j.needs) ? j.needs : j.needs ? [j.needs] : []);
/** Every job a job depends on, directly or not. */
function ancestors(w: Workflow, id: string, seen = new Set<string>()): Set<string> {
  for (const n of needs(w.jobs[id])) {
    if (!seen.has(n)) {
      seen.add(n);
      ancestors(w, n, seen);
    }
  }
  return seen;
}

describe('the release gate', () => {
  const build = load('build.yml');
  const ci = load('ci.yml');

  it('calls ci.yml as a job of the same run', () => {
    expect(build.jobs.ci.uses).toBe('./.github/workflows/ci.yml');
    expect(build.jobs.ci.with).toEqual({ gate: true });
    expect(Object.keys(ci.on)).toContain('workflow_call');
  });

  it('is needed, directly or not, by everything that publishes', () => {
    for (const id of ['docker', 'release']) {
      expect(ancestors(build, id).has('ci'), `${id} must depend on ci`).toBe(true);
    }
  });

  it('lets the packaging jobs run beside the gate, because they publish nothing', () => {
    // The bug this pins: desktop and headless waited for the whole gate, so a
    // release took the CI time plus the slowest build. They only upload
    // artefacts; `release` still needs `ci`, so a red commit cannot ship.
    for (const id of ['desktop', 'headless']) {
      expect(ancestors(build, id).has('ci'), `${id} should not wait for ci`).toBe(false);
    }
  });

  it('release names the gate and every build job on its own needs line', () => {
    expect(needs(build.jobs.release).sort()).toEqual([
      'ci',
      'desktop',
      'docker-manifest',
      'headless',
      'meta',
    ]);
  });

  it('keeps the gate run apart from the push-triggered CI run, and never cancels it', () => {
    const c = ci.concurrency as { group: string; 'cancel-in-progress': string };
    expect(c.group).toContain('inputs.gate');
    expect(c['cancel-in-progress']).toContain('!inputs.gate');
  });

  it('has no job called by the gate that asks for more than read access', () => {
    const text = fs.readFileSync(path.join(root, '.github/workflows/ci.yml'), 'utf8');
    expect(text).not.toMatch(/(contents|packages|id-token|pages): write/);
  });
});

describe('release targets', () => {
  const targets = [
    'linux-x86_64',
    'linux-aarch64',
    'linux-x86_64-musl',
    'linux-aarch64-musl',
    'windows-x86_64',
    'windows-aarch64',
  ];
  it('builds every CLI target the update manifest guard requires, named as std::env::consts says', () => {
    const m = (
      load('build.yml').jobs.headless.strategy as { matrix: { include: { label: string }[] } }
    ).matrix;
    expect(m.include.map((i) => i.label).sort()).toEqual([...targets].sort());
    const guard = fs.readFileSync(path.join(root, 'scripts/check-update-manifest.sh'), 'utf8');
    for (const t of targets) expect(guard).toContain(t);
  });
});

describe('desktop bundles', () => {
  it('build natively for every architecture and the release downloads each one', () => {
    const build = load('build.yml');
    const labels = build.jobs.desktop.strategy!.matrix.include.map((i) => i.label).sort();
    expect(labels).toEqual(['linux-aarch64', 'linux-x86_64', 'windows-aarch64', 'windows-x86_64']);
    const dl = (build.jobs.release.steps as Step[]).map((s) => s.run ?? '').join('\n');
    for (const l of labels) expect(dl).toContain(`desktop-${l}`);
  });
});

describe('the Docker image', () => {
  const steps = load('build.yml').jobs.docker.steps as Step[];
  // Match command lines, not prose: a comment that mentions `docker push` is not a push.
  const at = (re: RegExp): number => steps.findIndex((s) => re.test(s.run ?? ''));

  it('is built without pushing, then smoke-tested, then pushed', () => {
    const build = steps.find((s) => s.uses?.startsWith('docker/build-push-action'));
    expect(build?.with?.push).toBe(false);
    expect(build?.with?.load).toBe(true);
    const smoke = at(/^\s*docker run --rm --entrypoint unv-server/m);
    const push = at(/^\s*docker push /m);
    expect(smoke).toBeGreaterThan(steps.indexOf(build as Step));
    expect(push).toBeGreaterThan(smoke);
    expect(steps[push].if).toContain("publish == 'true'");
  });

  it('pushes only the per-architecture tag; the release tags come from the manifest job', () => {
    const run = steps[at(/^\s*docker push /m)].run as string;
    expect(run).toContain('outputs.version');
    expect(run).toContain('matrix.arch');
    expect(run).not.toContain('latest');
  });

  it('builds amd64 and arm64 on native runners and joins them into version, minor and latest', () => {
    const build = load('build.yml');
    const m = build.jobs.docker.strategy!.matrix;
    expect(m.include.map((i) => i.arch).sort()).toEqual(['amd64', 'arm64']);
    const join = build.jobs['docker-manifest'];
    expect(needs(join)).toContain('docker');
    expect(join.if).toContain("publish == 'true'");
    const run = (join.steps as Step[]).map((s) => s.run ?? '').join('\n');
    expect(run).toContain('imagetools create');
    for (const t of ['"$VERSION"', '"$MINOR"', 'latest']) expect(run).toContain(t);
    expect(run).toContain('-amd64');
    expect(run).toContain('-arm64');
  });
});

describe('documentation-only changes do not run the build', () => {
  const ci = load('ci.yml');
  const build = load('build.yml');
  const docs = load('docs.yml');
  type Push = { 'paths-ignore'?: string[]; branches?: string[]; tags?: string[] };
  const push = (w: Workflow) => w.on.push as Push;

  it('skips the heavy jobs through `changes`, not through a trigger filter on ci.yml', () => {
    const on = ci.on as { push: Push; pull_request: Record<string, unknown> | null };
    expect(on.push['paths-ignore']).toBeUndefined();
    expect(JSON.stringify(on.pull_request ?? {})).not.toMatch(/paths/);
    expect(needs(ci.jobs.test)).toContain('changes');
    expect(needs(ci.jobs.lint)).toContain('changes');
    expect(needs(ci.jobs.frontend)).toContain('changes');
    expect(ci.jobs.test.if).toContain('outputs.rust');
    expect(ci.jobs.frontend.if).toContain('outputs.frontend');
  });

  it('always reports one required result that fails on a failed job and tolerates a skipped one', () => {
    const ok = ci.jobs['ci-ok'] as Job & { name?: string };
    expect(ok.name).toBe('CI result');
    expect(ok.if).toContain('always()');
    expect(needs(ok).sort()).toEqual(['changes', 'frontend', 'lint', 'test']);
    const run = (ok.steps as Step[]).map((s) => s.run ?? '').join('\n');
    // The control: only these two outcomes pass. A cancelled or failed job must not.
    expect(run).toMatch(/success\|skipped\) ;;/);
    expect(run).toMatch(/\*\) .*exit 1/);
  });

  it('runs everything for the release gate, manual runs and the weekly schedule', () => {
    const text = fs.readFileSync(path.join(root, '.github/workflows/ci.yml'), 'utf8');
    expect(text).toMatch(/if \[ "\$GATE" = "true" \]; then everything/);
    expect(Object.keys(ci.on)).toContain('schedule');
  });

  it('keeps version files and source off the release workflow ignore list', () => {
    const ignored = push(build)['paths-ignore'] ?? [];
    expect(ignored.length).toBeGreaterThan(5);
    for (const must of ['README.md', 'website/**', 'CHANGELOG.md']) expect(ignored).toContain(must);
    // A version bump must still start the run, so none of these may ever be ignored.
    for (const p of [
      'package.json',
      'Cargo.toml',
      'Cargo.lock',
      'src-tauri/tauri.conf.json',
      'vault-core/Cargo.toml',
    ]) {
      expect(
        ignored.some((g) => g === p || (g.endsWith('/**') && p.startsWith(g.slice(0, -2)))),
        p,
      ).toBe(false);
    }
    expect(push(build).tags).toEqual(['v*']);
  });

  it('still deploys the site when website/ changes', () => {
    const ignored = push(docs)['paths-ignore'] ?? [];
    expect(ignored.some((g) => g.startsWith('website'))).toBe(false);
    expect(ignored).toContain('README.md');
  });
});

describe('Windows no longer pays for the same work twice', () => {
  const ci = load('ci.yml');
  const text = fs.readFileSync(path.join(root, '.github/workflows/ci.yml'), 'utf8');

  it('splits test from lint, so they run side by side', () => {
    expect(Object.keys(ci.jobs)).toEqual(expect.arrayContaining(['test', 'lint', 'frontend']));
    expect(needs(ci.jobs.lint)).not.toContain('test');
  });

  it('has no separate cargo check of the workspace (clippy --all-targets covers it)', () => {
    expect(text).not.toMatch(/cargo check --workspace/);
    expect(text).toMatch(/cargo clippy --workspace --locked --all-targets/);
  });

  it('installs cargo-deny as a pinned prebuilt binary, not from source', () => {
    expect(text).not.toMatch(/cargo install cargo-deny/);
    expect(text).toMatch(/cargo-deny@0\.\d+\.\d+/);
  });
});

describe('exporter validators', () => {
  const ex = load('exporters.yml');
  const text = fs.readFileSync(path.join(root, '.github/workflows/exporters.yml'), 'utf8');

  it('has wireguard, nginx and compose jobs', () => {
    for (const id of ['wireguard', 'nginx', 'compose']) expect(ex.jobs[id], id).toBeDefined();
  });

  it('proves each new validator can fail, with a control that must be refused', () => {
    for (const id of ['wireguard', 'nginx', 'compose']) {
      const body = JSON.stringify(ex.jobs[id].steps);
      expect(body, `${id} needs a control`).toMatch(/proves nothing/);
    }
  });

  it('triggers on the same paths for push and pull request, and every non-glob path exists', () => {
    const on = ex.on as { push: { paths: string[] }; pull_request: { paths: string[] } };
    expect(on.pull_request.paths).toEqual(on.push.paths);
    for (const p of on.push.paths) {
      if (p.includes('*')) {
        expect(fs.existsSync(path.join(root, p.replace(/\/?\*\*?$/, ''))), p).toBe(true);
      } else {
        expect(fs.existsSync(path.join(root, p)), p).toBe(true);
      }
    }
    for (const must of [
      'unv-app/src/exporters.rs',
      'unv-app/src/chunks.rs',
      'unv-app/src/refs.rs',
      'vault-core/src/config_check.rs',
      'src/ts/chunks/**',
    ]) {
      expect(on.push.paths).toContain(must);
    }
    expect(text).toContain('proves nothing');
  });
});

describe('pinning', () => {
  const files = [
    ...fs.readdirSync(path.join(root, '.github/workflows')).map((f) => `.github/workflows/${f}`),
    '.github/actions/rust-setup/action.yml',
  ];

  it('references every third-party action by a full commit SHA with a version comment', () => {
    const loose: string[] = [];
    for (const f of files) {
      for (const line of fs.readFileSync(path.join(root, f), 'utf8').split('\n')) {
        const m = /^\s*-?\s*uses:\s*(\S+)(.*)$/.exec(line);
        if (!m || m[1].startsWith('./')) continue;
        if (!/@[0-9a-f]{40}$/.test(m[1]) || !/#\s*\S+/.test(m[2]))
          loose.push(`${f}: ${line.trim()}`);
      }
    }
    expect(loose).toEqual([]);
  });

  it('uses no action that still runs on Node 20 (checked at the time of the bump)', () => {
    const text = files.map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n');
    // The pre-bump releases of the actions that moved to Node 24.
    for (const old of [
      'actions/checkout@11bd71901',
      'actions/setup-node@49933ea52',
      'actions/upload-artifact@ea165f8d6',
      'actions/download-artifact@d3f86a106',
      'docker/build-push-action@263435318',
      'Swatinem/rust-cache@bc2d2e71b',
    ]) {
      expect(text, old).not.toContain(old);
    }
  });
});

describe('schedules and Dependabot', () => {
  it('rebuilds the docs weekly, not nightly', () => {
    const docs = load('docs.yml');
    const cron = (docs.on.schedule as { cron: string }[])[0].cron;
    expect(cron.split(' ').slice(2, 5)).toEqual(['*', '*', '1']);
  });

  const dep = parseYaml(fs.readFileSync(path.join(root, '.github/dependabot.yml'), 'utf8')) as {
    updates: {
      'package-ecosystem': string;
      groups?: Record<string, unknown>;
      ignore?: { 'dependency-name': string; 'update-types'?: string[] }[];
      directory?: string;
      directories?: string[];
    }[];
  };
  const eco = (n: string) => dep.updates.find((u) => u['package-ecosystem'] === n);

  it('groups the families that must move together', () => {
    expect(Object.keys(eco('npm')?.groups ?? {})).toEqual(
      expect.arrayContaining(['vitest', 'tauri', 'lint']),
    );
    expect(Object.keys(eco('cargo')?.groups ?? {})).toEqual(
      expect.arrayContaining(['tauri', 'rustcrypto']),
    );
    expect(eco('github-actions')?.groups).toBeDefined();
  });

  it('keeps the format-sensitive crates off the automatic upgrade path', () => {
    const ignored = eco('cargo')?.ignore ?? [];
    for (const name of ['rusqlite', 'libsqlite3-sys', 'argon2']) {
      const rule = ignored.find((i) => i['dependency-name'] === name);
      expect(rule?.['update-types'], name).toContain('version-update:semver-major');
    }
  });

  it('never sets both directory and directories on one ecosystem', () => {
    for (const u of dep.updates)
      expect(Boolean(u.directory) && Boolean(u.directories), u['package-ecosystem']).toBe(false);
  });
});

describe('the Rust test job runs only the crates a change can affect', () => {
  // Runs the real classifier step from ci.yml on a list of changed files.
  const classify = (files: string[]): { crates: string; rust: boolean } => {
    const ci = load('ci.yml');
    const run = (ci.jobs.changes.steps as Step[]).find((s) => s.run?.includes('not_site='))!.run!;
    const body = run.slice(run.indexOf('not_site='), run.indexOf("printf 'changed files"));
    const script = `docs='^README\\.md$'\nnot_docs=$(printf '%s' "$files" | grep -Ev "$docs" || true)\n${body}\necho "$crates|$rust"`;
    const out = execFileSync('bash', ['-c', script], {
      env: { ...process.env, files: files.join('\n') },
      encoding: 'utf8',
    })
      .trim()
      .split('\n')
      .pop()!;
    const [crates, rust] = out.split('|');
    return { crates, rust: rust === 'true' };
  };

  it('tests one leaf crate alone', () => {
    expect(classify(['unv-cli/src/main.rs'])).toEqual({ crates: '-p unv-cli', rust: true });
  });

  it('tests a crate and everything built on it', () => {
    expect(classify(['unv-server/src/lib.rs']).crates).toBe(
      '-p unv-server -p unv-cli -p unenverse',
    );
  });

  it('tests the whole workspace for vault-core, manifests, the lockfile and anything unplaced', () => {
    for (const f of [
      'vault-core/src/lib.rs',
      'Cargo.lock',
      '.github/workflows/ci.yml',
      'scripts/x.sh',
    ])
      expect(classify([f]).crates).toBe('--workspace');
  });

  it('runs no Rust tests for a TypeScript-only change', () => {
    // The bug this pins: every change used to run the 45 minute Rust job.
    expect(classify(['src/ts/render.ts', 'tests/a.test.ts'])).toEqual({ crates: '', rust: false });
  });
});

describe('security audit workflow', () => {
  const wf = load('security.yml');
  const dir = path.join(root, '.github/workflows');

  it('runs the free scanners as separate jobs and never gates the required check', () => {
    for (const id of [
      'gitleaks',
      'zizmor',
      'hadolint',
      'trivy',
      'osv',
      'semgrep',
      'npm-signatures',
    ])
      expect(wf.jobs[id], id).toBeDefined();
    expect(needs(load('ci.yml').jobs['ci-ok']).sort()).toEqual([
      'changes',
      'frontend',
      'lint',
      'test',
    ]);
  });

  it('pins every third-party action to a commit', () => {
    for (const f of fs.readdirSync(dir).filter((n) => n.endsWith('.yml'))) {
      const text = fs.readFileSync(path.join(dir, f), 'utf8');
      for (const m of text.matchAll(/^\s*-?\s*uses:\s*([^\s#]+)/gm)) {
        const ref = m[1];
        if (ref.startsWith('./')) continue;
        expect(ref, `${f}: ${ref}`).toMatch(/@[0-9a-f]{40}$/);
      }
    }
  });

  it('never leaves the checkout token on disk in a job that does not push', () => {
    for (const f of fs.readdirSync(dir).filter((n) => n.endsWith('.yml'))) {
      const text = fs.readFileSync(path.join(dir, f), 'utf8');
      const steps = text.split(/^\s*- uses: actions\/checkout@/m).slice(1);
      for (const s of steps) {
        const head = s.split(/\n\s*- /)[0];
        expect(head, `${f}: checkout`).toContain('persist-credentials: false');
      }
    }
  });
});
