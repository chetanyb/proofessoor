//! Conversion from a beacon block to a zkBoost `NewPayloadRequest`.
//!
//! This is the protocol-critical step. It extracts the execution payload and,
//! for post-Deneb forks, the blob versioned hashes, parent beacon block root,
//! and execution requests, producing the owned SSZ type zkBoost accepts. The
//! `tree_hash_root` of that value is the `new_payload_request_root` zkBoost uses
//! to identify the request.

use anyhow::{Result, anyhow, bail};
use lighthouse_types::{
    BeaconBlockRef, EthSpec, Hash256, KzgCommitment, MainnetEthSpec, SignedBeaconBlock,
    VersionedHash,
};
use sha2::{Digest, Sha256};
use ssz_types::VariableList;
use zkboost_types::{
    Encode, NewPayloadRequest, NewPayloadRequestBellatrix, NewPayloadRequestCapella,
    NewPayloadRequestDeneb, NewPayloadRequestElectra, NewPayloadRequestFulu, TreeHash,
};

/// EIP-4844 versioned hash version byte for KZG commitments.
const VERSIONED_HASH_VERSION_KZG: u8 = 0x01;

/// Builds the zkBoost `NewPayloadRequest` for a decoded beacon block.
///
/// Returns an error for forks without an execution payload (pre-Bellatrix) and
/// for forks not yet supported by zkBoost (Gloas).
pub fn build(
    block: &SignedBeaconBlock<MainnetEthSpec>,
) -> Result<NewPayloadRequest<MainnetEthSpec>> {
    Ok(match block.message() {
        BeaconBlockRef::Base(_) | BeaconBlockRef::Altair(_) => {
            bail!("pre-Bellatrix blocks have no execution payload to prove")
        }
        BeaconBlockRef::Bellatrix(b) => NewPayloadRequest::Bellatrix(NewPayloadRequestBellatrix {
            execution_payload: b.body.execution_payload.execution_payload.clone(),
        }),
        BeaconBlockRef::Capella(b) => NewPayloadRequest::Capella(NewPayloadRequestCapella {
            execution_payload: b.body.execution_payload.execution_payload.clone(),
        }),
        BeaconBlockRef::Deneb(b) => NewPayloadRequest::Deneb(NewPayloadRequestDeneb {
            execution_payload: b.body.execution_payload.execution_payload.clone(),
            versioned_hashes: versioned_hashes(&b.body.blob_kzg_commitments)?,
            parent_beacon_block_root: b.parent_root,
        }),
        BeaconBlockRef::Electra(b) => NewPayloadRequest::Electra(NewPayloadRequestElectra {
            execution_payload: b.body.execution_payload.execution_payload.clone(),
            versioned_hashes: versioned_hashes(&b.body.blob_kzg_commitments)?,
            parent_beacon_block_root: b.parent_root,
            execution_requests: b.body.execution_requests.clone(),
        }),
        BeaconBlockRef::Fulu(b) => NewPayloadRequest::Fulu(NewPayloadRequestFulu {
            execution_payload: b.body.execution_payload.execution_payload.clone(),
            versioned_hashes: versioned_hashes(&b.body.blob_kzg_commitments)?,
            parent_beacon_block_root: b.parent_root,
            execution_requests: b.body.execution_requests.clone(),
        }),
        BeaconBlockRef::Gloas(_) => bail!("Gloas execution proofs are not yet supported"),
    })
}

/// The `new_payload_request_root` identifying the request.
pub fn root(request: &NewPayloadRequest<MainnetEthSpec>) -> Hash256 {
    request.tree_hash_root()
}

/// The length in bytes of the SSZ-encoded request body zkBoost receives.
pub fn ssz_len(request: &NewPayloadRequest<MainnetEthSpec>) -> usize {
    request.as_ssz_bytes().len()
}

/// Derives the blob versioned hashes from a block's KZG commitments.
fn versioned_hashes(
    commitments: &[KzgCommitment],
) -> Result<VariableList<VersionedHash, <MainnetEthSpec as EthSpec>::MaxBlobCommitmentsPerBlock>> {
    let hashes: Vec<VersionedHash> = commitments
        .iter()
        .map(kzg_commitment_to_versioned_hash)
        .collect();
    VariableList::new(hashes).map_err(|e| anyhow!("too many blob commitments: {e:?}"))
}

/// Computes the EIP-4844 versioned hash for a single KZG commitment.
fn kzg_commitment_to_versioned_hash(commitment: &KzgCommitment) -> VersionedHash {
    let mut hash: [u8; 32] = Sha256::digest(commitment.0).into();
    hash[0] = VERSIONED_HASH_VERSION_KZG;
    VersionedHash::from(hash)
}

#[cfg(test)]
mod tests {
    use lighthouse_types::{ForkName, ForkVersionDecode};

    use super::*;

    /// A real Fulu beacon block from the hoodi testnet (slot 3326688).
    const FULU_BLOCK: &[u8] = include_bytes!("../tests/fixtures/hoodi_block_3326688_fulu.ssz");

    /// Golden test: the recorded block must produce a stable request root and
    /// execution metadata, locking the borrowed-to-owned conversion against drift.
    #[test]
    fn builds_fulu_request_with_expected_root() {
        let block =
            SignedBeaconBlock::<MainnetEthSpec>::from_ssz_bytes_by_fork(FULU_BLOCK, ForkName::Fulu)
                .expect("fixture decodes as a fulu block");

        let request = build(&block).expect("builds a new payload request");

        assert_eq!(request.block_number(), 3067357);
        assert_eq!(request.gas_used(), 1981488);
        assert_eq!(
            root(&request).to_string(),
            "0xf1aaa504269559061901140a556e3dd10acafa0951e4b1b657fdf8cf2ed4fe27"
        );
    }
}
