import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import tailwindcss from '@tailwindcss/vite'

// https://vite.dev/config/
export default defineConfig({
  // The Tailwind plugin must come above the Svelte plugin.
  plugins: [tailwindcss(), svelte()],
})
