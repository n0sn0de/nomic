# Phase 1 Architecture and Trust Map

Run: `run-20260621T1244CDT-phase1-c5486a4`

Target source commit: `c5486a4a9b41de5474f991891ae22f734b1d5aec`

Working tree note: current `HEAD` was `188d30cb34cce8c8febfe93f35e0db1b29a92300`; the diff from the target commit only adds prior audit artifacts under `audit/run-20260621T0354CDT-c5486a4/`. Source analysis below treats `c5486a4` as the target source. Pinned Orga dependency source was inspected locally at cargo git checkout revision `3b3d25ade40d81cb64f19335535e3a47bb47778f`.

Safety: no public network, Bitcoin, IBC, RPC, frontend, relayer, signer, validator, or third-party infrastructure was contacted. This phase is source-level architecture review plus dependency tracing.

## Executive Summary

Nomic is an Orga ABCI application whose core state is an `InnerApp` containing NOM accounts/staking, nBTC/Bitcoin bridge state, IBC state, upgrade state, Cosmos auxiliary state, and feature-gated Ethereum/Babylon/FROST modules (`src/app.rs:118`). Most security boundaries enter through Orga's default transaction plugin chain, then dispatch into module `#[call]` methods.

The strongest architectural observation is that the pinned Orga ABCI wrapper flushes state even when an ABCI operation returns an error. `InternalApp::run` calls the operation and then unconditionally flushes state (`orga/src/abci/node.rs:482` to `:486`). `deliver_tx` catches the inner call result as data and returns `code = 1` only after `run` has already flushed (`orga/src/abci/node.rs:558` to `:590`). `begin_block` and `end_block` use the same `run` helper (`orga/src/abci/node.rs:510` to `:539`). Therefore every path that mutates state before a fallible validation or before a later fallible call is consensus-critical.

This observation cross-links directly to prior findings and adds several high-value follow-up leads. In particular, Bitcoin header reorg handling pops the old best chain before validating the replacement (`src/bitcoin/header_queue.rs:455` then `:458`), deposit relay marks an outpoint processed before checking `deposits_enabled` (`src/bitcoin/mod.rs:606` to `:619`), withdrawal debits nBTC before validating the Bitcoin output (`src/bitcoin/mod.rs:749` to `:813`), and FROST signature shares lack a current-iteration check (`src/frost/signing.rs:128` to `:151`).

## Module and Call Graph

Primary state container:

```text
InnerApp
  accounts: Accounts<Nom>
  staking: Staking<Nom>
  airdrop: Airdrop
  community_pool / incentive_pool / reward faucets
  bitcoin: Bitcoin
    headers: HeaderQueue
    checkpoints: CheckpointQueue
    accounts: Accounts<Nbtc>
    signatory_keys: Map<ConsensusKey, Xpub>
    recovery_scripts: Map<Address, Script>
    processed_outpoints: OutpointSet
    recovery_txs: RecoveryTxs
    fee_pool / reward_pool
  ibc: Orga Ibc
  upgrade: Upgrade
  incentives: Incentives
  cosmos: Cosmos
  ethereum / babylon / frost: feature-gated
```

External call graph:

```text
ABCI RequestDeliverTx bytes
  -> orga ABCIPlugin::DeliverTx
  -> DefaultPlugins
     QueryPlugin
     SdkCompatPlugin
     SignerPlugin
     ChainCommitmentPlugin
     NoncePlugin
     PayablePlugin
     FeePlugin
     InnerApp call dispatcher

InnerApp.relay_deposit
  -> Bitcoin.amount_after_deposit_fee
  -> InnerApp.validate_dest
  -> Bitcoin.relay_deposit
     -> HeaderQueue.get_by_height
     -> PartialMerkleTree.extract_matches
     -> SignatorySet.output_script / Dest.commitment_bytes
     -> processed_outpoints.insert
     -> CheckpointQueue.building_mut
     -> CheckpointTx.input.push_back
     -> Bitcoin.insert_pending

Bitcoin.headers.add
  -> HeaderQueue.add_into_iter
     -> possible pop_back_to for reorg
     -> verify_and_add_headers
        -> validate_time
        -> get_next_target
        -> header.validate_pow
        -> deque.push_back

InnerApp.withdraw_nbtc
  -> Bitcoin.withdraw
     -> Accounts<Nbtc>.withdraw
     -> Bitcoin.add_withdrawal
        -> validate_withdrawal
        -> give_miner_fee
        -> CheckpointTx.output.push_back

BeginBlock
  -> Upgrade.step
  -> Staking.begin_block
  -> Ibc.begin_block
  -> NOM reward faucet mints
  -> feature-gated FROST/Ethereum steps
  -> Bitcoin.take_pending
  -> InnerApp.try_credit_dest
  -> if should_push_checkpoint: Cosmos.build_outputs
  -> Bitcoin.begin_block_step
  -> Staking.punish_downtime
  -> nBTC reward distribution

Bitcoin.begin_block_step
  -> CheckpointQueue.maybe_step
     -> should_push
     -> maybe_push
     -> BuildingCheckpointMut.advance
     -> emergency disbursal generation
     -> new reserve input construction

Bitcoin.sign
  -> CheckpointQueue.sign
     -> Checkpoint.sign
     -> ThresholdSig.sign
     -> if signed: SigningCheckpointMut.advance

InnerApp.ibc_deliver
  -> deduct_nbtc_fee
  -> Orga Ibc.deliver
     -> ibc-rs dispatch/send_transfer
     -> incoming_transfer collection
  -> for `usat` transfers with memo Dest:
     -> burn transferred IBC nBTC
     -> Bitcoin.insert_pending

InnerApp.relay_op_key
  -> deduct_nbtc_fee
  -> Cosmos.relay_op_key
     -> Orga Ibc client lookup
     -> Tendermint client consensus root lookup
     -> ICS-23 outer/inner proof verification
     -> BaseAccount pubkey decode
     -> Cosmos.chains[client_id].op_keys_by_cons.insert
```

Dependency-sensitive call graph:

