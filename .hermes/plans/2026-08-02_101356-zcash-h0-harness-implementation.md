# Zcash H0 Harness Implementation Plan

> **For Hermes:** Execute this plan with isolated feature branches, strict RED–GREEN TDD, independent exact-head review, and PR-based integration into `develop`.

**Goal:** Implement the H0 harness-skeleton milestone from the Zakura-first ZEC/nZEC plan without adding Zcash bridge, custody, signing, minting, migration, deployment, or mainnet behavior.

**Architecture:** Add a standalone stable-Rust workspace at `tools/interchain-tests/`, isolated from Nomic’s pinned consensus crate and nightly toolchain. The workspace will provide typed scenario contracts, deterministic run identity, process/container lifecycle drivers, bounded readiness/retry/watchdog behavior, redacted artifact receipts, fixture daemons, and a dependency-isolated oracle. H0 exercises only fixture daemons and infrastructure scenarios `HAR-001`, `HAR-002`, and `HAR-005` through `HAR-010`; `HAR-003` and `HAR-004` remain explicitly `Blocked` until real Nomic/Zakura integration milestones.

**Tech stack:** Rust 1.96.0 stable; Cargo nested workspace and lockfile; Tokio; Serde/TOML/JSON; SHA-256; rootless Podman locally; Docker-compatible OCI execution in GitHub Actions; `cargo test`, Clippy, rustfmt, cargo-deny, detect-secrets, Skopeo, and Syft.

**Base:** `develop` at merge commit `121419ce71b1d5fbae316d9b7db9229666b11843`.

**Non-negotiable boundary:** This phase must not touch `src/`, root `Cargo.toml`, root `Cargo.lock`, Nomic state, Bitcoin bridge semantics, wallets, keyrings, chain endpoints, or deployed infrastructure. Test canaries are synthetic strings and must never be real credentials.

---

## Acceptance contract

H0 is complete only when all of the following are true:

1. `tools/interchain-tests` is a standalone nested Cargo workspace with its own pinned toolchain and `Cargo.lock`.
2. `HAR-001`, `HAR-002`, and `HAR-005` through `HAR-010` execute and pass against fixture daemons on the process backend.
3. Backend-conformance coverage executes the same contract against process and OCI-container backends using Podman locally and Docker-compatible commands in CI.
4. `HAR-003` and `HAR-004` are reported as `Blocked`, excluded from pass totals, and cannot be silently counted as passing or ignored.
5. Scenario JSON distinguishes `Pass`, `Fail`, and `Blocked`; an enabled scenario returning `MissingCapability` is a failure.
6. Runs use a semantic seed for behavior and a separate random run nonce only for resource names.
7. Readiness, RPC/probe, retry, scenario, teardown, and outer-watchdog deadlines are bounded. No blind sleeps or unbounded loops exist.
8. Failure collection survives child crash, panic, hang, timeout, and forced kill.
9. Uploadable artifacts contain no synthetic secret canaries from argv, environment, config, protocol traffic, or logs.
10. Cleanup is scoped by run ID and cannot remove a concurrent run’s resources.
11. The oracle crate has no dependency on Nomic, Bitcoin bridge code, harness production helpers, or future `src/zcash` code.
12. Source/image locks contain immutable versions and digests, including selected `linux/amd64` platform-manifest digests where available; no floating `latest` reference is accepted.
13. Dedicated CI commands compile and test the nested workspace explicitly; a green root build does not impersonate harness coverage.
14. Every feature branch is independently reviewed at an immutable head, passes local gates, is merged by a PR bound to that head, and leaves `develop` green.

---

## Reviewable branch topology

Branches are sequential, not one giant branch and not a conflict-prone parallel pile:

1. `plan/zcash-h0-harness-implementation`
   - This plan only.
2. `feat/zcash-h0-contract-core`
   - Nested workspace, contract schema, manifest loader/validator, result accounting, deterministic identity, oracle boundary, fixture CLI skeleton.
3. `feat/zcash-h0-process-lifecycle`
   - Process backend, dynamic endpoint allocation, readiness, typed retry, watchdog, process-tree cleanup, and scenarios `HAR-001`, `HAR-005`, `HAR-006`.
4. `feat/zcash-h0-artifact-safety`
   - Canonical receipts, retain-on-failure, redaction/canary scanner, concurrent-run isolation, collector survival, and scenarios `HAR-002`, `HAR-007`, `HAR-008`, `HAR-010`.
