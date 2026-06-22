//! Request-status model and its persistent store.
//!
//! The [`StatusStore`] trait is a narrow, swappable interface: stream mode
//! records each proof request and its outcome, deduplicates already-requested
//! roots across restarts, and exposes the latest processed slot. The default
//! [`JsonStatusStore`] keeps the state in memory and snapshots it to a JSON
//! file; a different backend (SQLite, etc.) can be dropped in behind the trait.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// Outcome of a recorded proof request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Submitted to zkBoost; outcome not yet known.
    Sent,
    /// All requested proofs completed.
    Complete,
    /// At least one requested proof failed.
    Failed,
}

/// A recorded proof request for one beacon block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockRecord {
    /// Slot of the beacon block.
    pub slot: u64,
    /// Beacon block root (0x-hex).
    pub beacon_block_root: String,
    /// Execution block number.
    pub execution_block_number: u64,
    /// The `new_payload_request_root` identifying the request (0x-hex).
    pub new_payload_request_root: String,
    /// Proof types requested.
    pub proof_types: Vec<String>,
    /// Latest known outcome.
    pub outcome: Outcome,
    /// Unix milliseconds when the request was submitted.
    pub requested_at_ms: u64,
    /// Unix milliseconds when the request resolved (completed or failed), if it has.
    #[serde(default)]
    pub resolved_at_ms: Option<u64>,
}

impl BlockRecord {
    /// Creates a record in the [`Outcome::Sent`] state, stamped with the current time.
    pub fn new(
        slot: u64,
        beacon_block_root: String,
        execution_block_number: u64,
        new_payload_request_root: String,
        proof_types: Vec<String>,
    ) -> Self {
        Self {
            slot,
            beacon_block_root,
            execution_block_number,
            new_payload_request_root,
            proof_types,
            outcome: Outcome::Sent,
            requested_at_ms: now_ms(),
            resolved_at_ms: None,
        }
    }
}

/// A narrow, swappable interface for persisting request status.
#[async_trait]
pub trait StatusStore: Send + Sync {
    /// Whether a request for this `new_payload_request_root` is already recorded.
    async fn seen(&self, root: &str) -> bool;

    /// Records (or replaces) a request record.
    async fn record(&self, record: BlockRecord) -> Result<()>;

    /// Updates the outcome of a recorded request, returning its request-to-resolution
    /// duration in milliseconds if the record existed.
    async fn set_outcome(&self, root: &str, outcome: Outcome) -> Result<Option<u64>>;

    /// The highest slot recorded so far, if any.
    async fn latest_slot(&self) -> Option<u64>;
}

/// In-memory status store that snapshots to a JSON file in a state directory.
#[derive(Debug)]
pub struct JsonStatusStore {
    path: PathBuf,
    state: Mutex<State>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    /// Records keyed by `new_payload_request_root`.
    records: HashMap<String, BlockRecord>,
}

impl State {
    fn seen(&self, root: &str) -> bool {
        self.records.contains_key(root)
    }

    fn insert(&mut self, record: BlockRecord) {
        self.records
            .insert(record.new_payload_request_root.clone(), record);
    }

    fn set_outcome(&mut self, root: &str, outcome: Outcome) -> Option<u64> {
        let record = self.records.get_mut(root)?;
        let now = now_ms();
        record.outcome = outcome;
        record.resolved_at_ms = Some(now);
        Some(now.saturating_sub(record.requested_at_ms))
    }

    fn latest_slot(&self) -> Option<u64> {
        self.records.values().map(|r| r.slot).max()
    }
}

impl JsonStatusStore {
    /// Loads (or initializes) the store from `state_dir/status.json`.
    pub async fn load(state_dir: &Path) -> Result<Self> {
        tokio::fs::create_dir_all(state_dir)
            .await
            .with_context(|| format!("failed to create state dir {}", state_dir.display()))?;
        let path = state_dir.join("status.json");

        let state = match tokio::fs::read(&path).await {
            Ok(bytes) => serde_json::from_slice(&bytes).context("failed to parse status.json")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => State::default(),
            Err(error) => {
                return Err(error).context("failed to read status.json");
            }
        };

        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    /// Atomically writes the current state to disk (temp file + rename).
    async fn persist(&self, state: &State) -> Result<()> {
        let json = serde_json::to_vec_pretty(state).context("failed to serialize status")?;
        let tmp = self.path.with_extension("json.tmp");
        tokio::fs::write(&tmp, &json)
            .await
            .context("failed to write status snapshot")?;
        tokio::fs::rename(&tmp, &self.path)
            .await
            .context("failed to commit status snapshot")?;
        Ok(())
    }
}

#[async_trait]
impl StatusStore for JsonStatusStore {
    async fn seen(&self, root: &str) -> bool {
        self.state.lock().await.seen(root)
    }

    async fn record(&self, record: BlockRecord) -> Result<()> {
        let mut state = self.state.lock().await;
        state.insert(record);
        self.persist(&state).await
    }

    async fn set_outcome(&self, root: &str, outcome: Outcome) -> Result<Option<u64>> {
        let mut state = self.state.lock().await;
        let duration = state.set_outcome(root, outcome);
        self.persist(&state).await?;
        Ok(duration)
    }

    async fn latest_slot(&self) -> Option<u64> {
        self.state.lock().await.latest_slot()
    }
}

/// In-memory status store without persistence (used when no state dir is set).
#[derive(Debug, Default)]
pub struct MemoryStatusStore {
    state: Mutex<State>,
}

#[async_trait]
impl StatusStore for MemoryStatusStore {
    async fn seen(&self, root: &str) -> bool {
        self.state.lock().await.seen(root)
    }

    async fn record(&self, record: BlockRecord) -> Result<()> {
        self.state.lock().await.insert(record);
        Ok(())
    }

    async fn set_outcome(&self, root: &str, outcome: Outcome) -> Result<Option<u64>> {
        Ok(self.state.lock().await.set_outcome(root, outcome))
    }

    async fn latest_slot(&self) -> Option<u64> {
        self.state.lock().await.latest_slot()
    }
}

/// Current Unix time in milliseconds.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(slot: u64, root: &str) -> BlockRecord {
        BlockRecord::new(
            slot,
            "0xbeacon".to_string(),
            slot - 1,
            root.to_string(),
            vec!["reth-zisk".to_string()],
        )
    }

    #[tokio::test]
    async fn records_dedup_and_latest_slot() {
        let dir = std::env::temp_dir().join("proofessoor_status_test_dedup");
        let _ = tokio::fs::remove_dir_all(&dir).await;

        let store = JsonStatusStore::load(&dir).await.expect("load");
        assert!(!store.seen("0xa").await);
        store.record(record(100, "0xa")).await.expect("record");
        store.record(record(105, "0xb")).await.expect("record");

        assert!(store.seen("0xa").await);
        assert!(!store.seen("0xc").await);
        assert_eq!(store.latest_slot().await, Some(105));

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn persists_and_reloads_across_restart() {
        let dir = std::env::temp_dir().join("proofessoor_status_test_reload");
        let _ = tokio::fs::remove_dir_all(&dir).await;

        let store = JsonStatusStore::load(&dir).await.expect("load");
        store.record(record(200, "0xroot")).await.expect("record");
        store
            .set_outcome("0xroot", Outcome::Complete)
            .await
            .expect("set outcome");

        // Reload as if after a restart.
        let reloaded = JsonStatusStore::load(&dir).await.expect("reload");
        assert!(reloaded.seen("0xroot").await);
        assert_eq!(reloaded.latest_slot().await, Some(200));

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