```text
Orga InternalApp::run
  load root state
  op(&state)
  flush state bytes
  store.put(root, bytes)

Orga SignerPlugin
  verifies Native / ADR36 / SDK / EthPersonalSign signatures
  provides Signer context

Orga NoncePlugin
  enforces monotonic nonce with max increment 1000
  writes nonce before inner call

Orga PayablePlugin
  runs payer subcall, then paid subcall with Paid context

Orga Ibc
  stores clients, connections, channels, sequences, commitments, receipts, acks under absolute prefixes
  delegates protocol validation to ibc-rs handlers

ICS-23
  `src/cosmos.rs` manually verifies Tendermint store proof then IAVL proof for staking/account keys
```

## State-Transition Map

### ABCI and Persistence

```text
Committed store root
  -> BeginBlock / DeliverTx / EndBlock loads root state
  -> module mutates in-memory Orga state
  -> Orga run flushes in-memory state to Merkle store
  -> ABCI response code/log is computed
  -> Tendermint commits app hash at block boundary
```

Important source-level behavior: the Orga wrapper does not provide automatic rollback for a failed inner call. For `DeliverTx`, the inner result is captured into `res`, but the closure returns `Ok((res, events, logs))`; `InternalApp::run` then flushes state before `deliver_tx` maps `res` to `code = 1`. For `BeginBlock` and `EndBlock`, `run` also flushes after the closure returns a `Result`, even if that result is `Err`. This makes mutation order part of the consensus safety model.

### Bitcoin Header Queue

```text
initial checkpoint header
  -> HeaderQueue.add(headers)
  -> append connected valid headers
  -> optional reorg if first replacement height <= current height
  -> prune if queue exceeds max length
```

Validation includes height continuity, previous block hash, median-time-past after enough context, difficulty target, and proof of work. Reorg processing removes the current chain suffix before replacement headers are verified, which is a key mutation-before-validation lead.

### Deposit and Pending Credit

```text
Bitcoin UTXO paying signatory set with Dest commitment
  -> relayer submits tx, height, merkle proof, vout, sigset index, Dest
  -> Nomic verifies header confirmation and merkle inclusion
  -> verifies output script against active/legacy Dest commitment
  -> records processed outpoint
  -> if deposit too old: create recovery tx
  -> else add UTXO input to Building checkpoint and insert pending nBTC transfer
  -> checkpoint reaches Complete
  -> BeginBlock drains completed checkpoint pending transfers
  -> credit Dest: native account / IBC transfer / Bitcoin withdrawal / feature-gated destinations
```

Deposits are not economically finalized for the destination until the checkpoint that spends them is fully signed and its pending transfers are drained in `BeginBlock`.

### Checkpoint Queue

```text
Building
  -> should_push true
  -> maybe_push creates next Building and previous Building advances to Signing
Signing
  -> signatories submit threshold signatures
  -> signed threshold reached
  -> Complete
Complete
  -> checkpoint transaction can be broadcast externally
  -> relay_checkpoint proves Bitcoin inclusion
  -> confirmed_index updated
```

`CheckpointQueue` stores ordered checkpoints, the current building index, and optional `confirmed_index` (`src/bitcoin/checkpoint.rs:1098`). Pushing a checkpoint depends on signatory quorum, min/max checkpoint intervals, pending work, fee collection, and a cap on unconfirmed checkpoints (`src/bitcoin/checkpoint.rs:2110` to `:2221`).

## Lifecycle Maps

### Bitcoin Deposit Lifecycle

1. User constructs a Bitcoin transaction output to `SignatorySet.output_script(Dest.commitment_bytes(), threshold)`.
2. Relayer submits `InnerApp.relay_deposit` with transaction, Bitcoin height, merkle proof, output index, signatory-set index, and `Dest` (`src/app.rs:362`).
3. App estimates amount after fees and validates the destination before forwarding to `Bitcoin.relay_deposit` (`src/app.rs:372` to `:384`).
4. `Bitcoin.relay_deposit` checks local header exists, required confirmations, merkle proof root and txid, vout bounds, minimum deposit, and output script match (`src/bitcoin/mod.rs:541` to `:603`).
5. Outpoint replay protection uses `processed_outpoints`; timeout is derived from signatory-set create time plus `max_deposit_age` (`src/bitcoin/mod.rs:606` to `:613`).
6. If deposits are disabled for the target checkpoint, the call returns an error (`src/bitcoin/mod.rs:615` to `:619`).
7. If the deposit is expired, a recovery transaction is created to move the funds from the old signatory set to the current building signatory set (`src/bitcoin/mod.rs:621` to `:633`; `src/bitcoin/recovery.rs:51` to `:87`).
8. Otherwise, the deposit becomes a checkpoint input, miner/deposit fees are taken, and pending nBTC is inserted for later destination credit (`src/bitcoin/mod.rs:640` to `:672`).
9. BeginBlock drains pending transfers from the latest completed checkpoint and credits the destination (`src/bitcoin/mod.rs:1101` to `:1122`; `src/app.rs:1008` to `:1011`).

Prior finding cross-link: `NOMIC-AUD-001` lives in step 4, where legacy commitment matching computes `expected_script` with `dest_bytes` instead of the loop variable `bytes` (`src/bitcoin/mod.rs:588` to `:595`). `NOMIC-AUD-004` lives in step 3/8, where pre-validation does not subtract the miner fee from the amount returned by `amount_after_deposit_fee` (`src/bitcoin/mod.rs:500` to `:505`).

### Bitcoin Withdrawal Lifecycle

1. Signed user calls `InnerApp.withdraw_nbtc` with a Bitcoin script and amount (`src/app.rs:620`).
2. `Bitcoin.withdraw` reads the `Signer` context and withdraws nBTC from the signer account (`src/bitcoin/mod.rs:743` to `:750`).
3. `Bitcoin.add_withdrawal` validates script length, no OP_RETURN, min checkpoint count, miner fee coverage, minimum amount, and dust (`src/bitcoin/mod.rs:762` to `:803`).
4. Miner fee is burned into `fee_pool`/building checkpoint fee collection.
5. Remaining sats become a checkpoint output in the current building checkpoint (`src/bitcoin/mod.rs:822` to `:835`).
6. When the checkpoint is signed and externally broadcast, Bitcoin miners can confirm the withdrawal transaction.