5. `feat/zcash-h0-container-ci`
   - OCI backend, source/image lock verification, process/container conformance `HAR-009`, dedicated CI jobs, resource receipts, and operator documentation.

Each feature branch begins from the newly merged `develop`, not from a stale local branch. Use ordinary merge commits for the plan/feature PRs so the branch decomposition remains visible in repository history. Delete source refs only after merged-state and target-head readback agree.

---

## Planned repository layout

```text
tools/interchain-tests/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── deny.toml
├── README.md
├── containers/
│   └── fixture.Containerfile
├── crates/
│   ├── harness/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── main.rs
│   │   │   ├── contract.rs
│   │   │   ├── manifest.rs
│   │   │   ├── identity.rs
│   │   │   ├── topology.rs
│   │   │   ├── readiness.rs
│   │   │   ├── retry.rs
│   │   │   ├── watchdog.rs
│   │   │   ├── artifacts.rs
│   │   │   ├── redaction.rs
│   │   │   ├── driver/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── process.rs
│   │   │   │   └── container.rs
│   │   │   └── scenario/
│   │   │       ├── mod.rs
│   │   │       └── h0.rs
│   │   └── tests/
│   │       ├── contract_validation.rs
│   │       ├── deterministic_identity.rs
│   │       ├── h0_process.rs
│   │       ├── h0_artifacts.rs
│   │       └── h0_container.rs
│   ├── fixture/
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   └── oracle/
│       ├── Cargo.toml
│       └── src/lib.rs
├── scenarios/
│   ├── manifest.toml
│   └── topology/smoke.toml
└── vectors/
    └── source-lock.toml
```

Exact module splits may be reduced when a file would contain only ceremonial forwarding. Reviewability is the goal; directory cosplay is not.

---

## Branch 1 — Contract core

### Task 1.1: Establish the isolated stable workspace

**Files:**
- Create: `tools/interchain-tests/Cargo.toml`
- Create: `tools/interchain-tests/rust-toolchain.toml`
- Create: `tools/interchain-tests/README.md`
- Create: `tools/interchain-tests/deny.toml`
- Create: `tools/interchain-tests/crates/harness/Cargo.toml`
- Create: `tools/interchain-tests/crates/harness/src/lib.rs`
- Create: `tools/interchain-tests/crates/harness/src/main.rs`
- Create: `tools/interchain-tests/crates/fixture/Cargo.toml`
- Create: `tools/interchain-tests/crates/fixture/src/main.rs`
- Create: `tools/interchain-tests/crates/oracle/Cargo.toml`
- Create: `tools/interchain-tests/crates/oracle/src/lib.rs`
- Modify: `.gitignore` only if generated H0 artifact roots need an explicit repository-local exclusion.

**RED:** Add a workspace smoke test or metadata assertion before implementation. Verify root Cargo does not discover the nested crates and nested Cargo does.

**Commands:**

```bash
cargo metadata --no-deps --format-version 1
cargo metadata --manifest-path tools/interchain-tests/Cargo.toml --no-deps --format-version 1
```

Expected RED: the nested manifest does not exist.

**GREEN:** Pin Rust `1.96.0`, resolver `2`, and explicit members. Keep dependencies local to the nested lockfile. The fixture initially exposes only `--help` and a typed mode parser; it does not open a port yet.

**Verification:**

```bash
cd tools/interchain-tests
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo test --workspace --all-features
cargo deny check
```

**Commit:** `feat(harness): establish isolated H0 workspace`

### Task 1.2: Define scenario contracts and honest result states

**Files:**
- Create: `crates/harness/src/contract.rs`
- Create: `crates/harness/src/manifest.rs`
- Create: `crates/harness/tests/contract_validation.rs`
- Create: `scenarios/manifest.toml`
- Create: `scenarios/topology/smoke.toml`

**Contract fields:**
- stable `id`, `case_id`, and `spec_version`;
- `status = "blocked" | "enabled"`;
- release requirement;
- phase, topology, backend/fidelity;
- semantic seed domain;
- capability, fixture, step, barrier, terminal-predicate, unchanged-projection, oracle, invariant, evidence lists;
- per-operation and overall deadlines;
- memory/artifact budgets;
- CI lane.

