# Nomic Defensive Audit Package

Run directory: `audit/run-20260621T0354CDT-c5486a4`

## 1. EXECUTIVE SUMMARY

Target commit: `c5486a4a9b41de5474f991891ae22f734b1d5aec`

Overall risk: incomplete but non-trivial. I did not find a locally reproduced unbacked nBTC mint or unauthorized Bitcoin transaction during this initial run. I did find two confirmed deposit/custody accounting failures, one confirmed feature-gated FROST liveness/participant-selection defect, and one high-risk threshold/quorum finding that should be treated as likely until protocol intent is confirmed.

Confirmed findings by severity:

| Severity | Count |
| --- | ---: |
| Critical | 0 |
| High | 0 |
| Medium | 3 |
| Low/Informational | 1 |

Likely issues requiring one verification step:

- `NOMIC-AUD-L001`: signatory sets are accepted with only `present_vp >= possible_vp / 2`, while the Bitcoin spend threshold is computed over `present_vp`, not total validator power. In edge cases this allows accepted reserve scripts spendable by 35% of total voting power on mainnet-equivalent `(2,3)` settings, or 50% for a two-validator equal-power set. Verification needed: confirm whether the intended custody threshold is over all validator voting power or only participating signers after a 50% quorum gate.
- `NOMIC-AUD-L002`: Bitcoin retarget timestamp arithmetic uses unchecked `u32` subtraction for an epoch timespan. Bitcoin's MTP rule does not require epoch-end timestamps to be greater than epoch-start timestamps, so a valid timestamp sequence can make `header.time() - prev_retarget` underflow and panic with overflow checks enabled. Verification needed: turn the local timestamp model into a full local header-chain regression with mined/simulated PoW.

Principal solvency and custody conclusions:

- Deposit proof validation checks the txid, Merkle root, vout bounds, script commitment, duplicate outpoint, amount floor, and confirmation boundary before adding pending nBTC. Legacy commitment handling is broken and can strand old-format deposits.
- Deposit destination pre-validation overstates the amount available to amount-sensitive destinations by checking but not subtracting the deposit miner fee. Direct BTC-destination deposits near fee/minimum boundaries can be accepted into a checkpoint and later fail credit without a refund path.
- Checkpoint signing and Bitcoin script threshold arithmetic are internally consistent in using strict `>` over a floor threshold, but that produces stricter-than-commented liveness behavior for exact 2/3 or 9/10 cases and lower-than-expected total-voting-power security when xpub participation is low.
- The review did not complete end-to-end state-machine conservation testing, crash injection, or differential Bitcoin Core validation.

Principal consensus and IBC conclusions:

- Bitcoin header validation has a likely retarget-boundary panic on MTP-compatible timestamp sequences with negative epoch timespans; this still needs a full local header-chain regression.
- IBC behavior is mostly delegated to Orga/ibc-rs. I reviewed custom direct-deposit and Cosmos proof glue but did not complete packet replay/ack/timeout models.
- Cosmos ICS-23 proofs are checked for membership under store roots and exact keys after verification; unresolved questions remain around proof height freshness and protobuf `Any` type-url binding.

Unresolved high-risk questions:

- Is the intended reserve custody threshold based on total validator voting power or only xpub-present voting power?
- Are any legacy native/IBC deposit commitments still in flight or stored in user wallets/relayer files?
- Are production builds expected to use `--no-default-features`, `legacy-bin`, `frost`, `babylon`, or `ethereum-full`?
- Does Orga transaction execution roll back all state mutations on an application call returning `Err` in every call path, including BeginBlock internal calls?

## 2. SCOPE AND REPRODUCIBILITY

Repositories and revisions:

- `nomic`: `c5486a4a9b41de5474f991891ae22f734b1d5aec`
- Workspace members: root package only, `nomic 9.2.0`
- Git dependencies observed from `Cargo.toml`/`Cargo.lock`:
  - `orga`: `3b3d25ade40d81cb64f19335535e3a47bb47778f`
  - `ed`: `a657be856792039ff60c2f67e7920e38cd3acffc`
  - `merk`: `d6f0490993bcf88f786c5271091aa9a84ff2fe69`
  - `abci2`: `27d8d7a5a6e458f5881e3eb13116cb2a3ae049b5`
  - `frost-secp256k1-tr`: `51fa7d09f3742563a35d065afcff6ad486430dac`
  - `helios consensus-core`: requested `0.7.0`, locked `545d809be0f135f69a8e6f613bb6bdd0fb4b22d1`
  - `pretty_env_logger`: `f9e35b6dbbf06de55222c944c9e1e176ce73b3a7`

