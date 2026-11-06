// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
import * as vscode from 'vscode';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { unvJson, runUnv } from './unv';
import {
  FIELD_NAMES,
  completionContext,
  entryNames,
  looksLocked,
  referenceFor,
  execArgs,
  hoverText,
  parseExposureReport,
  pastedText,
  guardHits,
  refAt,
  type ExecTask,
} from './refs';

let names: string[] = [];
let diagnostics: vscode.DiagnosticCollection;
let status: vscode.StatusBarItem;
const treeChanged = new vscode.EventEmitter<void>();

function setStatus(state: 'ok' | 'locked' | 'missing', count = 0): void {
  if (state === 'ok') {
    status.text = `$(key) UnENVerse: ${count}`;
    status.tooltip = `${count} entries available. Click to insert a reference.`;
  } else if (state === 'locked') {
    status.text = '$(lock) UnENVerse: locked';
    status.tooltip = 'Run `unv login` in a terminal, then click to refresh.';
  } else {
    status.text = '$(warning) UnENVerse: unv not found';
    status.tooltip = 'Set unenverse.path to the unv executable.';
  }
  status.show();
}

function exe(): string {
  return vscode.workspace.getConfiguration('unenverse').get<string>('path') || 'unv';
}

/** Entry names only; the CLI redacts everything else. */
async function refreshNames(): Promise<void> {
  try {
    const data = await unvJson<{ entries?: { provider?: string }[] } | { provider?: string }[]>(
      exe(),
      ['list'],
    );
    names = entryNames(data);
    setStatus('ok', names.length);
  } catch (e) {
    names = [];
    const msg = e instanceof Error ? e.message : String(e);
    setStatus(/ENOENT|not found/i.test(msg) && !looksLocked(msg) ? 'missing' : 'locked');
  }
  treeChanged.fire();
}

async function insertReference(): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  if (!editor) return;
  if (!names.length) await refreshNames();
  const pick = await vscode.window.showQuickPick(names, {
    placeHolder: names.length ? 'Entry to reference' : 'No entries (is the vault locked?)',
  });
  if (!pick) return;
  await editor.edit((b) => editor.selections.forEach((sel) => b.replace(sel, referenceFor(pick))));
}

async function scanWorkspace(): Promise<void> {
  // One unv call for the whole folder: each call costs a key derivation.
  for (const f of vscode.workspace.workspaceFolders ?? []) await scanTarget(f.uri.fsPath);
}

class EntryTree implements vscode.TreeDataProvider<string> {
  onDidChangeTreeData = treeChanged.event;
  getTreeItem(name: string): vscode.TreeItem {
    const item = new vscode.TreeItem(name, vscode.TreeItemCollapsibleState.None);
    item.iconPath = new vscode.ThemeIcon('key');
    item.tooltip = 'Click to insert ${' + name + '} at the cursor';
    item.command = { command: 'unenverse.insertEntry', title: 'Insert', arguments: [name] };
    item.contextValue = 'entry';
    return item;
  }
  getChildren(): string[] {
    return names;
  }
}

let guardBusy = false;

/**
 * Warn when text just inserted holds a value the vault holds (R35). VS Code offers no way to stop
 * a paste, so this warns after it and says how to undo; the text goes to `unv guard check` on
 * stdin and nowhere else, and only the entry names come back.
 */
async function guardPaste(e: vscode.TextDocumentChangeEvent): Promise<void> {
  if (guardBusy || e.document.uri.scheme === 'output') return;
  const text = pastedText(e.contentChanges);
  if (!text) return;
  guardBusy = true;
  try {
    let out: string;
    try {
      out = await runUnv(exe(), ['--json', 'guard', 'check'], undefined, 30000, text);
    } catch (err) {
      out = (err as { stdout?: string }).stdout ?? '';
    }
    const found = guardHits(out);
    if (found && found.length) {
      void vscode.window.showWarningMessage(
        `UnENVerse: this text holds a secret from your vault (${found.join(', ')}). Undo it and use a \${Provider/field} reference instead.`,
      );
    }
  } finally {
    guardBusy = false;
  }
}

async function scanFile(doc: vscode.TextDocument): Promise<void> {
  if (doc.uri.scheme !== 'file') return;
  const found = await scanTarget(doc.uri.fsPath);
  if (found === null) diagnostics.delete(doc.uri);
}

