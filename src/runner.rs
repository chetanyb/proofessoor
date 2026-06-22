//! Stream-mode orchestration.
//!
//! Consumes the Beacon API block event stream and, for each new non-optimistic
//! block, builds and submits a proof request — fire-and-forget, so submission
//! keeps pace with block arrival. A separate watcher task observes zkBoost's
//! proof events, records each outcome in the status registry, and optionally
//! downloads/verifies completed proofs. The daemon stops on SIGINT/SIGTERM.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures::StreamExt;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{info, warn};
use zkboost_client::ProofType;

use crate::beacon::{self, BlockEvent};
use crate::config::{BlockId, StreamArgs};
use crate::request;
use crate::status::{BlockRecord, JsonStatusStore, MemoryStatusStore, Outcome, StatusStore};
use crate::zkboost::{self, ProofEvent};

/// Delay before reconnecting after an event stream drops.
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

/// Runs stream mode: request proofs for new non-optimistic beacon blocks.
pub async fn run(args: StreamArgs) -> Result<()> {
    let beacon = Arc::new(beacon::Client::new(args.endpoints.beacon_rpc.clone())?);
    let zkboost = Arc::new(zkboost::Client::new(args.endpoints.zkboost_url.clone())?);
    let proof_types: Arc<Vec<ProofType>> = Arc::new(
        args.proof_types
            .iter()
            .map(|name| zkboost::parse_proof_type(name.as_str()))
            .collect::<Result<Vec<_>>>()?,
    );
    let semaphore = Arc::new(Semaphore::new(args.max_inflight));
    let artifacts = Arc::new(zkboost::Artifacts {
        download: args.download,
        verify: args.verify,
        out_dir: args.out_dir.clone(),
    });

    let store: Arc<dyn StatusStore> = match &args.state_dir {
        Some(dir) => {
            let store = JsonStatusStore::load(dir).await?;
            info!(
                state_dir = %dir.display(),
                latest_slot = ?store.latest_slot().await,
                "loaded request status from state directory"
            );
            Arc::new(store)
        }
        None => Arc::new(MemoryStatusStore::default()),
    };

    // Observe proof outcomes (and run artifact actions) independently of submission.
    let watcher = tokio::spawn(watch(zkboost.clone(), store.clone(), artifacts.clone()));

    let mut sigterm =
        signal(SignalKind::terminate()).context("failed to install SIGTERM handler")?;
    let mut tasks = JoinSet::new();

    info!(
        max_inflight = args.max_inflight,
        "streaming beacon block events"
    );

    'outer: loop {
        let mut events = Box::pin(beacon.subscribe_block_events());

        loop {
            let event = tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("received SIGINT, shutting down");
                    break 'outer;
                }
                _ = sigterm.recv() => {
                    info!("received SIGTERM, shutting down");
                    break 'outer;
                }
                event = events.next() => match event {
                    Some(Ok(event)) => event,
                    Some(Err(error)) => {
                        warn!(%error, "beacon event stream error; reconnecting");
                        break;
                    }
                    None => {
                        warn!("beacon event stream ended; reconnecting");
                        break;
                    }
                },
            };

            // Reap finished tasks so the join set does not grow unbounded.
            while tasks.try_join_next().is_some() {}

            if event.execution_optimistic {
                info!(slot = event.slot, "skipping optimistic block");
                continue;
            }

            // Bounded submission concurrency: wait for a free slot, but stay interruptible.
            let permit = tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("received SIGINT, shutting down");
                    break 'outer;
                }
                _ = sigterm.recv() => {
                    info!("received SIGTERM, shutting down");
                    break 'outer;
                }
                permit = semaphore.clone().acquire_owned() => {
                    permit.context("proof submission semaphore closed")?
                }
            };

            let beacon = beacon.clone();
            let zkboost = zkboost.clone();
            let proof_types = proof_types.clone();
            let store = store.clone();
            tasks.spawn(async move {
                let _permit = permit;
                if let Err(error) =
                    process_block(&beacon, &zkboost, &proof_types, &store, &event).await
                {
                    warn!(slot = event.slot, block = %event.block, %error, "block processing failed");
                }
            });
        }

        // Back off before reconnecting, but stay interruptible.
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("received SIGINT, shutting down");
                break 'outer;
            }
            _ = sigterm.recv() => {
                info!("received SIGTERM, shutting down");
                break 'outer;
            }
            _ = tokio::time::sleep(RECONNECT_DELAY) => {}
        }
    }

    info!(
        in_flight = tasks.len(),
        "stopping; draining in-flight submissions"
    );
    tasks.shutdown().await;
    watcher.abort();
    Ok(())
}