Toolchain:

- `rustc 1.81.0-nightly (506985649 2024-07-20)`
- `cargo 1.81.0-nightly (a2b58c3da 2024-07-16)`
- Host: `x86_64-unknown-linux-gnu`

Feature profiles:

| Profile | Command | Result |
| --- | --- | --- |
| Formatting | `cargo fmt --all -- --check` | Pass |
| Default | `cargo test --workspace --all-targets --no-run` | Blocked by missing `pkg-config`/OpenSSL for `openssl-sys` |
| All features | `cargo test --workspace --all-targets --all-features --no-run` | Blocked by missing `pkg-config`/OpenSSL |
| No default | `cargo check --lib --no-default-features` | Fails: `src/incentives.rs` uses optional `csv` without enabling feature |
| Minimal no-default | `cargo check --lib --no-default-features --features csv` | Passes with warnings |
| Mainnet-equivalent | `cargo check --lib --no-default-features --features full` | Blocked by missing `pkg-config`/OpenSSL |
| Devnet | `cargo check --lib --features devnet` | Blocked by missing `pkg-config`/OpenSSL |

Commands and raw outputs:

- Exact command log: `research-log.md`
- Phase 0 outputs: `phase0/`
- Build/test outputs: `tests/`
- Static scan outputs: `notes/`
- Local models: `findings/poc-*.out`

Excluded components and limitations:

- No public Nomic mainnet/testnet, relayer, RPC, frontend, seed, validator, signer, or explorer was probed.
- No real BTC, real private keys, or public Bitcoin transactions were used.
- Full build/test execution was blocked by local missing `pkg-config`/OpenSSL.
- `cargo audit`, `cargo deny`, and `cargo geiger` were not installed.
- No fuzzing, Miri, Kani, Loom, TLA+, or Bitcoin Core regtest differential tests were completed in this run.

## 3. ARCHITECTURE AND TRUST MODEL

Component map:

- `InnerApp` wraps accounts, staking, Bitcoin custody, IBC, upgrade, incentives, Cosmos proof glue, and optional Ethereum/Babylon/FROST modules.
- `Bitcoin` owns header queue, processed outpoint set, checkpoint queue, nBTC accounts, signer xpub registry, reward/fee pools, recovery scripts, and recovery transactions.
- `HeaderQueue` stores a bounded best-work Bitcoin header chain and supports reorgs within retained history.
- `CheckpointQueue` builds, signs, completes, confirms, and prunes chained Bitcoin checkpoint transactions.
- `SignatorySet` maps validator voting power and xpubs into weighted Bitcoin scripts.
- `ThresholdSig` validates ECDSA shares against checkpoint sighashes and tracks strict-threshold completion.
- `Relayer` is untrusted glue for headers, deposits, checkpoints, watched scripts, and fee estimates.
- `Cosmos` verifies extra ICS-23 membership proofs to map counterparty consensus keys to Bitcoin operator keys for emergency disbursal.
- Optional `Frost` implements DKG/signing state machines for testnet/Babylon-related flows.

Deposit lifecycle:

1. User/relayer obtains a deposit script from `SignatorySet::output_script(dest.commitment_bytes(), threshold)`.
2. Depositor pays BTC to that P2WSH output.
3. Relayer submits transaction, Bitcoin height, `PartialMerkleTree`, vout, sigset index, and `Dest`.
4. `InnerApp::relay_deposit` estimates post-fee amount and validates `Dest`.
5. `Bitcoin::relay_deposit` checks height/confirmations, Merkle root, txid, vout, value floor, script commitment, duplicate outpoint, deposit age, and deposits-enabled flag.
6. Accepted deposits become checkpoint inputs and pending nBTC credits.
7. Pending credits are processed after the containing checkpoint is fully signed.

Withdrawal lifecycle:

1. User calls `withdraw_nbtc`/`Bitcoin::withdraw` with a Bitcoin script and nBTC amount.
2. Code validates script length, non-OP_RETURN, checkpoint availability, fee coverage, minimum amount, and dust.
3. nBTC is withdrawn from the account and checkpoint output is added.
4. Validators/signers sign the frozen checkpoint transaction.
5. Relayer broadcasts completed Bitcoin checkpoint transaction.

Checkpoint lifecycle:

1. `begin_block_step` decides whether to push based on interval, pending work, fee collection, unconfirmed checkpoint count, and signatory quorum.
2. A new Building checkpoint is pushed with the current signatory set.
3. The previous Building checkpoint advances to Signing, computes reserve output, fees, emergency disbursal transactions, and sighashes.
4. Signers submit signatures batch-by-batch.
5. When all inputs/batches satisfy strict threshold, the checkpoint becomes Complete.
6. A relayer proves Bitcoin inclusion to advance `confirmed_index`.

