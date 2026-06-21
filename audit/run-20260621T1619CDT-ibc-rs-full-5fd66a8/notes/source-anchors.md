# Source Anchors

All paths are local to `/home/nitro/repos/nomic` unless an absolute Cargo registry/git checkout path is shown.

## Target And Dependencies

- `Cargo.toml:10-13` - Nomic depends on Orga at `3b3d25ade40d81cb64f19335535e3a47bb47778f` with `merk-verify` and `feat-ibc`.
- `Cargo.toml:100-118` - default features include `full` and `testnet`; `full` enables `orga/abci`, `orga/state-sync`, and related runtime deps.
- `Cargo.toml:154-155` - root release profile enables overflow checks.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/Cargo.toml:45-51` - Orga `feat-ibc` dependency requests `ibc = 0.54.0`, `ibc-proto = 0.47.0`, and `ics23 = 0.12.0`.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/Cargo.toml:77-95` - Orga features: default empty; `abci`; `merk-verify`; `merk-full`; `feat-ibc = ["ibc", "ics23", "prost-types", "ibc-proto", "tendermint"]`.
- `Cargo.lock:3542` and `Cargo.lock:4023-4024` - root lock resolves `ibc = 0.54.0` and `ibc-proto = 0.47.1`.
- `Cargo.lock:4807-4809` - root lock pins Orga to `3b3d25ade40d81cb64f19335535e3a47bb47778f`.
- `rest/Cargo.toml:10-23` - REST depends on Nomic default/full/testnet and directly on `ibc = 0.54.0`, `ibc-proto = 0.47.0`.
- `rest/Cargo.lock:4044-4045` - REST lock resolves `ibc-proto = 0.47.1`.
- `rest/Cargo.lock:4875-4877` - REST lock pins Orga to old rev `35988d76b58008e37794064c41f3d0ba102ca0c8`.

## Nomic IBC Entrypoints

- `src/app.rs:159-164` - top-level `InnerApp` contains callable `pub ibc: Ibc`.
- `src/app.rs:260-280` - `ibc_transfer_nbtc` validates `IbcDest`, withdraws signer nBTC, charges fee, and inserts pending `Dest::Ibc`.
- `src/app.rs:284-294` - `ibc_withdraw_nbtc` burns transfer-module coins and deposits native nBTC to signer.
- `src/app.rs:351-359` - `escrowed_nbtc` and `claim_escrowed_nbtc` expose transfer-module balance claiming.
- `src/app.rs:362-385` - Bitcoin deposit relay validates `Dest` and passes it to Bitcoin relay logic.
- `src/app.rs:403-407` - `validate_dest` only calls `IbcDest::validate` for IBC destinations.
- `src/app.rs:481-539` - `try_credit_dest` refunds failed credits only to `NativeAccount`/`EthAccount`; `Identity::None` has no refund branch.
- `src/app.rs:542-552` - `credit_dest` sends `Dest::Ibc` to `IbcDest::transfer`.
- `src/app.rs:651-654` - `ibc_deliver` charges nBTC fee and calls Orga `self.ibc.deliver`.
- `src/app.rs:656-681` - Nomic interprets successful incoming `usat` transfers with memo as native nBTC: parse memo, burn receiver transfer balance, insert native pending.
- `src/app.rs:963-1011` - `begin_block` runs IBC begin block, drains pending nBTC transfers, and credits destinations.
- `src/app.rs:1013-1021` - checkpoint construction can include Cosmos/IBC emergency outputs from `Cosmos::build_outputs`.
- `src/app.rs:1055-1068` - SDK protobuf txs that parse as Orga `IbcTx` are routed to `ibc_deliver`.
- `src/app.rs:1394-1446` - Amino `nomic/MsgIbcTransferOut` parses channel/port/denom/sender/timeout/memo and calls `ibc_transfer_nbtc`.
- `src/app.rs:1614-1621` - `IbcDest` fields.
- `src/app.rs:1624-1659` - `IbcDest::transfer` charges fee, mints transfer-module coins, builds `MsgTransfer`, calls `ibc.deliver_message`, and logs send errors without returning them.
- `src/app.rs:1695-1698` - channel-only fee exemption for `channel-1`.
- `src/app.rs:1700-1705` - `IbcDest::validate` checks only port/channel parsing and sender address parsing.
- `src/app.rs:2168-2170` - IBC percentage fee calculation.

