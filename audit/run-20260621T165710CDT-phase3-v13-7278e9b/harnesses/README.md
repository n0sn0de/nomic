# Phase 3 Harnesses

Target: `7278e9b8d8a5550306cdb11980cb0d0c51fcfe4e` / `v13.0.0`.

- `source_anchor_regression_checks.py` is a local source-anchored harness that fails if the specific vulnerable ordering for the highest-priority leads changes.
- `header_queue_reorg_regression_test.rs` is a proposed Rust unit test skeleton for `src/bitcoin/header_queue.rs`; it is not applied to the target checkout because this artifact branch is committing audit artifacts only.

Runtime commands captured under `results/`:

- `python3 harnesses/source_anchor_regression_checks.py`
- `cargo test header_queue::test::add_into_iterator -- --nocapture`
- `cargo test threshold -- --nocapture`