Emergency recovery lifecycle:

- Account recovery scripts and pending transfers are converted into emergency disbursal outputs when a checkpoint advances.
- Expired deposits can be moved from old to current signatory sets through `RecoveryTxs`, but the current implementation uses current commitment bytes and does not carry matched legacy commitment bytes.

IBC packet lifecycle:

- Native nBTC IBC transfer withdraws from `bitcoin.accounts`, charges fee, stores pending `Dest::Ibc`, then after checkpoint completion `IbcDest::transfer` mints into the transfer module and attempts `deliver_message`.
- Voucher withdrawal uses `ibc_withdraw_nbtc` to burn IBC-held nBTC and credit native nBTC.

ICS-23 proof lifecycle:

- `relay_op_key` looks up a trusted Tendermint client and consensus state at the provided height, verifies outer store membership and inner IAVL membership, checks expected staking/account key paths, decodes a BaseAccount and secp256k1 pubkey, and records consensus-key to operator-key mapping.

Privilege and trust boundaries:

| Boundary | Authentication | Authorization | Replay/freshness | State/economic effect |
| --- | --- | --- | --- | --- |
| `relay_deposit` | Bitcoin PoW + Merkle proof + commitment script | Anyone can relay | Outpoint set, confirmations, age | Adds reserve input and pending nBTC |
| `relay_checkpoint` | Bitcoin PoW + Merkle proof | Anyone can relay | Monotonic `confirmed_index` | Confirms checkpoint chain |
| `set_signatory_key` | Nomic signer | Must be validator operator | Duplicate normalized xpub check | Controls future reserve scripts |
| `sign` | Signature verifies under xpub-derived pubkey | Signatory membership in input state | One signature per pubkey | Advances Bitcoin spend authorization |
| `withdraw` | Nomic signer | Own balance | Account debit | Adds Bitcoin output |
| IBC transfer | IBC client/connection/channel internals | ibc-rs module | ibc-rs sequences/timeouts | Escrow/mint/burn nBTC |
| `relay_op_key` | ICS-23 proof against trusted client root | Consensus key in last header validator set | Provided height only | Emergency disbursal key mapping |

## 4. INVARIANT LEDGER

| Invariant area | Status | Evidence | Remaining uncertainty |
| --- | --- | --- | --- |
| A. Reserve and supply | Partially falsified | Deposit/withdraw/checkpoint accounting traced; deposit fee pre-validation mismatch modeled | No full state-machine conservation model |
| B. Bitcoin deposit validation | Partially falsified | `relay_deposit` branch review, `poc_legacy_commitment_model.py`, `poc_deposit_fee_prevalidation_model.py` | No end-to-end regtest PoC run |
| C. Bitcoin header chain | Partially reviewed; likely retarget arithmetic issue | Existing `tests/header_queue.rs`; `poc_header_retarget_underflow_model.py` | No mined local-chain or Bitcoin Core differential validation |
| D. Checkpoints | Partially reviewed | Fee/reserve/signing paths traced | No max-size/dust/adversarial batching model |
| E. Threshold/signers | Partially falsified/uncertain | `poc_threshold_model.py` shows low total-VP spend threshold when xpub participation is low | Protocol intent verification needed |
| F. FROST/DKG | Partially falsified | `from_staking` ordering bug identified | No full DKG/signing local test |
| G. Emergency recovery | Partially reviewed | Legacy recovery uses current commitment bytes | No local signed recovery transaction test |
| H. IBC/ICS-20 | Partially reviewed | Direct deposit flow traced | No packet replay/ack/timeout state model |
| I. ICS-23/Cosmos proofs | Partially reviewed | Exact store/key checks found | Height/type-url/freshness not fully proven |
| J. Consensus determinism | Partially reviewed | `HashMap`/`f64` scan captured | No multi-process differential test |
| K. Rust safety | Partially reviewed | No `unsafe` in target `src`/`tests`; panic/unwrap scan captured | Dependencies not geiger-reviewed |
| L. State/storage atomicity | Untested | Mutation-before-error sites noted | Orga rollback semantics not verified |
| M. Migrations/genesis | Partially reviewed | V5->V6 reviewed; V6->V7 contains `todo!()` | Migration test matrix not run |
| N. Relayer/DoS | Partially reviewed | Max header relay, length vecs, relayer maps scanned | No adversarial cost model |
| O. Economics/liveness | Partially reviewed | Fee collection and checkpoint gating read | No simulation |
| P. Supply chain | Partially recorded | Cargo metadata/tree captured | Scanner tools absent |

