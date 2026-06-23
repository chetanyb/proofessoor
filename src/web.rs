//! HTTP server: health, Prometheus metrics, and the dashboard API.
//!
//! Serves the requestor's view of the status registry for the dashboard, kept
//! distinct from zkBoost's own proving dashboard.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use metrics_exporter_prometheus::PrometheusHandle;
use serde::Serialize;

use crate::status::{Outcome, StatusStore};

/// Shared state for the HTTP handlers.
#[derive(Clone)]
struct AppState {
    metrics: PrometheusHandle,
    store: Arc<dyn StatusStore>,
}

/// Serves health, metrics, and the dashboard API on `addr` until the task is cancelled.
pub async fn serve(
    addr: SocketAddr,
    metrics: PrometheusHandle,
    store: Arc<dyn StatusStore>,
) -> Result<()> {
    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(render_metrics))
        .route("/api/blocks", get(blocks))
        .route("/api/status", get(status))
        .with_state(AppState { metrics, store });

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind HTTP listener on {addr}"))?;
    axum::serve(listener, app)
        .await
        .context("HTTP server error")
}

async fn health() -> impl IntoResponse {
    StatusCode::OK
}

async fn render_metrics(State(state): State<AppState>) -> impl IntoResponse {
    state.metrics.render()
}

/// Returns all recorded block requests (newest slot first).
async fn blocks(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.store.records().await)
}

/// A summary of recorded request outcomes for the dashboard tiles.
#[derive(Serialize)]
struct StatusSummary {
    total: usize,
    sent: usize,
    complete: usize,
    failed: usize,
    latest_slot: Option<u64>,
}

async fn status(State(state): State<AppState>) -> impl IntoResponse {
    let records = state.store.records().await;
    let mut summary = StatusSummary {
        total: records.len(),
        sent: 0,
        complete: 0,
        failed: 0,
        latest_slot: records.first().map(|record| record.slot),
    };
    for record in &records {
        match record.outcome {
            Outcome::Sent => summary.sent += 1,
            Outcome::Complete => summary.complete += 1,
            Outcome::Failed => summary.failed += 1,
        }
    }
    Json(summary)
}
