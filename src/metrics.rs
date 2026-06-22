//! Optional Prometheus metrics and health endpoints.
//!
//! Metric names live here as constants; the runner and watcher emit them via the
//! `metrics` facade. When no metrics address is configured the recorder is not
//! installed and the emit calls are cheap no-ops.

use std::net::SocketAddr;

use anyhow::{Context, Result};
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

/// Total non-optimistic blocks observed.
pub const BLOCKS_OBSERVED: &str = "proofessoor_blocks_observed_total";
/// Total blocks skipped (optimistic or already requested).
pub const BLOCKS_SKIPPED: &str = "proofessoor_blocks_skipped_total";
/// Total proof requests submitted to zkBoost.
pub const PROOF_REQUESTS: &str = "proofessoor_proof_requests_total";
/// Total proof requests that failed to submit.
pub const PROOF_REQUEST_FAILURES: &str = "proofessoor_proof_request_failures_total";
/// Total proofs that completed.
pub const PROOF_COMPLETIONS: &str = "proofessoor_proof_completions_total";
/// Total proofs that failed.
pub const PROOF_FAILURES: &str = "proofessoor_proof_failures_total";
/// Currently outstanding proof jobs (submitted, unresolved; one per proof type).
pub const INFLIGHT_REQUESTS: &str = "proofessoor_inflight_requests";
/// Highest slot for which a proof was requested.
pub const LATEST_REQUESTED_SLOT: &str = "proofessoor_latest_requested_slot";
/// Highest slot observed from the beacon event stream.
pub const LATEST_SEEN_SLOT: &str = "proofessoor_latest_seen_slot";
/// Slots between the latest seen block and the latest requested one.
pub const HEAD_LAG: &str = "proofessoor_head_lag_slots";
/// Time spent fetching, building, and submitting a request.
pub const REQUEST_DURATION: &str = "proofessoor_proof_request_duration_seconds";
/// Per-stage time within the request path (labeled by stage: fetch, ssz_decode, build, submit).
pub const REQUEST_STAGE_DURATION: &str = "proofessoor_request_stage_duration_seconds";
/// Time from request submission to proof completion (labeled by proof_type).
pub const COMPLETION_DURATION: &str = "proofessoor_proof_completion_duration_seconds";

/// Histogram buckets in seconds, spanning sub-millisecond CPU work to multi-minute proving.
const DURATION_BUCKETS: &[f64] = &[
    0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
    120.0, 300.0,
];

/// Installs the Prometheus recorder, returning a handle for rendering `/metrics`.
///
/// Histograms use explicit buckets so they render as Prometheus histograms (aggregatable
/// across instances) rather than client-side summaries.
pub fn install() -> Result<PrometheusHandle> {
    PrometheusBuilder::new()
        .set_buckets(DURATION_BUCKETS)
        .context("failed to configure metric buckets")?
        .install_recorder()
        .context("failed to install the Prometheus recorder")
}

/// Serves `/health` and `/metrics` on `addr` until the serving task is cancelled.
pub async fn serve(addr: SocketAddr, handle: PrometheusHandle) -> Result<()> {
    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(render))
        .with_state(handle);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind metrics listener on {addr}"))?;
    axum::serve(listener, app)
        .await
        .context("metrics server error")
}

async fn health() -> impl IntoResponse {
    StatusCode::OK
}

async fn render(State(handle): State<PrometheusHandle>) -> impl IntoResponse {
    handle.render()
}
