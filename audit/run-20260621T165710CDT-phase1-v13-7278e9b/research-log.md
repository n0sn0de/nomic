# Research Log - Phase 1 v13.0.0 / 7278e9b

Target: `7278e9b8d8a5550306cdb11980cb0d0c51fcfe4e` / `v13.0.0`

Research was performed against `/home/nitro/repos/nomic-v13-audit-7278e9b` and pinned dependencies resolved by that checkout.

## Source Read Pass

- Read `audit/audit-prompt.md` and followed the authorized defensive constraints.
- Mapped primary application entrypoints in `src/app.rs`.
- Reviewed Bitcoin relay and checkpoint paths in `src/bitcoin/mod.rs`, `src/bitcoin/header_queue.rs`, `src/bitcoin/checkpoint.rs`, `src/bitcoin/outpoint_set.rs`, `src/bitcoin/signatory.rs`, and `src/bitcoin/threshold_sig.rs`.
- Reviewed FROST local signer paths in `src/frost/mod.rs`, `src/frost/dkg.rs`, `src/frost/signing.rs`, and `src/frost/signer.rs`.
- Reviewed Cosmos proof relay in `src/cosmos.rs`.
- Reviewed pinned Orga ABCI persistence behavior in `/home/nitro/.cargo/git/checkouts/orga-bc3514d9e56736a0/e539a53/src/abci`.

## Captured Evidence

All source snippets used in the main report are stored under `results/` with line numbers from the v13 target or exact pinned dependency source.

## Non-Actions

- Did not probe public Nomic systems, public testnets, public RPC endpoints, relayers, validators, or real assets.
- Did not modify the v13 target worktree source.
