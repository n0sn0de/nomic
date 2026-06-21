# Research Log

Audit: full Nomic ibc-rs dependency path

Artifact directory: `audit/run-20260621T1619CDT-ibc-rs-full-5fd66a8/`

## Chronology

1. Fetched `origin/security-audit` and verified local `security-audit` matched `origin/security-audit` at `5fd66a8fb858a73eb2a22e997ac3dcb868470b04`.
2. Observed unrelated untracked local files and another untracked audit directory. I did not edit them.
3. Read `audit/audit-prompt.md` and focused the task on the Rust IBC stack (`ibc`, `ibc-proto`, `ibc-*`, `ics23`) rather than the spelling `ibs-rs`.
4. Searched repository IBC references with `rg`, then reviewed:
   - `Cargo.toml`
   - `Cargo.lock`
   - `rest/Cargo.toml`
   - `rest/Cargo.lock`
   - `src/app.rs`
   - `src/cosmos.rs`
   - `src/bin/nomic.rs`
   - `tests/ibc.rs`
   - `rest/src/main.rs`
5. Ran dependency resolution commands:
   - `cargo tree -i ibc --locked`
   - `cargo tree -i ibc-proto --locked`
   - `cargo metadata --locked --format-version 1`
   - `cargo tree --locked -e features -p nomic`
6. Identified the active Orga checkout from Cargo metadata:
   - `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a`
7. Reviewed Orga IBC integration:
   - `src/ibc/mod.rs`
   - `src/ibc/messages.rs`
   - `src/ibc/transfer.rs`
   - `src/ibc/impls.rs`
   - `src/ibc/router.rs`
8. Reviewed local ibc-rs crate sources for the callbacks Orga is expected to delegate to:
   - `ibc-app-transfer-0.54.0/src/module.rs`
   - `ibc-app-transfer-0.54.0/src/handler/mod.rs`
   - `ibc-core-channel-0.54.0/src/handler/acknowledgement.rs`
   - `ibc-core-connection-0.54.0/src/handler/conn_open_try.rs`
   - `ibc-core-connection-0.54.0/src/handler/conn_open_ack.rs`
9. Confirmed root resolved versions:
   - `ibc = 0.54.0`
   - `ibc-proto = 0.47.1`
   - all reachable `ibc-*` crates at `0.54.0`
   - `ics23 = 0.12.0`
   - Orga at `3b3d25ade40d81cb64f19335535e3a47bb47778f`
10. Confirmed `rest/Cargo.lock` divergence:
    - `rest/Cargo.lock` pins Orga at `35988d76b58008e37794064c41f3d0ba102ca0c8`
    - `cd rest && cargo metadata --locked --format-version 1` fails because the lockfile needs updating.
11. Ran build/test checks:
    - `cargo check --locked --lib --no-default-features`
    - `cargo test --locked --lib dest_json --no-default-features`
    - `cargo audit --version && cargo audit --file Cargo.lock`
12. Wrote findings and source anchors in this artifact directory only.

## Failed Attempts / Limitations

- `cargo check --locked --lib --no-default-features` failed because `src/incentives.rs` references the optional `csv` crate while the `csv` feature is disabled.
- `cargo test --locked --lib dest_json --no-default-features` failed before running tests because `openssl-sys` could not find OpenSSL via `pkg-config`, and `pkg-config` is not installed.
- `cargo audit` is not installed.
- `cd rest && cargo metadata --locked --format-version 1` failed because `rest/Cargo.lock` needs updating.
- I did not run ignored Hermes/bitcoind integration tests.
- I did not probe public Nomic infrastructure, public testnets, public relayers, or public RPCs.

## Verification Status

- Source review: complete for Nomic IBC entry points, Orga `feat-ibc`, ibc-rs transfer acknowledgement/timeout code, lockfiles, REST IBC query imports, and operator-key proof integration.
- Dependency version verification: complete for root lockfile. `rest/` verification is blocked by stale lockfile and recorded as a finding.
- Dynamic validation: partial. Root metadata and cargo tree worked. Build/test matrix did not pass due local dependency/configuration blockers.
- PoC/regression files: none added. Findings are source-anchored and defensive.

