# Full ibc-rs Dependency Audit - v13.0.0 / 7278e9b

Audit run: `run-20260621T165710CDT-ibc-rs-full-v13-7278e9b`

Target source of truth: `/home/nitro/repos/nomic-v13-audit-7278e9b`

Target commit: `7278e9b8d8a5550306cdb11980cb0d0c51fcfe4e`

Target tag: `v13.0.0`

This audit reviewed the Rust IBC and `ibc-rs` dependency surface as pinned and reached by v13.0.0. It did not connect to public RPCs, relayers, validators, testnets, or real assets.

## Captured Evidence

- `results/cargo-tree-ibc-surface.txt`: IBC dependency tree surface.
- `results/cargo-tree-invert-ibc.txt`: reverse dependency paths into IBC crates.
- `results/ibc-lock-packages.tsv`: exact IBC, Tendermint, Cosmos SDK proto, and ICS23 packages resolved in cargo metadata.
- `results/dependency-ibc-keyword-search.txt`: keyword search over pinned dependency source.
- `results/source-nomic-ibc-entrypoints.txt`: Nomic IBC entrypoints in `src/app.rs`.
- `results/source-orga-ibc-deliver.txt`: pinned Orga IBC delivery wrapper.
- `results/source-orga-ibc-transfer.txt`: pinned Orga ICS-20 transfer execution context.
- `results/source-orga-host-consensus-state.txt`: pinned Orga host consensus state lookup.
- `results/source-ibc-rs-send-transfer.txt`: pinned ibc-rs send-transfer path.
- `results/source-ibc-rs-ack-timeout.txt`: pinned ibc-rs acknowledgement/timeout handling.

## Pinned IBC Surface

The v13 lockfile resolves the main IBC stack to:

- `ibc 0.54.0`
- `ibc-app-transfer 0.54.0`
- `ibc-app-transfer-types 0.54.0`
- `ibc-core* 0.54.0`
- `ibc-clients 0.54.0`
- `ibc-client-tendermint 0.54.0`
- `ibc-proto 0.47.1`
- `ics23 0.12.0`

Related duplicate/drift surface:

- `cosmos-sdk-proto 0.19.0` and `0.23.0`.
- `tendermint 0.32.2`, `0.38.1`, and `0.40.0`.
- `tendermint-proto 0.32.2`, `0.38.1`, and `0.40.0`.
- Orga is pinned to `turbofish-org/orga` rev `e539a53e1eed9c3363062d419ea84d8d0d56e1c7` and re-exports/wraps the ibc-rs path used by Nomic.

## Nomic IBC Entry Points

Inbound IBC:

- `src/app.rs` lines 698-701: `ibc_deliver` deducts an nBTC fee and calls `self.ibc.deliver(messages)?`.
- `src/app.rs` lines 703-727: incoming transfers with denom `usat` and a parseable memo are burned from the IBC receiver balance and inserted into Bitcoin pending credits with `Identity::None`.

Outbound IBC:

- `src/app.rs` lines 1838-1873: `IbcDest::transfer` mints local IBC balance, builds an ICS-20 `MsgTransfer`, calls `ibc.deliver_message`, logs any error, and returns `Ok(())`.

## ICS-20 Send And Refund Behavior

Pinned Orga transfer validation is permissive:

- `orga/src/ibc/transfer.rs` lines 110-154: send/receive/mint/burn/escrow/unescrow validation methods return `Ok(())`.

Pinned Orga transfer execution performs actual balance changes:

- `orga/src/ibc/transfer.rs` lines 187-203: burn subtracts from account balance.
- `orga/src/ibc/transfer.rs` lines 205-218: mint adds to account balance.
- `orga/src/ibc/transfer.rs` lines 221-243: escrow subtracts from sender and adds to escrow account.
- `orga/src/ibc/transfer.rs` lines 245-264: unescrow subtracts from escrow and adds to receiver.

Implication: ibc-rs validation may accept paths that later fail in execution if Nomic/Orga balances are insufficient or malformed. The pinned ibc-rs acknowledgement and timeout paths call application callbacks for refunds and propagate callback errors; commitments are not safely removable if the refund callback fails. I did not find a local proof of a double-refund in the pinned path, but failed refunds can become liveness/retry hazards.

