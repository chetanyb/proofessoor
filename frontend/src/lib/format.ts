// Pure derivations over BlockRecords — no DOM, no fetch, no reactivity — so the
// timing math and the cadence/scaling logic can be unit-tested in isolation.

import type { BlockRecord } from './types'

export const prepMs = (r: BlockRecord) => r.requested_at_ms - r.observed_at_ms

export const provingMs = (r: BlockRecord): number | null =>
  r.resolved_at_ms === null ? null : r.resolved_at_ms - r.requested_at_ms

export const e2eMs = (r: BlockRecord): number | null =>
  r.resolved_at_ms === null ? null : r.resolved_at_ms - r.observed_at_ms

export const fmt = (ms: number | null) => (ms === null ? '—' : `${ms} ms`)

export const shortRoot = (h: string) => `${h.slice(0, 8)}…${h.slice(-6)}`

/**
 * Split a resolved block into prep% + proving%, summing to 100% of its own
 * end-to-end. Bars built from this fill their track exactly, so the inline bar
 * reads as a prep:proving ratio while magnitude lives in the numeric columns.
 */
export const splitPct = (r: BlockRecord) => {
  const e = Math.max(1, e2eMs(r) ?? 1)
  return { prep: (prepMs(r) / e) * 100, proving: ((provingMs(r) ?? 0) / e) * 100 }
}

export interface Domain {
  min: number
  max: number
}

/** Fastest/slowest end-to-end among completed blocks — the log-scaling domain. */
export const e2eDomain = (blocks: BlockRecord[]): Domain => {
  const xs = blocks.filter((b) => b.outcome === 'complete').map((b) => e2eMs(b)!)
  if (!xs.length) return { min: 1000, max: 3000 }
  return { min: Math.min(...xs), max: Math.max(...xs) }
}

const MIN_BAR = 6
const MAX_BAR = 56

/**
 * Bar height for the cadence strip on a log curve between the domain's min and
 * max. Log spreads the clustered normal blocks apart while letting outliers —
 * and failures, which exceed max and clamp — tower above them.
 */
export const barHeight = (
  r: BlockRecord,
  domain: Domain,
  minBar = MIN_BAR,
  maxBar = MAX_BAR,
): number => {
  const e = e2eMs(r)
  if (e === null) return 18
  const { min, max } = domain
  if (max <= min) return maxBar
  const t = Math.log(e / min) / Math.log(max / min)
  return Math.round(minBar + (maxBar - minBar) * Math.min(1, Math.max(0, t)))
}

export type Cell = { kind: 'block'; r: BlockRecord } | { kind: 'gap' }

/**
 * Oldest→newest cells for the most recent `limit` blocks, inserting up to four
 * gap stubs per run of skipped slot numbers so the strip shows rhythm, not just
 * bars. A gap only means "no record for that slot" — the cause is unknown.
 */
export const buildCadence = (blocks: BlockRecord[], limit: number): Cell[] => {
  const recent = blocks.slice(0, limit).reverse()
  const cells: Cell[] = []
  for (let i = 0; i < recent.length; i++) {
    if (i > 0) {
      const missed = recent[i].slot - recent[i - 1].slot - 1
      for (let g = 0; g < Math.min(missed, 4); g++) cells.push({ kind: 'gap' })
    }
    cells.push({ kind: 'block', r: recent[i] })
  }
  return cells
}

export interface ProvingStats {
  fastest: BlockRecord
  slowest: BlockRecord
  median: number
}

/** Fastest, median, and slowest proving times across all completed blocks. */
export const provingStats = (blocks: BlockRecord[]): ProvingStats | null => {
  const done = blocks.filter((b) => b.outcome === 'complete')
  if (!done.length) return null
  const sorted = [...done].sort((a, b) => provingMs(a)! - provingMs(b)!)
  return {
    fastest: sorted[0],
    slowest: sorted[sorted.length - 1],
    median: provingMs(sorted[Math.floor(sorted.length / 2)])!,
  }
}