Mutation-order note: the account withdrawal occurs before validation (`src/bitcoin/mod.rs:749` before `:813`). Under Orga failed-call persistence this is a self-loss/liveness lead, not yet an externally profitable theft path.

### Checkpoint Lifecycle

1. BeginBlock determines whether a checkpoint should be pushed (`src/app.rs:1013` to `:1021`).
2. `CheckpointQueue.should_push` enforces no existing Signing checkpoint, time interval, no Bitcoin header backfill, pending work or max interval, fee collection, max unconfirmed checkpoints, and signatory quorum (`src/bitcoin/checkpoint.rs:2110` to `:2221`).
3. `maybe_push` creates a new `Checkpoint` from the current validator signatory keys and sets `deposits_enabled` for the new building checkpoint (`src/bitcoin/checkpoint.rs:2230` to `:2262`).
4. If the previous checkpoint exists, `maybe_step` advances it: generates the concrete Bitcoin checkpoint transaction, emergency disbursal transactions, reserve output, excess inputs/outputs, and fee-rate adjustment (`src/bitcoin/checkpoint.rs:1990` to `:2101`).
5. Validators/signatories submit signatures through `Bitcoin.sign` and `CheckpointQueue.sign` (`src/bitcoin/mod.rs:891`; `src/bitcoin/checkpoint.rs:2287` to `:2310`).
6. Once all threshold signatures are present, the checkpoint advances from Signing to Complete and can be broadcast externally.
7. `relay_checkpoint` proves inclusion in sufficiently confirmed Bitcoin headers and updates `confirmed_index` (`src/bitcoin/mod.rs:680` to `:735`).

### Emergency Recovery Lifecycle

1. Users can set a recovery script signed by their account; script length is bounded by the withdrawal script length config (`src/bitcoin/mod.rs:441` to `:457`).
2. During checkpoint advance, emergency disbursal generation enumerates nBTC accounts with recovery scripts, pending checkpoint transfers whose `Dest` can be rendered to an output script, and remote-chain external outputs (`src/bitcoin/checkpoint.rs:1386` to `:1429`).
3. Outputs under `emergency_disbursal_min_tx_amt` are skipped (`src/bitcoin/checkpoint.rs:1432` to `:1435`).
4. Final emergency disbursal transactions are batched, linked through an intermediate transaction, locktimed, and then fees are deducted (`src/bitcoin/checkpoint.rs:1424` to `:1502`).
5. Excess reserve value is paid back to the signatory set with a `[0]` commitment script and must be handled out of band by signatories (`src/bitcoin/checkpoint.rs:1477` to `:1491`).
6. Pending expired deposits use a separate recovery transaction path that recreates the destination commitment against the newer signatory set (`src/bitcoin/recovery.rs:51` to `:87`).

### Threshold-Signing Lifecycle

1. Validators with a declared signatory xpub become Bitcoin signatories for a checkpoint. `possible_vp` includes all validators, while `present_vp` includes only validators with signatory keys (`src/bitcoin/signatory.rs:139` to `:154`).
2. Signatories are sorted and truncated to `MAX_SIGNATORIES`; removed signatories reduce `present_vp` (`src/bitcoin/signatory.rs:339` to `:346`).
3. Signatory quorum requires `present_vp >= possible_vp / 2` (`src/bitcoin/signatory.rs:356` to `:379`).
4. Signature threshold is computed over `present_vp` (`src/bitcoin/signatory.rs:351` to `:352`; `src/bitcoin/threshold_sig.rs:199` to `:202`).
5. For each Bitcoin input, signatures are submitted by xpub-derived pubkeys, verified against the input sighash, and accumulated by voting power (`src/bitcoin/threshold_sig.rs:288` to `:307`).
6. A transaction is signed once accumulated voting power is strictly greater than the threshold (`src/bitcoin/threshold_sig.rs:231` to `:235`).

Prior likely finding cross-link: `NOMIC-AUD-L001` is structurally present in steps 1 to 4: if barely over half of total voting power submits signatory keys, the spending threshold is a fraction of that present set rather than a fraction of total validator voting power.

### FROST DKG and Signing Lifecycle

Feature-gated under `all(feature = "frost", feature = "testnet")`.

1. `Frost::Config::from_staking` derives participants from staking state, but sorts ascending by stake and maps absent validators to key `0` (`src/frost/mod.rs:35` to `:64`).
2. A DKG group moves through `Round1 -> Round2 -> Attesting -> Complete` based on counts (`src/frost/dkg.rs:13` to `:63`).
3. Signed participants submit round 1 packages, round 2 packages, and pubkey attestations through group share ranges (`src/frost/mod.rs:140` to `:189`).
4. Signing moves through Round1 commitments and Round2 signature shares. Commitment submission validates `iteration == self.iteration` (`src/frost/signing.rs:101` to `:123`).
5. Signature share submission checks duplicate shares and whether commitments exist for `(iteration, participant)`, but does not check `iteration == self.iteration` (`src/frost/signing.rs:128` to `:151`).
6. Timeout advances iteration and resets counters, but old commitment/share maps remain (`src/frost/signing.rs:57` to `:70`).

Prior confirmed finding cross-link: `NOMIC-AUD-002` is step 1. New lead: stale-iteration signature shares can advance `sig_shares_len` and potentially trigger a failed aggregation against current-iteration shares.

### IBC Packet Lifecycle

