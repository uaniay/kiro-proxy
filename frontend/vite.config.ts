import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  base: '/_ui/',
  server: {
    proxy: {
      '/_ui/api': 'http://localhost:9199',
    },
  },
  build: {
    outDir: 'dist',
  },
})
