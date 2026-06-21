# Source Anchors

Line numbers are from the working tree at `HEAD` `188d30cb34cce8c8febfe93f35e0db1b29a92300`; source files are unchanged from target `c5486a4a9b41de5474f991891ae22f734b1d5aec`.

## Nomic Core

- `src/app.rs:118`: `InnerApp` state container.
- `src/app.rs:362`: `InnerApp::relay_deposit`.
- `src/app.rs:403`: destination validation.
- `src/app.rs:542`: destination credit dispatch.
- `src/app.rs:620`: `withdraw_nbtc`.
- `src/app.rs:651`: `ibc_deliver`.
- `src/app.rs:963`: `BeginBlock` implementation.
- `src/app.rs:1008`: drains Bitcoin pending transfers.
- `src/app.rs:1013`: checkpoint push and external output build.
- `src/app.rs:1624`: `IbcDest::transfer`.
- `src/app.rs:1762`: `Dest` enum.
- `src/app.rs:1960`: `Dest::commitment_bytes`.
- `src/app.rs:1972`: `Dest::legacy_commitment_bytes`.
- `src/app.rs:1995`: `Dest::to_output_script`.
- `src/app.rs:2191`: upgrade window.

## Bitcoin Bridge

- `src/bitcoin/mod.rs:407`: `set_signatory_key`.
- `src/bitcoin/mod.rs:441`: `set_recovery_script`.
- `src/bitcoin/mod.rs:473`: `amount_after_deposit_fee`.
- `src/bitcoin/mod.rs:525`: `relay_deposit`.
- `src/bitcoin/mod.rs:584`: deposit commitment construction.
- `src/bitcoin/mod.rs:606`: processed outpoint replay protection.
- `src/bitcoin/mod.rs:615`: deposits-enabled rejection after replay insertion.
- `src/bitcoin/mod.rs:680`: `relay_checkpoint`.
- `src/bitcoin/mod.rs:740`: `withdraw`.
- `src/bitcoin/mod.rs:762`: `validate_withdrawal`.
- `src/bitcoin/mod.rs:808`: `add_withdrawal`.
- `src/bitcoin/mod.rs:891`: `sign`.
- `src/bitcoin/mod.rs:1096`: `take_pending`.
- `src/bitcoin/mod.rs:1125`: miner fee accounting.

## Header Queue

- `src/bitcoin/header_queue.rs:17`: `MAX_RELAY = 250`.
- `src/bitcoin/header_queue.rs:405`: header relay call.
- `src/bitcoin/header_queue.rs:434`: `add_into_iter`.
- `src/bitcoin/header_queue.rs:455`: reorg `pop_back_to`.
- `src/bitcoin/header_queue.rs:458`: verify/add replacement headers after pop.
- `src/bitcoin/header_queue.rs:501`: per-header validation loop.
- `src/bitcoin/header_queue.rs:522`: per-header append/current-work update.

## Checkpoints and Recovery

- `src/bitcoin/checkpoint.rs:1098`: `CheckpointQueue` fields.
- `src/bitcoin/checkpoint.rs:1376`: emergency disbursal time/context.
- `src/bitcoin/checkpoint.rs:1386`: account recovery script outputs.
- `src/bitcoin/checkpoint.rs:1399`: pending transfer emergency outputs.
- `src/bitcoin/checkpoint.rs:1424`: final emergency disbursal batching.
- `src/bitcoin/checkpoint.rs:1461`: intermediate recovery input.
- `src/bitcoin/checkpoint.rs:1477`: excess value back to signatory set.
- `src/bitcoin/checkpoint.rs:1990`: `maybe_step`.
- `src/bitcoin/checkpoint.rs:2110`: `should_push`.
- `src/bitcoin/checkpoint.rs:2230`: `maybe_push`.
- `src/bitcoin/checkpoint.rs:2287`: checkpoint signature submission.
- `src/bitcoin/recovery.rs:51`: expired deposit recovery transaction creation.

## Signatories and Threshold Signatures

- `src/bitcoin/signatory.rs:111`: signatory set from validator context.
- `src/bitcoin/signatory.rs:143`: `possible_vp` increments for all validators.
- `src/bitcoin/signatory.rs:145`: validators without xpub skipped from present set.
- `src/bitcoin/signatory.rs:339`: sort and truncate signatories.
- `src/bitcoin/signatory.rs:351`: signature threshold over `present_vp`.
- `src/bitcoin/signatory.rs:356`: quorum threshold over `possible_vp`.
- `src/bitcoin/signatory.rs:378`: quorum check.
- `src/bitcoin/threshold_sig.rs:182`: threshold set from signatory set.
- `src/bitcoin/threshold_sig.rs:199`: threshold over total present voting power.
- `src/bitcoin/threshold_sig.rs:231`: strict threshold comparison.
- `src/bitcoin/threshold_sig.rs:288`: signature verification before mutation.

## Cosmos, IBC, and Proofs

- `src/cosmos.rs:132`: `relay_op_key`.
- `src/cosmos.rs:151`: loads consensus state at supplied height.
- `src/cosmos.rs:161`: checks latest header validator set.
- `src/cosmos.rs:175`: verifies proofs.
- `src/cosmos.rs:178`: staking key path check.
- `src/cosmos.rs:185`: account key path check.
- `src/cosmos.rs:195`: decodes `Any.value` as secp256k1 pubkey.
- `src/cosmos.rs:276`: manual ICS-23 proof verification.
- Orga `src/ibc/mod.rs:71`: IBC state container.
- Orga `src/ibc/mod.rs:139`: `Ibc::deliver`.
- Orga `src/ibc/mod.rs:170`: `deliver_message`.

## FROST

- `src/frost/mod.rs:35`: participant config from staking.
- `src/frost/mod.rs:140`: DKG round 1 submission.
- `src/frost/mod.rs:159`: DKG round 2 submission.
- `src/frost/mod.rs:179`: pubkey attestation.
- `src/frost/mod.rs:192`: signing commitment submission.
- `src/frost/mod.rs:213`: signature share submission.
- `src/frost/dkg.rs:13`: DKG states.
- `src/frost/dkg.rs:49`: DKG state machine.
- `src/frost/signing.rs:57`: timeout iteration advance.
- `src/frost/signing.rs:101`: commitment submission with iteration check.
- `src/frost/signing.rs:128`: signature share submission without current-iteration check.

## Upgrades and Orga

- `src/app/migrations.rs:29`: V5 to V6 migration.
- `src/app/migrations.rs:90`: V6 to V7 `todo!()`.
- Orga `src/plugins/mod.rs:38`: `DefaultPlugins`.
- Orga `src/plugins/signer.rs:151`: signature verification.
- Orga `src/plugins/nonce.rs:96`: nonce validation and write-before-inner-call.
- Orga `src/plugins/payable.rs:162`: payer then paid call.
- Orga `src/abci/node.rs:458`: `InternalApp::run`.
- Orga `src/abci/node.rs:482`: operation result captured before flush.
- Orga `src/abci/node.rs:483`: flush regardless of operation result value.
- Orga `src/abci/node.rs:510`: `begin_block` through `run`.
- Orga `src/abci/node.rs:558`: `deliver_tx` through `run`.
- Orga `src/abci/node.rs:582`: inner error mapped to ABCI `code = 1`.
