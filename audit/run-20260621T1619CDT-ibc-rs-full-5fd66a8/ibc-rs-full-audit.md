# Full ibc-rs Dependency Path Security Audit

Audit time: 2026-06-21 16:19 CDT

Target branch/base: `security-audit` at `5fd66a8fb858a73eb2a22e997ac3dcb868470b04`

Scope note: Jason's "ibs-rs" request maps to Nomic's Rust IBC dependency surface: `ibc`, `ibc-proto`, `ibc-*`, `ics23`, and Orga's `feat-ibc` integration. I found no literal `ibs-rs` crate.

## Executive Summary

This audit followed Nomic's consensus call paths into the exact Rust IBC stack reached through Orga. The most important result is a confirmed refund bug in Orga's ICS-20 acknowledgement callback: failed acknowledgements do not refund escrowed or burned transfer tokens, while ibc-rs core still deletes the packet commitment. For Nomic-origin nBTC (`usat`) IBC transfers, this can permanently lock funds when a counterparty returns an error acknowledgement.

I did not find evidence that a malicious counterparty can mint unbacked native nBTC through the normal incoming `usat`+memo path. Nomic only treats a transfer as native-returning when Orga records an incoming transfer on the receiver-chain-source path, then burns the IBC transfer-module balance before inserting native nBTC into pending Bitcoin/app destinations. The main high-confidence risk is loss/locking on failed acknowledgements, not over-mint.

The second security lead is a likely consensus/ABCI denial-of-service in Orga's host consensus-state lookup: connection handshake code can request a locally retained host consensus state by message-supplied height, and Orga indexes a bounded deque with unchecked subtraction and `unwrap()`. If reachable with an old-but-not-future height, this can panic instead of returning a protocol error.

The REST subcrate also has a reproducibility problem: `rest/Cargo.lock` is stale against `rest/Cargo.toml` and the root Orga pin. `cargo metadata --locked` fails in `rest/`, and the lockfile still references an older Orga git revision.

## Exact Target And Dependency Versions

Root manifest:

- `nomic` version: `9.2.0`
- Orga dependency: `https://github.com/nomic-io/orga.git` pinned at `3b3d25ade40d81cb64f19335535e3a47bb47778f`, features `merk-verify` and `feat-ibc`
- Default features: `full`, `testnet`
- Release profile: `overflow-checks = true`

Root lockfile resolved IBC path:

- `ibc = 0.54.0`
- `ibc-proto = 0.47.1` even though Orga/rest manifests request `0.47.0` semver-compatible
- `ics23 = 0.12.0`
- All reachable `ibc-*` crates in both root and `rest/` lockfiles are `0.54.0`: `ibc-apps`, `ibc-app-transfer`, `ibc-app-transfer-types`, `ibc-app-nft-transfer`, `ibc-app-nft-transfer-types`, `ibc-client-tendermint`, `ibc-client-tendermint-types`, `ibc-client-wasm-types`, `ibc-clients`, `ibc-core`, `ibc-core-channel`, `ibc-core-channel-types`, `ibc-core-client`, `ibc-core-client-context`, `ibc-core-client-types`, `ibc-core-commitment-types`, `ibc-core-connection`, `ibc-core-connection-types`, `ibc-core-handler`, `ibc-core-handler-types`, `ibc-core-host`, `ibc-core-host-cosmos`, `ibc-core-host-types`, `ibc-core-router`, `ibc-core-router-types`, `ibc-derive = 0.8.0`, `ibc-primitives`.

Important version divergence:

- `Cargo.lock` pins Orga at `3b3d25ade40d81cb64f19335535e3a47bb47778f`.
- `rest/Cargo.lock` still pins Orga at `35988d76b58008e37794064c41f3d0ba102ca0c8`.
- `rest/Cargo.toml` directly depends on `ibc = 0.54.0` and `ibc-proto = 0.47.0`, but `rest/Cargo.lock` resolves `ibc-proto = 0.47.1`.
- Running `cargo metadata --locked` in `rest/` fails because the lockfile needs to be updated.

Toolchain recorded:

- `rustc 1.81.0-nightly (506985649 2024-07-20)`
- `cargo 1.81.0-nightly (a2b58c3da 2024-07-16)`

## Call-Path Map

### Native nBTC To IBC

User entry points:

- Amino `nomic/MsgIbcTransferOut` is converted into `InnerApp::ibc_transfer_nbtc`.
- CLI `ibc-transfer` builds the same `IbcDest`.
- Bitcoin interchain deposits encode `Dest::Ibc` into the Bitcoin deposit commitment.

Consensus path:

1. `ibc_transfer_nbtc` validates only syntactic destination fields, withdraws nBTC from the signer, charges a fee, and inserts a pending `Dest::Ibc`.
2. `begin_block` drains Bitcoin pending transfers and calls `try_credit_dest`.
3. `credit_dest` dispatches `Dest::Ibc` to `IbcDest::transfer`.
4. `IbcDest::transfer` optionally charges another IBC fee, mints the transfer-module `usat` balance to the sender, builds an ibc-rs `MsgTransfer`, and calls Orga `Ibc::deliver_message(IbcMessage::Ics20(...))`.
5. Orga calls ibc-rs `send_transfer`, which escrows or burns through Orga's `TokenTransferExecutionContext` and writes packet commitments/sequences through Orga's `IbcContext`.

### Relayer-Facing IBC Messages

1. `ConvertSdkTx` identifies protobuf txs that parse as Orga `IbcTx`.
2. Nomic maps such txs to `ibc_deliver`, charging a fixed nBTC fee to the transaction signer.
3. `ibc_deliver` calls `self.ibc.deliver`, which converts raw Cosmos tx body messages into either `MsgEnvelope` (ICS-02/03/04/26) or `MsgTransfer` (ICS-20), then dispatches into ibc-rs.
4. Orga `ValidationContext::validate_message_signer` compares each IBC message signer to the authenticated Orga SDK signer.

### Incoming nBTC Return / Memo Path

1. Orga's transfer module records an `incoming_transfer` only when `is_receiver_chain_source(...)` is true and the transfer packet succeeded.
2. Nomic filters for `denom == "usat"` and a nonempty memo.
3. It parses the memo as a Nomic `Dest`, burns the receiver's IBC transfer-module `usat` balance, mints native nBTC into pending, and later credits the destination in `begin_block`.

### Acknowledgements, Timeouts, Replay State

1. ibc-rs core validates packet proofs and calls the transfer module's acknowledgement/timeout callbacks.
2. Orga stores packet commitments, receipts, acknowledgements, and sequence numbers under absolute IBC state prefixes.
3. Timeouts call ibc-rs transfer refund logic through Orga.
4. Failed acknowledgements do not, because Orga's ack execute callback is a stub. This is Finding 1.

### REST / Query Surface

The `rest/` crate is a sidecar query server. It imports `ibc` and `ibc-proto` directly to shape connection/client JSON, but it is not the consensus IBC executor. Its lockfile drift is still a supply-chain and operator reproducibility risk.

## Findings

### Finding 1 - Confirmed High: failed ICS-20 acknowledgements do not refund nBTC

Status: Confirmed from source. Not reproduced in a local Hermes test because the available e2e test is ignored and local builds are blocked by dependency/tooling issues listed below.

Affected path:

`MsgIbcTransferOut` or Bitcoin `Dest::Ibc` -> `IbcDest::transfer` -> Orga `Ibc::deliver_message(Ics20)` -> ibc-rs packet send -> relayer submits `MsgAcknowledgement` -> ibc-rs acknowledgement handler -> Orga transfer module.

Expected behavior:

For an unsuccessful ICS-20 acknowledgement, ibc-rs transfer's default module code calls `refund_packet_token_execute`. If the sending chain is the token source, it unescrows tokens to the sender. Otherwise it mints vouchers back to the sender.

Observed behavior:

Orga's `Transfer::on_acknowledgement_packet_validate` delegates to ibc-rs validation, so failed acknowledgements validate that a refund would be legal. But `on_acknowledgement_packet_execute` returns `ModuleExtras::empty(), Ok(())` and does not call ibc-rs `on_acknowledgement_packet_execute` or `refund_packet_token_execute`. ibc-rs core then deletes the packet commitment after the callback returns success.

Impact:

- For Nomic-origin `usat`, the outbound packet escrows native nBTC in the channel escrow account. If the counterparty returns an error acknowledgement, Nomic accepts the acknowledgement, deletes the packet commitment, and does not unescrow to the sender.
- After the commitment is deleted, a later timeout is a no-op, so the normal timeout-refund path cannot recover the funds.
- The locked escrow balance can be counted in `Cosmos::build_outputs` for emergency disbursal output construction, which means failed-ack user funds may be treated as remote-chain escrow rather than user-refundable balance.
- This is a direct funds-locking/liveness failure for ICS-20 nBTC transfers. It is not an unbacked mint based on reviewed code.

