# Phase 2 Executable Invariants

Run: `run-20260621T1610CDT-phase2-c5486a4`

Target source commit: `c5486a4a9b41de5474f991891ae22f734b1d5aec`

Current branch during run: `security-audit`

Current HEAD during run: `5fd66a8fb858a73eb2a22e997ac3dcb868470b04`

Scope note: `HEAD` contains audit artifacts beyond the authorized source target. Source analysis and source anchors continue to treat `c5486a4a9b41de5474f991891ae22f734b1d5aec` as the target application source unless explicitly discussing audit artifacts.

Safety: all work was local-only. No public Nomic, Bitcoin, IBC, RPC, relayer, signer, validator, frontend, explorer, or third-party infrastructure was contacted.

## Purpose

Phase 1 produced an architecture and trust map plus several candidate source-order hazards. Phase 2 turns the strongest candidates into small executable models that can be rerun and extended in future phases. These models are not full Nomic/Orga integration tests; they are source-order invariant traces designed to make the suspected state transitions concrete.

Generated code lives under:

```text
audit/run-20260621T1610CDT-phase2-c5486a4/models/
```

Captured outputs live under:

```text
audit/run-20260621T1610CDT-phase2-c5486a4/results/
```

## Verification Commands

Executed from `/home/nitro/repos/nomic`:

```bash
python3 audit/run-20260621T1610CDT-phase2-c5486a4/models/run_all.py \
  > audit/run-20260621T1610CDT-phase2-c5486a4/results/run-all-models.out \
  2> audit/run-20260621T1610CDT-phase2-c5486a4/results/run-all-models.err
```

Result: exit code 0. `results/run-all-models.err` is empty.

```bash
python3 -m py_compile audit/run-20260621T1610CDT-phase2-c5486a4/models/*.py \
  > audit/run-20260621T1610CDT-phase2-c5486a4/results/py-compile.out \
  2> audit/run-20260621T1610CDT-phase2-c5486a4/results/py-compile.err
```

Result: exit code 0. `results/py-compile.err` is empty.

## Model Inventory

| Model | Lead | Purpose |
| --- | --- | --- |
| `mutation_persistence_model.py` | P1-001 | Distinguish pinned Orga source-order persistence from transactional rollback. |
| `header_reorg_mutation_model.py` | P1-002 plus new P2 refinement | Model rejected Bitcoin reorg mutation and `current_work` accounting behavior. |
| `deposit_outpoint_poison_model.py` | P1-003 | Model disabled-deposit outpoint replay poisoning. |
| `withdrawal_debit_before_validation_model.py` | P1-004 | Model invalid withdrawal debit-before-validation. |
| `frost_stale_iteration_model.py` | P1-005 | Model stale FROST share counters poisoning current signing iteration. |
| `cosmos_relay_op_key_temporal_model.py` | P1-006 | Model temporal/type-url validation gap shape. |

The shared harness in `models/common.py` provides two execution modes:

- `source_order_deliver_tx`: commits mutated state even when the call returns a model error, matching the pinned Orga source order observed in `InternalApp::run` plus `deliver_tx`.
- `transactional_deliver_tx`: control model that rolls back on error.

## Results Summary

### P2-001: Failed ABCI Calls Persist Mutations in the Source-Order Model

Source anchors:

- Pinned Orga `InternalApp::run`: flush after operation result, `orga/src/abci/node.rs:482-486`
- Pinned Orga `deliver_tx`: inner result mapped to ABCI code after `run`, `orga/src/abci/node.rs:558-590`

Model result:

- Source-order failed call returned `code = 1` and committed `counter = 1`.
- Transactional control returned `code = 1` and kept `counter = 0`.

Interpretation: this model confirms the Phase 1 root hazard at the source-order level. It should be promoted to a minimal Orga runtime regression because it is the dependency behavior that turns later mutation-order issues into persistent state changes.

Status: model-confirmed, needs runtime harness confirmation.

