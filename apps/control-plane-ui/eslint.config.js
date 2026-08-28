import js from '@eslint/js'
import reactHooks from 'eslint-plugin-react-hooks'
import globals from 'globals'
import tseslint from 'typescript-eslint'

/**
 * The frontend half of this repository's quality policy.
 *
 * The Rust side denies `unwrap`, `panic`, and indexing, forbids `unsafe`, and
 * fails the build on any production file over 150 lines. This is the nearest
 * equivalent for TypeScript: no `any` escaping the type system, no floating
 * promises, no unused code, and the same file-size discipline -- so a reviewer
 * reading either half of the repository meets the same standard.
 *
 * The type-aware rules are scoped to `.ts`/`.tsx` rather than applied
 * globally, because this very file is JavaScript and is not in a TypeScript
 * project. Applying them everywhere makes ESLint fail on its own
 * configuration.
 */
export default tseslint.config(
  { ignores: ['dist', 'coverage', 'node_modules'] },

  js.configs.recommended,

  {
    files: ['**/*.{ts,tsx}'],
    extends: [...tseslint.configs.strictTypeChecked],
    languageOptions: {
      globals: globals.browser,
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    plugins: { 'react-hooks': reactHooks },
    rules: {
      ...reactHooks.configs.recommended.rules,

      // The counterpart of docs/architecture/file-size-policy.md. Test files
      // are exempt there and here, for the same reason: thorough tests should
      // not be expensive to write.
      'max-lines': ['error', { max: 150, skipBlankLines: true, skipComments: true }],

      // `any` is this language's `unwrap`: it does not fail, it just stops
      // checking.
      '@typescript-eslint/no-explicit-any': 'error',

      // An unawaited promise in a handler is how an error disappears.
      '@typescript-eslint/no-floating-promises': 'error',
    },
  },

  {
    files: ['**/*.test.{ts,tsx}', 'src/test-setup.ts'],
    rules: {
      'max-lines': 'off',
      // A test asserting on what was sent has to name the shape it expects;
      // that is the assertion, not a hole in the type system.
      '@typescript-eslint/no-unsafe-type-assertion': 'off',
    },
  },

  {
    files: ['vite.config.ts'],
    languageOptions: { globals: globals.node },
  },
)
