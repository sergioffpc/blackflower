/** @fileoverview Check the exact index tree without changing staged files. */
import {execFileSync, spawnSync} from 'node:child_process';
import {existsSync, mkdirSync, mkdtempSync, rmSync, symlinkSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {resolve} from 'node:path';

const root = execFileSync('git', ['rev-parse', '--show-toplevel'],
    {encoding: 'utf8'}).trim();
const gitDirectory = execFileSync('git', ['rev-parse', '--absolute-git-dir'],
    {encoding: 'utf8'}).trim();
const index = execFileSync('git', [
  'rev-parse', '--path-format=absolute', '--git-path', 'index',
], {encoding: 'utf8'}).trim();
const snapshot = mkdtempSync(resolve(tmpdir(), 'blackflower-staged-style-'));

try {
  execFileSync('git', ['checkout-index', '--all', `--prefix=${snapshot}/`],
      {cwd: root, stdio: 'inherit'});
  const checker = resolve(snapshot, 'tools/style/check.mjs');
  if (!existsSync(checker)) {
    throw new Error('Stage the style tooling before committing source files.');
  }

  // Tool installations are local prerequisites, not source files in the index.
  for (const path of [
    'tools/style/node_modules', 'tools/cooker/.venv', 'build/style/bin',
  ]) {
    const installed = resolve(root, path);
    if (existsSync(installed)) {
      mkdirSync(resolve(snapshot, path, '..'), {recursive: true});
      symlinkSync(installed, resolve(snapshot, path), 'dir');
    }
  }

  const result = spawnSync(process.execPath, [checker], {
    cwd: snapshot,
    env: {
      ...process.env,
      GIT_DIR: gitDirectory,
      GIT_WORK_TREE: snapshot,
      GIT_INDEX_FILE: index,
    },
    stdio: 'inherit',
  });
  if (result.error) console.error(result.error.message);
  process.exitCode = result.error ? 1 : (result.status ?? 1);
} finally {
  rmSync(snapshot, {recursive: true, force: true});
}