## 5. ATTACK-SURFACE MATRIX

| Entry point | Actor | Data controlled | Validation | State affected | Economic/resource effect | Relevant tests/evidence | Findings |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `Bitcoin::relay_deposit` | Depositor/relayer | tx, proof, vout, sigset, dest | Header, Merkle root, txid, script, outpoint, amount | processed outpoints, checkpoint input, pending credits | Mint after checkpoint signing | `poc_legacy_commitment_model.py` | `NOMIC-AUD-001` |
| `InnerApp::relay_deposit` pre-validation | Depositor | amount-sensitive destination and output amount | Destination validated with overstated post-fee amount | checkpoint pending transfers | User deposit accepted then credit can fail without refund | `poc_deposit_fee_prevalidation_model.py` | `NOMIC-AUD-004` |
| Header retarget | Relayer/miner | header timestamps around retarget | MTP and target calculation | header queue / consensus execution | Possible deterministic panic/halt under valid timestamp pattern | `poc_header_retarget_underflow_model.py` | `NOMIC-AUD-L002` |
| `CheckpointQueue::maybe_push` | Validators/xpub participants | validator set and xpub availability | `has_quorum` | checkpoint queue, reserve script | Custody threshold/liveness | `poc_threshold_model.py` | `NOMIC-AUD-L001` |
| `FrostConfig::from_staking` | Validator set / prior absent set | stake weights, absent addresses | sort/truncate only | FROST groups | DKG liveness/key participant quality | source review | `NOMIC-AUD-002` |
| `IbcDest::transfer` | Depositor/relayer | channel, receiver, sender, timeout, memo | ID parsing and sender bech32 | IBC transfer module | Remote mint or local escrow | source review | unresolved |
| `Cosmos::relay_op_key` | Relayer/counterparty | proofs, height, cons key | ICS-23 membership and key equality | operator-key map | Emergency disbursal output | source review | unresolved |
| Header relay | Relayer/miner | headers | PoW, target, linkage, MTP | header queue | Enables deposit/checkpoint proofs | existing tests | none confirmed |
| Withdrawal | User | script and amount | length, OP_RETURN, fee/min/dust | checkpoint output, account debit | reserve outflow | source review | none confirmed |

## 6. CONFIRMED FINDINGS

### NOMIC-AUD-001

ID: `NOMIC-AUD-001`

Title: Legacy deposit commitments are never matched, and expired legacy deposits cannot be recovered correctly

Severity: Medium, potentially High if legacy deposit addresses remain in use with material funds

Confidence: High

CWE or weakness class: CWE-682 Incorrect Calculation / CWE-670 Always-Incorrect Control Flow Implementation

Affected commit: `c5486a4a9b41de5474f991891ae22f734b1d5aec`

Affected files and symbols:

- `src/bitcoin/mod.rs`, `Bitcoin::relay_deposit`, lines 584-603
- `src/bitcoin/recovery.rs`, `RecoveryTxs::create_recovery_tx`, lines 51-68
- `src/app.rs`, `Dest::legacy_commitment_bytes`, lines 1972-1982

Attacker: No active attacker required; any user/relayer attempting to relay an old-format native or IBC deposit can trigger it.

Preconditions:

- A Bitcoin UTXO was paid to a script using one of the legacy commitment encodings.
- The user/relayer submits the corresponding current `Dest` expecting fallback matching.

Violated invariant: B1, B2, B4, G3.

Summary:

The legacy fallback loop computes `expected_script` with `dest_bytes` instead of the loop variable `bytes`. Since `dest_bytes` still contains the current v0 commitment when the fallback branch is entered, every legacy candidate is compared against the current script again. Legacy scripts therefore never match. The expired-deposit recovery path also rebuilds current commitment bytes and has no way to carry the matched legacy commitment bytes into `RecoveryTxs`.

Root cause:

The fallback loop updates `dest_bytes = bytes` only after a match, but the comparison itself does not use `bytes`.

Reachable execution path:

`InnerApp::relay_deposit` -> `Bitcoin::amount_after_deposit_fee` -> `Bitcoin::relay_deposit` -> script comparison branch.

Expected behavior:

If the output script matches any accepted legacy commitment encoding for the supplied `Dest`, relay should accept it and use the matched commitment bytes for checkpoint input/recovery construction.

Actual behavior:

Current code rejects every legacy commitment with `Output script does not match signer set`.

Security impact:

Legacy deposits can be permanently uncredited and, once expired, are not recoverable through the intended local recovery transaction flow. Funds are locked under an old signatory set unless operators coordinate out-of-band recovery.

