<script lang="ts">
  import Header from './components/Header.svelte'
  import CadenceStrip from './components/CadenceStrip.svelte'
  import RequestsTable from './components/RequestsTable.svelte'
  import BlockModal from './components/BlockModal.svelte'
  import { fetchDashboard, isLive } from './lib/api'
  import type { BlockRecord, StatusSummary } from './lib/types'

  let blocks = $state<BlockRecord[]>([])
  let summary = $state<StatusSummary | null>(null)
  let connected = $state(false)
  let live = $state(false)
  let paused = $state(false)
  let selected = $state<BlockRecord | null>(null)

  let lastSlot = -1
  let lastAdvanceMs = 0

  async function refresh() {
    if (paused) return
    try {
      const data = await fetchDashboard()
      blocks = data.blocks
      summary = data.summary
      connected = true
      const slot = data.summary.latest_slot ?? -1
      if (slot > lastSlot) {
        lastSlot = slot
        lastAdvanceMs = Date.now()
      }
      live = isLive(lastAdvanceMs, Date.now())
    } catch {
      connected = false
      live = false
    }
  }

  $effect(() => {
    refresh()
    const id = setInterval(refresh, 3000)
    return () => clearInterval(id)
  })
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && (selected = null)} />

<div class="min-h-dvh">
  <Header {summary} {connected} {live} />

  <main class="mx-auto flex max-w-7xl flex-col gap-6 px-8 py-7">
    <CadenceStrip {blocks} {summary} onSelect={(r) => (selected = r)} />
    <RequestsTable
      {blocks}
      {paused}
      onTogglePause={() => (paused = !paused)}
      onSelect={(r) => (selected = r)}
    />
  </main>

  {#if selected}
    <BlockModal record={selected} onClose={() => (selected = null)} />
  {/if}
</div>