## CLI And Tests

- `src/bin/nomic.rs:1661-1682` - `interchain-deposit` constructs Bitcoin `Dest::Ibc` with one-day timeout.
- `src/bin/nomic.rs:1745-1767` - CLI `ibc-withdraw-nbtc` calls `ibc_withdraw_nbtc`.
- `src/bin/nomic.rs:1832-1848` - CLI `grpc` exposes Orga IBC gRPC query server.
- `src/bin/nomic.rs:1854-1902` - CLI `ibc-transfer` constructs `IbcDest` and calls `ibc_transfer_nbtc`.
- `tests/ibc.rs:229-232` - IBC e2e test exists but is ignored.
- `tests/ibc.rs:465-524` - e2e creates channel and relays incoming transfer.
- `tests/ibc.rs:592-647` - e2e sends Nomic nBTC out over IBC and clears packets.
- `tests/ibc.rs:680-722` - e2e sends IBC voucher back to Nomic with a withdrawal memo.

## Cosmos / Proof / Emergency Output Integration

- `src/cosmos.rs:72-130` - `build_outputs` iterates IBC clients/connections/channels, sums transfer escrow `Nbtc` balances, and builds remote-chain emergency outputs.
- `src/cosmos.rs:132-214` - `relay_op_key` loads IBC client, checks Tendermint client type, reads a consensus state root, verifies proofs, checks current validator-set membership, and stores operator key.
- `src/cosmos.rs:275-299` - `Proof::verify` verifies outer Tendermint-store and inner IAVL ICS-23 membership proofs.
- `src/cosmos.rs:343-392` - remote validator set to Bitcoin signatory set conversion.
- `src/cosmos.rs:397-515` - off-chain operator-key relayer queries remote RPC with proofs and submits `relay_op_key`.

## Orga IBC Integration

- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/mod.rs:69-77` - Orga `Ibc` contains `IbcContext` and router.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/mod.rs:85-130` - IBC host state maps: clients, connections, channel ends, sequences, commitments, receipts, acknowledgements.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/mod.rs:139-147` - `Ibc::deliver` iterates raw IBC messages and collects incoming transfer info.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/mod.rs:170-187` - `Ibc::deliver_message` dispatches ICS-26 messages to ibc-rs core and ICS-20 transfers to ibc-rs `send_transfer`.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/mod.rs:190-204` - update-client headers are cached and transfer module's `incoming_transfer` is returned.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/messages.rs:81-89` - raw bytes parse into Cosmos tx then IBC messages.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/messages.rs:100-124` - tx body messages are accepted if they parse as `MsgEnvelope` or `MsgTransfer`.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/router.rs:19-37` - only the transfer module is routed, and only for the transfer port.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/impls.rs:25-65` - IBC begin block stores host Tendermint consensus states and caps retention at `100_000`.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/impls.rs:79-105` - `validate_message_signer` checks IBC message signer against Orga signer context.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/impls.rs:134-150` - `host_consensus_state` uses unchecked arithmetic and `unwrap()`.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/impls.rs:176-178` - commitment prefix is `ibc`.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/impls.rs:236-272` - packet commitment, receipt, and acknowledgement lookup implementations.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/impls.rs:382-430` - packet commitment/receipt/ack storage and deletion implementations.

## Orga Transfer Module

- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/transfer.rs:46-55` - transfer module account map and transient `incoming_transfer`.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/transfer.rs:85-100` - ADR-028 escrow account derivation.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/transfer.rs:103-155` - validation hooks currently allow send/receive/mint/burn/escrow/unescrow.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/transfer.rs:187-265` - execution hooks implement burn, mint, escrow, and unescrow against Orga balances.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/transfer.rs:372-430` - receive callback delegates to ibc-rs and records `incoming_transfer` only on successful receiver-chain-source transfers.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/transfer.rs:433-444` - acknowledgement validation delegates to ibc-rs transfer validation.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/transfer.rs:446-452` - acknowledgement execution is an empty success callback and does not refund failed acknowledgements.
- `/home/nitro/.cargo/git/checkouts/orga-93bd515e30e7ca9a/3b3d25a/src/ibc/transfer.rs:455-480` - timeout validation/execution delegates to ibc-rs transfer timeout logic.

## ibc-rs Upstream Behavior

- `/home/nitro/.cargo/registry/src/index.crates.io-6f17d22bba15001f/ibc-app-transfer-0.54.0/src/module.rs:197-217` - upstream transfer acknowledgement validation requires refund validation on unsuccessful ack.
- `/home/nitro/.cargo/registry/src/index.crates.io-6f17d22bba15001f/ibc-app-transfer-0.54.0/src/module.rs:219-261` - upstream transfer acknowledgement execution refunds on unsuccessful ack and emits ack events.
- `/home/nitro/.cargo/registry/src/index.crates.io-6f17d22bba15001f/ibc-app-transfer-0.54.0/src/module.rs:280-309` - upstream timeout execution refunds.
- `/home/nitro/.cargo/registry/src/index.crates.io-6f17d22bba15001f/ibc-app-transfer-0.54.0/src/handler/mod.rs:14-40` - refund execution unescrows if sender chain is source, otherwise mints vouchers back.
- `/home/nitro/.cargo/registry/src/index.crates.io-6f17d22bba15001f/ibc-core-channel-0.54.0/src/handler/acknowledgement.rs:70-78` - ibc-rs core calls module ack execute and then deletes the packet commitment.
- `/home/nitro/.cargo/registry/src/index.crates.io-6f17d22bba15001f/ibc-core-connection-0.54.0/src/handler/conn_open_try.rs:50-60` - connection open try rejects future local consensus height.
- `/home/nitro/.cargo/registry/src/index.crates.io-6f17d22bba15001f/ibc-core-connection-0.54.0/src/handler/conn_open_try.rs:119-123` - then calls host `host_consensus_state` for message-supplied local consensus height.
- `/home/nitro/.cargo/registry/src/index.crates.io-6f17d22bba15001f/ibc-core-connection-0.54.0/src/handler/conn_open_ack.rs:40-49` - connection open ack rejects future local consensus height.
- `/home/nitro/.cargo/registry/src/index.crates.io-6f17d22bba15001f/ibc-core-connection-0.54.0/src/handler/conn_open_ack.rs:123-127` - then calls host `host_consensus_state` for message-supplied local consensus height.

## REST IBC Query Surface

- `rest/src/main.rs:24-29` - REST imports `ibc` and `ibc-proto` for Tendermint client and connection response shaping.
- `rest/src/main.rs:1251-1328` - connection client-state endpoint unwraps connection/client parsing and converts to raw Tendermint client state.
- `rest/src/main.rs:1330-1374` - connection channels endpoint queries Orga IBC connection channels and maps states/orderings.
- `rest/src/main.rs:1376-1409` - connection endpoint converts Orga connection end to `ibc-proto` raw connection end.
- `rest/src/main.rs:1411-1429` - all connections endpoint queries Orga IBC connections.

## Build/Test Limitations Anchors

- `src/incentives.rs:54` and `src/incentives.rs:73` - no-default-features `cargo check` fails because `csv` is referenced without the optional dependency enabled.
- Local command output: `cargo test --locked --lib dest_json --no-default-features` failed in `openssl-sys` because `pkg-config` is missing.
- Local command output: `cargo audit` is unavailable.
- Local command output: `cd rest && cargo metadata --locked --format-version 1` fails because `rest/Cargo.lock` needs updating.