Practical exploitability:

This is a compatibility/custody failure rather than an adversarial theft path. Impact depends on whether legacy deposit commitments are still in-flight or user-accessible.

Local reproduction:

Run:

```bash
python3 audit/run-20260621T0354CDT-c5486a4/poc_legacy_commitment_model.py
```

The model prints the exact source loop and shows current code rejects `script(legacy)` while the one-line fixed comparison accepts it.

Minimal proof of concept:

`findings/poc-legacy-commitment-model.out`

Why existing protections fail:

`legacy_commitment_bytes()` generates candidates, but they are not used in the `output_script()` comparison. `RecoveryTxs::create_recovery_tx` independently recomputes current commitment bytes.

Recommended fix:

- In the fallback loop, compute `sigset.output_script(&bytes, threshold)`.
- Pass the matched commitment bytes into `RecoveryTxInput` and use those bytes for both old input construction and new output construction.
- Add an end-to-end regression for each legacy native and IBC commitment format, including expired-deposit recovery.

Suggested patch:

```rust
for bytes in legacy_commitments {
    let expected_script =
        sigset.output_script(&bytes, self.checkpoints.config.sigset_threshold)?;
    if output.script_pubkey == expected_script {
        matched = true;
        dest_bytes = bytes;
        break;
    }
}
```

Regression test:

Create a local unit/regtest case that builds a deposit output with each legacy commitment, relays it, verifies a checkpoint input is added with the legacy bytes, then advances time past `max_deposit_age` and verifies recovery txs spend and re-lock with the same matched commitment.

Variant analysis:

`src/bitcoin/recovery.rs` has the sibling bug of recomputing current commitment bytes. Relayer watched-script persistence also derives only current commitment scripts.

Operational mitigation:

Privately identify any legacy deposit addresses still in user flows or relayer stores. Warn operators not to rely on the current recovery path for legacy commitments until patched.

Disclosure sensitivity:

Private. Details could help identify stranded funds or recovery coordination weaknesses.

### NOMIC-AUD-002

ID: `NOMIC-AUD-002`

Title: FROST participant selection chooses absent and lowest-stake validators first

Severity: Medium for FROST/Babylon/testnet feature profiles; Low/Informational if unsupported in production

Confidence: High

CWE or weakness class: CWE-693 Protection Mechanism Failure / liveness failure

Affected commit: `c5486a4a9b41de5474f991891ae22f734b1d5aec`

Affected files and symbols:

- `src/frost/mod.rs`, `Config::from_staking`, lines 35-49
- `src/app.rs`, `step_frost`, lines 862-883

Attacker: Validator set participant or any condition that causes a validator to be marked absent.

Preconditions:

- `frost` and `testnet`/`babylon` feature path is enabled.
- FROST group creation runs from staking state.

Violated invariant: F2, F6, O liveness.

Summary:

`validators.sort_by_key()` sorts ascending and assigns absent validators key `0`, then takes the first `top_n`. This selects absent validators and the lowest-staked validators, not the top active validators. Since DKG requires all configured participants to complete round 1/2/attestation, selecting absent participants can repeatedly stall DKG.

Root cause:

Ascending sort is used where descending stake order and absent exclusion/penalty were intended.

Reachable execution path:

`InnerApp::step_frost` -> `FrostConfig::from_staking` -> `FrostGroup::with_config` -> DKG state machine.

Expected behavior:

Choose the highest-staked non-absent validators, or explicitly define a deterministic replacement policy.

Actual behavior:

Absent validators have the lowest key and are taken first.

Security impact:

FROST group formation can stall, select weak participants, or exclude intended top validators. For Babylon/FROST-dependent functionality this can break liveness and signer-set assumptions.

Practical exploitability:

Feature-gated. An absent participant from a previous group is preferentially selected into the next group, creating a self-reinforcing liveness failure.

Local reproduction:

Source-level deterministic: with validator stakes `[100, 50, 10]`, absent `{100}`, `top_n = 2`, current code selects absent `100` and `10`, not `50` and `10` or `50` and `100` depending policy.

Recommended fix:

Filter absent validators out before sorting, and sort descending by staked amount, for example with `std::cmp::Reverse(v.amount_staked)`.

Regression test:

Given a synthetic staking set with three validators and one absent high-stake validator, assert the selected participants exclude the absent validator and preserve descending stake priority.

### NOMIC-AUD-003

ID: `NOMIC-AUD-003`

Title: `--no-default-features` build is not supported despite being part of the requested matrix

Severity: Informational

Confidence: High

Affected files and symbols:

