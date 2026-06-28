# proofessoor + zkboost + ere (GPU)

A self-contained Docker Compose stack: **proofessoor** (requestor + dashboard) →
**zkboost** (proof coordinator + dashboard) → **ere-server-zisk** (real ZisK
prover on the GPUs).

## Layout

One proof type (`reth-zisk`) proven across **4 GPUs by a single ere-server**
(devices `0-3`). To use more proof types, add another `ere-server-*` service and
a second `[[zkvm]]` block pointing at it (give it different `device_ids`).

## Prerequisites

- NVIDIA drivers + **NVIDIA Container Toolkit** (so Docker can reserve GPUs).
- An **EL RPC** that serves `debug_executionWitnessByBlockHash` for the blocks
  you prove (e.g. a hoodi reth supernode). Put it in `zkboost.toml` → `el_endpoint`.
  A rate-limited public RPC will cause `WitnessTimeout` failures.

## Run

```bash
# 1. set el_endpoint in zkboost.toml
# 2. point at your beacon API: export BEACON_RPC=<your beacon RPC>

# published proofessoor image (set PROOFESSOOR_IMAGE or edit the default in the file):
docker compose up -d

# or build proofessoor from source:
docker compose -f docker-compose.yml -f docker-compose.local.yml up -d
```

| What | URL |
| --- | --- |
| proofessoor dashboard | http://localhost:9100 |
| zkboost dashboard | http://localhost:3000/dashboard |

## Notes

- **First boot is slow.** `ERE_ZISK_SETUP_ON_INIT=1` makes the prover precompute
  setup before it serves proofs — expect several minutes. The stack stays up while
  it boots: zkBoost retries witness fetches within a request, but proofessoor does
  not re-submit a failed block — it just keeps requesting new ones. So early blocks
  may fail or queue until the prover is ready.
- Image versions match zkboost `v0.8.0` (ere/ere-guests `v0.12.1`). If you bump
  zkboost, re-check the pinned ere version in zkboost's `Cargo.toml`.
- By default `proofessoor` runs the published image (set `PROOFESSOOR_IMAGE`);
  add `-f docker-compose.local.yml` to build it from source instead.
