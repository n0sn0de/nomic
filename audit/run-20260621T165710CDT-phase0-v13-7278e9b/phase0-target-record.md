# Phase 0 Target Record - v13.0.0 / 7278e9b

Audit run: `run-20260621T165710CDT-phase0-v13-7278e9b`

Target source of truth: `/home/nitro/repos/nomic-v13-audit-7278e9b`

Target commit: `7278e9b8d8a5550306cdb11980cb0d0c51fcfe4e`

Target tag: `v13.0.0`

This phase was redone from the v13.0.0 target worktree. The previous `c5486a4` artifacts were not used as source of truth. No probes were connected to public Nomic systems, public testnets, public RPCs, public relayers, public validators, or real assets.

## Recorded Identity

- `results/git-rev-parse-head.txt`: target HEAD is `7278e9b8d8a5550306cdb11980cb0d0c51fcfe4e`.
- `results/git-rev-parse-v13-tag.txt`: `v13.0.0^{commit}` resolves to the same commit.
- `results/git-tags-points-at-head.txt`: includes `v13.0.0`.
- `results/git-status-short.txt`: target worktree status was captured before analysis.
- Package metadata identifies the root crate as `nomic` version `13.0.0` with Rust edition `2021`.

## Toolchain

- `results/rustc-version-verbose.txt`: `rustc 1.81.0-nightly (506985649 2024-07-20)`.
- `results/cargo-version.txt`: `cargo 1.81.0-nightly (a2b58c3da 2024-07-16)`.
- `results/tool-availability.txt`: `jq` and `rg` were available; `cargo-audit`, `cargo-deny`, `cargo-geiger`, and `cargo-vet` were not available in this environment.

## Workspace And Features

Captured files:

- `results/cargo-metadata-format1.json` and `results/cargo-metadata-format1.stderr`
- `results/nomic-package-metadata.json`
- `results/workspace-members.json`
- `results/cargo-locate-project.txt`
- `results/manifest-lock-toolchain-listing.txt`
- `results/manifest-lock-toolchain-sha256.txt`

Important feature facts from the v13 metadata:

- Default features: `full`, `babylon`, `frost`, `ethereum-full`.
- `full` enables Bitcoin RPC, CLI, Tokio, Orga `merk-full`, Orga ABCI, Orga state sync, CSV, Warp, Reqwest, Tendermint RPC, Cosmos SDK proto, and `home`.
- `testnet` enables `signet`.
- `devnet` is declared but empty.
- `ethereum-full` pulls in the Ethereum/Alloy/Helios stack.
- `legacy-bin` is optional and has build-script behavior that can run `git fetch --tags --force` and `build.sh` when enabled.

Profiles and panic/overflow settings:

- `Cargo.toml` has `[profile.release] overflow-checks = true`.
- No explicit panic strategy was found in the searched manifests/config paths.

## Dependency Surface

Captured files:

- `results/cargo-tree-all-features.txt`
- `results/cargo-tree-duplicates.txt`
- `results/git-dependencies.tsv`
- `results/patch-git-source-search.txt`

Pinned git dependencies recorded from Cargo metadata:

- `abci2` from `turbofish-org/abci2` rev `27d8d7a5a6e458f5881e3eb13116cb2a3ae049b5`.
- `orga` and `orga-macros` from `turbofish-org/orga` rev `e539a53e1eed9c3363062d419ea84d8d0d56e1c7`.
- `ed` and `ed-derive` from `turbofish-org/ed` rev `a657be856792039ff60c2f67e7920e38cd3acffc`.
- `frost-core`, `frost-rerandomized`, and `frost-secp256k1-tr` from `ZcashFoundation/frost` rev `51fa7d09f3742563a35d065afcff6ad486430dac`.
- `merk` patched to `nomic-io/merk` rev `ece89c3adac49da4f6f202874964df18563b5c5d`.
- `consensus-core` from `a16z/helios` rev/tag `0.7.0`, resolved to `545d809be0f135f69a8e6f613bb6bdd0fb4b22d1`.
- `pretty_env_logger` from `seanmonstar/pretty-env-logger` rev `f9e35b6dbbf06de55222c944c9e1e176ce73b3a7`.

Notable duplicate dependency families in `cargo tree --duplicates` include Alloy 0.2/0.8 families, Cosmos SDK proto 0.19/0.23, Tendermint 0.32/0.38/0.40, multiple `prost` versions, multiple `http`/`hyper` stacks, and many macro/support crates. The full duplicate tree is in `results/cargo-tree-duplicates.txt`.

## Build Scripts, Protobuf, And Generated Code

Captured files:

- `results/build-rs-files.txt`
- `results/build-rs-source.txt`
- `results/proto-files-maxdepth3.txt`
- `results/proto-generated-files.txt`
- `results/rg-files.txt`

The root `build.rs` always records the git branch in `GIT_BRANCH`. With `legacy-bin`, it can read network configs, run `git fetch --tags --force`, pick a legacy tag, and execute `build.sh` with selected environment variables. No generated protobuf Rust source was identified under the target tree by the captured searches; proto inputs and generated-file searches are recorded in the result files.

## Build Matrix

All build/test/clippy commands were local to the target checkout. No command contacted public Nomic infrastructure.

- `results/cargo-fmt-check.txt`: `cargo fmt --all -- --check` succeeded with exit status 0.
- `results/build-matrix-cargo-check-default.txt`: `cargo check` failed with exit status 101 when `librocksdb-sys v0.16.0+8.10.0` could not generate bindings because `rocksdb/include/rocksdb/c.h` included `stdbool.h`, which clang could not find.
- `results/build-matrix-cargo-check-all-features.txt`: `cargo check --all-features` failed with the same `librocksdb-sys` / `stdbool.h` failure.
- `results/build-matrix-cargo-check-release-default.txt`: `cargo check --release` failed with the same `librocksdb-sys` / `stdbool.h` failure.
- `results/build-matrix-cargo-check-testnet-feature.txt`: `cargo check --features testnet` failed with the same `librocksdb-sys` / `stdbool.h` failure.
- `results/build-matrix-cargo-check-devnet-feature.txt`: `cargo check --features devnet` failed with the same `librocksdb-sys` / `stdbool.h` failure.
- `results/build-matrix-cargo-test-all-features-no-run.txt`: `cargo test --all-features --no-run` failed with the same `librocksdb-sys` / `stdbool.h` failure.
- `results/cargo-clippy-workspace-all-targets-all-features.txt`: `cargo clippy --workspace --all-targets --all-features` failed with the same `librocksdb-sys` / `stdbool.h` failure.
- `results/build-matrix-cargo-check-no-default-features.txt`: `cargo check --no-default-features` progressed past the RocksDB path and failed in patched `merk` because `tree::Tree` does not implement `Debug` while `tree/link.rs` derives `Debug`.

## Phase 0 Limitations

- Rust compilation, full test execution, and clippy were blocked by local native dependency/toolchain state for `librocksdb-sys`.
- `cargo check --no-default-features` exposed a separate patched `merk` compile failure.
- Specialized dependency audit tools were not installed, so dependency review was based on `cargo metadata`, `Cargo.lock`, `cargo tree`, and direct source inspection.
