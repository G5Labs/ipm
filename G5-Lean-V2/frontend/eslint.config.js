import js from '@eslint/js'
import globals from 'globals'
import jsxA11y from 'eslint-plugin-jsx-a11y'
import reactHooks from 'eslint-plugin-react-hooks'
import reactRefresh from 'eslint-plugin-react-refresh'
import tseslint from 'typescript-eslint'
import { defineConfig, globalIgnores } from 'eslint/config'

export default defineConfig([
  globalIgnores(['dist']),
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommended,
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
      jsxA11y.flatConfigs.recommended,
    ],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    rules: {
      // Warn (not error) for unused vars — many existing usages across codebase.
      // Vars starting with _ are fully ignored.
      '@typescript-eslint/no-unused-vars': ['warn', {
        argsIgnorePattern: '^_',
        varsIgnorePattern: '^_',
        destructuredArrayIgnorePattern: '^_',
        caughtErrorsIgnorePattern: '^_',
      }],
      // Warn (not error) for explicit any — too many existing usages to block CI
      '@typescript-eslint/no-explicit-any': 'warn',
      // Warn for `Function` type — existing usages in test helpers
      '@typescript-eslint/no-unsafe-function-type': 'warn',
      // Allow useless escapes — common in regex/template literals
      'no-useless-escape': 'warn',
      // Downgrade react-refresh to warn — many files export helpers alongside components
      'react-refresh/only-export-components': 'warn',
      // React compiler rules: downgrade to warn for pre-existing patterns.
      // These should be incrementally fixed but must not block CI today.
      'react-hooks/set-state-in-effect': 'warn',
      'react-hooks/preserve-manual-memoization': 'warn',
      'react-hooks/purity': 'warn',
      'react-hooks/refs': 'warn',
      'react-hooks/static-components': 'warn',
      // jsx-a11y ratchet (checklist-design audit 2026-08-11): the recommended
      // set enforces as errors; only the rules with pre-existing violations at
      // adoption time are downgraded to warn so existing debt stays visible
      // without blocking CI. Do not add rules here — fix new code instead.
      'jsx-a11y/no-static-element-interactions': 'warn',
      'jsx-a11y/click-events-have-key-events': 'warn',
      'jsx-a11y/label-has-associated-control': 'warn',
      'jsx-a11y/no-autofocus': 'warn',
      'jsx-a11y/no-noninteractive-element-interactions': 'warn',
      'jsx-a11y/interactive-supports-focus': 'warn',
      'jsx-a11y/no-noninteractive-tabindex': 'warn',
    },
  },
])

