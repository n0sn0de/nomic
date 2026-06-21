# Phase 1 Architecture And Trust Map - v13.0.0 / 7278e9b

Audit run: `run-20260621T165710CDT-phase1-v13-7278e9b`

Target source of truth: `/home/nitro/repos/nomic-v13-audit-7278e9b`

Target commit: `7278e9b8d8a5550306cdb11980cb0d0c51fcfe4e`

Target tag: `v13.0.0`

This trust map was re-derived from the v13.0.0 target. Older `c5486a4` audit text was not reused as authority.

## Source Anchors Captured

- `results/source-header-queue-reorg.txt`: `src/bitcoin/header_queue.rs` lines 434-632.
- `results/source-bitcoin-relay-deposit.txt`: `src/bitcoin/mod.rs` lines 522-671.
- `results/source-app-try-credit-dest.txt`: `src/app.rs` lines 526-584.
- `results/source-app-ibc-dest-transfer.txt`: `src/app.rs` lines 1838-1873.
- `results/source-cosmos-relay-op-key.txt`: `src/cosmos.rs` lines 132-214.
- `results/source-frost-signer-secret-nonce.txt`: `src/frost/signer.rs` lines 34-105 and 463-589.
- `results/source-orga-abci-run-deliver-flush.txt`: pinned Orga `src/abci/node.rs` lines 465-605.
- `results/source-orga-abci-node-bufstore-flush.txt`: pinned Orga `src/abci/mod.rs` lines 216-240.

## System Map

The v13 application is a single Orga/ABCI app with these high-risk trust boundaries:

- Bitcoin header relay and proof relay: callers submit Bitcoin headers, merkle proofs, deposit transactions, checkpoint indexes, and destination encodings. Nomic trusts local header queue state and proof verification before crediting nBTC or creating recovery transactions.
- Checkpoint and signatory set management: on-chain state drives checkpoint signing, deposit windows, threshold scripts, validator voting power snapshots, fee rates, and withdrawal batching.
- FROST signing: local signer processes manage communication keys, DKG key packages, signing nonces, commitments, and signature shares on disk and through chain calls.
- IBC receive and send: local ABCI calls deliver raw IBC messages, route incoming ICS-20 transfers into pending Bitcoin credits, and send nBTC over IBC by minting/burning/escrowing through Orga's IBC transfer module.
- Cosmos proof relay: `relay_op_key` accepts IBC client id, height, consensus key, and storage proofs that bind Cosmos consensus keys to operator account keys.
- Ethereum/Babylon feature surfaces: enabled by default through `babylon`, `frost`, and `ethereum-full`, increasing build and dependency surface even when specific paths were not fully exercised in this phase.

## Failed-Call Persistence Boundary

Pinned Orga is important for v13 threat modeling. `InternalApp::run` calls the closure, then flushes state back to the store when the mutex can be recovered, regardless of whether the closure returned an inner application error. `deliver_tx` converts those inner errors to `ResponseDeliverTx { code: 1 }`. The ABCI wrapper then flushes the `BufStore` after `app.deliver_tx(...)` returns.

Source anchors:

- Orga `src/abci/node.rs` lines 489-495 flush state after `op(&state)`.
- Orga `src/abci/node.rs` lines 565-605 maps inner errors to failed deliver responses.
- Orga `src/abci/mod.rs` lines 220-227 flushes the buffered store after `deliver_tx`.

Consequence: any Nomic call that mutates state before returning an error must be treated as persistent unless a more local rollback mechanism is identified.

## Findings And Leads

### P1 - Confirmed: rejected Bitcoin reorg candidates mutate header queue state

`HeaderQueue::add_into_iter` pops the existing suffix at or above the replacement height before verifying and adding the candidate suffix. It then rejects the call if `added_work <= removed_work`. `pop_back_to` returns removed work but does not subtract that work from `current_work`; `verify_and_add_headers` appends candidate headers and increments `current_work`.

Anchors:

- `src/bitcoin/header_queue.rs` lines 445-459: pop, then verify/add, then compare.
- `src/bitcoin/header_queue.rs` lines 519-525: `current_work` is incremented while headers are pushed.
- `src/bitcoin/header_queue.rs` lines 620-632: popped work is accumulated but not subtracted from `current_work`.
- Orga source above confirms failed deliver calls can persist prior mutations.

Impact: a lower-work competing header suffix can return an error while still replacing the in-memory/persisted header suffix and inflating `current_work`. This can desynchronize Bitcoin header work accounting and downstream deposit confirmation decisions. Phase 2 models reproduce the invariant break.

### P2 - Likely: disabled-deposit relay attempts can poison the processed-outpoint set

