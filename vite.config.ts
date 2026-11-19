import react from '@vitejs/plugin-react';
import { configDefaults, defineConfig } from 'vitest/config';

// Tauri expects a fixed dev port and a relative build.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  base: './',
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: {
    target: 'safari16',
    sourcemap: false,
  },
  test: {
    // Sessions' worktrees are folders inside the main checkout (.claude/worktrees/<name>), each
    // with a whole copy of src/ and node_modules of its own. Left in, `npm test` there ran every
    // worktree's tests as well as its own, some twenty times the suite, and theirs failed on a
    // second copy of React. A worktree's own run starts in its folder, so it still finds its tests.
    exclude: [...configDefaults.exclude, '.claude/**'],
  },
});
