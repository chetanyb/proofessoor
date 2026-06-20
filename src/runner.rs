//! Stream-mode orchestration.
//!
//! Consumes the Beacon API block event stream and, for each new non-optimistic
//! block, builds and submits a proof request. Concurrency is bounded by a
//! semaphore so requests cannot pile up faster than they drain, and the loop
//! shuts down gracefully on SIGINT/SIGTERM.

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
use crate::{request, zkboost};

/// Delay before reconnecting after the beacon event stream drops.
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

/// Runs stream mode: request proofs for new non-optimistic beacon blocks.
///
/// The beacon event stream is reconnected after a transient drop; the daemon
/// only stops on SIGINT/SIGTERM.
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
    let wait = args.wait;

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

            // Bounded concurrency: wait for a free slot, but stay interruptible.
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
                    permit.context("proof request semaphore closed")?
                }
            };

            let beacon = beacon.clone();
            let zkboost = zkboost.clone();
            let proof_types = proof_types.clone();
            tasks.spawn(async move {
                let _permit = permit;
                if let Err(error) =
                    process_block(&beacon, &zkboost, &proof_types, &event, wait).await
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

    // Submitted requests already reached zkBoost; abort any local in-flight work.
    info!(
        in_flight = tasks.len(),
        "stopping; aborting in-flight requests"
    );
    tasks.shutdown().await;
    Ok(())
}

/// Fetches, builds, and submits the proof request for a single block event.
async fn process_block(
    beacon: &beacon::Client,
    zkboost: &zkboost::Client,
    proof_types: &[ProofType],
    event: &BlockEvent,
    wait: bool,
) -> Result<()> {
    let block_id = BlockId::Root(event.block.to_string());
    let fetched = beacon.get_block(&block_id).await?;
    let payload_request = request::build(fetched.block())?;
    let local_root = request::root(&payload_request);

    let server_root = zkboost.request_proof(&payload_request, proof_types).await?;
    if server_root != local_root {
        anyhow::bail!(
            "new_payload_request_root mismatch: local {local_root} != server {server_root}"
        );
    }

    info!(
        slot = fetched.slot(),
        beacon_block_root = %fetched.root(),
        fork = %fetched.fork(),
        execution_block_number = payload_request.block_number(),
        new_payload_request_root = %server_root,
        "proof requested"
    );

    if wait {
        zkboost.wait_for_proofs(server_root, proof_types).await?;
    }
    Ok(())
}