1. Permissionless relayer submits `InnerApp.ibc_deliver` with raw IBC messages (`src/app.rs:651`).
2. App deducts an nBTC fee, then passes messages to Orga IBC (`src/app.rs:653` to `:654`).
3. Orga IBC decodes and dispatches each message through ibc-rs handlers or ICS-20 transfer send (`orga/src/ibc/mod.rs:139` to `:188`).
4. Orga IBC stores clients, consensus states, connections, channels, send/receive/ack sequences, commitments, receipts, and acknowledgements under absolute prefixes (`orga/src/ibc/mod.rs:85` to `:130`).
5. Incoming `usat` transfers with a parseable `Dest` memo are burned from the IBC transfer module and inserted into Bitcoin pending nBTC transfers (`src/app.rs:656` to `:680`).
6. Outgoing IBC destinations from deposits mint into the transfer module, construct an ICS-20 transfer, and call `ibc.deliver_message`; errors are logged and suppressed (`src/app.rs:1640` to `:1657`).

### ICS-23 Proof-Verification Lifecycle

1. `relay_op_key` takes `client_id`, IBC height, consensus key, staking operator-address proof, and auth account proof (`src/cosmos.rs:132` to `:140`).
2. It loads the IBC client and requires a Tendermint client type (`src/cosmos.rs:141` to `:149`).
3. It loads the consensus state root for the supplied height (`src/cosmos.rs:151` to `:157`).
4. It hashes `cons_key` into a Tendermint consensus address and checks that address in the client's latest validator set (`src/cosmos.rs:159` to `:173`).
5. It verifies outer Tendermint store membership and inner IAVL membership using ICS-23 specs for the `staking` and `acc` stores (`src/cosmos.rs:175` to `:176`; `src/cosmos.rs:276` to `:299`).
6. It validates exact proof keys for `staking` operator address and `acc` account path (`src/cosmos.rs:178` to `:189`).
7. It decodes `BaseAccount.pub_key.value` as a secp256k1 pubkey and records `cons_key -> op_key` (`src/cosmos.rs:192` to `:211`).

New lead: step 4 checks membership in `client.last_header()` rather than the validator set corresponding to the proved historical root, and step 7 does not check `Any.type_url`.

### Upgrade and Migration Lifecycle

1. `BeginBlock` calls `upgrade.step(current_consensus_version, in_upgrade_window(now))` before most other block logic (`src/app.rs:963` to `:970`).
2. Mainnet upgrade windows are weekdays from 17:00 to 17:10 UTC; testnet has no restriction (`src/app.rs:2191` to `:2202`).
3. `InnerApp` declares Orga state versions 5 through 7 while `CONSENSUS_VERSION` is 14 (`src/app.rs:118`; `src/app.rs:205`).
4. V5 to V6 migration changes Bitcoin checkpoint/header config, wipes/reseeds headers on mainnet, and backfills checkpoint scripts from included CSV (`src/app/migrations.rs:29` to `:87`).
5. V6 to V7 migration is `todo!()` (`src/app/migrations.rs:90` to `:95`), so the actual enabled feature set and binary target matter for upgrade safety.

Prior confirmed finding cross-link: `NOMIC-AUD-003` is build/release adjacent and affects reproducibility of source validation with `--no-default-features`.

## Privilege and Capability Map

```text
Unsigned relayer
  - Bitcoin header relay, deposit relay, checkpoint relay, checkpoint signatures where exempted by module logic, IBC relay messages, some feature-gated relays.

Signed account
  - nBTC withdrawal, native transfers, recovery script declaration, fee-paying IBC relay/operator-key relay, upgrade signal, feature-gated user actions.

Validator operator account
  - signatory xpub declaration if the signer has a staking consensus key.
  - FROST DKG/signing submissions for assigned participant share ranges.
  - Bitcoin checkpoint signatures if holding the corresponding xpub private key.

Nomic consensus validators
  - order transactions, set Tendermint time/header context, execute BeginBlock/EndBlock, produce app hash, enforce upgrade activation.

Bitcoin miners/full-node model
  - external source of proof-of-work finality for deposits and checkpoint confirmations.
  - choose whether checkpoint transactions are included and at what fee pressure.

IBC relayers and remote chains
  - submit client updates, channel/packet messages, and ICS-20 transfer packets.
  - remote chain consensus root is trusted only through IBC light-client verification.

Bitcoin signatories
  - external custody of xpub-derived private keys and coordination for checkpoint, recovery, and emergency disbursal transaction broadcast.

Build/release operator
  - selects feature set and binary build inputs such as generated proto/legacy artifacts.
```

## External-Input Map

```text
ABCI transaction bytes
  -> Orga Decode / SdkCompat / Signer / Nonce / Payable / Fee / InnerApp call

Bitcoin block headers
  -> HeaderQueue.add -> PoW/difficulty/time/reorg logic

Bitcoin transaction + partial merkle proof + height
  -> relay_deposit or relay_checkpoint -> local header queue confirmation checks

Bitcoin scripts and Dest encodings
  -> withdrawal outputs, deposit commitments, emergency recovery outputs

Validator xpubs and signatures
  -> signatory key map, threshold signatures, recovery tx signatures

IBC messages
  -> ibc-rs handlers via Orga Ibc, transfer module state, packet sequences/commitments

ICS-23 proofs
  -> Cosmos.relay_op_key -> Tendermint store/IAVL proof verification

Tendermint block context
  -> BeginBlock time, height, hash, proposer, validator set updates, upgrade window

Feature-gated Ethereum/Babylon/FROST inputs
  -> present in source but not primary mainnet default unless features enabled

Migration embedded files
  -> checkpoint JSON, reserve-script CSV, generated proto/legacy build outputs
```

## Cross-Chain Finality Map