- `Cargo.toml`, optional `csv` feature
- `src/incentives.rs`, lines 54 and 73

Summary:

`cargo check --lib --no-default-features` fails because `src/incentives.rs` uses `csv::Reader` while `csv` is optional and only enabled by `full`. `cargo check --lib --no-default-features --features csv` passes.

Impact:

This is primarily a build-matrix/configuration issue. It increases the chance that unsupported feature combinations are accidentally used or that feature-dependent consensus differences are missed.

Local reproduction:

See `tests/cargo-check-lib-no-default.err`.

Recommended fix:

Either make `csv` non-optional, guard incentives code behind a feature, or explicitly document that no-default builds are unsupported.

### NOMIC-AUD-004

ID: `NOMIC-AUD-004`

Title: Deposit pre-validation does not subtract the miner fee, so accepted direct BTC-destination deposits can later fail without refund

Severity: Medium

Confidence: High

CWE or weakness class: CWE-682 Incorrect Calculation / state-machine accounting mismatch

Affected commit: `c5486a4a9b41de5474f991891ae22f734b1d5aec`

Affected files and symbols:

- `src/bitcoin/mod.rs`, `Bitcoin::amount_after_deposit_fee`, lines 473-515
- `src/bitcoin/mod.rs`, `Bitcoin::relay_deposit`, lines 648-672
- `src/app.rs`, `InnerApp::relay_deposit`, lines 363-386
- `src/app.rs`, `try_credit_dest`, lines 481-520

Attacker: A depositor or wallet/relayer flow that constructs an amount-sensitive destination, especially `Dest::Bitcoin`.

Preconditions:

- Deposit destination validation depends on the exact post-fee amount.
- The deposit amount is near that destination's fee/minimum/dust boundary.

Violated invariant: A4, A5, B3, B4.

Summary:

`amount_after_deposit_fee` computes `amount.checked_sub(miner_fee_amount)` only as an underflow check and discards the subtraction result. `InnerApp::relay_deposit` then validates the destination with an overstated amount. The actual relay path later subtracts the miner fee before storing the pending destination. After the checkpoint is signed, `try_credit_dest` re-validates with the lower actual amount; for `Identity::None` deposit credits, a failure has no refund branch.

Root cause:

The pre-validation helper and actual relay accounting are not the same transition. The helper omits an assignment for the miner-fee subtraction.

Reachable execution path:

`InnerApp::relay_deposit` -> `Bitcoin::amount_after_deposit_fee` -> `validate_dest(Dest::Bitcoin, overstated_amount)` -> `Bitcoin::relay_deposit` -> `insert_pending(dest, actual_lower_amount, Identity::None)` -> `begin_block` -> `try_credit_dest` -> `validate_dest(Dest::Bitcoin, actual_lower_amount)` fails -> no refund for `Identity::None`.

Expected behavior:

The amount used for destination pre-validation should exactly match the amount that will later be credited, or failed destination crediting should preserve/refund the pending value.

Actual behavior:

A boundary amount can pass pre-validation but fail final crediting after becoming part of a signed checkpoint.

Security impact:

The affected depositor's BTC can enter the reserve without receiving native nBTC, an IBC packet, or a Bitcoin withdrawal output for the committed destination. This is a bounded custody/liveness failure rather than a theft path.

Local reproduction:

Run:

```bash
python3 audit/run-20260621T0354CDT-c5486a4/poc_deposit_fee_prevalidation_model.py
```

The model shows concrete output-satoshi examples where the overstated pre-validation amount passes `validate_withdrawal`, while the actual post-miner-fee amount fails.

Recommended fix:

- Assign the miner-fee subtraction in `amount_after_deposit_fee`.
- Prefer a single shared helper that computes the exact pending coin amount used by both pre-validation and relay.
- Add a regression for direct `Dest::Bitcoin` deposits at boundary minus one, boundary, and boundary plus one.
- Consider a refund or durable failed-credit queue for `Identity::None` pending transfers.

## 7. DEFENSE-IN-DEPTH ISSUES

### NOMIC-AUD-L001

Classification: Likely vulnerability requiring one specified verification step

Title: Accepted signatory sets can be spendable by far less than the configured fraction of total validator voting power

Severity: High if custody threshold is intended over total validator voting power; otherwise documentation/configuration mismatch

Evidence:

- `SignatorySet::signature_threshold` uses `present_vp`, not `possible_vp` (`src/bitcoin/signatory.rs:351-353`).
- `SignatorySet::quorum_threshold` returns `possible_vp / 2` and `has_quorum` allows `present_vp >= possible_vp / 2` (`src/bitcoin/signatory.rs:355-379`).
- `CheckpointQueue::should_push` and `maybe_push` accept that quorum (`src/bitcoin/checkpoint.rs:2208-2218`, `2244-2257`).
- Runtime and Bitcoin script completion both use strict greater-than over the present-vp threshold (`src/bitcoin/threshold_sig.rs:199-235`; `src/bitcoin/signatory.rs:451-455`).

Local model:

```bash
python3 audit/run-20260621T0354CDT-c5486a4/poc_threshold_model.py
```

Model output includes:

- Mainnet-equivalent `(2,3)`, `possible_vp=100`, `present_vp=51`: signatory set accepted; minimum signer power is 35% of total possible voting power.
- Mainnet-equivalent `(2,3)`, two equal validators and one xpub present: signatory set accepted; one 50% validator can spend.

Risk:

If Nomic's custody guarantee is “more than 2/3 of validator voting power must authorize reserve spends,” the implementation falls short whenever xpub participation is low but above half. A minority signer coalition plus missing xpubs from honest validators could control reserve BTC below the intended threshold.

Verification step:

Confirm with protocol owners whether the intended spend threshold is:

1. Strictly greater than the configured ratio of `present_vp`, after a separate 50% xpub quorum gate, or
2. Strictly greater than the configured ratio of total validator voting power (`possible_vp`), or at least a 2/3 xpub quorum.

Recommended hardening:

- Store both possible and present voting power in the script policy calculation.
- Require xpub quorum at least as high as the spend threshold, or compute script threshold as a function of `possible_vp`.
- Add property tests for every small validator-set size and uneven voting-power distribution.

### Other hardening notes

- `IbcDest::transfer` logs and suppresses `ibc.deliver_message` errors after minting into the IBC transfer module. This may be intentional as a local escrow fallback, but exact success/failure accounting should be asserted.
- `Cosmos::relay_op_key` decodes `Any.value` as secp256k1 without checking `Any.type_url`, and checks supplied proofs against the provided height while checking validator membership in the latest header. Needs a proof-height/type-url binding test.
- `build.rs` with `legacy-bin` can run `git fetch --tags --force` and shell `build.sh`; this is a supply-chain and reproducibility risk for legacy-enabled builds.
- `src/app/migrations.rs` has `InnerAppV6 -> InnerAppV7` as `todo!()`. If reachable in an upgrade path, this is a deterministic halt.
- Capacity limiting only disables deposits for newly-created checkpoints after the last completed reserve output reaches the cap. `relay_deposit` does not enforce an aggregate pending/building capacity check, so in-flight deposits to an enabled checkpoint can exceed the configured cap.

### NOMIC-AUD-L002

Classification: Likely vulnerability requiring one specified verification step

Title: Bitcoin retarget timestamp calculation can underflow before PoW validation

Severity: Medium for mainnet-equivalent settings under a miner/timestamp adversary; higher for low-difficulty local/test configurations

Evidence:

- `HeaderQueue::get_next_target` calls `calculate_next_target(previous_header, first_reorg_height)` at retarget boundaries (`src/bitcoin/header_queue.rs:538-540`).
- `calculate_next_target` computes `let mut timespan = header.time() - prev_retarget;` using `u32` timestamps (`src/bitcoin/header_queue.rs:585-595`).
- The local MTP check only requires each new timestamp to exceed the median of the previous 11 headers (`src/bitcoin/header_queue.rs:636-653`).
- Root `Cargo.toml` enables release overflow checks, so this subtraction is a deterministic panic rather than a harmless wrap in release builds.

Local model:

```bash
python3 audit/run-20260621T0354CDT-c5486a4/poc_header_retarget_underflow_model.py
```

The model constructs timestamps satisfying the same MTP rule where the last block before retarget has timestamp `1337` and the first block in the period has timestamp `10000`; the Rust `u32` subtraction would underflow.

Risk:

A valid Bitcoin-consensus timestamp pattern can be invalid for Nomic's arithmetic. A miner/hashpower adversary able to produce the preceding epoch, or a low-difficulty test/local configuration, can make a relayed retarget header panic before the code reaches `validate_pow`.

Verification step:

Create a local header-chain regression that mines or simulates the 2016-header sequence and submits the retarget-boundary header to `HeaderQueue::add`, then compare target calculation with Bitcoin Core's signed/clamped timespan behavior.

Recommended fix:

Use signed or checked timestamp differences matching Bitcoin Core semantics, clamp negative timespans to `target_timespan / 4`, and add retarget tests for negative, zero, boundary, and extreme positive timespans.

## 8. REJECTED HYPOTHESES

