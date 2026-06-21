# Phase 3 Runtime Harnesses - v13.0.0 / 7278e9b

Audit run: `run-20260621T165710CDT-phase3-v13-7278e9b`

Target source of truth: `/home/nitro/repos/nomic-v13-audit-7278e9b`

Target commit: `7278e9b8d8a5550306cdb11980cb0d0c51fcfe4e`

Target tag: `v13.0.0`

Phase 3 built local harnesses around the highest-priority Phase 2 leads. No harness connects to public Nomic systems, public testnets, public RPCs, relayers, validators, or real assets.

## Harness Inventory

- `harnesses/source_anchor_regression_checks.py`: source-anchored assertions that the target still has the risky orderings identified in Phase 1 and Phase 2.
- `harnesses/header_queue_reorg_regression_test.rs`: proposed Rust unit-test skeleton for the rejected-reorg mutation. This file is an artifact only and was not applied to the target source tree.
- `harnesses/README.md`: brief usage notes.

## Commands And Results

### Source-anchor harness

Command:

`python3 audit/run-20260621T165710CDT-phase3-v13-7278e9b/harnesses/source_anchor_regression_checks.py`

Result:

- `results/source-anchor-regression-checks.txt`: exit status 0.
- Output: `source anchor regression checks passed`.

The source-anchor harness verifies all of the following against the v13 target path:

- `HeaderQueue::add_into_iter` calls `pop_back_to` before `verify_and_add_headers`.
- `pop_back_to` accumulates removed work but does not write `self.current_work`.
- `relay_deposit` inserts into `processed_outpoints` before checking `deposits_enabled`.
- `IbcDest::transfer` mints local IBC balance before swallowing a failed `deliver_message`.

### Rust test attempts

Command:

`cargo test header_queue::test::add_into_iterator -- --nocapture`

Result:

- `results/cargo-test-header-queue-add-into-iterator.txt`: exit status 101.
- Build failed in `librocksdb-sys v0.16.0+8.10.0` because clang could not find `stdbool.h` from `rocksdb/include/rocksdb/c.h`.

Command:

`cargo test threshold -- --nocapture`

Result:

- `results/cargo-test-threshold-filter.txt`: exit status 101.
- Build failed with the same `librocksdb-sys` / `stdbool.h` issue.

## Harness Findings

### Confirmed: risky source ordering is present in the exact v13 target

Status: Confirmed.

The Python harness passed against `/home/nitro/repos/nomic-v13-audit-7278e9b`, confirming that the source orderings modeled in Phase 2 are present in `7278e9b8d8a5550306cdb11980cb0d0c51fcfe4e`.

### Confirmed limitation: Rust runtime tests are blocked by local native dependency state

Status: Confirmed environment limitation.

Both targeted cargo test attempts failed before executing Rust tests because `librocksdb-sys` could not generate RocksDB bindings. This is the same failure seen in Phase 0. The proposed Rust regression harness remains unexecuted in this artifact set.

## Recommended Regression Tests

Add a native test under the target source tree when the RocksDB/native build environment is fixed:

- Build a header queue with a trusted suffix.
- Submit a lower-work competing suffix beginning at an existing height.
- Assert that the call returns an error.
- Assert that queue contents and `current_work` are unchanged after the error.

The artifact `harnesses/header_queue_reorg_regression_test.rs` sketches this test shape.
