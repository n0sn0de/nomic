# Nomic interchain test harness

This directory is an isolated Cargo workspace for the H0 interchain test
harness. It intentionally does not inherit the repository's root workspace or
toolchain. Run its checks from this directory:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo test --workspace --all-features
cargo deny check
```

The fixture currently validates its command line only. It does not open a
listener or implement service behavior.
