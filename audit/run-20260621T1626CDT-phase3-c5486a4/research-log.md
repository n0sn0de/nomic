# Phase 3 Research Log

Run: `run-20260621T1626CDT-phase3-c5486a4`

## Chronology

1. Reviewed prior Phase 1/2 artifacts and current repository state. Current branch was `security-audit`; HEAD and `origin/security-audit` were both `08e550075b50b2d68662dfa5c48dbe2885b325ad`.
2. Created `harnesses/orga_persistence` to convert the Phase 2 failed-call persistence model into a Rust harness using pinned Orga APIs.
3. Initial offline lockfile generation failed because the standalone harness had no lockfile and the local sparse index was missing a crate entry. A subsequent non-offline `cargo test` updated dependency metadata and exposed a Rust/Cargo version mismatch with a floating dependency. The final harness uses copied/pinned lockfiles and final verification commands used `CARGO_NET_OFFLINE=true`.
4. First Orga harness design used `MerkStore` to resemble production storage. That pulled `merk-full`, which required `pkg-config`/OpenSSL and RocksDB bindgen support. Because the invariant is Orga source order and flush behavior, not RocksDB itself, the harness was reduced to nested `BufStore` over `MapStore`.
5. The Orga harness then passed: a failed `DeliverTx` returned `code = 1` but persisted a prior counter increment after buffer flush/reload.
6. Created `harnesses/header_queue_probe` using real `nomic::bitcoin::header_queue` types and the local height-42/43 fixtures already present in Nomic tests.
7. The HeaderQueue harness first proved rejected same-height replacement truncates the queue from height 43 to height 42.
8. Extended the HeaderQueue harness to re-add the valid height-43 header and prove `chain_work` is inflated by one extra header's work, exposing stale `current_work` after the rejected replacement.
9. Formatted both harness crates and reran final verification commands offline.

## Final Verification

```sh
CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/home/nitro/repos/nomic/target cargo test --manifest-path audit/run-20260621T1626CDT-phase3-c5486a4/harnesses/orga_persistence/Cargo.toml
```

Passed:

```text
test failed_deliver_tx_persists_mutation_in_audit_internal_app_source_order ... ok
```

```sh
CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/home/nitro/repos/nomic/target cargo test --manifest-path audit/run-20260621T1626CDT-phase3-c5486a4/harnesses/header_queue_probe/Cargo.toml
```

Passed:

```text
test failed_same_height_replacement_truncates_queue_before_returning_error ... ok
```

Toolchain:

```text
rustc 1.81.0-nightly (506985649 2024-07-20)
cargo 1.81.0-nightly (a2b58c3da 2024-07-16)
```

## Notes

- Intermediate build logs are retained under `results/` because they explain the harness evolution from Merk-backed to MapStore-backed persistence testing.
- The final harnesses do not require public chain connectivity and do not broadcast transactions or packets.
- The HeaderQueue result is a direct source-level behavior confirmation; the full Nomic app transaction route remains the next verification layer.