## Findings And Leads

### IBC-1 - Confirmed: outbound IBC transfer can mint locally and return success after send failure

Source:

- `src/app.rs` lines 1852-1855: local mint.
- `src/app.rs` lines 1869-1873: `deliver_message` error logged and swallowed.

Impact: a failed outbound IBC message can leave local minted transfer balance without a packet. This is the same confirmed finding modeled in Phase 2.

Recommended fix direction: make `deliver_message` failure abort the call, or defer local mint until after send validation/commitment succeeds, or add explicit pending-failure accounting and a deterministic retry/refund path.

### IBC-2 - Likely: incoming IBC memo credits use `Identity::None`, limiting refund handling on later credit failure

Source:

- `src/app.rs` lines 703-727: incoming `usat` memo transfers are burned and inserted into pending Bitcoin credits with `Identity::None`.
- `src/app.rs` lines 526-584: failed `try_credit_dest` only refunds native and Ethereum identities.

Impact: if a memo destination later fails during pending-credit processing, there is no IBC sender identity to refund through the local failure handler. A complete proof needs the pending-credit execution path and a failing destination that passes initial memo parsing.

### IBC-3 - Likely: host consensus state lookup can panic/underflow on out-of-window heights

Source:

- Pinned Orga `src/ibc/impls.rs` lines 134-145 computes `self.host_consensus_states.len() - 1 - (self.height - height.revision_height())`, then calls `.get(index)?` and `.unwrap()`.

Impact: if a relayed IBC path can request a host consensus state outside the retained window, or when the deque is empty, unsigned underflow or unwrap panic can occur. The exact external reachability depends on ibc-rs calls into `host_consensus_state`, but this is a high-value hardening target.

Recommended fix direction: replace arithmetic with checked subtraction and return a typed `ContextError` for future/old heights or empty history.

### IBC-4 - Needs Verification: proof verification and commitment lifecycle drift under duplicate Tendermint/Cosmos stacks

Source:

- Cargo tree shows Tendermint 0.32, 0.38, and 0.40 families and Cosmos SDK proto 0.19 and 0.23 in the same build.
- The IBC path itself is on Tendermint 0.38 via ibc-rs/Orga, while Nomic also directly uses Tendermint 0.40 and older Cosmos SDK proto versions in other paths.

Impact: duplicate proto/Tendermint families increase the chance of type conversion mistakes, mismatched serialization expectations, or future build drift. No concrete exploit was proven in this run; keep this as a dependency hygiene and review target.

### IBC-5 - Needs Verification: permissive transfer validation shifts correctness into execution callbacks

Source:

- Pinned Orga transfer validation returns `Ok(())` for mint/burn/escrow/unescrow validation.
- Execution callbacks perform balance mutation and can fail.

Impact: ibc-rs core may accept a message through validation only to fail in application execution. For acknowledgements/timeouts, this appears to preserve packet commitment liveness rather than produce a double spend, but exact behavior should be covered by local tests once the native build issue is fixed.

## Reviewed Risk Areas

- ICS-20 acknowledgements/refunds: no double-refund proof found; callback failures remain a liveness/retry risk.
- Packet lifecycle: Nomic inbound path burns vouchers before inserting Bitcoin pending credit; outgoing path has a confirmed mint-before-send/suppressed-error issue.
- Channel/client/consensus-state lookup failure behavior: host consensus lookup has unchecked arithmetic/unwrap risk in pinned Orga.
- Proof verification: no public proof probes performed; proof-related review was static and source-anchored.
- Panic/unwrap surfaces reachable from Nomic: host consensus lookup is the highest-value IBC panic candidate found.
- Dependency lock/build drift: full cargo check/test was blocked by `librocksdb-sys`; IBC dependency versions and duplicate families were recorded from lockfile/tree output.

## Limitations

- Full Rust IBC runtime harnesses could not be executed because target builds failed in `librocksdb-sys` on this host.
- Static analysis did not include cargo-audit/cargo-deny because those tools were not available.
- No live chain, public RPC, public relayer, validator, or asset was touched.
