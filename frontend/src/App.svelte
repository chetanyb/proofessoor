<script lang="ts">
  type Outcome = 'sent' | 'complete' | 'failed'

  interface BlockRecord {
    slot: number
    beacon_block_root: string
    execution_block_number: number
    new_payload_request_root: string
    proof_types: string[]
    outcome: Outcome
    observed_at_ms: number
    requested_at_ms: number
    resolved_at_ms: number | null
  }

  interface StatusSummary {
    total: number
    sent: number
    complete: number
    failed: number
    latest_slot: number | null
  }

  let blocks = $state<BlockRecord[]>([])
  let summary = $state<StatusSummary | null>(null)
  let connected = $state(false)

  async function refresh() {
    try {
      const [b, s] = await Promise.all([
        fetch('/api/blocks').then((r) => r.json()),
        fetch('/api/status').then((r) => r.json()),
      ])
      blocks = b
      summary = s
      connected = true
    } catch {
      connected = false
    }
  }

  $effect(() => {
    refresh()
    const id = setInterval(refresh, 3000)
    return () => clearInterval(id)
  })

  const tiles = $derived([
    { label: 'Total', value: summary?.total ?? 0, accent: 'text-surface-950-50' },
    { label: 'In-flight', value: summary?.sent ?? 0, accent: 'text-warning-500' },
    { label: 'Completed', value: summary?.complete ?? 0, accent: 'text-success-500' },
    { label: 'Failed', value: summary?.failed ?? 0, accent: 'text-error-500' },
  ])

  const prepMs = (r: BlockRecord) => r.requested_at_ms - r.observed_at_ms
  const e2eMs = (r: BlockRecord) =>
    r.resolved_at_ms === null ? null : r.resolved_at_ms - r.observed_at_ms
  const fmtMs = (ms: number | null) => (ms === null ? '—' : `${ms} ms`)
  const shortRoot = (h: string) => `${h.slice(0, 8)}…${h.slice(-6)}`
  const outcomePreset = (o: Outcome) =>
    o === 'complete'
      ? 'preset-tonal-success'
      : o === 'failed'
        ? 'preset-tonal-error'
        : 'preset-tonal-warning'
</script>

<div class="min-h-dvh">
  <header
    class="sticky top-0 z-10 border-b border-surface-200-800 bg-surface-50-950/80 backdrop-blur-sm"
  >
    <div class="mx-auto flex max-w-7xl items-center justify-between gap-4 px-8 py-4">
      <div class="flex items-center gap-3">
        <div
          class="size-2.5 rounded-full {connected
            ? 'animate-pulse bg-success-500'
            : 'bg-error-500'}"
        ></div>
        <div>
          <h1 class="text-xl/6 font-semibold tracking-tight">proofessoor</h1>
          <p class="text-xs/4 text-surface-600-400">clientless execution-proof requestor</p>
        </div>
      </div>
      <div class="flex items-center gap-6">
        <div class="text-right">
          <p class="text-xs/4 text-surface-600-400">latest slot</p>
          <p class="mono text-lg/6 font-semibold">{summary?.latest_slot ?? '—'}</p>
        </div>
        <a class="btn preset-tonal-surface" href="/metrics" target="_blank" rel="noreferrer">
          metrics
        </a>
      </div>
    </div>
  </header>

  <main class="mx-auto flex max-w-7xl flex-col gap-8 px-8 py-8">
    <section class="grid grid-cols-2 gap-4 lg:grid-cols-4">
      {#each tiles as tile (tile.label)}
        <div class="card border border-surface-200-800 bg-surface-50-950 p-5">
          <p class="text-xs/4 text-surface-600-400 uppercase">{tile.label}</p>
          <p class="mono mt-2 text-3xl/9 font-bold {tile.accent}">{tile.value}</p>
        </div>
      {/each}
    </section>

    <section class="card overflow-hidden border border-surface-200-800 bg-surface-50-950">
      <header class="flex items-center justify-between border-b border-surface-200-800 px-6 py-4">
        <h2 class="text-lg/6 font-semibold">Proof requests</h2>
        <p class="text-xs/4 text-surface-600-400">auto-refreshing every 3s</p>
      </header>

      {#if blocks.length === 0}
        <p class="px-6 py-16 text-center text-surface-600-400">
          No proof requests yet — waiting for beacon blocks…
        </p>
      {:else}
        <div class="table-wrap">
          <table class="table">
            <thead>
              <tr>
                <th>Slot</th>
                <th>Exec #</th>
                <th>Proof types</th>
                <th>Status</th>
                <th class="text-right">Prep</th>
                <th class="text-right">End-to-end</th>
                <th>Request root</th>
              </tr>
            </thead>
            <tbody class="[&>tr]:hover:bg-surface-100-900">
              {#each blocks as block (block.new_payload_request_root)}
                <tr>
                  <td class="mono">{block.slot}</td>
                  <td class="mono text-surface-700-300">{block.execution_block_number}</td>
                  <td>
                    <div class="flex flex-wrap gap-1">
                      {#each block.proof_types as type (type)}
                        <span class="badge preset-tonal-primary">{type}</span>
                      {/each}
                    </div>
                  </td>
                  <td><span class="badge {outcomePreset(block.outcome)}">{block.outcome}</span></td>
                  <td class="mono text-right text-surface-700-300">{fmtMs(prepMs(block))}</td>
                  <td class="mono text-right font-medium">{fmtMs(e2eMs(block))}</td>
                  <td class="mono text-surface-600-400">{shortRoot(block.new_payload_request_root)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>
  </main>
</div>
