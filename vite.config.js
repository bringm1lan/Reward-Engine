import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { nodePolyfills } from 'vite-plugin-node-polyfills';

const NODE_GLOBAL_SHIMS = [
  'vite-plugin-node-polyfills/shims/buffer',
  'vite-plugin-node-polyfills/shims/global',
  'vite-plugin-node-polyfills/shims/process',
];

export default defineConfig({
  plugins: [
    react(),
    nodePolyfills({
      include: ['buffer', 'crypto', 'stream', 'util'],
      globals: { Buffer: true, global: true, process: true },
    }),
  ],
  optimizeDeps: {
    exclude: NODE_GLOBAL_SHIMS,
    include: [
      '@privy-io/react-auth',
      '@solana/web3.js',
      '@solana/wallet-adapter-base',
      '@solana/wallet-adapter-react',
    ],
    esbuildOptions: { target: 'es2020' },
  },
  build: {
    target: 'es2020',
    sourcemap: false,
    commonjsOptions: {
      include: [/@privy-io\/react-auth/, /@solana\/web3\.js/, /node_modules/],
      transformMixedEsModules: true,
    },
  },
  server: { port: 5175, strictPort: false },
});
