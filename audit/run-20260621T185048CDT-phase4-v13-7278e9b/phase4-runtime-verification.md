# Phase 4 Runtime Verification - v13.0.0 / 7278e9b

Audit run: `run-20260621T185048CDT-phase4-v13-7278e9b`

Target source of truth: `/home/nitro/repos/nomic-v13-audit-7278e9b`

Target commit: `7278e9b8d8a5550306cdb11980cb0d0c51fcfe4e`

Target tag: `v13.0.0`

This phase continues the v13 audit after the local `librocksdb-sys` issue was resolved. All commands were local to the target checkout. No probes were connected to public Nomic systems, public testnets, public RPCs, public relayers, public validators, or real assets.

## Purpose

Phase 0 through Phase 3 established source and model evidence for several leads, but Rust builds/tests were blocked by the local RocksDB/clang sysroot issue. Phase 4 reran the previously blocked build and test surface and added a temporary source-level regression test for the highest-priority Bitcoin header reorg atomicity finding.

## Target And Toolchain Record

Captured files:

- `results/target-git-rev-parse-head.txt`
- `results/target-git-rev-parse-v13-tag.txt`
- `results/target-git-status-short-before.txt`
- `results/target-git-status-short-after-regression-revert.txt`
- `results/target-git-status-short-final.txt`
- `results/rustc-version-verbose.txt`
- `results/cargo-version.txt`

The target and tag both resolve to `7278e9b8d8a5550306cdb11980cb0d0c51fcfe4e`. The final target worktree status was clean after the temporary regression patch was reverted.

## Build Matrix Update

The previous native dependency blocker is resolved for the main build profiles:

- `results/cargo-check-default.txt`: `cargo check` passed, exit status 0.
- `results/cargo-check-all-features.txt`: `cargo check --all-features` passed, exit status 0.
- `results/cargo-check-release-default.txt`: `cargo check --release` passed, exit status 0.
- `results/cargo-fmt-check.txt`: `cargo fmt --all -- --check` passed, exit status 0.

Remaining build/test blockers are now distinct from `librocksdb-sys`:

- `results/cargo-check-no-default-features.txt`: `cargo check --no-default-features` still fails in patched `merk` because `tree::Tree` does not implement `Debug` while `tree/link.rs` derives `Debug`.
- `results/cargo-test-all-features-no-run.txt`: `cargo test --all-features --no-run` fails compiling integration tests due mixed `bitcoin` crate versions and checked/unchecked address type mismatches.
- `results/cargo-clippy-workspace-all-targets-all-features.txt`: `cargo clippy --workspace --all-targets --all-features` reaches the same integration-test compile failures.

## Existing Tests

- `results/cargo-test-lib-header-queue-add-into-iterator.txt`: existing happy-path header queue unit filter passed.
- `results/cargo-test-lib-signatory-filter.txt`: existing signatory script parsing filter passed.
- `results/cargo-test-lib-threshold-filter.txt`: `cargo test --lib threshold` matched zero tests.
- `results/cargo-test-lib.txt`: unmodified `cargo test --lib` ran 55 tests and failed one test, `ethereum::consensus::relayer::tests::get_updates`, because it unwrapped an error decoding an external response body.
- `results/cargo-test-lib-skip-ethereum-consensus-relayer-get-updates.txt`: with that single external-response test skipped, the library suite passed: 48 passed, 6 ignored, 1 filtered.
- `results/cargo-test-header-queue-add-into-iterator.txt`: the non-`--lib` filter failed while compiling integration test targets, again due `bitcoin` crate/type drift.

## New Runtime Regression

Patch artifact:

- `patches/header-queue-rejected-lower-work-reorg-test.patch`

Command:

`cargo test --lib rejected_lower_work_reorg_preserves_queue_state -- --nocapture`

Captured output:

- `results/cargo-test-lib-rejected-lower-work-reorg-regression.txt`

Result:

