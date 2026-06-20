//! `proofessoor` — a minimal, clientless execution-proof requestor for zkBoost.
//!
//! The binary parses and validates the CLI, initializes logging, and dispatches
//! to the requested subcommand.

mod beacon;
mod config;
mod request;
mod runner;
mod status;
mod zkboost;

use anyhow::{Context, Result};
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::config::{CheckArgs, Cli, Command, RequestArgs};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(&cli.log_level)?;

    match cli.command {
        Command::Request(args) => run_request(args).await,
        Command::Stream(args) => runner::run(args).await,
        Command::Check(args) => run_check(args).await,
    }
}

/// Initializes the global tracing subscriber.
///
/// `RUST_LOG` takes precedence; otherwise the `--log-level` value is used.
fn init_tracing(log_level: &str) -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(log_level))
        .with_context(|| format!("invalid log level '{log_level}'"))?;

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    Ok(())
}

/// Handles the `request` subcommand.
///
/// Fetches the requested beacon block, builds the zkBoost payload request,
/// submits it for proving, and reports the resulting `new_payload_request_root`.
async fn run_request(args: RequestArgs) -> Result<()> {
    let artifacts = zkboost::Artifacts {
        download: args.download,
        verify: args.verify,
        out_dir: args.out_dir.clone(),
    };
    if artifacts.needs_proof_bytes() && !args.wait {
        anyhow::bail!("--download, --verify, and --out-dir require --wait");
    }

    let beacon = beacon::Client::new(args.endpoints.beacon_rpc.clone())?;
    let zkboost = zkboost::Client::new(args.endpoints.zkboost_url.clone())?;
    let proof_types = args
        .proof_types
        .iter()
        .map(|name| zkboost::parse_proof_type(name.as_str()))
        .collect::<Result<Vec<_>>>()?;

    let block = beacon.get_block(&args.block_id).await?;
    let payload_request = request::build(block.block())?;
    let local_root = request::root(&payload_request);

    let server_root = zkboost
        .request_proof(&payload_request, &proof_types)
        .await?;

    // The server recomputes the root from the SSZ body we sent; a mismatch means
    // our encoding disagrees with zkBoost's and the request is not what we built.
    if server_root != local_root {
        anyhow::bail!(
            "new_payload_request_root mismatch: local {local_root} != server {server_root}"
        );
    }

    tracing::info!(
        slot = block.slot(),
        beacon_block_root = %block.root(),
        fork = %block.fork(),
        execution_block_hash = %payload_request.block_hash(),
        execution_block_number = payload_request.block_number(),
        new_payload_request_root = %server_root,
        request_bytes = request::ssz_len(&payload_request),
        proof_types = %render_proof_types(&args.proof_types),
        "proof requested"
    );

    if args.wait {
        zkboost
            .wait_for_proofs(server_root, &proof_types, &artifacts)
            .await?;
    }
    Ok(())
}

/// Handles the `check` subcommand.
///
/// Confirms zkBoost is reachable, reports the provable proof types, and fails
/// if any requested proof type is not available.
async fn run_check(args: CheckArgs) -> Result<()> {
    let client = zkboost::Client::new(args.zkboost_url.clone())?;
    let available = client.proof_types().await?;

    let provable: Vec<&str> = available
        .iter()
        .filter(|info| info.can_prove)
        .map(|info| info.proof_type.as_str())
        .collect();

    tracing::info!(
        zkboost_url = %args.zkboost_url,
        provable = %provable.join(","),
        "zkBoost reachable"
    );

    if args.proof_types.is_empty() {
        return Ok(());
    }

    let missing: Vec<&str> = args
        .proof_types
        .iter()
        .map(config::ProofTypeName::as_str)
        .filter(|name| !provable.contains(name))
        .collect();

    if missing.is_empty() {
        tracing::info!(
            requested = %render_proof_types(&args.proof_types),
            "all requested proof types are available"
        );
        Ok(())
    } else {
        anyhow::bail!(
            "requested proof types not available on zkBoost: {}",
            missing.join(",")
        )
    }
}

/// Renders proof types as a comma-separated string for logging.
fn render_proof_types(proof_types: &[config::ProofTypeName]) -> String {
    proof_types
        .iter()
        .map(config::ProofTypeName::as_str)
        .collect::<Vec<_>>()
        .join(",")
}
