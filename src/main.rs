//! `proofessoor` — a minimal, clientless execution-proof requestor for zkBoost.
//!
//! The binary parses and validates the CLI, initializes logging, and dispatches
//! to the requested subcommand.

mod beacon;
mod config;
mod zkboost;

use anyhow::{Context, Result};
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::config::{CheckArgs, Cli, Command, RequestArgs, StreamArgs};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(&cli.log_level)?;

    match cli.command {
        Command::Request(args) => run_request(args).await,
        Command::Stream(args) => run_stream(args),
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
/// Fetches the requested beacon block and reports its metadata.
async fn run_request(args: RequestArgs) -> Result<()> {
    let beacon = beacon::Client::new(args.endpoints.beacon_rpc.clone())?;
    let block = beacon.get_block(&args.block_id).await?;

    tracing::info!(
        slot = block.slot(),
        beacon_block_root = %block.root(),
        fork = %block.fork(),
        proof_types = %render_proof_types(&args.proof_types),
        zkboost_url = %args.endpoints.zkboost_url,
        "fetched beacon block"
    );
    Ok(())
}

/// Handles the `stream` subcommand.
fn run_stream(args: StreamArgs) -> Result<()> {
    let proof_types = render_proof_types(&args.proof_types);
    tracing::info!(
        beacon_rpc = %args.endpoints.beacon_rpc,
        zkboost_url = %args.endpoints.zkboost_url,
        max_inflight = args.max_inflight,
        proof_types = %proof_types,
        "stream: configuration parsed"
    );
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