- Exit status 101.
- The test deterministically failed with `rejected lower-work reorg changed queue height`.
- Actual height after rejected reorg: `1`.
- Expected height before rejected reorg: `2`.

The temporary test constructs a local low-difficulty header queue, adds a two-header best chain, then submits a one-header competing suffix starting at height 1. The suffix is validly connected and valid proof-of-work, but has lower total work than the removed two-header suffix, so `add_into_iter` returns the expected error. On v13.0.0, state is not restored after the error: the queue height changes from 2 to 1.

This elevates the Phase 1/2 header reorg lead from source/model evidence to a deterministic local Rust reproduction.

## Confirmed Finding: Rejected Lower-Work Reorg Mutates Header Queue State

Classification: Confirmed vulnerability.

Affected commit: `7278e9b8d8a5550306cdb11980cb0d0c51fcfe4e` / `v13.0.0`.

Affected source:

- `src/bitcoin/header_queue.rs::HeaderQueue::add_into_iter`
- `src/bitcoin/header_queue.rs::HeaderQueue::verify_and_add_headers`
- `src/bitcoin/header_queue.rs::HeaderQueue::pop_back_to`

Root cause:

- `add_into_iter` removes the old suffix with `pop_back_to(first.height)` before final cumulative-work comparison.
- `verify_and_add_headers` appends the candidate suffix and increments `current_work`.
- If `added_work <= removed_work`, the function returns an error after both mutations.
- `pop_back_to` does not subtract removed work from `current_work`.
- Pinned Orga failed-call handling recorded in Phase 1 can flush state after inner application errors, so this mutation ordering is consensus-relevant.

Violated invariants:

- C2: cumulative work must drive chain selection correctly.
- C3: reorganization logic must reject unsupported or lower-work reorgs safely.
- L2: state mutation before validation completion must not persist a partial transition.

Attacker model:

- An untrusted header relayer submits a validly connected but lower-work competing header suffix at a height already present in the queue.

Deterministic reproduction:

- Apply the saved patch to the v13 target.
- Run `cargo test --lib rejected_lower_work_reorg_preserves_queue_state -- --nocapture`.
- Observe the assertion failure captured in `results/cargo-test-lib-rejected-lower-work-reorg-regression.txt`.

Expected result:

- The function returns an error and the queue height, tip, and cumulative work remain unchanged.

Actual result:

- The function returns an error, but queue height changes from 2 to 1. The Phase 2 model additionally showed cumulative work accounting diverging from queued work.

Security impact:

- A rejected Bitcoin fork can still alter Nomic's accepted header queue. Depending on when the failed call is delivered and flushed, this can desynchronize Bitcoin header state used by deposit confirmation, reorg handling, and downstream reserve accounting.

Minimal remediation:

- Validate the candidate suffix and compare work against the old suffix before mutating the live queue, or perform the operation on a cloned/staged queue and commit only after all checks pass.
- When removing headers, subtract removed work from `current_work` or recompute `current_work` from the resulting queue.
- Add the Phase 4 regression test, or an equivalent test, to the native suite.

## Test Gaps Confirmed By Phase 4

- The existing header queue unit tests cover happy-path append and invalid target, but not rejected lower-work reorg atomicity.
- The integration test suite does not currently compile under the resolved v13 dependency graph because test code mixes `bitcoin` 0.29 and 0.32 types through `bitcoind`/`bitcoincore-rpc`.
- One library test reaches an external Ethereum consensus response and is not hermetic; this is inconsistent with the local-only audit strategy.
- Threshold arithmetic currently has no directly named `threshold` test matched by `cargo test --lib threshold`.

## Limitations

- The Phase 4 runtime regression was added temporarily to the detached target worktree and reverted after capture; only the patch and result output are committed as audit artifacts.
- Full integration test execution remains blocked until test dependency type drift is fixed.
- No live chain, public RPC, public relayer, validator, or real asset was used.
