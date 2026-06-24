<script lang="ts">
  import type { Snippet } from 'svelte'

  // A styled hover/focus tooltip wrapping a trigger. Use for rich hints where
  // the native title is too plain; keep native title where content is dense
  // (e.g. the cadence bars) or lives inside an overflow-clipped container.
  let {
    text,
    wide = false,
    children,
  }: { text: string; wide?: boolean; children: Snippet } = $props()
</script>

<span class="group relative inline-flex items-center">
  {@render children()}
  <span
    role="tooltip"
    class="pointer-events-none absolute bottom-full z-20 mb-2 hidden rounded-md border border-line bg-ink px-2.5 py-1.5 text-[11px]/4 text-chalk/80 shadow-xl group-hover:block group-focus-within:block {wide
      ? 'left-0 w-64 text-left'
      : 'left-1/2 -translate-x-1/2 whitespace-nowrap'}">{text}</span
  >
</span>
