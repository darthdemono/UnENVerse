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
      // A floor just under the measured figures (statements 99.52, branches
      // 95.35, functions 99.13, lines 99.9 on 2026-10-18). It stops the number
      // sliding, nothing more: raise it when coverage rises, never to chase it.
      thresholds: { statements: 99, branches: 95, functions: 99, lines: 99 },
    },
  },
});