/// Fetches, builds, and submits the proof request for a single block event.
async fn process_block(
    beacon: &beacon::Client,
    zkboost: &zkboost::Client,
    proof_types: &[ProofType],
    store: &Arc<dyn StatusStore>,
    event: &BlockEvent,
) -> Result<()> {
    let block_id = BlockId::Root(event.block.to_string());
    let fetched = beacon.get_block(&block_id).await?;
    let payload_request = request::build(fetched.block())?;
    let local_root = request::root(&payload_request);
    let root_hex = local_root.to_string();

    // Skip blocks already requested (in this run or a previous one).
    if store.seen(&root_hex).await {
        info!(slot = fetched.slot(), root = %local_root, "request already recorded; skipping");
        return Ok(());
    }

    let server_root = zkboost.request_proof(&payload_request, proof_types).await?;
    if server_root != local_root {
        anyhow::bail!(
            "new_payload_request_root mismatch: local {local_root} != server {server_root}"
        );
    }

    store
        .record(BlockRecord::new(
            fetched.slot(),
            fetched.root().to_string(),
            payload_request.block_number(),
            root_hex,
            proof_types.iter().map(|p| p.as_str().to_string()).collect(),
        ))
        .await?;

    info!(
        slot = fetched.slot(),
        beacon_block_root = %fetched.root(),
        fork = %fetched.fork(),
        execution_block_number = payload_request.block_number(),
        new_payload_request_root = %server_root,
        "proof requested"
    );
    Ok(())
}

/// Observes proof events, recording outcomes and running artifact actions.
///
/// Reconnects after a transient stream drop; runs until aborted on shutdown.
async fn watch(
    zkboost: Arc<zkboost::Client>,
    store: Arc<dyn StatusStore>,
    artifacts: Arc<zkboost::Artifacts>,
) {
    loop {
        let mut events = Box::pin(zkboost.subscribe_proof_events());
        while let Some(event) = events.next().await {
            let event = match event {
                Ok(event) => event,
                Err(error) => {
                    warn!(%error, "proof event stream error; reconnecting");
                    break;
                }
            };
            if let Err(error) = handle_proof_event(&zkboost, &store, &artifacts, event).await {
                warn!(%error, "failed to handle proof event");
            }
        }
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

/// Records a single proof event's outcome and runs artifact actions on completion.
async fn handle_proof_event(
    zkboost: &zkboost::Client,
    store: &Arc<dyn StatusStore>,
    artifacts: &zkboost::Artifacts,
    event: ProofEvent,
) -> Result<()> {
    match event {
        ProofEvent::ProofComplete(complete) => {
            let root_hex = complete.new_payload_request_root.to_string();
            if !store.seen(&root_hex).await {
                return Ok(());
            }
            store.set_outcome(&root_hex, Outcome::Complete).await?;
            info!(root = %root_hex, proof_type = %complete.proof_type, "proof complete");
            if artifacts.needs_proof_bytes() {
                zkboost
                    .collect_artifacts(
                        complete.new_payload_request_root,
                        complete.proof_type,
                        artifacts,
                    )
                    .await?;
            }
        }
        ProofEvent::ProofFailure(failure) => {
            let root_hex = failure.new_payload_request_root.to_string();
            if !store.seen(&root_hex).await {
                return Ok(());
            }
            store.set_outcome(&root_hex, Outcome::Failed).await?;
            warn!(
                root = %root_hex,
                proof_type = %failure.proof_type,
                reason = ?failure.reason,
                error = %failure.error,
                "proof failed"
            );
        }
    }
    Ok(())
}
