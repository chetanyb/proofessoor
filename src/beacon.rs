//! Beacon API client.
//!
//! Fetches beacon blocks as SSZ and decodes them using the fork named by the
//! `Eth-Consensus-Version` response header. Responses are treated as untrusted:
//! status, headers, and body are all validated before use.

use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use lighthouse_types::{ForkName, ForkVersionDecode, Hash256, MainnetEthSpec, SignedBeaconBlock};
use url::Url;

use crate::config::BlockId;

/// Default timeout applied to Beacon API requests.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Header carrying the consensus fork used to encode an SSZ beacon block.
const CONSENSUS_VERSION_HEADER: &str = "Eth-Consensus-Version";

/// A beacon block fetched from the Beacon API, tagged with its decoded fork.
#[derive(Debug)]
pub struct FetchedBlock {
    fork: ForkName,
    block: SignedBeaconBlock<MainnetEthSpec>,
}

impl FetchedBlock {
    /// The consensus fork the block was decoded as.
    pub fn fork(&self) -> ForkName {
        self.fork
    }

    /// The block's slot.
    pub fn slot(&self) -> u64 {
        self.block.slot().as_u64()
    }

    /// The block root (`hash_tree_root` of the beacon block message).
    pub fn root(&self) -> Hash256 {
        self.block.canonical_root()
    }

    /// The decoded signed beacon block.
    pub fn block(&self) -> &SignedBeaconBlock<MainnetEthSpec> {
        &self.block
    }
}

/// HTTP client for the Beacon API.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    endpoint: Url,
}

impl Client {
    /// Creates a client targeting the given Beacon API base URL.
    pub fn new(endpoint: Url) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .context("failed to build Beacon API HTTP client")?;
        Ok(Self { http, endpoint })
    }

    /// Fetches a beacon block by identifier (`GET /eth/v2/beacon/blocks/{block_id}`).
    pub async fn get_block(&self, block_id: &BlockId) -> Result<FetchedBlock> {
        let path = format!("/eth/v2/beacon/blocks/{}", block_id.to_path_segment());
        let url = self
            .endpoint
            .join(&path)
            .context("failed to construct beacon block URL")?;

        let response = self
            .http
            .get(url)
            .header(reqwest::header::ACCEPT, "application/octet-stream")
            .send()
            .await
            .context("failed to reach the Beacon API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("Beacon API returned {status} for block {block_id}: {body}");
        }

        let fork_name: ForkName = response
            .headers()
            .get(CONSENSUS_VERSION_HEADER)
            .ok_or_else(|| {
                anyhow!("Beacon API response missing {CONSENSUS_VERSION_HEADER} header")
            })?
            .to_str()
            .context("Eth-Consensus-Version header was not valid text")?
            .parse()
            .map_err(|error: String| anyhow!("unknown consensus version: {error}"))?;

        let bytes = response
            .bytes()
            .await
            .context("failed to read the beacon block body")?;

        let block = SignedBeaconBlock::from_ssz_bytes_by_fork(&bytes, fork_name)
            .map_err(|e| anyhow!("failed to decode beacon block as {fork_name}: {e:?}"))?;

        Ok(FetchedBlock {
            fork: fork_name,
            block,
        })
    }
}