### P2-002: Rejected Bitcoin Header Reorg Persists Alternate Tip and Inflates Work in the Source-Order Model

Source anchors:

- Reorg pop happens before validation: `src/bitcoin/header_queue.rs:455`
- Rejection happens after replacement verification/add: `src/bitcoin/header_queue.rs:458-463`
- Replacement append updates `current_work`: `src/bitcoin/header_queue.rs:522-525`
- `pop_back_to` returns removed work but does not decrement `current_work`: `src/bitcoin/header_queue.rs:620-632`

Model trace:

```text
Initial queue:
  hashes: h0 h1 h2 h3 h4 h5
  actual work: 60
  current_work: 60

Rejected replacement:
  alt4 work 1, prev h3
  alt5 work 1, prev alt4

Source-order committed queue after code=1:
  hashes: h0 h1 h2 h3 alt4 alt5
  actual queued work: 42
  current_work: 62

Transactional control after code=1:
  hashes: h0 h1 h2 h3 h4 h5
  actual work: 60
  current_work: 60
```

Interpretation: this strengthens P1-002. The original lead was mutation before validation. The Phase 2 model adds a second source-order issue: `pop_back_to` does not reduce `current_work`, so appended replacement headers are added on top of stale work. In the rejected-reorg case, failed-call persistence leaves both an alternate low-work tip and inflated `current_work`. Even successful reorgs should be checked for inflated work accounting.

Potential impact if reproduced in a real harness: corrupted Bitcoin best-chain view, incorrect finality decisions, deposit/checkpoint proof disruption, and possible acceptance of future low-work extensions as if they were heavier.

Status: strongest Phase 2 lead; model-confirmed source-order hazard, needs Rust regression against real `HeaderQueue`/Orga persistence.

### P2-003: Disabled Deposit Can Poison Processed Outpoint

Source anchors:

- Outpoint insertion: `src/bitcoin/mod.rs:606-613`
- `deposits_enabled` rejection after insertion: `src/bitcoin/mod.rs:615-619`

Model result:

- First relay with `deposits_enabled = false` returned `code = 1` and committed `processed_outpoints = ["txid:0"]`.
- Retry with `deposits_enabled = true` returned `code = 1` with `Output has already been relayed`.
- Transactional control allowed the enabled retry to succeed.

Interpretation: the model captures the replay-poisoning path exactly as Phase 1 described. A full app regression needs real checkpoint/deposit setup to determine reachable states and blast radius.

Status: model-confirmed, needs app-state regression.

### P2-004: Invalid Withdrawal Can Debit Before Validation

Source anchors:

- Account withdrawal before `add_withdrawal`: `src/bitcoin/mod.rs:740-751`
- Withdrawal validation inside `add_withdrawal`: `src/bitcoin/mod.rs:762-813`

Model result:

- Invalid withdrawal returned `code = 1`.
- Source-order committed balance changed from `100` to `60`, with no checkpoint output.
- Transactional control preserved balance `100`.

Interpretation: this remains primarily a self-loss or composed-call hazard unless a third party can induce a victim-signed invalid withdrawal or exploit interaction with payable/multicall flows. It is still important because it is a clean instance of mutation-before-validation.

Status: model-confirmed, likely lower direct severity than header/deposit paths.

### P2-005: Stale FROST Shares Can Poison Current Iteration

Source anchors:

- Timeout resets counters but leaves maps: `src/frost/signing.rs:65-70`
- Commitments check current iteration: `src/frost/signing.rs:101-123`
- Signature shares do not check current iteration: `src/frost/signing.rs:128-151`
- Aggregation filters current-iteration shares: `src/frost/signing.rs:196-205`

Model result:

- Prepared signing state was iteration 1, Round2, with iteration 1 signing package.
- Submitting stale iteration 0 shares returned `code = 1` during aggregate.
- Source-order committed state became `Complete` with stale shares `0:1` and `0:2`, no aggregate signature.
- A later current-iteration retry failed with `Not in round 2`.
- Transactional control rolled back stale shares and allowed current shares to complete.