`relay_deposit` checks merkle proof, output script, amount, and duplicate outpoint, then inserts the outpoint into `processed_outpoints` before checking `checkpoint.deposits_enabled`.

Anchors:

- `src/bitcoin/mod.rs` lines 603-610: duplicate check and outpoint insertion.
- `src/bitcoin/mod.rs` lines 612-616: disabled checkpoint check occurs after insertion.
- Orga failed-call persistence boundary applies to this ordering.

Impact: if an otherwise valid deposit can be relayed against a checkpoint with `deposits_enabled == false`, the failed call can still mark the outpoint as processed and block a later successful relay. Reachability depends on checkpoint lifecycle timing; normal deposit relay appears intended for enabled checkpoints, so this remains Likely rather than fully proven.

### P2 - Confirmed: IBC destination transfer mints before send and swallows send failure

`IbcDest::transfer` deducts any fee, mints a local IBC balance to `sender_address`, builds `MsgTransfer`, calls `ibc.deliver_message`, logs any error at debug level, and returns `Ok(())`.

Anchors:

- `src/app.rs` lines 1852-1855: local mint.
- `src/app.rs` lines 1857-1868: transfer message construction.
- `src/app.rs` lines 1869-1873: `deliver_message` error is logged and swallowed.

Impact: if `deliver_message` fails before burn/escrow/packet commitment, the caller path can still complete while local IBC balance remains minted and no outbound packet is produced. This is a confirmed state-machine hazard. Phase 2 includes an executable local model.

### P2 - Likely: failed pending-credit destinations can strand deposits with `Identity::None`

`try_credit_dest` refunds only `Identity::NativeAccount` and `Identity::EthAccount` on failed credit. Deposits from Bitcoin and incoming IBC memo transfers insert pending credits with `Identity::None`.

Anchors:

- `src/app.rs` lines 526-584: failed-credit handling and refund match.
- `src/app.rs` lines 538-543: source comment notes state mutation after `credit_dest`.
- `src/bitcoin/mod.rs` line 669: Bitcoin deposits insert pending with `Identity::None`.
- `src/app.rs` lines 725-727: incoming IBC memo credits insert pending with `Identity::None`.

Impact: if a destination validates initially but fails during later pending-credit execution, funds can be neither credited nor refunded. Further path-specific proof is needed for exact trigger conditions.

### P3 - Likely: Cosmos operator-key relay mixes proof height with latest validator-set membership

`relay_op_key` fetches the consensus root for the supplied height, but checks the consensus key against `client.last_header()?.validator_set`. It then verifies `staking` and `acc` proofs against the older root. It also decodes `Any.value` as a secp256k1 public key without checking `Any.type_url`.

Anchors:

- `src/cosmos.rs` lines 151-157: root from supplied consensus height.
- `src/cosmos.rs` lines 161-173: validator membership from latest header.
- `src/cosmos.rs` lines 175-200: proof verification and `Any.value` decode without type-url check.

Impact: stale or mismatched proofs may be accepted if a consensus key is present in the latest validator set but proofs are taken from a different height. A complete local proof needs a modeled client history and proof corpus.

### P3 - Confirmed defense-in-depth issue: FROST secret store writes plaintext values to a WAL and retains signing nonces

`SecretStore::put` appends both key and value bytes to `secret_store.wal`. Signing commitments store nonces by `(group_index, sig_index, iteration, participant)`, and signing reads them later without deleting them.

Anchors:

- `src/frost/signer.rs` lines 65-70: append-only WAL file open.
- `src/frost/signer.rs` lines 98-104: `put` logs key and value bytes.
- `src/frost/signer.rs` lines 486-497: signing nonces are stored.
- `src/frost/signer.rs` lines 556-565: signing nonces are read for signing; no deletion follows in lines 566-589.

Impact: local filesystem compromise, backups, or support bundles can expose sensitive signer material and signing nonces. This is not a remote exploit by itself, but it is a meaningful operational risk for validators/signers.

### P3 - Likely economic/configuration risk: threshold arithmetic can permit low total-power Bitcoin spend authority

Signatory-set quorum and Bitcoin spend threshold use present voting power and floor division. The path accepts a set at `present_vp >= possible_vp / 2`, then requires signatures strictly greater than `present_vp * threshold / possible_vp`.

Impact: for `possible_vp = 100`, `present_vp = 50`, and a two-thirds threshold, the generated script can require 34% of total possible voting power to spend. This may be intended as a liveness tradeoff, but it is a sharp trust assumption.

## Phase 1 Limitations

- Architecture review was source-based and local only.
- Full Rust runtime tests were deferred to Phase 3 and were blocked by the local RocksDB/clang sysroot issue described in Phase 0.
- Some leads are marked Likely where source ordering is clear but a fully minimized end-to-end local proof was not completed.
