// Data layer: fetch the requestor's dashboard state and the pure liveness rule.
// Reactivity lives in App.svelte; this stays free of runes so it's testable.

import type { BlockRecord, StatusSummary } from './types'

export interface Dashboard {
  blocks: BlockRecord[]
  summary: StatusSummary
}

export async function fetchDashboard(): Promise<Dashboard> {
  const [blocks, summary] = await Promise.all([
    fetch('/api/blocks').then((r) => r.json() as Promise<BlockRecord[]>),
    fetch('/api/status').then((r) => r.json() as Promise<StatusSummary>),
  ])
  return { blocks, summary }
}

/** A requestor that stops seeing new slots has stalled, not merely answered. */
const STALE_AFTER_MS = 30_000

/** Live = a new head was observed within the stale window. */
export const isLive = (lastAdvanceMs: number, nowMs: number, staleAfterMs = STALE_AFTER_MS) =>
  lastAdvanceMs > 0 && nowMs - lastAdvanceMs < staleAfterMs
