# Phase 3 Runtime Harnesses

Run: `run-20260621T1626CDT-phase3-c5486a4`

Target source commit: `c5486a4a9b41de5474f991891ae22f734b1d5aec`

Branch HEAD when this phase began: `08e550075b50b2d68662dfa5c48dbe2885b325ad`

Pinned Orga source inspected: `3b3d25ade40d81cb64f19335535e3a47bb47778f`

## Scope

Phase 3 converted the strongest Phase 2 source-order models into audit-local Rust harnesses. The run stayed local: no Nomic mainnet/testnet, public Bitcoin, public IBC, explorer, validator, signer, relayer, RPC, or frontend probing was performed. One early `cargo test` attempt without `CARGO_NET_OFFLINE=true` updated crates.io/git dependency metadata after offline lockfile generation failed; that was dependency resolution only, not chain or service probing. Final verification commands were run offline.

Generated code is under:

- `harnesses/orga_persistence/`
- `harnesses/header_queue_probe/`

Final passing logs are:

- `results/orga-persistence-cargo-test-final-pass.out`
- `results/orga-persistence-cargo-test-final-pass.err`
- `results/header-queue-cargo-test-final-pass.out`
- `results/header-queue-cargo-test-final-pass.err`

## Confirmed: Failed `DeliverTx` Persists Prior Mutation

Cross-links: Phase 1 `P1-001`; Phase 2 `mutation_persistence_model.py`.

Status: runtime-confirmed against pinned Orga public APIs and an audit-local mirror of the private `InternalApp` source order.

Source anchors:

- Pinned Orga `InternalApp::run` executes the operation at `orga/src/abci/node.rs:482`, then flushes state and writes root bytes at `:483-486`, then returns `Ok(res)` at `:488`.
- Pinned Orga `deliver_tx` wraps the inner call result as data inside the closure at `orga/src/abci/node.rs:559-572`; only after `run` returns does it map an inner error to `code = 1` at `:574-595`.

Harness:

- `harnesses/orga_persistence/src/lib.rs`
- Test: `failed_deliver_tx_persists_mutation_in_audit_internal_app_source_order`

What it proves:

1. A toy Orga app increments `counter`.
2. The same call then returns `Err("boom after mutation")`.
3. The audit-local `deliver_tx` returns ABCI `code = 1`.
4. After flushing the nested Orga buffers, reloading state shows `counter == 1`.

The harness uses `ABCIPlugin`, Orga `State`, Orga `Call`, Orga `Store`, and nested `BufStore`/`MapStore` buffering. It mirrors the private `InternalApp` source order because the real type is not public. A Merk-backed first attempt was abandoned because Orga `merk-full` pulls system dependencies (`pkg-config`/OpenSSL and RocksDB bindgen) that were unnecessary for the invariant; the source-order property does not depend on RocksDB.

Final command:

```sh
CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/home/nitro/repos/nomic/target cargo test --manifest-path audit/run-20260621T1626CDT-phase3-c5486a4/harnesses/orga_persistence/Cargo.toml
```

Result:

```text
test failed_deliver_tx_persists_mutation_in_audit_internal_app_source_order ... ok
test result: ok. 1 passed; 0 failed
```

Security meaning:

Any Nomic path that mutates consensus state before a fallible validation or later fallible call must be treated as mutation-persistent even when the transaction is rejected. This promotes the Phase 1 architectural observation from source-inspected likely behavior to runtime-confirmed root cause.

## Confirmed: Rejected Header Replacement Mutates `HeaderQueue`

Cross-links: Phase 1 `P1-002`; Phase 2 `header_reorg_mutation_model.py`.

Status: runtime-confirmed against real `nomic::bitcoin::header_queue` types.

Source anchors:

- The function comment says invalid headers return an error and the queue "will not be modified" at `src/bitcoin/header_queue.rs:430-433`.
- `add_into_iter` calls `pop_back_to(first.height)` before validating the replacement headers at `src/bitcoin/header_queue.rs:445-458`.
- `verify_and_add_headers` can then reject the replacement for an incorrect previous block hash at `src/bitcoin/header_queue.rs:501-509`.
- `pop_back_to` pops headers in-place at `src/bitcoin/header_queue.rs:620-632`.

Harness:

- `harnesses/header_queue_probe/src/lib.rs`
- Test: `failed_same_height_replacement_truncates_queue_before_returning_error`

What it proves:

1. Configure a local `HeaderQueue` from the existing trusted height-42 fixture.
2. Add a valid height-43 Bitcoin header fixture.
3. Submit a same-height replacement with an invalid `prev_blockhash`.
4. `add_into_iter` returns an error containing `incorrect previous block hash`.
5. Despite the error, `queue.height()` is now `42`, proving the valid height-43 header was popped before validation failed.

Final command:

```sh
CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/home/nitro/repos/nomic/target cargo test --manifest-path audit/run-20260621T1626CDT-phase3-c5486a4/harnesses/header_queue_probe/Cargo.toml
```

Result:

```text
test failed_same_height_replacement_truncates_queue_before_returning_error ... ok
test result: ok. 1 passed; 0 failed
```

Security meaning:

This confirms the queue mutation independently of Orga persistence. Combined with the Orga failed-call result above, a rejected header relay transaction can persist a Bitcoin header queue rollback if the call path reaches `HeaderQueue::add_into_iter`.

## Confirmed: Rejected Replacement Leaves Stale Work Accounting

Cross-links: Phase 2 refinement of `P1-002`.

Status: runtime-confirmed against real `HeaderQueue` through public `WorkHeader.chain_work`.

Source anchors:

- `pop_back_to` returns removed work but does not subtract it from `self.current_work` at `src/bitcoin/header_queue.rs:620-632`.
- Successful header append derives new `chain_work` from `*self.current_work + header_work` at `src/bitcoin/header_queue.rs:522-525`.
- Pruning does subtract work at `src/bitcoin/header_queue.rs:473-475`, showing the omission in `pop_back_to` is not an unavoidable pattern.

Harness extension:

After the rejected replacement truncates height 43, the test re-adds the original valid height-43 header. The public `WorkHeader.chain_work` for the re-added header equals `trusted_work + header_43_work + header_43_work`, not the expected `trusted_work + header_43_work`.

Security meaning:

The rejected-reorg path can leave both an altered queue and inflated work state. Even after a later valid header repair, the chain-work score can be inflated relative to the actual queue contents. This strengthens Phase 2's model-confirmed lead into a direct Rust confirmation.

## Remaining Verification Limits

- The Orga persistence harness mirrors private `InternalApp` using public Orga components; it does not instantiate the private type directly.
- The Orga harness uses `MapStore` rather than `MerkStore`. The tested invariant is source order and flush behavior, not RocksDB persistence.
- The HeaderQueue harness is in-memory and does not execute the full signed Nomic transaction path through `InnerApp`, `SignerPlugin`, `NoncePlugin`, fee payment, or ABCI commit.
- No public chain data or external RPCs were used. Fixtures are local and synthetic/known test data.
- BeginBlock/EndBlock failed-hook persistence remains source-inspected but not separately runtime-harnessed in Phase 3.

## Suggested Phase 4

1. Add an app-level regression that drives `InnerApp` or the closest feasible Orga plugin chain through `bitcoin.headers.add`, proving a rejected relay transaction persists the queue mutation in the full call stack.
2. Prototype fixes:
   - validate header replacement on a clone or temporary queue before mutating live state;
   - make `pop_back_to` decrement `current_work` if it remains part of the mutation path;
   - consider transactional rollback in Orga for failed ABCI operations or an explicit "commit only on success" wrapper for consensus-critical calls.
3. Extend the same runtime approach to Phase 2's FROST stale-share model and deposit replay-poisoning lead.