```text
Nomic finality
  - Tendermint commits app state after ABCI block execution.
  - Failed call persistence in Orga changes what "failed" means for state effects.

Bitcoin header finality
  - HeaderQueue tracks best chain by cumulative work and configured network params.
  - Deposit finality uses `min_confirmations`.
  - Checkpoint confirmation uses `min_checkpoint_confirmations`.
  - Reorg replacement requires more work than removed work, but removal precedes validation.

Bitcoin deposit economic finality
  - A deposit UTXO is not credited immediately.
  - It becomes an input in a Building checkpoint, is signed into a Complete checkpoint, then pending nBTC is credited in BeginBlock.
  - The checkpoint transaction must still be broadcast and confirmed on Bitcoin to complete custody movement.

Bitcoin withdrawal finality
  - nBTC is debited and a checkpoint output is queued.
  - The user receives BTC only after signatories complete the checkpoint transaction and Bitcoin confirms it.

IBC finality
  - IBC packet proofs rely on light-client consensus states stored in Orga IBC.
  - Packet replay is sequence/commitment/receipt based in ibc-rs.
  - Nomic-specific incoming transfers convert remote `usat` with memo `Dest` into pending nBTC.

Cosmos operator-key finality
  - `relay_op_key` verifies account/staking proofs against an IBC consensus root for a supplied height.
  - It separately checks consensus-key membership against the latest cached Tendermint header, creating a temporal consistency question.

Emergency recovery finality
  - Emergency disbursal transactions are constructed at checkpoint advance time using Nomic nBTC account state, pending transfers, external outputs, and recovery scripts.
  - Their Bitcoin spendability depends on signatory signatures, locktime, and Bitcoin confirmation.
```

## Persistence and Crash-Recovery Map

Persistent consensus state is Orga-encoded under the ABCI app root. Maps and deques are stored in Merkle-backed Orga collections. Important persistent objects:

- `Bitcoin.headers.deque/current_work`: trusted Bitcoin header view.
- `Bitcoin.checkpoints.queue/index/confirmed_index`: bridge custody state machine.
- `Bitcoin.accounts`: nBTC supply ownership.
- `Bitcoin.processed_outpoints`: deposit replay protection and timeout index.
- `Bitcoin.signatory_keys`: validator consensus key to Bitcoin xpub.
- `Bitcoin.recovery_scripts`: user emergency destinations.
- `Bitcoin.recovery_txs`: expired deposit recovery queue.
- `Bitcoin.fee_pool/reward_pool`: nBTC fee/reward accounting.
- `Ibc.ctx`: clients, consensus states, connections, channels, sequences, commitments, receipts, acknowledgements.
- `Cosmos.chains`: remote consensus key to operator key map.
- `Frost.groups`: DKG packages, attestation counts, signing commitments/shares, signature state.
- `Upgrade`: signaled/upgrading version state.

Crash recovery follows the last committed Merkle root. The main persistence risk identified in this phase is not crash replay itself, but that errored ABCI operations can flush partial state. Therefore crash-recovery invariants must be written as "no fallible path mutates before all validation needed for that path" rather than assuming transactional rollback.

## Trust Boundary Inventory

Each boundary lists: input source, authentication, authorization, replay protection, freshness, domain separation, validation, state mutated, economic effect, failure behavior, resource bound, and consensus-criticality.

### TB-01: Native/SDK Transaction Envelope

- Input source: ABCI transaction bytes from peers/mempool/proposer.
- Authentication: Orga `SignerPlugin` verifies Native, ADR36, SDK, or Ethereum-personal signatures (`orga/src/plugins/signer.rs:151` to `:243`).
- Authorization: inner calls inspect `Signer` context; unsigned calls are accepted only where module code permits it.
- Replay protection: `NoncePlugin` requires signed-call nonce greater than stored nonce and within increment limit 1000 (`orga/src/plugins/nonce.rs:96` to `:117`).
- Freshness: nonce and SDK chain-id sign bytes; block time/height context for BeginBlock.
- Domain separation: Native hashes raw call bytes; SDK includes chain id and nonce; ADR36 has JSON sign/MsgSignData form; Ethereum path uses Ethereum personal-sign prefix.
- Validation performed: decode, signature, nonce, fee/payable rules, module call validation.
- State mutated: nonce map before inner call, payer/paid effects, fee deductions, module state.
- Economic effect: fees, account movements, bridge state changes.
- Failure behavior: failed inner calls may still persist earlier mutations because Orga flushes state after the operation.
- Resource bound: payable subcall encoded length cap 200,000; nonce jump cap 1000; module-specific bounds.
- Consensus-critical: yes.

### TB-02: Bitcoin Header Relay

- Input source: permissionless header list via `Bitcoin.headers.add`.
- Authentication: Bitcoin proof-of-work, not a signer.
- Authorization: no account authorization; fee exempt in module (`src/bitcoin/header_queue.rs:405` to `:418`).
- Replay protection: rejects redundant first header; chain continuity and work checks.
- Freshness: cumulative-work best chain and median-time-past checks.
- Domain separation: network-specific header config; Bitcoin header serialization/hash/target rules.
- Validation performed: max 250 headers, consecutive heights, prev-hash link, median time, retarget target, proof of work.
- State mutated: header deque and cumulative work; old chain suffix may be popped for reorg.
- Economic effect: controls deposit/checkpoint proof validity and Bitcoin finality view.
- Failure behavior: intended comment says invalid headers do not modify state, but source pops/appends before all validation. Under Orga failed-call persistence this can persist partial state.
- Resource bound: `MAX_RELAY = 250`, header queue max length.
- Consensus-critical: yes.

### TB-03: Bitcoin Deposit Relay

- Input source: permissionless Bitcoin tx, merkle proof, height, vout, sigset index, destination.
- Authentication: Bitcoin header PoW plus merkle inclusion proof.
- Authorization: none beyond valid proof/output; fee exempt.
- Replay protection: `processed_outpoints` keyed by `(txid, vout)`.
- Freshness: minimum confirmations and deposit age timeout from signatory-set create time.
- Domain separation: `Dest.commitment_bytes` includes version byte plus SHA-256 of Orga-encoded `Dest`; legacy commitment path exists for Native/Ibc.
- Validation performed: header height, confirmation depth, merkle root/txid, vout bounds, min amount, script matches signatory set, dest prevalidation.
- State mutated: processed outpoints, recovery tx queue, checkpoint input, fee pool, pending transfers.
- Economic effect: mints nBTC accounting for BTC deposit; later credits destination; collects miner/deposit fees.
- Failure behavior: processed outpoint is inserted before `deposits_enabled` check; any later error can poison replay state if failed calls persist.
- Resource bound: one merkle proof/one txid, min amount, checkpoint/input capacity via Orga/transaction bounds.
- Consensus-critical: yes.

### TB-04: Bitcoin Checkpoint Confirmation Relay