Interpretation: under `frost,testnet` or any future production activation, stale signing material can make a signing task unrecoverable if failed-call persistence is real. The model does not validate FROST cryptography; it validates the Nomic state-machine ordering.

Status: model-confirmed for state-machine hazard, feature-gated.

### P2-006: `relay_op_key` Temporal and Type-URL Gap

Source anchors:

- Historical consensus root and latest validator-set membership: `src/cosmos.rs:151-176`
- `Any.value` decoded as secp256k1 without `type_url` check: `src/cosmos.rs:195-203`

Model result:

- The model accepted a proof at height 10 while checking validator membership at latest height 20.
- The model accepted an `Any.type_url` of `/cosmos.crypto.ed25519.PubKey` because type URL is not part of the modeled acceptance condition.

Interpretation: this remains a validation-gap model, not an exploit proof. The strongest concrete statement is that the code's temporal consistency invariant is unclear: proof root height and validator-set membership height differ. Type URL should be checked even if decode failures catch many malformed values.

Status: model-demonstrated hardening/correctness issue, needs remote-chain state model.

## Updated Finding Priority

1. Header queue reorg mutation plus `current_work` inflation: highest priority for Phase 3. This affects Bitcoin finality and bridge safety.
2. Orga failed-call persistence runtime confirmation: root dependency behavior needed to graduate model traces to confirmed vulnerabilities.
3. Deposit outpoint poisoning: likely bridge liveness/funds-lock risk if disabled-deposit states are reachable for valid deposits.
4. FROST stale iteration poisoning: high when FROST is enabled, but feature-gated in inspected source.
5. Withdrawal debit-before-validation: likely self-loss/composed-flow risk; still a clear invariant violation.
6. `relay_op_key` temporal/type-url gap: correctness/hardening until a concrete remote-chain exploit model exists.

## Generated Code Notes

The models deliberately duplicate only the security-relevant source order rather than importing Nomic internals. This keeps them stable, readable, and safe to run anywhere, but it means they are not a substitute for real Rust regressions.

The most important future reuse point is `models/common.py`: future phases can add more mutation-before-error models by providing a state object and comparing `source_order_deliver_tx` against `transactional_deliver_tx`.

## Verification Limits

- The models do not instantiate real Orga `InternalApp`, Nomic `InnerApp`, or Merkle stores.
- The models do not create real Bitcoin headers, transactions, scripts, merkle proofs, or FROST cryptographic packages.
- The header model mirrors observed source order and work arithmetic, but a Rust regression is required to confirm exact behavior with `Uint256`, Orga `Deque`, and real `WrappedHeader`.
- The deposit and withdrawal models do not prove full reachability of valid preconditions in live app state.
- The Cosmos model does not prove invalid proof acceptance; it only demonstrates temporal/type-url acceptance conditions that need a stronger remote-chain model.

## Suggested Phase 3

1. Build an audit-local Rust harness or test crate that instantiates pinned Orga `InternalApp` with a tiny mutate-then-error app and proves failed `DeliverTx` persistence.
2. Add a real `HeaderQueue` regression using synthetic/regtest headers or minimal valid fixtures. Assert both failed-reorg state persistence and `current_work` behavior across failed and successful reorgs.
3. Build an app-level deposit fixture that reaches a checkpoint with `deposits_enabled = false`, relays a valid matching output, and inspects `processed_outpoints` after failure.
4. Build an app-level withdrawal regression that funds a synthetic signer, submits invalid output script/amount, and checks account balance after ABCI `code = 1`.
5. Convert the FROST stale-iteration model into a feature-gated Rust unit test if the dependency types can be constructed locally without external services.
6. Extend the mutation-before-error sweep to `BeginBlock`, emergency disbursal construction, outgoing IBC destination transfer, payable call composition, and migrations.
