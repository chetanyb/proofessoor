// Shapes returned by the requestor's HTTP API. Pure data, no runtime — shared
// by the data layer (api.ts) and the pure helpers (format.ts).

export type Outcome = 'sent' | 'complete' | 'failed'

export interface BlockRecord {
  slot: number
  beacon_block_root: string
  execution_block_number: number
  new_payload_request_root: string
  proof_types: string[]
  outcome: Outcome
  /** Short failure category (e.g. WitnessTimeout); present when failed. */
  reason: string | null
  /** Free-form failure detail; present when failed. */
  error: string | null
  observed_at_ms: number
  requested_at_ms: number
  resolved_at_ms: number | null
}

export interface StatusSummary {
  total: number
  sent: number
  complete: number
  failed: number
  latest_slot: number | null
}