- Input source: permissionless Bitcoin merkle proof for a checkpoint transaction.
- Authentication: local Bitcoin header queue and merkle inclusion.
- Authorization: fee exempt, no signer required.
- Replay protection: rejects `cp_index <= confirmed_index`.
- Freshness: `min_checkpoint_confirmations`.
- Domain separation: txid must equal locally constructed checkpoint tx for `cp_index`.
- Validation performed: header exists, confirmation depth, merkle root/txid count, txid equality.
- State mutated: `checkpoints.confirmed_index`.
- Economic effect: declares custody checkpoint confirmed and affects unconfirmed checkpoint limits/fee adjustment.
- Failure behavior: validation mostly precedes mutation; still consensus-critical under Orga persistence.
- Resource bound: one merkle proof/one txid.
- Consensus-critical: yes.

### TB-05: nBTC Withdrawal and Direct Bitcoin Destination

- Input source: signed account call or deposit/pending `Dest::Bitcoin`.
- Authentication: signer for direct withdrawal; deposit path uses Bitcoin proof auth.
- Authorization: account balance for signed withdrawal; destination validation for deposit path.
- Replay protection: signed nonce for direct withdrawal; checkpoint output state for pending destination.
- Freshness: min withdrawal checkpoints and current building checkpoint.
- Domain separation: Bitcoin output script rules; no `Dest` commitment once converted to a withdrawal output.
- Validation performed: script length, not OP_RETURN, min checkpoint count, fee coverage, min amount, dust.
- State mutated: nBTC accounts, fee pool, building checkpoint outputs.
- Economic effect: burns/locks nBTC accounting and creates BTC output in future checkpoint.
- Failure behavior: `Bitcoin.withdraw` debits account before output validation; `Dest::Bitcoin` from pending path calls `add_withdrawal` after checkpoint pending has been drained.
- Resource bound: max withdrawal script length, min amount, dust, checkpoint tx size indirectly.
- Consensus-critical: yes.

### TB-06: Pending Credit Fanout

- Input source: completed checkpoint pending queue, deposit `Dest`, incoming IBC transfer memo.
- Authentication: upstream deposit/IBC/checkpoint validation.
- Authorization: destination-specific validation; sender identity used for feature-gated destinations.
- Replay protection: pending map is drained by removing keys.
- Freshness: only last completed checkpoint pending transfers are drained.
- Domain separation: `Dest` enum variants define target subsystem.
- Validation performed: `try_credit_dest` attempts destination credit and handles some errors without reverting.
- State mutated: nBTC accounts, IBC transfer module, Bitcoin withdrawals, rewards, feature-gated modules.
- Economic effect: final nBTC credit, bridge transfer, reward allocation, or burn.
- Failure behavior: `take_pending` removes entries before credit attempts. `try_credit_dest` comments that a failed transfer should not revert because checkpoint on Bitcoin may already exist (`src/app.rs:481` to `:530`).
- Resource bound: pending map size and destination-specific limits.
- Consensus-critical: yes.

### TB-07: Signatory Key and Recovery Script Declarations

- Input source: signed account calls to Bitcoin module.
- Authentication: Orga signer context.
- Authorization: signatory key requires signer to have a validator consensus key; recovery script only requires account signer.
- Replay protection: signed nonce; map overwrite semantics.
- Freshness: next checkpoint creation reads current signatory map; recovery scripts read during emergency generation.
- Domain separation: xpub network must match configured Bitcoin network; recovery scripts are Bitcoin scripts.
- Validation performed: validator consensus key lookup, xpub network match, script length bound.
- State mutated: `signatory_keys`, `recovery_scripts`.
- Economic effect: controls future custody signatory set and emergency recovery outputs.
- Failure behavior: validation precedes map insert in inspected paths.
- Resource bound: one xpub/script per call; max withdrawal script length.
- Consensus-critical: yes.

### TB-08: Checkpoint Signature Submission

- Input source: permissionless signature batch plus xpub and checkpoint index.
- Authentication: ECDSA verification against xpub-derived pubkeys and checkpoint sighashes.
- Authorization: only pubkeys in the checkpoint threshold signature set can contribute voting power.
- Replay protection: duplicate pubkey signatures rejected.
- Freshness: checkpoint must not be Building; signatures apply to a specific checkpoint index and current header height is recorded.
- Domain separation: Bitcoin sighash/signature message per input; xpub child derivation by signatory-set index.
- Validation performed: checkpoint status, pubkey membership, duplicate check, signature verification, threshold check.
- State mutated: per-input threshold signature maps, signed voting power, checkpoint status when complete.
- Economic effect: enables Bitcoin custody movement for deposits, withdrawals, fees, and reserve.
- Failure behavior: `ThresholdSig.sign` verifies before mutating; aggregate checkpoint advancement is still a mutation-after-threshold path requiring regression coverage.
- Resource bound: `LengthVec<u16, Signature>` batch, `MAX_SIGNATORIES`, threshold voting power.
- Consensus-critical: yes.

### TB-09: BeginBlock Automation

- Input source: Tendermint block context and existing app state.
- Authentication: Tendermint consensus for block header/time/hash.
- Authorization: protocol scheduled logic; no user signer.
- Replay protection: deterministic block height/time sequence and stored timers/indexes.
- Freshness: current block time, current Bitcoin header height, checkpoint intervals, reward timers.
- Domain separation: module-specific state machines and Orga BeginBlock dispatch.
- Validation performed: upgrade window, staking begin block, IBC begin block, checkpoint push conditions, reward timing.
- State mutated: upgrade, staking, rewards, dev/community/incentive pools, pending transfers, checkpoints, offline signer punishments.
- Economic effect: inflation, nBTC reward distribution, checkpoint advancement, downtime punishment.
- Failure behavior: Orga `begin_block` uses the same flush-after-operation helper, so partial BeginBlock mutations can persist if an error is returned.
- Resource bound: timers, checkpoint config, map/deque iteration bounds.
- Consensus-critical: yes.

### TB-10: Emergency Recovery

