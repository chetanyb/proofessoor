//! zkBoost HTTP API client.
//!
//! Wraps the subset of the zkBoost Proof Node API that `proofessoor` needs.
//! Responses are treated as untrusted: status codes are checked and bodies are
//! decoded into explicit types.

use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;
use url::Url;

/// Default timeout applied to zkBoost HTTP requests.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Capabilities of a single proof type advertised by a zkBoost server.
///
/// Only the fields `proof_type` validation needs are decoded; the server may
/// send additional fields (`kind`, `can_verify`), which are ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct ProofTypeInfo {
    /// The proof type identifier (e.g. `reth-zisk`). Encoded on the wire as a string.
    pub proof_type: String,
    /// Whether the server can generate this proof.
    pub can_prove: bool,
}

/// Response body for `GET /v1/proof_types`.
#[derive(Debug, Deserialize)]
struct ProofTypesResponse {
    proof_types: Vec<ProofTypeInfo>,
}

/// HTTP client for the zkBoost Proof Node API.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    endpoint: Url,
}

impl Client {
    /// Creates a client targeting the given zkBoost base URL.
    pub fn new(endpoint: Url) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .context("failed to build zkBoost HTTP client")?;
        Ok(Self { http, endpoint })
    }

    /// Fetches the proof types advertised by the server (`GET /v1/proof_types`).
    pub async fn proof_types(&self) -> Result<Vec<ProofTypeInfo>> {
        let url = self
            .endpoint
            .join("/v1/proof_types")
            .context("failed to construct proof_types URL")?;

        let response = self
            .http
            .get(url)
            .send()
            .await
            .context("failed to reach zkBoost")?
            .error_for_status()
            .context("zkBoost returned an error status for GET /v1/proof_types")?;

        let body: ProofTypesResponse = response
            .json()
            .await
            .context("failed to decode the proof_types response")?;

        Ok(body.proof_types)
    }
}
