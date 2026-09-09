/** @fileoverview Validate tracked and non-ignored repository source files. */
import {execFileSync, spawnSync} from 'node:child_process';
import {existsSync, readFileSync} from 'node:fs';
import {dirname, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';

const toolDirectory = dirname(fileURLToPath(import.meta.url));
const root = resolve(toolDirectory, '../..');
process.chdir(root);
process.env.PATH = `${root}/build/style/bin:${process.env.PATH}`;
const write = process.argv.includes('--write');
if (process.argv.slice(2).some((argument) => argument !== '--write')) {
  throw new Error('Usage: npm --prefix tools/style run check|format');
}
const files = [...new Set(execFileSync('git', [
  'ls-files', '--cached', '--others', '--exclude-standard', '-z',
], {encoding: 'utf8'}).split('\0').filter(Boolean))]
    .filter(existsSync).sort();
const markdown = files.filter((file) => /\.md$/i.test(file));
const json = files.filter((file) => /\.jsonc?$/i.test(file));
const javascript = files.filter((file) => /\.(?:js|mjs|cjs)$/i.test(file));
const workflows = files.filter(
    (file) => /^\.github\/workflows\/[^/]+\.ya?ml$/.test(file));
const cpp = files.filter(
    (file) => /\.(?:c|cc|cpp|cxx|h|hh|hpp|hxx)$/i.test(file));
const python = files.filter((file) => /\.py$/i.test(file));
const shell = files.filter((file) => /\.(?:sh|bash)$/i.test(file) ||
  (!/\.[^/]+$/.test(file.split('/').at(-1)) &&
    /^#![^\n]*\b(?:bash|sh)\b/.test(readFileSync(file, 'utf8'))));
let failed = false;
/**
 * Run a tool, retaining any failure for the final exit status.
 * @param {string} command Executable name or path.
 * @param {!Array<string>} args Arguments passed without shell expansion.
 */
function run(command, args) {
  const result = spawnSync(command, args, {stdio: 'inherit'});
  if (result.error) console.error(result.error.message);
  if (result.error || result.status !== 0) failed = true;
}
/**
 * Run a tool from the locked local npm installation.
 * @param {string} name Installed executable name.
 * @param {!Array<string>} args Tool arguments.
 */
function npmTool(name, args) {
  run(resolve(toolDirectory, 'node_modules/.bin', name), args);
}

// Syntax checks precede formatting so permissive formatters cannot repair and
// silently accept invalid JSON (such as comments in a strict JSON file).
if (json.length) {
  npmTool('eslint', ['--config', 'tools/style/eslint.config.mjs',
    '--max-warnings', '0', ...json]);
}
if (failed) process.exit(1);
if (javascript.length) {
  npmTool('eslint', ['--config', 'tools/style/eslint.config.mjs',
    '--max-warnings', '0', ...(write ? ['--fix'] : []), ...javascript]);
}
if (cpp.length) {
  run(process.env.BLACKFLOWER_CLANG_FORMAT || 'clang-format-21',
      [write ? '-i' : '--dry-run', '--Werror', ...cpp]);
}
if (python.length) {
  run(resolve(root, 'tools/cooker/.venv/bin/pyink'), [
    ...(write ? [] : ['--check']),
    '--config', 'tools/cooker/pyproject.toml', ...python,
  ]);
  run(resolve(root, 'tools/cooker/.venv/bin/python'), [
    'tools/style/python_function_size.py', ...python,
  ]);
}
if (markdown.length + json.length) {
  npmTool('prettier', [write ? '--write' : '--check', ...markdown, ...json]);
}
if (markdown.length) npmTool('markdownlint-cli2', markdown);
if (shell.length) {
  run('shellcheck', ['--severity=style', ...shell]);
  run('shfmt', ['-i', '2', '-ci', '-bn', write ? '-w' : '-d', ...shell]);
  for (const file of shell) {
    const lines = readFileSync(file, 'utf8').split('\n');
    for (const [index, line] of lines.entries()) {
      if ([...line].length > 80) {
        console.error(`${file}:${index + 1}: shell line exceeds 80 columns`);
        failed = true;
      }
    }
  }
}
// actionlint skips ShellCheck when unavailable; the explicit call above makes
// its absence an error instead of silently reducing coverage.
if (workflows.length) {
  run('actionlint', [
    '-shellcheck', 'shellcheck', '-config-file', '.github/actionlint.yaml',
    ...workflows,
  ]);
}
console.log(`Checked ${cpp.length} C/C++, ${python.length} Python, ` +
  `${javascript.length} JavaScript, ` +
  `${markdown.length} Markdown, ${json.length} JSON/JSONC, ` +
  `${shell.length} shell files and GitHub Actions workflows.`);
process.exitCode = failed ? 1 : 0;
