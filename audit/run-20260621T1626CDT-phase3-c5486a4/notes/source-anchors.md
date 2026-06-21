# Source Anchors

Target source commit: `c5486a4a9b41de5474f991891ae22f734b1d5aec`

Pinned Orga dependency: `3b3d25ade40d81cb64f19335535e3a47bb47778f`

## Orga Failed-Call Persistence

File: `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/abci/node.rs`

- `InternalApp::run` starts at line 458.
- The operation is called at line 482.
- State is flushed and root bytes are written at lines 483-486.
- `run` returns the operation result at line 488.
- `deliver_tx` starts at line 558.
- `deliver_tx` captures the inner call result inside the `run` closure at lines 559-572.
- `deliver_tx` maps inner errors to ABCI `code = 1` at lines 582-589.

## HeaderQueue Mutation Before Validation

File: `src/bitcoin/header_queue.rs`

- Invalid header comment says the queue will not be modified at lines 430-433.
- `add_into_iter` starts at line 434.
- Same-height or lower replacements call `pop_back_to(first.height)` at line 455.
- Replacement validation begins only after the pop at line 458.
- Previous-hash validation fails at lines 506-509.
- Appends update `self.current_work` from stale state at lines 522-525.
- `pop_back_to` pops headers at lines 620-632 and does not update `self.current_work`.
- Pruning subtracts removed work at lines 473-475.

## Prior Audit Cross-Links

- Phase 1 root cause: `audit/run-20260621T1244CDT-phase1-c5486a4/phase1-architecture-trust-map.md`, `P1-001`.
- Phase 1 HeaderQueue lead: same report, `P1-002`.
- Phase 2 executable models: `audit/run-20260621T1610CDT-phase2-c5486a4/phase2-executable-invariants.md`.
- Phase 2 model files:
  - `audit/run-20260621T1610CDT-phase2-c5486a4/models/mutation_persistence_model.py`
  - `audit/run-20260621T1610CDT-phase2-c5486a4/models/header_reorg_mutation_model.py`
