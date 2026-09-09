/** @fileoverview Exercise style rejection through disposable Git commits. */
import assert from 'node:assert/strict';
import {execFileSync, spawnSync} from 'node:child_process';
import {cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const fixture = mkdtempSync(resolve(tmpdir(), 'blackflower-style-hooks-test-'));
const env = {...process.env};
for (const key of Object.keys(env)) {
  if (key.startsWith('GIT_')) delete env[key];
}
env.GIT_CONFIG_NOSYSTEM = '1';
env.GIT_CONFIG_GLOBAL = '/dev/null';

/**
 * Run Git against the disposable repository, throwing on unexpected failure.
 * @param {...string} args Git arguments.
 * @return {string} Command output.
 */
function git(...args) {
  const options = {cwd: fixture, env, encoding: 'utf8'};
  return execFileSync('git', args, options).trim();
}

try {
  const files = execFileSync('git', [
    'ls-files', '-co', '--exclude-standard', '-z',
  ], {cwd: root, encoding: 'utf8'}).split('\0').filter(Boolean);
  for (const path of new Set(files)) {
    if (!existsSync(resolve(root, path))) continue;
    mkdirSync(dirname(resolve(fixture, path)), {recursive: true});
    cpSync(resolve(root, path), resolve(fixture, path));
  }
  for (const path of [
    'tools/code_quality/node_modules', 'tools/content_pipeline/.venv',
    'build/style/bin',
  ]) {
    mkdirSync(dirname(resolve(fixture, path)), {recursive: true});
    symlinkSync(resolve(root, path), resolve(fixture, path), 'dir');
  }

  git('init', '--quiet', '--initial-branch=main');
  git('config', 'user.name', 'Style Hook Test');
  git('config', 'user.email', 'style-hook@example.invalid');
  // Only this disposable fixture disables signing.
  git('config', 'commit.gpgsign', 'false');
  git('config', 'core.hooksPath', '.githooks');
  git('add', '--all');
  git('commit', '--quiet', '-m', 'test: accept formatted source');
  const head = git('rev-parse', 'HEAD');

  const probes = [
    ['probe.json', '{"value":1}\n', '{\n  "value": 1\n}\n'],
    ['probe.md', '# Probe\n\n### Skipped level\n', '# Probe\n\n## Level\n'],
    ['probe.sh', '#!/bin/bash\nprintf "%s\\n" $1\n',
      '#!/bin/bash\n# Print one argument.\nprintf \'%s\\n\' "$1"\n'],
    ['probe.mjs', 'export var value = "bad"\n',
      'export const value = \'good\';\n'],
    ['probe.cc', 'int value=1;\n', 'int value = 1;\n'],
    ['probe.py', 'value=1\n', 'value = 1\n'],
  ];
  for (const [name, bad, good] of probes) {
    const path = resolve(fixture, name);
    writeFileSync(path, bad);
    git('add', name);
    writeFileSync(path, good);
    const result = spawnSync('git', [
      'commit', '--quiet', '-m', 'test: reject unformatted staged content',
    ], {cwd: fixture, env, encoding: 'utf8'});
    assert.notEqual(result.status, 0, `${name}: unformatted commit succeeded`);
    assert.match(`${result.stdout}\n${result.stderr}`, new RegExp(name));
    assert.equal(git('rev-parse', 'HEAD'), head);
    assert.equal(git('show', `:${name}`), bad.trim());
    assert.equal(readFileSync(path, 'utf8'), good);
    git('add', name);
    console.log(`Rejected staged ${name}; preserved index and working copy.`);
  }
  git('commit', '--quiet', '-m', 'test: accept corrected staged content');
  console.log('Formatted source commits successfully.');
} finally {
  rmSync(fixture, {recursive: true, force: true});
}