**RED tests:**
- duplicate IDs fail;
- unknown status fails;
- enabled contract with no executable binding fails;
- missing oracle/evidence/deadline/budget fails;
- `Blocked` does not increase pass count;
- enabled `MissingCapability` reports `Fail`;
- all `HAR-001..010` IDs exist exactly once;
- only `HAR-003` and `HAR-004` are blocked for H0.

**GREEN:** Implement bounded TOML parsing and validation. Avoid generic maps where typed enums are possible. Unknown fields should fail closed.

**CLI contract:**

```bash
cargo run -p nomic-bridge-harness -- contract validate --manifest scenarios/manifest.toml
cargo run -p nomic-bridge-harness -- scenario list --manifest scenarios/manifest.toml --json
```

JSON output must include counts for `enabled`, `blocked`, `pass`, `fail`, and schema version. Listing does not execute and cannot claim passes.

**Commit:** `feat(harness): add typed scenario contract inventory`

### Task 1.3: Separate semantic seed from resource nonce

**Files:**
- Create: `crates/harness/src/identity.rs`
- Create: `crates/harness/tests/deterministic_identity.rs`

**RED tests:**
- same source-lock digest + scenario + topology + corpus index yields same semantic seed;
- different case/spec/topology changes the seed;
- separate run nonce changes resource names only;
- normalized semantic receipt excludes nonce-derived endpoints, paths, and container IDs;
- resource labels reject unsafe characters and remain bounded.

**GREEN:** Use SHA-256 over a canonical length-delimited domain encoding. Use OS randomness only for the run nonce. Never derive keys or chain data from the run nonce.

**Commit:** `feat(harness): separate semantic and resource identity`

### Task 1.4: Establish an independent oracle boundary

**Files:**
- Extend: `crates/oracle/src/lib.rs`
- Extend: `crates/harness/tests/contract_validation.rs`

**RED tests:** A synthetic receipt/protocol mutation corpus must independently detect changed amount, omitted event, wrong asset tag, reordered transition, duplicate bucket membership, outpoint mutation, Merkle sibling mutation, Merkle index mutation, branch-ID mutation, signature mutation, fee mutation, expiry mutation, and IBC denom-trace mutation. H0 fixtures use typed synthetic protocol records; they do not claim Zcash/Nomic cryptographic verification. Each named field must have a control case that passes and a one-field mutation that fails for the expected oracle reason.

**GREEN:** Oracle accepts only public receipt-model structures. It must not depend on `nomic`, the harness crate, the root package, or future production bridge modules. Add a test that parses `cargo metadata` and rejects prohibited dependency/package names.

**Commit:** `test(harness): enforce independent oracle boundary`

---

## Branch 2 — Process lifecycle

### Task 2.1: Implement the fixture daemon as a hostile lifecycle target

**Files:**
- Extend: `crates/fixture/src/main.rs`
- Test through: `crates/harness/tests/h0_process.rs`

**Fixture modes:**
- healthy readiness and deterministic echo;
- delayed readiness;
- typed transient failures before success;
- permanent failure;
- panic/crash after readiness;
- hang ignoring normal work completion;
- synthetic canaries in argv/env/config/request/log channels.

The fixture binds only to an explicitly supplied loopback address and dynamic port. It prints structured JSON events to its assigned artifact stream. No blind sleeps are used for orchestration; fixture delay modes exist only as bounded adversarial inputs.

**RED:** Spawn tests fail because the fixture has no service/readiness mode.

**GREEN:** Add fixture behavior with no shell invocation and no raw environment dump.

**Commit:** `test(harness): add hostile lifecycle fixture daemon`

### Task 2.2: Dynamic endpoints and typed readiness

**Files:**
- Create: `crates/harness/src/topology.rs`
- Create: `crates/harness/src/readiness.rs`
- Create: `crates/harness/src/driver/mod.rs`
- Create: `crates/harness/src/driver/process.rs`

**RED tests:**
- two concurrent allocations do not collide;
- readiness verifies expected component/run/network identity;
- wrong identity fails permanently;
- delayed readiness succeeds inside budget;
- missing listener times out with stable error class;
- no hard-coded service ports occur in implementation/config.

**GREEN:** Pass reserved loopback listeners or use a race-bounded allocation protocol. Readiness responses include schema, component, run ID, and capability list.

**Commit:** `feat(harness): add bounded process readiness`

### Task 2.3: Classified retries and outer watchdog

