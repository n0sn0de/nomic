# Research Log

Run: `run-20260621T1244CDT-phase1-c5486a4`

Target source commit: `c5486a4a9b41de5474f991891ae22f734b1d5aec`

Started from successor context for prior `nomic_audit_deep_run`. Current `HEAD` was `188d30cb34cce8c8febfe93f35e0db1b29a92300`; `git diff --name-only c5486a4..HEAD` showed only prior audit artifacts under `audit/run-20260621T0354CDT-c5486a4/`.

## Inputs Read

- `audit/audit-prompt.md`
- `audit/run-20260621T0354CDT-c5486a4/audit-package.md`
- `audit/run-20260621T0354CDT-c5486a4/research-log.md`
- Core Nomic source files under `src/app.rs`, `src/bitcoin/*`, `src/cosmos.rs`, `src/frost/*`, `src/app/migrations.rs`, `build.rs`
- Pinned Orga checkout under `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a`
- Selected registry dependencies for IBC/ICS-23 proof paths where source search identified relevant types

## Source Areas Mapped

- App container and entrypoints: `src/app.rs`
- Bitcoin bridge: `src/bitcoin/mod.rs`
- Header queue: `src/bitcoin/header_queue.rs`
- Checkpoints/emergency disbursal: `src/bitcoin/checkpoint.rs`
- Signatory sets and threshold signatures: `src/bitcoin/signatory.rs`, `src/bitcoin/threshold_sig.rs`
- Expired deposit recovery transactions: `src/bitcoin/recovery.rs`
- IBC/Cosmos operator-key proofs: `src/cosmos.rs`
- FROST DKG/signing: `src/frost/mod.rs`, `src/frost/dkg.rs`, `src/frost/signing.rs`
- Upgrades/migrations: `src/app/migrations.rs`
- Orga plugins/persistence: `src/plugins/{mod,signer,nonce,payable}.rs`, `src/abci/node.rs`, `src/ibc/mod.rs`

## Confirmed Architectural Observations

- `InnerApp` holds the major security-critical modules: accounts/staking, Bitcoin bridge, IBC, upgrade, incentives, Cosmos auxiliary state, and feature-gated Ethereum/Babylon/FROST.
- The pinned Orga `DefaultPlugins` order is `Query -> SdkCompat -> Signer -> ChainCommitment -> Nonce -> Payable -> Fee -> InnerApp`.
- The pinned Orga ABCI wrapper flushes state after the operation closure. For `DeliverTx`, inner call errors are captured as values, then ABCI `code = 1` is set after state flushing.
- Bitcoin deposits are credited only after the checkpoint containing their input reaches Complete and `BeginBlock` drains pending transfers.
- Checkpoint creation and emergency disbursal generation are automatic `BeginBlock` work, not explicit user calls.
- IBC Nomic-specific logic burns incoming `usat` transfers with parseable `Dest` memos and inserts pending nBTC transfers; outgoing deposit destinations mint into the IBC transfer module before attempting `deliver_message`.
- `relay_op_key` manually verifies two ICS-23 membership proofs against a stored IBC consensus root, then stores a remote consensus-key to operator-key mapping.

## Prior Findings Cross-Linked

- `NOMIC-AUD-001`: deposit legacy commitment matching, linked to deposit lifecycle/TB-03.
- `NOMIC-AUD-002`: FROST participant selection, linked to FROST lifecycle/TB-14.
- `NOMIC-AUD-003`: no-default-features build failure, linked to upgrade/build assurance.
- `NOMIC-AUD-004`: deposit amount pre-validation, linked to deposit and direct Bitcoin destination lifecycles.
- `NOMIC-AUD-L001`: signatory quorum/threshold mismatch, linked to threshold-signing lifecycle/TB-08.
- `NOMIC-AUD-L002`: header retarget timestamp underflow, linked to Bitcoin header relay/TB-02.

## New Leads Logged

- `NOMIC-AUD-P1-001`: failed ABCI calls likely persist partial mutations due Orga flush behavior.
- `NOMIC-AUD-P1-002`: invalid/insufficient-work Bitcoin reorg can pop old headers or append partial replacement headers before returning error.
- `NOMIC-AUD-P1-003`: deposit outpoint can be marked processed before `deposits_enabled` rejection.
- `NOMIC-AUD-P1-004`: withdrawal debits account before withdrawal output validation.
- `NOMIC-AUD-P1-005`: FROST stale-iteration signature shares can mutate share counters before aggregation failure.
- `NOMIC-AUD-P1-006`: Cosmos `relay_op_key` mixes historical proof root with latest validator-set membership and does not check `Any.type_url`.

## Local Scripts or Models

No local scripts or executable models were created in Phase 1. Recommended Phase 2 work is to add focused local harnesses/regression tests for the leads above.

## Safety Notes

No probes were connected to public Nomic, Bitcoin, IBC, RPC, relayer, signer, validator, seed, explorer, or third-party systems. No real BTC, private keys, user data, crafted network packets, or external publication were used.
