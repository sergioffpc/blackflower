import json from '@eslint/json';
import javascript from '@eslint/js';
import stylistic from '@stylistic/eslint-plugin';
import google from 'eslint-config-google';
import jsdoc from 'eslint-plugin-jsdoc';

// Keep Google's published rule options while adapting removed ESLint APIs.
const googleRules = {};
for (const [name, options] of Object.entries(google.rules)) {
  if (name === 'require-jsdoc' || name === 'valid-jsdoc') continue;
  let rule = name;
  if (name === 'no-new-object') rule = 'no-object-constructor';
  if (name === 'func-call-spacing') rule = 'function-call-spacing';
  if (Object.hasOwn(stylistic.rules, rule)) rule = `@stylistic/${rule}`;
  googleRules[rule] = options;
}

export default [
  {
    files: ['**/*.{js,mjs,cjs}'],
    plugins: {'@stylistic': stylistic, jsdoc},
    languageOptions: {ecmaVersion: 2024, sourceType: 'module'},
    settings: {
      jsdoc: {
        mode: 'closure',
        tagNamePreference: {returns: 'return', file: 'fileoverview'},
      },
    },
    rules: {
      ...javascript.configs.recommended.rules,
      ...googleRules,
      '@stylistic/quotes': [
        'error', 'single', {allowTemplateLiterals: 'always'},
      ],
      '@stylistic/max-len': ['error', {
        code: 80,
        tabWidth: 2,
        ignoreUrls: true,
        ignorePattern: '^(import |export .* from )',
      }],
      'no-duplicate-imports': 'error',
      'no-restricted-syntax': ['error', {
        selector: 'ExportDefaultDeclaration',
        message: 'Use named exports under the Google JavaScript guide.',
      }],
      'jsdoc/require-jsdoc': ['error', {
        require: {
          FunctionDeclaration: true,
          MethodDefinition: true,
          ClassDeclaration: true,
        },
      }],
      'jsdoc/check-param-names': 'error',
      'jsdoc/check-tag-names': 'error',
      'jsdoc/valid-types': 'error',
      'jsdoc/require-param': 'error',
      'jsdoc/require-param-type': 'error',
      'jsdoc/require-returns': 'error',
      'jsdoc/require-returns-type': 'error',
    },
  },
  {
    files: ['tools/style/*.{js,mjs,cjs}'],
    languageOptions: {globals: {console: 'readonly', process: 'readonly'}},
  },
  {
    // ESLint's flat configuration interface requires a default export.
    files: ['tools/style/eslint.config.mjs'],
    rules: {'no-restricted-syntax': 'off'},
  },
  {
    files: ['**/*.json', '**/*.jsonc'],
    plugins: {json},
    language: 'json/json',
    rules: {
      'json/no-duplicate-keys': 'error',
      'json/no-unsafe-values': 'error',
    },
  },
  {
    files: ['**/*.jsonc', '.vscode/*.json'],
    language: 'json/jsonc',
    languageOptions: {allowTrailingCommas: false},
  },
];
