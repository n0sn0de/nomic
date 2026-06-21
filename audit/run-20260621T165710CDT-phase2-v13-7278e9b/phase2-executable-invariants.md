# Phase 2 Executable Invariants - v13.0.0 / 7278e9b

Audit run: `run-20260621T165710CDT-phase2-v13-7278e9b`

Target source of truth: `/home/nitro/repos/nomic-v13-audit-7278e9b`

Target commit: `7278e9b8d8a5550306cdb11980cb0d0c51fcfe4e`

Target tag: `v13.0.0`

Phase 2 turned the highest-priority v13 source leads into local executable models. These are deliberately small and source-anchored; they are not network probes and do not use public Nomic infrastructure.

## Model Inventory

- `models/header_queue_reorg_model.py`: models `HeaderQueue::add_into_iter`, `verify_and_add_headers`, and `pop_back_to` ordering.
- `models/deposit_outpoint_order_model.py`: models `relay_deposit` outpoint insertion before disabled-checkpoint failure.
- `models/ibc_transfer_failure_model.py`: models `IbcDest::transfer` mint-before-send and swallowed `deliver_message` error.
- `models/threshold_effective_power_model.py`: models present-voting-power threshold arithmetic for Bitcoin spend authority.
- `models/run_all.py`: runs all models.

## Verification Commands

- `python3 audit/run-20260621T165710CDT-phase2-v13-7278e9b/models/run_all.py`
- `python3 -m py_compile audit/run-20260621T165710CDT-phase2-v13-7278e9b/models/*.py`

Results:

- `results/model-run-all.txt`: exit status 0.
- `results/py-compile-models.txt`: exit status 0.

## Invariants And Results

### HQ-1: rejected reorg candidates must not mutate the committed header queue

Status: Confirmed failed invariant in the local model.

Source basis:

- `src/bitcoin/header_queue.rs` lines 445-459: existing suffix is popped before the candidate is verified/added and before work comparison.
- `src/bitcoin/header_queue.rs` lines 519-525: candidate headers increment `current_work`.
- `src/bitcoin/header_queue.rs` lines 620-632: popped headers are not subtracted from `current_work`.
- Pinned Orga `src/abci/node.rs` lines 489-495 and `src/abci/mod.rs` lines 220-227 indicate failed deliver calls can still flush state.

Model output:

- Rejected candidate returned `new best chain must include more work`.
- Height stayed at 103 after replacement suffix insertion.
- `current_work` changed from 130 to 132.
- Queue work sum was 112.
- `work_invariant_holds: False`.

Security meaning: a failed call can still replace header queue contents and desynchronize `current_work` from the queue.

### DEP-1: a failed disabled-checkpoint deposit relay must not consume the outpoint

Status: Likely failed invariant, pending exact checkpoint lifecycle reachability.

Source basis:

- `src/bitcoin/mod.rs` lines 603-610: outpoint inserted.
- `src/bitcoin/mod.rs` lines 612-616: deposits-disabled error returned after insertion.
- Failed-call persistence applies through pinned Orga.

Model output:

- First relay error: `Deposits are disabled for the given checkpoint`.
- `processed_after_first_error: [('txid', 0)]`.
- Retry error: `Output has already been relayed`.
- Credited amount: 0.

Security meaning: if callers can hit this state with an otherwise valid deposit, the deposit can be denied and made unrelayable.

### IBC-1: a failed outbound IBC send must not leave local minted balance without a packet

Status: Confirmed failed invariant in the local model.

Source basis:

- `src/app.rs` lines 1852-1855: local IBC transfer balance is minted.
- `src/app.rs` lines 1869-1873: `deliver_message` error is logged and not returned.

Model output:

- Outcome: `logged and swallowed: channel not found`.
- Sender IBC balance: 100.
- Packet sent: False.

Security meaning: local state can show minted outbound IBC funds even when no transfer packet was produced.

### THR-1: Bitcoin spend threshold should be expressed against total possible power or an explicit liveness assumption

Status: Likely economic/configuration risk.

Source basis:

- `src/bitcoin/signatory.rs` and `src/bitcoin/threshold_sig.rs` use present voting power and strict-greater-than threshold checks.

Model output examples:

- `possible=100 present=50 accepted=True required_present_to_spend=34 required_possible_pct=34.00%`.
- `possible=100 present=51 accepted=True required_present_to_spend=35 required_possible_pct=35.00%`.
- `possible=4 present=2 accepted=True required_present_to_spend=2 required_possible_pct=50.00%`.

Security meaning: if only the minimum signatory set is present, the effective Bitcoin spend authority can be far below two-thirds of total possible voting power.

## Limitations

- Models are minimized reproductions of source ordering, not full node simulations.
- Rust integration tests were left for Phase 3 and were blocked by the local `librocksdb-sys`/`stdbool.h` issue.
- The disabled-checkpoint deposit result remains Likely until a complete local checkpoint lifecycle proof shows that externally submitted deposits can reach `deposits_enabled == false` after all earlier validations pass.
