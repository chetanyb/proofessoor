//! zkBoost HTTP API client.
//!
//! Wraps the subset of the zkBoost Proof Node API that `proofessoor` needs.
//! Responses are treated as untrusted: status codes are checked and bodies are
//! decoded into explicit types.

use std::collections::HashSet;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use futures::StreamExt;
use serde::Deserialize;
use url::Url;
use zkboost_client::{
    Hash256, MainnetEthSpec, NewPayloadRequest, ProofEvent, ProofType, zkBoostClient,
};

/// Default timeout applied to zkBoost HTTP requests.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Parses a proof type name (e.g. `reth-zisk`) into a zkBoost [`ProofType`].
pub fn parse_proof_type(name: &str) -> Result<ProofType> {
    name.parse()
        .map_err(|e| anyhow!("unknown proof type '{name}': {e}"))
}

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
    inner: zkBoostClient,
}

impl Client {
    /// Creates a client targeting the given zkBoost base URL.
    pub fn new(endpoint: Url) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .context("failed to build zkBoost HTTP client")?;
        let inner = zkBoostClient::with_http_client(endpoint.clone(), http.clone());
        Ok(Self {
            http,
            endpoint,
            inner,
        })
    }

    /// Submits a proof request and returns the `new_payload_request_root`
    /// computed by the server (`POST /v1/execution_proof_requests`).
    pub async fn request_proof(
        &self,
        request: &NewPayloadRequest<MainnetEthSpec>,
        proof_types: &[ProofType],
    ) -> Result<Hash256> {
        let response = self
            .inner
            .request_proof(request, proof_types)
            .await
            .context("zkBoost rejected the proof request")?;
        Ok(response.new_payload_request_root)
    }

    /// Waits for every requested proof to complete or fail, logging each result.
    ///
    /// Subscribes to the proof event stream filtered to `root`. Returns an error
    /// if any requested proof fails or the stream ends before all are resolved.
    pub async fn wait_for_proofs(&self, root: Hash256, proof_types: &[ProofType]) -> Result<()> {
        let mut events = Box::pin(self.inner.subscribe_proof_events(Some(root)));
        let mut remaining: HashSet<ProofType> = proof_types.iter().copied().collect();
        let mut failed: Vec<ProofType> = Vec::new();

        while !remaining.is_empty() {
            let Some(event) = events.next().await else {
                bail!("zkBoost proof event stream ended before all proofs resolved");
            };
            let event = event.context("error reading the zkBoost proof event stream")?;

            // Ignore events for proof types we did not request.
            if !remaining.remove(&event.proof_type()) {
                continue;
            }

            match event {
                ProofEvent::ProofComplete(complete) => {
                    tracing::info!(%root, proof_type = %complete.proof_type, "proof complete");
                }
                ProofEvent::ProofFailure(failure) => {
                    tracing::warn!(
                        %root,
                        proof_type = %failure.proof_type,
                        reason = ?failure.reason,
                        error = %failure.error,
                        "proof failed"
                    );
                    failed.push(failure.proof_type);
                }
            }
        }

        if failed.is_empty() {
            Ok(())
        } else {
            let names: Vec<&str> = failed.iter().map(ProofType::as_str).collect();
            bail!("proof generation failed for: {}", names.join(","))
        }
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
