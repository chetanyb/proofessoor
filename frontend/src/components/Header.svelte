<script lang="ts">
  import type { StatusSummary } from '../lib/types'

  let {
    summary,
    connected,
    live,
  }: { summary: StatusSummary | null; connected: boolean; live: boolean } = $props()
</script>

<header class="sticky top-0 z-10 border-b border-line bg-ink/85 backdrop-blur-sm">
  <div class="mx-auto flex max-w-7xl items-center justify-between gap-4 px-8 py-3">
    <div class="flex items-center gap-3">
      <img src="/mascot.png" alt="" class="size-10 shrink-0" />
      <div>
        <h1 class="text-xl/6 font-semibold tracking-tight">proofessoor</h1>
        <p class="text-xs/4 text-chalk/45">clientless execution-proof requestor</p>
      </div>
    </div>
    <div class="flex items-center gap-6">
      <div class="flex items-center gap-2">
        {#if !connected}
          <span class="size-2 rounded-full bg-ember"></span>
          <span class="text-xs/4 text-chalk/45">offline</span>
        {:else if live}
          <span class="size-2 animate-pulse rounded-full bg-spark"></span>
          <span class="text-xs/4 text-chalk/45">live</span>
        {:else}
          <span class="size-2 rounded-full bg-gold"></span>
          <span class="text-xs/4 text-chalk/45">idle</span>
        {/if}
      </div>
      <div class="text-right">
        <p class="text-[10px]/3 tracking-widest text-chalk/40 uppercase">latest slot</p>
        <p class="mono text-lg/6 font-semibold">{summary?.latest_slot ?? '—'}</p>
      </div>
      <a
        class="rounded-md border border-line px-3 py-1.5 text-sm/5 text-chalk/70 transition-colors hover:border-violet hover:text-chalk"
        href="/metrics"
        target="_blank"
        rel="noreferrer">metrics</a
      >
    </div>
  </div>
</header>