**Files:**
- Create: `crates/harness/src/retry.rs`
- Create: `crates/harness/src/watchdog.rs`
- Extend: `crates/harness/tests/h0_process.rs`

**RED tests:**
- `NotReady` retries only within declared attempt/deadline budget;
- identity mismatch and invalid data fail immediately;
- forced hang reaches timeout;
- outer watchdog kills the complete fixture process tree;
- timeout cannot be retried into a green result.

**GREEN:** Use typed transient/permanent classes and a supervisor-owned deadline. Record attempt count and elapsed monotonic duration in receipts; expected values must not depend on wall-clock timestamps.

**Commit:** `feat(harness): bound retries and process watchdogs`

### Task 2.4: Enable process-backed H0 scenarios

**Files:**
- Create: `crates/harness/src/scenario/mod.rs`
- Create: `crates/harness/src/scenario/h0.rs`
- Extend: `crates/harness/tests/h0_process.rs`

**Scenarios enabled here:**
- `HAR-001` clean process bootstrap/identity/readiness/teardown;
- `HAR-005` same-seed semantic replay with normalized receipts;
- `HAR-006` retry classification, hang timeout, and process-tree teardown.

**Required RED/GREEN evidence:** For each scenario, enable the manifest contract first, run it, observe a real missing assertion/capability failure, then implement the minimum path. Do not use random process failure as RED evidence.

**Commit:** `test(harness): enable core H0 process scenarios`

---

## Branch 3 — Artifact safety and isolation

### Task 3.1: Canonical receipt and external collection model

**Files:**
- Create: `crates/harness/src/artifacts.rs`
- Create: `crates/harness/src/redaction.rs`
- Create: `crates/harness/tests/h0_artifacts.rs`

**Receipt fields:**
- receipt schema and runner version;
- scenario/case/spec/topology IDs;
- semantic seed/corpus index and run ID;
- normalized component identity and ordered transitions;
- deadline/resource outcome;
- pass/fail/blocked class and stable reason;
- artifact manifest with SHA-256 and redaction status.

**RED tests:**
- receipt serialization changes with unordered maps or host paths;
- collector loses evidence after child crash/kill;
- synthetic canaries survive raw collection;
- artifact manifest accepts traversal/symlink escape;
- collector error incorrectly allows upload.

**GREEN:** Canonical deterministic JSON, external supervisor collection, path confinement, no symlink escape, content scan before upload eligibility, and fail-closed collector status.

**Commit:** `feat(harness): add canonical redacted receipts`

### Task 3.2: Retain-on-failure and cleanup receipts

**RED tests:**
- intentional assertion failure removes artifacts when retention is off;
- retention preserves the bounded local environment when enabled;
- uploadable bundle remains separate from local retained state;
- cleanup command targets only the exact run ID;
- local retention has an expiry marker.

**GREEN:** Implement a typed cleanup receipt and idempotent scoped cleanup. Never print RPC cookies, raw environment, key-shaped fixture data, or host topology.

**Commit:** `feat(harness): retain failures without leaking state`

### Task 3.3: Concurrent isolation

**RED tests:**
- two identical semantic seeds run concurrently with different nonces;
- endpoints, directories, labels, and process groups are disjoint;
- cleaning run A leaves run B ready and responsive;
- stale run cleanup cannot match a prefix of another run ID.

**GREEN:** Exact labels and ownership markers, not path prefixes or broad process-name matching.

**Commit:** `test(harness): prove scoped parallel cleanup`

### Task 3.4: Enable artifact H0 scenarios

**Scenarios:**
- `HAR-002` retain on failure;
- `HAR-007` parallel isolation and scoped cleanup;
- `HAR-008` independent oracle and complete synthetic mutation rejection. Its executable scenario must invoke every Task 1.4 control/mutation pair—amount, omitted event, asset tag, transition order, duplicate bucket membership, outpoint, Merkle sibling, Merkle index, branch ID, signature, fee, expiry, and IBC denom trace—require each control to pass, require each one-field mutation to fail for its expected stable reason, and emit the complete case ledger in the receipt. A missing, skipped, duplicate, unexpectedly passing, or wrong-reason case fails `HAR-008`; library-only unit coverage cannot satisfy the scenario;
- `HAR-010` canaries absent from upload bundle after crash/panic/kill, collector failure blocks upload.

Every enabled scenario must have an executable assertion and produce a compact receipt. Synthetic canaries must be unmistakably test-only and generated per run.