- Input source: account recovery scripts, nBTC account balances, pending destinations, remote-chain external outputs, checkpoint reserve.
- Authentication: recovery scripts authenticated by signed account calls; remote outputs authenticated by IBC/Cosmos state; reserve by checkpoint state.
- Authorization: account owners choose scripts; protocol creates disbursal outputs.
- Replay protection: tied to checkpoint advance state and batches.
- Freshness: generated at checkpoint advance using current state and locktime.
- Domain separation: Bitcoin scripts and transaction batches; `[0]` reserve commitment for signatory-set excess output.
- Validation performed: script length at set time, output min amount, tx size bounds, fee deduction, signature message population.
- State mutated: emergency disbursal batches in checkpoint.
- Economic effect: last-resort BTC payout path for nBTC holders and pending transfers.
- Failure behavior: many mutations occur while building transactions; errors after partial batch construction need rollback-style regression tests due Orga persistence.
- Resource bound: min tx amount, locktime interval, max final tx size.
- Consensus-critical: yes.

### TB-11: IBC Packet Relay

- Input source: raw IBC transaction/messages from relayers.
- Authentication: ibc-rs light-client, connection, channel, packet proof logic.
- Authorization: IBC channel/port capability in ibc-rs; Nomic charges nBTC fee for `ibc_deliver`.
- Replay protection: packet sequence, commitments, receipts, acknowledgements in Orga IBC state.
- Freshness: client consensus heights and packet timeouts.
- Domain separation: IBC client/connection/channel/port/sequence keys and ICS-20 denom trace.
- Validation performed: ibc-rs message dispatch plus Nomic `usat`/memo parsing for incoming transfers.
- State mutated: IBC client/channel/packet state, transfer module balances, Bitcoin pending transfers.
- Economic effect: cross-chain nBTC movement and relay fee.
- Failure behavior: incoming path generally returns errors; outgoing `IbcDest::transfer` mints into transfer module before `deliver_message` and suppresses transfer errors.
- Resource bound: raw tx/message sizes by encoding/gas, packet sequence maps.
- Consensus-critical: yes.

### TB-12: Cosmos ICS-23 Operator Key Relay

- Input source: signed fee-paying call with IBC client id, height, consensus key, staking/account proofs.
- Authentication: Orga signer for fee payer; ICS-23 proofs against stored IBC consensus root; Tendermint client type check.
- Authorization: permissionless relay of remote operator keys.
- Replay protection: existing identical key returns error; overwrite with different key is allowed by source.
- Freshness: supplied historical root plus latest-header validator-set membership check.
- Domain separation: ICS-23 Tendermint store proof for store names `staking` and `acc`; IAVL inner proof.
- Validation performed: client exists/type, consensus state exists, consensus key in latest validator set, proof membership, exact key paths, BaseAccount pubkey decode.
- State mutated: `Cosmos.chains[client_id].op_keys_by_cons`.
- Economic effect: controls remote-chain external emergency outputs and possibly signer attribution.
- Failure behavior: insert occurs after proof checks; temporal mismatch remains a candidate correctness issue.
- Resource bound: proof encoding uses `LengthVec<u16, u8>` for inner/outer proof byte blobs.
- Consensus-critical: yes.

### TB-13: Upgrade and Migration

- Input source: validator/account upgrade signals and block time.
- Authentication: signed calls for signals; Tendermint block context for window.
- Authorization: Orga upgrade module threshold policy.
- Replay protection: stored signal/version state.
- Freshness: mainnet weekday 17:00-17:10 UTC window; testnet unrestricted.
- Domain separation: consensus version byte vector.
- Validation performed: upgrade module checks and migration implementations.
- State mutated: upgrade state and migrated application state.
- Economic effect: can change every bridge/account/IBC invariant at activation.
- Failure behavior: migrations can mutate broad state; V6 to V7 is `todo!()` in source and must be handled by build/feature/version policy.
- Resource bound: migration loops over headers/scripts/state; depends on state size.
- Consensus-critical: yes.

### TB-14: FROST DKG and Signing

- Input source: signed validator participant DKG packages, pubkey attestations, commitments, and signature shares.
- Authentication: Orga signer mapped to configured share range.
- Authorization: only configured participants can submit for their share range.
- Replay protection: per-participant maps for round packages, attestations, commitments, and shares.
- Freshness: signing `iteration` and timeout state.
- Domain separation: group index, signing index, iteration, and message stored in signing state; cryptographic package handling delegated to `frost-secp256k1-tr`.
- Validation performed: DKG state transitions, package counts, pubkey equality attestations, commitment iteration, duplicate checks, aggregate signature verification.
- State mutated: DKG maps/counts, group pubkey, signing commitments/shares, aggregate signature.
- Economic effect: feature-gated custody/control for Babylon-related flows.
- Failure behavior: signature share submission can mutate old-iteration shares and counters before aggregate errors.
- Resource bound: configured participants, shares, top-n, threshold.
- Consensus-critical: yes when feature enabled.

## Cross-Link to Prior Findings

Confirmed prior findings:

- `NOMIC-AUD-001`: legacy deposit commitment matching uses `dest_bytes` inside the legacy loop instead of `bytes`; recovery uses current commitment bytes. Phase 1 places this in TB-03 and the deposit lifecycle.
- `NOMIC-AUD-002`: FROST participant selection sorts ascending and gives absent validators sort key zero. Phase 1 places this in TB-14 and the FROST lifecycle.
- `NOMIC-AUD-003`: no-default-features build fails without `csv`. Phase 1 treats this as release/build assurance, not an architecture boundary.
- `NOMIC-AUD-004`: deposit pre-validation does not subtract miner fee in `amount_after_deposit_fee`; direct Bitcoin destinations can later fail. Phase 1 places this in TB-03/TB-05.

Likely prior findings:

- `NOMIC-AUD-L001`: accepted signatory sets can be spendable by less total validator voting power than expected because quorum is over possible voting power but spending threshold is over present signatory voting power. Phase 1 places this in TB-08 and threshold-signing lifecycle.
- `NOMIC-AUD-L002`: Bitcoin retarget timestamp subtraction can underflow. Phase 1 places this in TB-02; still needs Bitcoin Core differential or focused unit regression.

