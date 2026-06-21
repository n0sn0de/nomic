Audit run started: 2026-06-21T03:55:36-05:00
Target cwd: /home/nitro/repos/nomic
Initial HEAD: c5486a4a9b41de5474f991891ae22f734b1d5aec

## git-rev-parse
timestamp: 2026-06-21T03:55:49-05:00
command: git rev-parse HEAD
exit: 0

## git-status
timestamp: 2026-06-21T03:55:49-05:00
command: git status --short
exit: 0

## rustc-version
timestamp: 2026-06-21T03:55:49-05:00
command: rustc --version --verbose
exit: 0

## cargo-version
timestamp: 2026-06-21T03:56:08-05:00
command: cargo --version
exit: 0

## cargo-metadata
timestamp: 2026-06-21T03:56:08-05:00
command: cargo metadata --format-version 1
exit: 0

## cargo-tree-all-features
timestamp: 2026-06-21T03:56:20-05:00
command: cargo tree --all-features
exit: 0

## cargo-tree-duplicates
timestamp: 2026-06-21T03:56:21-05:00
command: cargo tree -d
exit: 0

## cargo-features-jq
timestamp: 2026-06-21T03:56:55-05:00
command: jq .packages\[\]\ \|\ select\(.name==\"nomic\"\)\ \|\ \{name\,version\,features\,dependencies\,targets\} audit/run-20260621T0354CDT-c5486a4/phase0/cargo-metadata.out
exit: 0

## workspace-members-jq
timestamp: 2026-06-21T03:56:55-05:00
command: jq \{workspace_members\,workspace_default_members\,resolve_root:.resolve.root\} audit/run-20260621T0354CDT-c5486a4/phase0/cargo-metadata.out
exit: 0

## git-dependencies-lock
timestamp: 2026-06-21T03:56:55-05:00
command: rg -n source\ =\ \"git\\+ Cargo.lock
exit: 0

## cfg-feature-branches
timestamp: 2026-06-21T03:56:55-05:00
command: rg -n \#\\\[cfg\|cfg\\\(\|cfg_attr\|feature\ =\|testnet\|mainnet\|devnet\|signet\|fuzzing\|debug_assert\|overflow\|panic Cargo.toml build.rs src tests networks genesis
exit: 0

## build-scripts-find
timestamp: 2026-06-21T03:56:55-05:00
command: find . -name build.rs -o -name \*.proto -o -path ./src/babylon/proto/gen/\*.rs
exit: 0

## patch-crates-rg
timestamp: 2026-06-21T03:56:56-05:00
command: rg -n \^\\\[patch\|\^\\\[replace\|patch\\. Cargo.toml rest/Cargo.toml wasm/Cargo.toml Cargo.lock
exit: 1

## lockfile-git-packages
timestamp: 2026-06-21T03:56:56-05:00
command: jq -r .packages\[\]\ \|\ select\(.source\|type==\"string\"\ and\ startswith\(\"git+\"\)\)\ \|\ \[.name\,.version\,.source\]\ \|\ @tsv audit/run-20260621T0354CDT-c5486a4/phase0/cargo-metadata.out
exit: 0

## profiles-rg
timestamp: 2026-06-21T03:56:56-05:00
command: rg -n \^\\\[profile\|overflow-checks\|panic\|lto\|codegen-units\|debug-assertions Cargo.toml rest/Cargo.toml wasm/Cargo.toml .cargo .github workflows src
exit: 2

## cargo-fmt-check
timestamp: 2026-06-21T04:00:22-05:00
command: cargo fmt --all -- --check
exit: 0

## cargo-test-default-no-run
timestamp: 2026-06-21T04:00:23-05:00
command: cargo test --workspace --all-targets --no-run
exit: 101

## cargo-test-no-default-no-run
timestamp: 2026-06-21T04:00:29-05:00
command: cargo test --workspace --no-default-features --all-targets --no-run
exit: 101

## cargo-test-all-features-no-run
timestamp: 2026-06-21T04:00:33-05:00
command: cargo test --workspace --all-targets --all-features --no-run
exit: 101

## cargo-check-lib-no-default
timestamp: 2026-06-21T04:01:05-05:00
command: cargo check --lib --no-default-features
exit: 101

## cargo-check-lib-default
timestamp: 2026-06-21T04:01:30-05:00
command: cargo check --lib
exit: 101

## cargo-check-lib-devnet
timestamp: 2026-06-21T04:01:35-05:00
command: cargo check --lib --features devnet
exit: 101

## cargo-check-lib-mainnet-equivalent
timestamp: 2026-06-21T04:01:36-05:00
command: cargo check --lib --no-default-features --features full
exit: 101

## cargo-check-lib-minimal-csv
timestamp: 2026-06-21T04:02:16-05:00
command: cargo check --lib --no-default-features --features csv
exit: 0

## tool-availability
timestamp: 2026-06-21T04:02:46-05:00
command: bash -lc for\ t\ in\ cargo-audit\ cargo-deny\ cargo-geiger\ cargo-vet\ cargo-fuzz\ cargo-miri\;\ do\ command\ -v\ \"\$t\"\ \|\|\ true\;\ done
exit: 0

## rg-unsafe
timestamp: 2026-06-21T04:02:46-05:00
command: rg -n \\bunsafe\\b src tests Cargo.toml build.rs
exit: 1

## rg-panic-unwrap
timestamp: 2026-06-21T04:02:46-05:00
command: rg -n \\bunwrap\\\(\|\\bexpect\\\(\|panic\!\\\(\|assert\!\\\(\|assert_eq\!\\\(\|assert_ne\!\\\(\|unimplemented\!\\\(\|unreachable\!\\\(\|\\\[\[\^\\\]\]+\\\] src tests build.rs
exit: 0

## rg-arithmetic-casts
timestamp: 2026-06-21T04:02:46-05:00
command: rg -n \\bas\\s+\(u8\|u16\|u32\|u64\|usize\|i8\|i16\|i32\|i64\|isize\)\|checked_\|saturating_\|wrapping_\|overflowing_\|\\+\|\\-\|\\\*\|/ src/bitcoin src/app.rs src/cosmos.rs src/frost src/babylon src/ethereum -g \*.rs
exit: 0

## rg-nondeterminism
timestamp: 2026-06-21T04:02:46-05:00
command: rg -n HashMap\|HashSet\|BTreeMap\|SystemTime\|UNIX_EPOCH\|thread_rng\|random\|rand::\|f32\|f64\|usize\|read_dir\|std::fs\|spawn\|thread\|par_iter src tests build.rs
exit: 0

## cargo-audit
timestamp: 2026-06-21T04:02:46-05:00
command: cargo audit
exit: 101

## cargo-deny
timestamp: 2026-06-21T04:02:47-05:00
command: cargo deny check
exit: 101

## cargo-geiger
timestamp: 2026-06-21T04:02:47-05:00
command: cargo geiger --all-features
exit: 101

## poc-threshold-model
timestamp: 2026-06-21T04:04:49-05:00
command: python3 audit/run-20260621T0354CDT-c5486a4/poc_threshold_model.py
exit: 0

## poc-legacy-commitment-model
timestamp: 2026-06-21T04:04:49-05:00
command: python3 audit/run-20260621T0354CDT-c5486a4/poc_legacy_commitment_model.py
exit: 0

## poc-deposit-fee-prevalidation-model
timestamp: 2026-06-21T04:18:18-05:00
command: python3 audit/run-20260621T0354CDT-c5486a4/poc_deposit_fee_prevalidation_model.py
exit: 0

## poc-header-retarget-underflow-model
timestamp: 2026-06-21T04:18:18-05:00
command: python3 audit/run-20260621T0354CDT-c5486a4/poc_header_retarget_underflow_model.py
exit: 0

## poc-header-retarget-underflow-model-refresh
timestamp: 2026-06-21T04:19:42-05:00
command: python3 audit/run-20260621T0354CDT-c5486a4/poc_header_retarget_underflow_model.py
exit: 0
