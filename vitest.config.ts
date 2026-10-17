import { defineConfig } from 'vitest/config';

export default defineConfig({
  resolve: {
    alias: { '@': '/src' },
  },
  test: {
    environment: 'jsdom',
    include: ['tests/**/*.test.ts'],
    setupFiles: ['tests/setup.ts'],
    restoreMocks: true,
    coverage: {
      provider: 'v8',
      include: ['src/ts/**/*.ts'],
      exclude: ['src/ts/types.ts'],
      reporter: ['text', 'html', 'lcov', 'json-summary'],
      // A floor just under the measured figures (statements 71.85, branches
      // 61.53, functions 73.63, lines 74.24 on 2026-10-17). It stops the number
      // sliding, nothing more: raise it when coverage rises, never to chase it.
      thresholds: { statements: 71, branches: 60, functions: 73, lines: 74 },
    },
  },
});