/** Scan a file or folder with one `unv scan --exposed`; null when unv could not say. */
async function scanTarget(target: string): Promise<number | null> {
  const dir = await mkdtemp(join(tmpdir(), 'unenverse-scan-'));
  const report = join(dir, 'report.txt');
  try {
    await runUnv(exe(), ['scan', '--exposed', target, '--out', report], undefined, 120000);
    const found = parseExposureReport(await readFile(report, 'utf8'));
    const byPath = new Map<string, typeof found>();
    for (const f of found) byPath.set(f.path, [...(byPath.get(f.path) ?? []), f]);
    // Clear files that were flagged before and are clean now.
    diagnostics.forEach((uri) => {
      if (uri.fsPath.startsWith(target) && !byPath.has(uri.fsPath)) diagnostics.delete(uri);
    });
    for (const [path, list] of byPath) {
      const uri = vscode.Uri.file(path);
      const doc = await vscode.workspace.openTextDocument(uri);
      diagnostics.set(
        uri,
        list.map((f) => {
          const line = Math.max(0, Math.min(f.line - 1, doc.lineCount - 1));
          const d = new vscode.Diagnostic(
            doc.lineAt(line).range,
            `This line holds a value that is a secret in your vault (${f.fingerprint}). Use a \${Provider/field} reference instead.`,
            vscode.DiagnosticSeverity.Warning,
          );
          d.source = 'UnENVerse';
          return d;
        }),
      );
    }
    return found.length;
  } catch {
    return null; // locked vault or no unv: say nothing rather than guess
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
}

export function activate(ctx: vscode.ExtensionContext): void {
  diagnostics = vscode.languages.createDiagnosticCollection('unenverse');
  status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 50);
  status.command = 'unenverse.insertReference';
  ctx.subscriptions.push(diagnostics, status, treeChanged);
  void refreshNames();

  ctx.subscriptions.push(
    vscode.commands.registerCommand('unenverse.refresh', () => refreshNames()),
    vscode.commands.registerCommand('unenverse.insertReference', () => insertReference()),
    vscode.commands.registerCommand('unenverse.insertEntry', async (name?: string) => {
      const editor = vscode.window.activeTextEditor;
      if (editor && name)
        await editor.edit((b) =>
          editor.selections.forEach((s) => b.replace(s, referenceFor(name))),
        );
    }),
    vscode.commands.registerCommand('unenverse.copyReference', async (name?: string) => {
      if (name) await vscode.env.clipboard.writeText(referenceFor(name));
    }),
    vscode.commands.registerCommand('unenverse.scanWorkspace', () => scanWorkspace()),
    vscode.window.registerTreeDataProvider('unenverse.entries', new EntryTree()),
    vscode.commands.registerCommand('unenverse.scanThisFile', async () => {
      const doc = vscode.window.activeTextEditor?.document;
      if (doc) await scanFile(doc);
    }),
    vscode.workspace.onDidSaveTextDocument((doc) => {
      if (vscode.workspace.getConfiguration('unenverse').get<boolean>('scanOnSave'))
        void scanFile(doc);
    }),
    vscode.workspace.onDidChangeTextDocument((e) => {
      if (vscode.workspace.getConfiguration('unenverse').get<boolean>('guardPaste'))
        void guardPaste(e);
    }),
    vscode.workspace.onDidCloseTextDocument((doc) => diagnostics.delete(doc.uri)),

    vscode.languages.registerCompletionItemProvider(
      { scheme: 'file' },
      {
        async provideCompletionItems(doc, pos) {
          const c = completionContext(doc.lineAt(pos.line).text.slice(0, pos.character));
          if (!c) return undefined;
          if (c.kind === 'provider') {
            return names.map(
              (n) => new vscode.CompletionItem(n, vscode.CompletionItemKind.Variable),
            );
          }
          return FIELD_NAMES.map(
            (f) => new vscode.CompletionItem(f, vscode.CompletionItemKind.Field),
          );
        },
      },
      '{',
      '/',
    ),

    vscode.languages.registerHoverProvider(
      { scheme: 'file' },
      {
        async provideHover(doc, pos) {
          const ref = refAt(doc.lineAt(pos.line).text, pos.character);
          if (!ref || ref.head.includes(':')) return undefined;
          let shown: { fingerprint?: string; length?: number } | null = null;
          try {
            const args = ['get', ref.head, ...(ref.field ? ['--field', ref.field] : [])];
            const d = await unvJson<{ value?: { fingerprint?: string; length?: number } }>(
              exe(),
              args,
            );
            shown = d.value ?? null;
          } catch {
            shown = null;
          }
          return new vscode.Hover(new vscode.MarkdownString(hoverText(ref, shown)));
        },
      },
    ),

    vscode.tasks.registerTaskProvider('unv', {
      provideTasks: () => [],
      resolveTask(task) {
        const def = task.definition as unknown as ExecTask & { type: string };
        let args: string[];
        try {
          args = execArgs(def);
        } catch {
          return undefined;
        }
        // ProcessExecution: argv, no shell, so nothing in the task can be parsed as one.
        return new vscode.Task(
          task.definition,
          task.scope ?? vscode.TaskScope.Workspace,
          task.name,
          'unv',
          new vscode.ProcessExecution(exe(), args),
        );
      },
    }),
  );
}

export function deactivate(): void {}
