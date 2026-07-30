import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte({ compilerOptions: { runes: true } })],
  resolve: {
    conditions: ['browser'],
  },
  test: {
    include: ['src/**/*.test.ts'],
    environment: 'jsdom',
    globals: true,
  },
});