Primary evidence:

- Nomic sends nBTC through `IbcDest::transfer`, minting transfer-module balance then calling `ibc.deliver_message(Ics20)`.
- ibc-rs core acknowledgement execution calls the module callback, then deletes the packet commitment.
- Upstream ibc-rs transfer ack execution refunds on unsuccessful ack.
- Orga's acknowledgement execution callback is empty.

Recommended fix:

Delegate Orga's `on_acknowledgement_packet_execute` to ibc-rs transfer's `on_acknowledgement_packet_execute`, mirroring the timeout callback style, and add a regression test where a valid error acknowledgement refunds `usat` to the Nomic sender and deletes the packet commitment exactly once.

Suggested regression:

1. Create/open a transfer channel in an isolated two-chain test.
2. Send Nomic `usat` to a receiver that the counterparty transfer app rejects, or inject a valid error acknowledgement from a local counterparty harness.
3. Relay the acknowledgement to Nomic.
4. Assert sender transfer balance or native claim path is restored, channel escrow decreases, packet commitment is removed, and a duplicate ack/timeout cannot double-refund.

### Finding 2 - Likely Medium: old host consensus heights can panic Orga IBC handshakes

Status: Likely. Source evidence is strong, but I did not build a complete local handshake PoC.

Affected path:

Relayer-submitted connection handshake messages (`ConnOpenTry`, `ConnOpenAck`) call ibc-rs connection handlers. Those handlers compare message-supplied local consensus heights against current host height, but then request the local host consensus state at that height from Orga.

Observed behavior:

Orga keeps at most `100_000` host consensus states. Its `host_consensus_state` computes:

`self.host_consensus_states.len() - 1 - (self.height - height.revision_height())`

and then unwraps the deque result. There is no checked subtraction, lower-bound test, revision-number check, or graceful "state pruned" error.

Impact:

If an IBC connection handshake reaches this path with a height that is not in the retained deque but is not greater than the current height, the subtraction can underflow or the lookup can unwrap `None`. With Nomic's release overflow checks enabled, this can become an ABCI panic instead of a rejected transaction. A malicious counterparty chain that already has a Nomic client may be able to satisfy earlier counterparty proof checks and still choose an old local consensus height to trigger the local lookup.

Recommended fix:

Change `host_consensus_state` to validate revision number and retention bounds with checked arithmetic, then return a `ContextError` when the requested state is unavailable. Add direct tests for current, oldest retained, too-old, future, and wrong-revision heights.

### Finding 3 - Confirmed Medium: `rest/` lockfile drift breaks reproducible IBC dependency auditing

Status: Confirmed.

Observed behavior:

- Root `Cargo.lock` pins Orga to the requested `3b3d25ade40d81cb64f19335535e3a47bb47778f`.
- `rest/Cargo.lock` still pins Orga to `35988d76b58008e37794064c41f3d0ba102ca0c8`.
- `rest/Cargo.toml` requests `ibc-proto = 0.47.0`, while `rest/Cargo.lock` resolves `ibc-proto = 0.47.1`.
- `cargo metadata --locked` from `rest/` fails with "the lock file ... needs to be updated".

Impact:

The REST crate is not the consensus IBC executor, but it is an operator-facing component that directly imports `ibc`/`ibc-proto` for IBC query formatting. A stale lockfile makes it easy to audit, build, or ship the wrong Orga/IBC graph, and it prevents locked build verification.

Recommended fix:

Regenerate `rest/Cargo.lock` against the current root manifest and Orga pin, or remove the nested lockfile if `rest/` should always build as part of the root workspace. Then add CI that runs `cargo metadata --locked` or `cargo check --locked` in `rest/`.

### Finding 4 - Likely Low/Medium: outbound `IbcDest::transfer` swallows send errors after minting local transfer balance

Status: Likely. No supply inflation found.

Observed behavior:

`IbcDest::transfer` mints `usat` into the transfer module for `self.sender`, then calls `ibc.deliver_message(Ics20(msg_transfer))`. If the ibc-rs send path returns an error, Nomic logs the error but still returns `Ok(())`.

Impact:

For invalid channels, invalid state, expired timeout, or other local send failures, the original native pending transfer is consumed and the caller sees success. The user may be able to recover by claiming/burning the local transfer-module balance back to native nBTC, but that is implicit and not tied to the failed destination. For Bitcoin interchain deposits, the sender is just data inside the deposit commitment, so a bad commitment can strand funds in a local IBC balance rather than following the normal deposit refund path.

Security interpretation:

This does not appear to create unbacked nBTC because the minted transfer-module balance is backed by nBTC removed from the native/pending path. It is still a state-machine clarity and funds-liveness risk, and it complicates incident response because failed sends look successful.

Recommended fix:

Either return the `deliver_message` error and let `try_credit_dest` perform the existing refund behavior, or explicitly model the local transfer-module refund as a first-class outcome with events/tests.

## Non-Findings And Positive Checks

- No literal `ibs-rs` crate or module was found.
- IBC message authorization is not purely relayer-trusted: Orga validates each IBC message signer against the authenticated SDK signer in `validate_message_signer`.
- Packet replay protection is delegated to ibc-rs and backed by Orga's `commitments`, `receipts`, `acks`, and sequence maps. I found no Nomic-side bypass of those maps.
- Incoming `usat`+memo handling did not show an obvious unbacked mint: Orga only records `incoming_transfer` for successful receiver-chain-source packets, and Nomic burns the receiver's transfer-module balance before minting native pending nBTC.
- Timeout refunds are wired to ibc-rs transfer logic, unlike acknowledgements.
- ICS-23 operator-key proof verification uses the Tendermint consensus state's commitment root and checks both the staking key and account key shapes. I did not find a generic invalid-proof acceptance. Residual risk remains around stale proof heights and current-validator-set matching.

## Additional Risk Notes

- `relay_op_key` verifies proofs against the supplied consensus height but checks validator membership against the client's most recent header. If a remote validator's operator key changes over time, a stale proof for a currently active consensus key may be accepted. This sits at the Nomic/IBC host integration layer and should get a separate model test.
- `IbcDest::is_fee_exempt` checks only `source_channel == "channel-1"` and does not also check the port/counterparty identity. If channel numbering or routing changes, fee exemption can apply more broadly than intended.
- `TransferInfo` uses `u64` amounts parsed from ibc-rs events. This is compatible with Nomic's `Amount`, but it intentionally rejects larger ICS-20 amounts rather than supporting full ICS-20 integer width.
- The REST IBC endpoints use many `unwrap()` calls on user path/query data. That is not a consensus safety issue, but a bad REST request can plausibly crash the sidecar.

## Next-Step Tests

High priority:

1. Regression for failed ICS-20 acknowledgement refund of Nomic-origin `usat`.
2. Regression that duplicate ack after refund cannot double-refund and timeout after ack is a no-op.
3. Regression for `host_consensus_state` at current, oldest retained, pruned, future, and wrong-revision heights.

Medium priority:

4. Fuzz `IbcTx::try_from(Tx)` and `RawIbcTx::decode` with malformed `Any` payloads and multi-message batches.
5. Model `IbcDest::transfer` local failure cases and assert final accounting: native account, transfer-module account, escrow account, reward fees, packet commitment.
6. Add a locked `rest/` build check to CI.
7. Add a stale operator-key proof test for `relay_op_key`.

## Verification And Limitations

Commands run:

- `git fetch origin security-audit`
- `cargo tree -i ibc --locked`
- `cargo tree -i ibc-proto --locked`
- `cargo metadata --locked --format-version 1`
- `cargo check --locked --lib --no-default-features`
- `cargo test --locked --lib dest_json --no-default-features`
- `cargo audit --version && cargo audit --file Cargo.lock`
- `cd rest && cargo metadata --locked --format-version 1`

Results:

- Root dependency metadata and cargo tree succeeded.
- `cargo check --locked --lib --no-default-features` failed because `src/incentives.rs` uses the optional `csv` crate when the `csv` feature is disabled.
- `cargo test --locked --lib dest_json --no-default-features` failed before tests due missing `pkg-config`/OpenSSL discovery from dev dependencies.
- `cargo audit` is not installed in this environment.
- `rest` locked metadata failed because `rest/Cargo.lock` needs to be updated.
- I did not run ignored Hermes/bitcoind IBC e2e tests, did not probe public networks, and did not use public Nomic infrastructure.