**Commit:** `test(harness): enable H0 artifact safety scenarios`

### Task 3.5: Enforce declared resource budgets

**Files:**
- Extend: `crates/harness/src/artifacts.rs`
- Extend: `crates/harness/src/watchdog.rs`
- Extend: `crates/harness/tests/h0_artifacts.rs`

**Measured resources:**
- process-tree peak RSS from Linux `/proc` using exact supervised PIDs/process groups;
- run-root disk bytes without following symlinks outside the confined directory;
- uploadable and local-retained artifact bytes as separate counters;
- elapsed monotonic time, already bounded by operation/scenario/watchdog deadlines.

**RED tests:**
- memory, run-disk, upload-artifact, and retained-artifact values at the declared ceiling pass;
- each value at `ceiling + 1` fails with a stable `ResourceBudgetExceeded` class;
- integer overflow, unreadable accounting state, vanished-process races, symlink escape, and partial collector output fail closed rather than reporting zero;
- a budget failure remains a failed original attempt and cannot be retried into green;
- concurrent-run accounting includes only the exact run-ID-owned process tree and artifact roots.

**GREEN:** Add typed budget snapshots and a supervisor-owned enforcement check before terminal success. The receipt records measured value, configured ceiling, unit, source, and whether the sample is complete. Sampling uncertainty or unsupported accounting is an explicit failure for required H0 lanes. Fixture tests use bounded deterministic allocation modes for exact boundary assertions; measured cold/warm host runs supply operational ceilings later in Branch 4.

**Commit:** `feat(harness): enforce H0 resource budgets`

---

## Branch 4 — Container backend, immutable locks, and CI

### Task 4.1: Build a runtime-neutral OCI fixture

**Files:**
- Create: `containers/fixture.Containerfile`
- Create: `crates/harness/src/driver/container.rs`
- Create: `crates/harness/tests/h0_container.rs`

**Runtime contract:**
- support explicit `podman` or `docker` executable selection;
- invoke with argument arrays, never shell interpolation;
- isolated network and labels per run;
- dynamic host endpoint publication or direct inspected endpoint;
- exact run-ID cleanup;
- bounded logs/inspect calls;
- image reference must include a digest for locked external images; the locally built fixture is identified by its content/image ID in the receipt.

**RED tests:**
- absent runtime is a typed unavailable result, not a pass;
- wrong runtime identity fails;
- same probe differs between backends;
- broad cleanup can remove another run.

**GREEN:** Implement OCI lifecycle and normalized endpoint/error classes. Unit tests remain runnable without a container runtime; the required integration lane fails if explicitly selected and unavailable.

**Commit:** `feat(harness): add OCI fixture backend`

### Task 4.2: Lock external sources and images

**Files:**
- Create/extend: `vectors/source-lock.toml`
- Add validation in: `crates/harness/src/manifest.rs` or a narrow `source_lock.rs` module.

**Lock entries:**
- Nomic base commit;
- Zakura `v1.0.5` source commit;
- Zakura OCI index digest and selected `linux/amd64` manifest digest;
- Zakura binary release checksums;
- compatibility-sidecar source commit and OCI index/platform digest;
- declared license/SBOM provenance and retrieval timestamp;
- no claim of minisign verification until a reviewed public-key trust root exists.

**Verification tools:** Skopeo resolves immutable OCI metadata; Syft generates SBOM evidence. Do not commit registry credentials, signed download URLs, host paths, or unbounded generated SBOMs unless the plan explicitly selects a compact reviewed artifact.

**RED tests:** floating tag, missing digest, duplicate component, malformed SHA-256, unsupported platform, or unreviewed trust assertion fails validation.

**Commit:** `build(harness): pin H0 source and image inputs`

### Task 4.3: Enable backend conformance

**Scenario:** `HAR-009` runs one identical fixture contract against process and container drivers and compares normalized capability/error/receipt projections.

**RED:** Enable `HAR-009` before container driver conformance exists and observe the expected assertion failure.

**GREEN:** Same readiness schema, fixture action, permanent/transient error classes, and terminal predicate on both backends. Backend-specific IDs stay only in diagnostic artifacts.

**Commit:** `test(harness): prove process and OCI conformance`

### Task 4.4: Add dedicated CI jobs

**Files:**
- Modify: `.github/workflows/ci.yml`