- Direct malformed legacy commitments causing double-credit: rejected for current code. The branch rejects legacy outputs instead of accepting them.
- `unsafe` in target source: rejected for `src`/`tests`; `rg '\bunsafe\b'` returned no hits. Dependencies were not geiger-reviewed.
- No-default build success: rejected. It fails without `csv`, but passes with `--features csv`.
- Exact-threshold signer completion with `>=`: rejected. Code and Bitcoin script both use strict `>`, so exact 2/3 or 9/10 does not complete.

## 9. TEST AND COVERAGE REPORT

Tests/builds run:

- `cargo fmt --all -- --check`: pass
- `cargo check --lib --no-default-features --features csv`: pass
- Default/all-features/devnet/mainnet-equivalent no-run builds: blocked by missing local `pkg-config`/OpenSSL
- `cargo check --lib --no-default-features`: fails due optional `csv`

Models added:

- `poc_threshold_model.py`: exact local model of present-vp quorum and strict threshold behavior
- `poc_legacy_commitment_model.py`: exact local model of legacy commitment comparison bug
- `poc_deposit_fee_prevalidation_model.py`: exact local model of the deposit miner-fee pre-validation mismatch
- `poc_header_retarget_underflow_model.py`: local MTP/timestamp model for retarget timespan underflow

Coverage gaps:

- No coverage-guided fuzz targets were added.
- No regtest Bitcoin Core differential tests were completed.
- No state-machine model for deposits/reorgs/checkpoints/IBC was completed.
- No crash/restart/failure injection was completed.
- No ARM64 or release-vs-debug differential transition tests were run.

## 10. DEPENDENCY AND SUPPLY-CHAIN REPORT

Captured artifacts:

- `cargo metadata --format-version 1`
- `cargo tree --all-features`
- `cargo tree -d`

Notable dependency facts:

- `Cargo.toml` pins Orga with `merk-verify` and `feat-ibc`; `full` enables `orga/merk-full`, `orga/abci`, and `orga/state-sync`.
- Duplicate dependency tree is substantial, including multiple `prost`, `cosmos-sdk-proto`, `cosmrs`, `secp256k1`, `hashbrown`, and Ethereum/IBC packages. Raw duplicate tree is in `phase0/cargo-tree-duplicates.out`.
- Build scripts:
  - Root `build.rs` sets `GIT_BRANCH`; with `legacy-bin`, it fetches tags and executes `build.sh`.
  - Numerous generated Babylon protobuf files exist under `src/babylon/proto/gen/`.
- Scanner tools unavailable: `cargo audit`, `cargo deny`, `cargo geiger`.

Supply-chain limitations:

- No advisory database scan completed.
- No proc-macro/build-script audit beyond root `build.rs`.
- No comparison of pinned Git dependencies to releases.

## 11. PRIORITIZED REMEDIATION PLAN

1. Privately decide and document the custody threshold invariant for xpub quorum and total validator voting power; if total power is intended, patch `SignatorySet`/checkpoint creation before any further custody growth.
2. Patch legacy commitment matching and recovery commitment propagation; inventory any legacy deposit commitments still in flight.
3. Fix deposit pre-validation so it uses the exact post-miner-fee amount and add a failed-credit recovery path for `Identity::None`.
4. Add threshold arithmetic property tests over small validator sets, uneven power, duplicate keys, absent xpubs, and strict `>`/`>=` boundaries.
5. Add end-to-end deposit tests for current and legacy commitments, expiry, recovery, duplicate relay, amount-sensitive destinations, and reorg removal/reinclusion.
6. Fix FROST participant ordering and absent handling before relying on FROST/Babylon flows.
7. Patch retarget timestamp arithmetic and add Bitcoin Core differential tests for negative/short/long epoch timespans.
8. Restore a reproducible build/test environment with `pkg-config`/OpenSSL or vendored OpenSSL and run the full feature matrix.
9. Add IBC packet state-machine tests for success/timeout/ack replay and direct-deposit failure paths.
10. Run supply-chain tools and review reachable Orga/merk/ed/IBC/Bitcoin dependencies.

## 12. RESIDUAL RISK

This run produced useful Phase 0 artifacts and several targeted findings, but it is not a complete custody audit. The largest residual risks are threshold-policy ambiguity, untested Orga atomicity, unmodeled Bitcoin reorg/deep-pruning behavior, unverified retarget arithmetic against Bitcoin Core, untested IBC ack/timeout exactly-once behavior, incomplete dependency review, and lack of broader differential validation against Bitcoin Core/IBC references.

No vulnerability details were published or sent to public infrastructure.