## New Candidate Leads from Phase 1

These are not marked confirmed until backed by local regression/model tests against the app or minimal Orga harness.

### NOMIC-AUD-P1-001: Failed ABCI Calls Persist Partial Mutations

Evidence: `InternalApp::run` flushes state after the operation regardless of the operation result (`orga/src/abci/node.rs:482` to `:486`), and `deliver_tx` packages inner errors into a successful closure result before setting ABCI `code = 1` (`orga/src/abci/node.rs:558` to `:590`). `begin_block`/`end_block` also use `run`.

Impact: any mutate-before-error path can turn a rejected transaction or failed block hook into persistent state mutation. This is an architectural root cause behind several concrete leads below.

Next verification: minimal Orga ABCI harness with a call that mutates then returns `Err`, asserting the mutation persists after `DeliverTx` code 1.

### NOMIC-AUD-P1-002: Invalid Bitcoin Header Reorg May Truncate or Partially Modify Header Queue

Evidence: `HeaderQueue.add_into_iter` calls `pop_back_to(first.height)` before `verify_and_add_headers` (`src/bitcoin/header_queue.rs:455` then `:458`). `verify_and_add_headers` pushes each header as it is verified and can later return errors (`src/bitcoin/header_queue.rs:501` to `:525`). If `added_work <= removed_work`, an error is returned after replacement headers have already been appended (`src/bitcoin/header_queue.rs:458` to `:463`).

Impact: with failed-call persistence, a relayed invalid or insufficient-work reorg could remove valid headers or leave a partial replacement chain. That can distort Bitcoin finality, block or alter deposits/checkpoint confirmations, and affect all cross-chain economic paths.

Next verification: local unit/regression with a header queue seeded to height N, submit a competing reorg whose first replacement causes a later validation/work error, then inspect persisted queue after ABCI code 1.

### NOMIC-AUD-P1-003: Deposit Outpoint Poisoning When Deposits Are Disabled

Evidence: `processed_outpoints.insert` happens before `if !checkpoint.deposits_enabled { return Err(...) }` (`src/bitcoin/mod.rs:606` to `:619`).

Impact: a valid deposit output for a checkpoint with disabled deposits can be marked processed by a transaction that returns error. Later legitimate relay or recovery handling may be blocked by replay protection.

Next verification: local app-state test using a checkpoint with `deposits_enabled = false`, a syntactically valid matching deposit, and post-error inspection of `processed_outpoints`.

### NOMIC-AUD-P1-004: Withdrawal Debit Before Output Validation

Evidence: `Bitcoin.withdraw` withdraws account funds at `src/bitcoin/mod.rs:749`, then calls `add_withdrawal`; validation happens inside `add_withdrawal` at `src/bitcoin/mod.rs:813`.

Impact: invalid withdrawal scripts or amounts may debit the caller while returning an error. This appears primarily self-loss/liveness, but it is part of the broader mutate-before-error class and may interact with paid calls or composed flows.

Next verification: local ABCI regression for invalid script/amount after a funded account withdrawal.

### NOMIC-AUD-P1-005: FROST Stale-Iteration Signature Share Poisoning

Evidence: commitment submission requires current iteration (`src/frost/signing.rs:110`), timeout increments iteration and resets counters without clearing maps (`src/frost/signing.rs:65` to `:70`), but signature share submission does not check current iteration and accepts any `(iteration, participant)` that has a commitment (`src/frost/signing.rs:128` to `:149`). Aggregation filters current-iteration shares, so stale shares can affect counters without supplying aggregate material.

Impact: when FROST/Babylon is enabled, stale shares may push signing state to `Complete`, trigger aggregate failure, and persist poisoned counters/shares under Orga failed-call persistence.

Next verification: isolated FROST signing model with iteration 0 commitments, timeout to iteration 1, then stale iteration 0 signature shares.

### NOMIC-AUD-P1-006: `relay_op_key` Temporal and Type-URL Validation Gap

Evidence: proofs are verified against the consensus root for the supplied height (`src/cosmos.rs:151` to `:176`), but consensus-key membership is checked against `client.last_header()` (`src/cosmos.rs:161` to `:173`). `BaseAccount.pub_key.value` is decoded as secp256k1 without checking `Any.type_url` (`src/cosmos.rs:195` to `:203`).

Impact: likely correctness/hardening issue around historical validator membership and account key type authenticity. Needs a concrete remote-chain state model before severity assignment.

Next verification: model two adjacent validator sets and account states to determine whether a removed/added validator can bind an operator key from a different height.

## Verification Limits

- This phase did not run an end-to-end local node, ABCI harness, Bitcoin Core differential, or property tests.
- Orga failed-call persistence was verified by source inspection, not by a runtime regression.
- Feature-gated Ethereum/Babylon/FROST paths were mapped at source level but not built/executed in all feature combinations.
- IBC protocol correctness was mostly delegated to pinned Orga/ibc-rs source; this phase inspected Nomic-specific wrapping and selected Orga state boundaries, not a full ibc-rs audit.
- Bitcoin script/transaction size/fee behavior was mapped by source structure; no transaction vectors were generated.

## Suggested Next Phase

Phase 2 should be executable invariant testing focused on persistence and bridge safety:

1. Build a minimal Orga ABCI rollback harness proving whether failed `DeliverTx` and failed `BeginBlock` persist mutations.
2. Add a header-queue invalid-reorg regression covering pop-before-verify and partial append-before-error.
3. Add a deposit disabled-checkpoint regression for processed-outpoint poisoning.
4. Add a withdrawal invalid-output regression for debit-before-validation.
5. Add a FROST stale-iteration signing model/test under `frost,testnet` features.
6. Continue prior work on `NOMIC-AUD-L001` with a signatory voting-power model and `NOMIC-AUD-L002` with Bitcoin Core retarget differentials.
7. Sweep all fallible calls for mutation-before-error patterns, prioritizing `BeginBlock`, emergency disbursal generation, IBC outgoing transfer, and migrations.