**Jobs:**
1. `harness-unit`
   - install Rust 1.96.0 with rustfmt/clippy;
   - cache nested Cargo registry/git/target by nested lockfile;
   - `cargo fmt --all -- --check`;
   - `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
   - `cargo test --workspace --all-features`;
   - validate manifest and source lock.
2. `harness-smoke`
   - build fixture OCI image;
   - run H0 process and container scenarios with fixed PR corpus;
   - always collect compact receipt and redacted diagnostics;
   - job timeout remains outside the harness watchdog;
   - no retry converts failure into success.

Use `working-directory: tools/interchain-tests` or exact `--manifest-path` on every nested Cargo command. Keep root Nomic jobs unchanged.

**Commit:** `ci(harness): run isolated H0 verification`

### Task 4.5: Document operator commands and measured ceilings

**Files:**
- Extend: `tools/interchain-tests/README.md`

Document:
- prerequisites and runtime selection;
- contract validation/list/run commands;
- fixed seed and corpus replay;
- artifact trust split;
- cleanup command;
- process/container smoke;
- exact verification matrix;
- cold/warm measured wall time, peak RSS, disk, and artifact bytes from this host and CI where observable;
- explicit statement that H0 proves harness infrastructure only—not Nomic, Zakura consensus, Zcash PoW, bridge custody, or nZEC accounting.

**Commit:** `docs(harness): publish H0 verification contract`

---

## Per-branch execution and review protocol

For every feature branch:

1. Fetch and verify live `origin/develop`.
2. Create an isolated worktree from that exact commit.
3. Run a baseline nested-workspace gate when the workspace exists.
4. Give Codex the exact branch task, files, boundaries, TDD rule, and required commands.
5. Preserve real RED output before production implementation.
6. Review the complete staged diff, including new files.
7. Run focused tests, then full nested workspace format/clippy/test/deny/manifest gates.
8. Run `git diff --check`, secret/path scans, and `detect-secrets` on changed files.
9. Obtain an independent reviewer verdict against the immutable head and task contract.
10. Push with the GitHub App helper, verify remote branch head, open a PR to `develop`, and wait for CI or record explicitly that no configured check exists.
11. Re-query PR head/base/mergeability/reviews/inline comments immediately before merge.
12. Merge with the API `sha` field bound to the reviewed head using merge-commit history.
13. Verify PR `MERGED`, target `develop` head, expected tree/content, and source-ref deletion.
14. Start the next branch only from that new target head.

Codex may edit only its assigned isolated worktree. It must not push, merge, change remotes, access credentials, read wallets/keyrings, or touch files outside the declared scope. Parent jun0n0s owns review, commits, GitHub mutation, and final receipts.

---

## Complete local verification matrix

Run from `tools/interchain-tests`:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
cargo run -p nomic-bridge-harness -- contract validate --manifest scenarios/manifest.toml
cargo run -p nomic-bridge-harness -- scenario list --manifest scenarios/manifest.toml --json
cargo run -p nomic-bridge-harness -- scenario run --manifest scenarios/manifest.toml --lane h0-process --seed h0-pr-v1
NOMIC_HARNESS_CONTAINER_RUNTIME=podman cargo run -p nomic-bridge-harness -- scenario run --manifest scenarios/manifest.toml --lane h0-container --seed h0-pr-v1
```

Repository-level checks:

```bash
git diff --check origin/develop...HEAD
git status --short --ignored
detect-secrets scan --all-files tools/interchain-tests
```

The parent reviewer must inspect `git diff origin/develop...HEAD` file-by-file. Generated build output, local artifacts, container layers, test credentials, environment dumps, and internal host paths must not be staged.

---

## Risks and stop conditions

Stop and report rather than weakening the gate if:

- process-tree cleanup cannot be made deterministic on the supported Linux runner;
- container conformance requires privileged or host-network execution;
- artifact redaction depends on a deny-list too weak to catch injected canaries;
- a green result depends on retries after scenario failure;
- a fixture/runtime uses fixed shared ports or broad name-based cleanup;
- a source/image digest cannot be independently resolved;
- CI cannot explicitly execute the nested workspace;
- a required H0 scenario is skipped, ignored, or represented as blocked;
- Codex modifies Nomic production code, root dependency files, wallet/key material, deployment configuration, or unrelated repository surfaces;
- the implementation begins importing Zakura/Zcash consensus crates or claims bridge security before H2/H3.

H0 should be boring, bounded, and hostile to false greens. That is the point.
