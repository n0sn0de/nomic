# Harness-first plan for ZEC deposits and nZEC on Nomic

- **Status:** Conceptual implementation plan; no bridge implementation is included in this branch
- **Plan date:** 2026-08-02
- **Repository:** `n0sn0de/nomic`
- **Base branch:** `develop`
- **Base commit:** `3dccaf5d6349430148fa490cc4a0bddbf2ef433e`
- **Plan branch:** `plan/zcash-deposits-harness-first`
- **Primary objective:** Extend Nomic with a separately accounted ZEC reserve and nZEC asset while preserving the existing BTC/nBTC bridge and its behavior.
- **First deliverable:** A local, hermetic Nomic↔Zakura test harness with executable golden scenarios, protocol vectors, fault injection, and retained receipts. Zakura is the primary Zcash node; bridge implementation starts only after that harness gate is green.

---

## 1. Executive position

The implementation must begin with the test system, not with a `src/zcash` directory full of unexercised optimism.

The recommended sequence is:

1. Extract the useful process-control ideas already embedded in `tests/bitcoin.rs` into a first-class local interchain harness.
2. Prove the existing BTC deposit, checkpoint, signing, withdrawal, and IBC paths in that harness without changing their behavior.
3. Add a Zakura `v1.0.5` regtest network, a pinned Zakura `zcashd-compat` wallet/transaction-factory sidecar, exact Zcash protocol vectors, and deterministic fault controls. Zebra and Zallet remain differential oracles, not the primary node path.
4. Define every ZEC bridge scenario as a compiled executable contract with fixtures, steps, invariants, budgets, and evidence requirements. Before ZEC support exists, the catalog reports that contract as `Blocked`; it does not count as passing. Each implementation slice must first activate a contract and produce a real assertion failure, then make it pass.
5. Use the harness to settle two critical feasibility questions:
   - Can Nomic deterministically and economically verify Zcash headers, Equihash, difficulty, cumulative work, and reorganizations on-chain?
   - Which transparent custody design can fit Zcash Script limits without quietly weakening Nomic’s security model?
6. Only then implement Zcash consensus types, the light client, custody, checkpointing, nZEC accounting, relayers, signers, CLI surfaces, and IBC integration in small test-driven slices.
7. Treat any mainnet launch as a separate, capped, pausable release with an audit and an explicit governance acceptance of residual custody risk.

### Recommended first protocol scope

The first supported reserve should be **transparent Zcash custody**:

- Deposits are recognized when a Zcash transaction creates the exact registered transparent P2SH bridge output.
- The funding transaction may contain any shielded component legal for its active transaction version—Sprout/Sapling in v4, Sapling/Orchard in v5, and Sapling/Orchard/Ironwood in v6—while creating the transparent reserve output. A shielded-origin payment can therefore fund the bridge, but the output, amount, claim link, and later spend are public.
- Withdrawals initially target explicit transparent P2PKH or P2SH receivers.
- Nomic does not initially hold Sprout, Sapling, Orchard, or Ironwood notes, decrypt shielded notes, maintain shielded note trees, create shielded proofs, or perform direct shielded withdrawals.
- UI and CLI language must say **transparent custody**. Calling this a “private ZEC bridge” would be dishonest.

### Explicit transaction-version and pool matrix

“v5+” is not a protocol specification. Before H3 can pass, the source lock contains an exact matrix for each supported Zakura release and Zcash activation height:

| Operation | Initial hypothesis | Required evidence |
|---|---|---|
| Deposit transaction parsing | Accept every transaction version that can legally create a transparent output at the active launch upgrade: v4, v5, and—when NU6.3 is active—v6; reject v1-v3 and unknown/future versions | Per-version full serialization/mined-txid vectors, active branch/upgrade context, Zakura block inclusion, bounded parser, negative versions |
| V4 deposit effects | Transparent + optional Sprout JoinSplits + optional Sapling; no Orchard/Ironwood | Legacy full-encoding txid, ZIP-243 where signatures are tested, mixed-component blocks, duplicate/malformed bounds |
| V5 deposit effects | Transparent + optional Sapling + optional Orchard; no Sprout/Ironwood | ZIP-244 effects/auth digest tree and mixed-component vectors |
| V6 deposit effects | Transparent + optional Sapling + optional Orchard + optional Ironwood; no Sprout | Launch-pinned, activation-applicable NU6.3/v6 consensus text and maintained implementation vectors, including Ironwood and anchors moved from effects to authorizing data. The inspected ZIP-229 snapshot is still marked Draft and is not alone sufficient launch authority. |
| Checkpoint construction | One explicitly selected transaction version valid for the active network—v5 before an accepted NU6.3 boundary or v6 when required—transparent-only inputs/outputs, `SIGHASH_ALL`, explicit nonzero expiry margin | Zakura mempool/block acceptance, independent version-specific builder/sighash, expiry/fee vectors |
| Shielded-origin factories | Compatibility sidecar for the named pools/upgrade window it supports; Zallet or another pinned maintained wallet for Orchard/Ironwood cases it no longer supports | Pool/version/activation capability receipt, raw transaction, and Zakura cross-check; factory limits cannot narrow consensus deposit recognition |

H3 pins the exact active matrix, and every dependency-promotion and launch gate rechecks the live target network’s upgrade and Zakura policy. A transparent reserve address remains payable by any network-valid version, so unsupported-but-valid active versions are a launch blocker, not a normal “reject and move on” case. Future versions require new RED/GREEN slices and a support-lease update before activation.

### Two blockers that must be resolved before production code

1. **Custody script mismatch:** Nomic currently creates Bitcoin P2WSH reserve outputs. Zcash transparent receivers are P2PKH/P2SH, not SegWit or Taproot. Nomic’s weighted script grows with the validator set and cannot simply be wrapped in Zcash P2SH.
2. **Light-client cost:** A Zcash mainnet header is much larger than an 80-byte Bitcoin header and includes an Equihash solution. Native verification, storage, pruning, batching, and deterministic execution must be measured inside Nomic’s actual consensus environment.

No mainnet funds should depend on guessed answers to either question.

---

## 2. Scope and non-goals

### In scope

- A repository-owned local test harness for Nomic, Bitcoin, a Zakura-based Zcash network, bridge workers, signers, and optional IBC peers.
- Characterization of the existing BTC/nBTC behavior before shared code is extracted.
- A Zcash header light client or a documented, explicitly weaker alternative if native verification proves infeasible.
- Transparent ZEC deposit custody, proof relay, checkpoint batching, signing, broadcast, confirmation, withdrawal, recovery, and emergency handling.
- A distinct `Nzec` accounting domain and IBC-visible asset denomination.
- Separate Zcash relayer, signer, keys, configuration, metrics, and operator controls.
- Network-upgrade-aware transaction construction and signature hashing.
- Supply/reserve conservation checks and independent receipts.
- Mainnet launch controls: pause, caps, rate limits, observation period, and rollback/halt behavior.

### Explicit non-goals for the first production release

- Custody of Sprout, Sapling, Orchard, or Ironwood shielded notes.
- Direct shielded withdrawal outputs.
- Reusing Bitcoin private keys, extended keys, signer homes, deposit indexes, or domain separators for Zcash.
- Treating Zcash as an IBC chain. The Zcash proof relayer is not Hermes and is not an IBC light client.
- Replacing the BTC bridge with an unproven generic “UTXO bridge” abstraction.
- Using archived stock `zcashd` or the Zakura compatibility sidecar as the production verifier. The sidecar is a harness wallet/factory and an optional operator compatibility surface; Zakura remains the consensus node.
- Validator-attested Zcash headers presented as equivalent to proof-of-work verification.
- Uncapped mainnet value behind unaudited threshold cryptography or a small signer committee.
- Automatic recovery from a deep post-credit Zcash reorganization by minting, burning, or clawing back user balances.

---

## 3. Evidence from the pinned Nomic tree

The plan is anchored to the exact base commit above. The fork and `nomic-io/nomic` upstream `develop` branch both pointed to that commit when this plan was prepared.

| Existing surface | Evidence in the base tree | Consequence for the plan |
|---|---|---|
| End-to-end process tests | `tests/bitcoin.rs` exercises bitcoind/Nomic/signer/relayer deposit, withdrawal, checkpoint, pending-credit, key-update, and recovery paths; `tests/ibc.rs` separately exercises the Hermes/IBC topology. Thirteen integration tests across `tests/` are ignored in the pinned tree. | Preserve separate attributable receipts; extract lifecycle/readiness/driver seams and make required cases runnable rather than describing one monolithic test as stronger than it is. |
| Deposit verification | `src/bitcoin/mod.rs::relay_deposit` checks confirmation depth, a Bitcoin partial Merkle proof, output script, outpoint uniqueness, amount, sigset age, and fees before placing an input in a building checkpoint and minting pending nBTC. | ZEC needs its own transaction/proof types and namespaced outpoints, but should preserve this verification order and pending-credit discipline. |
| Header tracking | `src/bitcoin/header_queue.rs::HeaderQueue` stores and validates Bitcoin headers and work. | Do not parameterize it until Zcash fork, difficulty, Equihash, header size, and upgrade rules are characterized. Implement a separate Zcash queue first. |
| Checkpoint state machine | `src/bitcoin/checkpoint.rs` models building, signing, complete checkpoints; transaction batches; fees; change; input signature messages; and emergency disbursal. | Reuse the state-machine concepts, not Bitcoin transaction encodings. Zcash expiry height and ZIP-317 fees require different transition rules. |
| Reserve script | `src/bitcoin/signatory.rs::SignatorySet::output_script` emits P2WSH; the redeem script encodes validator voting weights and destination bytes. | It cannot be used on Zcash. The destination commitment and validator policy need a different transparent custody construction. |
| Signer | `src/bitcoin/signer.rs` derives ECDSA child keys, queries messages, signs them, and submits compact signatures. | Add a domain-separated Zcash signer. Never point existing Bitcoin key material at Zcash messages. |
| Relayer | `src/bitcoin/relayer.rs` relays headers, scans watched scripts, constructs deposit proofs, broadcasts checkpoints, and retains a local deposit index. | Separate generic process plumbing from Bitcoin RPC/transaction semantics; add idempotent Zcash rescan and locally constructed Merkle paths. |
| Asset type | `src/bitcoin/mod.rs::Nbtc` is a single `Symbol` named `usat`; `Bitcoin` owns nBTC accounts and pending transfers. | Add `Nzec` and separate accounts. Never represent ZEC as `Coin<Nbtc>` or use a runtime string to distinguish liabilities. |
| Application/IBC wiring | `src/app.rs` hard-codes nBTC fee payment, sends, incoming `usat` handling, IBC deposits, and withdrawals. `src/cosmos.rs` queries nBTC IBC escrow explicitly. | Multi-asset changes reach calls, protobuf dispatch, fees, IBC escrow, queries, CLI, migration, and supply accounting—not only the bridge module. |
| CLI | `src/bin/nomic.rs` exposes BTC deposit, interchain deposit, withdrawal, signer, relayer, send-nBTC, and IBC commands. | Add explicit ZEC/nZEC commands and flags. Ambiguous generic commands are a later compatibility decision. |
| Existing FROST | The optional `frost-secp256k1-tr` dependency and `src/frost/` implement Taproot/Schnorr-oriented FROST. | Zcash transparent P2PKH/P2SH `CHECKSIG` uses ECDSA. Existing FROST does not make a Zcash threshold key appear by renaming it. |
| Toolchain | The tree pins `nightly-2024-07-21`; `.github/workflows/build.yml` uses `actions-rs/toolchain@v1`, `cargo test --release -- --nocapture`, and `cargo build --release --features full --bin nomic`; there is no root Cargo workspace. | Put the harness in an explicit nested workspace/toolchain/lockfile and add named CI jobs—do not assume root commands discover it. |
| Migration debt | `src/app/migrations.rs` leaves `InnerAppV6 → InnerAppV7` as `todo!()`; older nested migrations also contain `unreachable!()` assumptions. | Close and fixture-test the existing migration path before appending Zcash state or claiming two-binary upgrade safety. |

### Characterization rule

Before extracting any shared bridge code:

- Record current BTC scenario receipts at the pinned commit.
- Add assertions around behavior, not implementation details.
- Refactor one seam at a time.
- Require receipt equivalence for BTC after every extraction.
- Do not make BTC and ZEC share a trait merely because both have `txid:vout`. Share code only after the second implementation proves the abstraction.

### Confirmation terminology

The current Bitcoin code accepts a deposit/checkpoint when:

```text
tip_height - inclusion_height >= min_confirmations
```

That variable is really a **minimum depth after the inclusion height**; a human counting the inclusion block calls it `min_confirmations + 1` confirmations. BTC characterization must preserve that exact behavior. New Zcash code uses an unambiguous field such as `min_blocks_after_inclusion`, publishes both depth conventions in queries, and tests `D-1`, `D`, and `D+1` for deposits and checkpoint confirmations. No cross-asset refactor may “fix” the Bitcoin off-by-one as an incidental cleanup.

For Zcash, `D = min_blocks_after_inclusion` is defined for the whole unsigned range without `H + C - 1`: `maturity_height = inclusion_height.checked_add(D)`, and the canonical tip must be at least that height before the work/challenge gates can advance. `D=0` means eligible at the inclusion height and is permitted only in regtest/shadow fixtures; an `Active` mainnet configuration requires a positive, ADR-selected depth plus work and observation thresholds. Overflow is a typed configuration/proof error. Vectors cover `D=0`, `D=1`, the selected production value, and height overflow.

---

## 4. Zcash is not “Bitcoin with another ticker”

| Concern | Current Nomic Bitcoin path | Zcash requirement |
|---|---|---|
| Reserve output | P2WSH weighted redeem script | Transparent P2PKH or P2SH; no P2WSH/Taproot |
| Script limits | Witness script avoids the 520-byte pushed-element ceiling | P2SH redeem script is pushed in `scriptSig`; pushed elements are capped at 520 bytes and standard P2SH sigops are capped |
| Signature digest | Bitcoin SegWit/BIP-143 semantics through the pinned Bitcoin crate | V4 uses ZIP-243; v5 uses ZIP-244; v6 uses the launch-pinned NU6.3 modifications to the ZIP-244 digest tree. Branch ID alone does not select the structure. All transparent prevout amounts and locking scripts are version-specific inputs. |
| Transaction ID | Bitcoin txid plus witness separation | V4 uses the legacy full-encoding mined txid. V5 uses ZIP-244 effects txid/auth commitment; v6 extends that tree for Ironwood and moves shielded anchors. V5/v6 witnessed identity is the typed 64-byte txid/auth pair, not one Bitcoin-style 32-byte wtxid. |
| Transaction lifetime | Checkpoint transactions are not height-expiring in the same manner | Zcash transactions carry expiry height; signatures and broadcasts must be rebuilt when stale |
| Fee policy | Fee rate and estimated virtual bytes | ZIP-317 logical actions and conventional fee; policy must be versioned and tested |
| Header | 80-byte Bitcoin header | 1,487 bytes for inspected production parameters: 140 fixed bytes plus CompactSize-encoded 1,344-byte Equihash solution; inspected regtest is 177 bytes with a 36-byte solution. Functional regtest costs are not production costs. |
| Proof of work | Bitcoin SHA-256d target and retarget rules | SHA-256d difficulty filter plus Equihash solution and Zcash-specific difficulty rules |
| Upgrades | Bitcoin network assumptions in the pinned crate | Zcash network upgrades, activation heights, branch IDs, transaction versions, and changing header commitments |
| Merkle proof RPC | Bitcoin node supplies a tx-out proof path used by the relayer | Zakura `v1.0.5` does not expose `gettxoutproof` in its inspected `Rpc` trait; the relayer must fetch the raw block from Zakura and construct the Bitcoin-inherited tx Merkle branch itself |
| Privacy | No shielded pools | Deposits can be shielded-origin but custody output is transparent; direct shielded custody/withdrawal is separate work |
| Unit | satoshi | zatoshi; both use 10^-8 base units but must remain different nominal types |

### Script-size feasibility fact

The archived Zcash node source retains these transparent Script limits:

- maximum pushed script element: 520 bytes;
- maximum script: 10,000 bytes;
- standard P2SH policy caps expensive sigops;
- its own policy comment identifies a 15-of-15 compressed-key multisig redeem script as 513 bytes.

Nomic’s `SignatorySet::est_witness_vsize()` returns `79*n + 39` and equals 513 at six signatories, but that value is **estimated Bitcoin witness virtual bytes**, not redeem-script bytes. The actual weighted redeem script is built dynamically from pubkeys, encoded voting powers, threshold, and destination commitment; it grows per signatory and the base code has no Zcash 520-byte P2SH bound. The plan therefore does not use the 513-vbyte estimate as script-size evidence. Direct wrapping remains rejected until exact generated-script vectors prove every configured case, and it cannot scale to an ordinary full validator set under the pushed-element ceiling.

For the recommended standard committee hypothesis, the geometry is concrete but economically ugly: a compressed-key `11-of-15 CHECKMULTISIG` redeem script is 513 bytes. With exactly 11 worst-case standard 73-byte signatures including hash-type bytes, its scriptSig is 1,331 bytes and the serialized transparent input is 1,374 bytes. Under the inspected ZIP-317 Revision-0 constants this contributes 10 logical actions, implying a 50,000-zatoshi conventional fee before any larger output-side contribution. A policy probe with all 15 signatures is 1,670 serialized input bytes; two such inputs are 3,340 bytes, 23 actions, and 115,000 zatoshis. The production builder emits exactly 11 signatures, but `ZCP-003`/`ZCP-013` lock both the intended and maximal-policy cases and recompute them for every supported fee revision. Script fit does not imply cheap custody.

---

## 5. Security and trust model

### Required safety properties

1. **Proof before mint:** No nZEC liability is created unless Nomic consensus verifies a transaction effect, inclusion path, accepted header chain, network, confirmation policy, registered output, amount, destination binding, and unprocessed outpoint.
2. **Namespaced identity:** A Zcash outpoint can never collide with, deserialize as, or satisfy a Bitcoin outpoint. Chain, network, transaction version, and asset domains are explicit types.
3. **Destination integrity:** A relayer cannot change the Nomic or IBC destination attached to a deposit.
4. **Conservation:** Every state transition preserves the enumerated nZEC liability/reserve equation. There is no generic mint escape hatch.
5. **Key separation:** BTC and ZEC signing material are independently generated and stored. Domain tags alone are not a substitute for distinct keys.
6. **Finality honesty:** Confirmation depth, cumulative work, and observation time reduce ordinary reorg risk; they do not create mathematical finality. A greater-work conflict beyond the local finality-risk boundary enters the authenticated halt/deficit policy and raises governance/operator alarms.
7. **Independent relayers:** Any operator can relay valid headers, deposits, and checkpoint confirmations. A designated relayer may improve liveness but has no mint authority.
8. **Consensus determinism:** Header, proof, transaction, fee, and signature verification produce identical results on every Nomic validator and do not call external RPCs from consensus.
9. **Upgrade fail-closed:** Unknown Zcash transaction versions, branch IDs, network upgrades, or fee rules halt new construction/relay rather than guessing.
10. **Bounded launch:** Governance can pause deposits and withdrawals independently, cap per-deposit and aggregate liabilities, rate-limit growth, and preserve already-held users’ recovery path.
11. **Authorized signing only:** A signer signs only a locally verified, on-chain-authorized checkpoint package and maintains crash-safe anti-equivocation state. A remote RPC response containing a digest is not authority.
12. **Exact economic partition:** Every zatoshi and every nZEC claim occupies exactly one named reserve, liability, protocol-equity, or explicit-deficit class at each durable state; unconfirmed change, old inputs, IBC escrow/vouchers, and burned-but-unpaid withdrawals cannot be double-counted or omitted.
13. **Exit after exposure:** Custody obligations, evidence retention, and any enforceable bond/slashing horizon survive validator removal or unbonding until all outputs controlled by that key epoch are swept or otherwise resolved.
14. **Anchor discipline:** A trusted Zcash checkpoint or migration can create apparent proof authority and is therefore governed like mint authority: bridge-disabled transition, reproducible evidence, delay/veto, and supply/reserve reconciliation are mandatory.

### On-chain safety states and authority

The bridge needs an explicit state machine, not scattered booleans:

| State | Deposits | Normal withdrawals | Sweeps/recovery | Allowed transition authority |
|---|---|---|---|---|
| `Disabled` | reject | reject unless resolving pre-existing state | reviewed migration/recovery only | delayed governance plus veto |
| `Shadow` | observe only; no liability | reject | dry-run only | delayed governance plus veto |
| `Active` | within per-tx/rate/aggregate caps | within rate/fee policy | allowed | normal deterministic rules |
| `DepositsPaused` | reject | allowed if solvent | allowed | narrow guardian may enter; cannot exit |
| `WithdrawalsPaused` | policy-defined reject/queue | reject new execution | safety sweeps and reviewed recovery only | narrow guardian may enter; cannot exit |
| `FullHalt` | reject | reject | only pre-authorized safety actions | narrow guardian may enter; delayed governance exits |
| `Deficit` | reject | no discretionary or preferential settlement | evidence preservation and approved loss-waterfall execution only | automatic invariant trip; delayed governance plus veto resolves |
| `Recovery` | reject | only claims included by the approved recovery plan | deterministic plan execution | delayed governance plus veto |

Authority rules:

- A guardian can only reduce risk immediately: pause or lower an unused cap. It cannot unpause, raise caps, change committee/threshold, replace an anchor, edit a destination, redirect funds, or select favored creditors.
- Unpause, cap increase, custody-policy change, anchor change, migration, and recovery plan require delayed governance with an independent veto window and a machine-checkable proposed-state diff.
- Permissionless relayers remain permissionless in every state where their proof type is accepted; pause authority never grants proof or mint authority.
- Caps are checked atomically against all liabilities and in-flight obligations under same-block concurrency. Lowering a cap below current exposure does not erase claims; it prevents new exposure.
- Before L1, governance must approve a deficit/loss waterfall covering deep reorg, key loss/theft, invalid mint, and migration failure. The plan must define claim priority, any insurance/bond, pro-rata behavior, and prohibited preferential settlement. “Governance will decide later” is not a recovery design.

### Threat actors

The harness and design must cover:

- malicious or stale header relayers;
- an eclipsed, stale, maliciously configured, or pruned-beyond-retention Zakura node;
- a relayer redirecting deposits or replaying outpoints;
- invalid/withheld/equivocating signers;
- a custody committee below threshold;
- a Byzantine Nomic validator set within and beyond the assumed fault bound;
- deep Zcash reorganizations;
- Zcash network upgrades during a building/signing checkpoint;
- fee spikes, expiry, mempool rejection, and censorship;
- compromised bridge worker hosts;
- users supplying ambiguous, wrong-network, shielded-only, or malformed addresses;
- integer, denomination, and cross-asset confusion;
- dependency compromise or behavioral drift between Zakura, its `zcashd-compat` sidecar, Zebra/Zallet differential oracles, and Rust libraries.

### Trust modes that must be named correctly

| Mode | Verification | Permitted use |
|---|---|---|
| Native PoW/SPV light client | Nomic verifies header linkage, Equihash, target/difficulty, time rules, cumulative work, fork choice, and the relayed transaction’s Merkle inclusion. It does **not** fully validate every Zcash block body, shielded proof, history tree, coinbase/funding rule, or semantic block-commitment input. | Preferred production target only if governance accepts the explicit SPV assumption that the greatest-work eligible chain is Zcash-consensus-valid, after deterministic cost and audit gates. |
| Succinct external proof | Nomic verifies a proof generated over Zcash consensus with a reviewed verifier and explicit setup/security assumptions. | Research fallback if native verification is infeasible. |
| Validator-attested headers | Nomic accepts a threshold of validator assertions without independently verifying Zcash PoW. | Harness spike or tightly capped experiment only; never described as trust-minimized. |
| Single relayer oracle | One process says a deposit happened. | Forbidden for value-bearing deployment. |

---

## 6. Harness-first architecture

### 6.1 Primary Zcash node decision: Zakura

The harness and production worker design are based around **Zakura**, not a lowest-common-denominator Zcash RPC abstraction.

The initial lock is:

| Component | Immutable source/release pin | Harness role |
|---|---|---|
| Zakura source | `zakura-core/zakura` `v1.0.5`, commit `57c2898f1e2202a05215d2f0bc92c1e3478c8cda` | Primary Zcash consensus node, RPC, mempool, mining, fork/reorg controls, and chain data |
| Zakura OCI image | `zakuracore/zakura:1.0.5@sha256:9162c9217e840404e7c67b3425d100f0e2ed36ec258009593cb59cefe0ef0f66` | Default containerized node; the lock file must also record the selected platform-manifest digest |
| Zakura binary release | `zakurad-v1.0.5`; x86_64 SHA-256 `ef4de5db7967474c55809a146d88b3dd7defd3ba38e0c1a7271bfbd1d848a8b5`, aarch64 SHA-256 `829350633c88464e71d39dda133875fc7737f708779090a5096252b1ac6a72ac` | Native-process lane and container-independent reproduction |
| Zakura compatibility sidecar source | `valargroup/zcashd` `v1.1.0`, commit `c44e282a1b72ceeaef7fb494a8757cfde0a4d5fc` | Test wallet/transaction factory and zcashd-RPC compatibility oracle; never the Nomic consensus source |
| Compatibility sidecar OCI image | `zakuracore/zcashd:v1.1.0@sha256:d2a743c690f140f912825159cf90d5309c27f79071ebf5de54526edcdb9efbd0` | Split-container wallet lane, pinned to one Zakura peer |

The Zakura release commit is GitHub-verified and its release publishes checksums plus a `SHA256SUMS.txt.minisig`. A release signature is trusted only after the Zakura minisign public key and its provenance are pinned through a separately reviewed trust-root process; the key was not found in the inspected `v1.0.5` source/installer. Until that trust root exists, the harness verifies the source-lock checksum and GitHub asset digest but must not claim minisignature verification. An OCI tag is never accepted without its digest.

#### Verified Zakura capabilities that shape the harness

- `Regtest` is a first-class network mode.
- Configured regtest accepts explicit network-upgrade activation heights and checkpoints.
- `generate` mines immediate regtest blocks only when PoW is disabled; it randomizes coinbase data, so scenario receipts assert branch relationships and effects rather than brittle fixed block hashes.
- `getblocktemplate` and `submitblock` provide explicit block-construction and competing-branch paths.
- `invalidateblock` works for known, non-finalized state; `reconsiderblock` works for blocks retained in the invalidated-block cache. Deep/finalized-fork tests must use separately constructed branches or isolated node state rather than pretending invalidation is unlimited.
- `getblock`, `getblockheader`, `getrawtransaction`, `sendrawtransaction`, `gettxout`, address-UTXO/index methods, and tip queries cover the bridge scanner’s core read path.
- The `v1.0.5` RPC trait does **not** expose `gettxoutproof`; proof construction therefore belongs in the Nomic relayer from raw Zakura block data.
- Zakura’s repository ships a four-node regtest e2e with legacy-only, Zakura-only, and dual-stack peers; it exercises block propagation, from-scratch catch-up, restart matrices, checkpoints, non-finalized reorgs, JSONL traces, and a trace oracle.
- Zakura can front a single-peer `zcashd-compat` sidecar. That sidecar retains transparent and Sapling wallet flows, but its Orchard support stops at NU6.3 and it does not support Ironwood. Shielded-origin test coverage must therefore name the pool and upgrade height; it may use Zallet as a differential factory where the sidecar is intentionally unable to construct the transaction.
- Zakura `v1.0.5` source contains v6/Ironwood transaction, subtree, txid, and sighash paths. The harness still binds those paths to activation-applicable consensus text and independent vectors; code presence is not launch authority.
- The sidecar source’s `COPYING` is MIT for directly included code but warns that default builds depend on Berkeley DB with AGPL terms and that other build dependencies carry their own licenses. H0 records the complete image SBOM/license set before caching, redistributing, or publishing derived images.
- Zakura supports archive and pruned storage. Bridge relayers default to archive mode. A pruned configuration is accepted only when retained block bodies exceed confirmation depth, fork window, maximum worker outage/rescan horizon, and an explicit safety margin; the harness tests both success inside retention and fail-closed behavior outside it.

#### Integration contract

1. Nomic header, deposit, checkpoint, and recovery workers query **Zakura RPC directly**.
2. The compatibility sidecar is pinned to Zakura as its sole P2P peer and is used only to create/fund/sign test transactions or to check legacy wallet/RPC behavior.
3. Production bridge correctness never depends on wallet RPC, ZMQ, sidecar chainstate, or a single Zakura-specific trace field.
4. Zebra, stock/compatibility `zcashd`, Zallet, `librustzcash`, and official vectors are differential oracles. Agreement with Zakura alone is not independent verification.
5. The harness preserves Zakura logs, RPC capability discovery, metrics, JSONL traces, and raw blocks in failure artifacts, then runs both the bridge accounting oracle and Zakura’s trace invariants.
6. A Zakura version bump is a reviewed compatibility change: update source/release/image digests, diff the RPC schema and consensus dependencies, run all protocol/node/fork/pruning lanes, then promote the lock.
7. Zakura’s experimental P2P-v2 transport is tested because the node owns it; the bridge itself remains transport-agnostic above authenticated local RPC.

### 6.2 Framework decision

Interchaintest, Starship, and Hermes provide useful patterns, but none should be imported wholesale as the semantic model:

| Inspiration | Keep | Reject for this first harness |
|---|---|---|
| Interchaintest | Typed drivers, isolated Docker networks, lifecycle cleanup, retained data on failure, JSON event receipts, eventual assertions | Forcing Nomic, Zakura, or bridge workers into Cosmos SDK/IBC chain interfaces; making Go the source of Zcash protocol truth |
| Starship | Declarative topology overlays, pinned images, readiness probes, resource profiles, multi-validator layouts | Requiring Kubernetes for the default local loop; fixed shared ports; chain assumptions tied to Cosmos genesis tooling |
| Hermes IBC test framework | Driver/fixture/test separation, test overrides, explicit topology, eventual-consistency helpers, real Hermes lane | Treating the Nomic↔Zcash proof path as IBC; inheriting a Gaia-centric driver hierarchy |

**Recommended implementation:** a repository-owned Rust harness in an isolated package, with process/container drivers and exact protocol-vector helpers. Rust matches Nomic, Zakura, and current Zcash libraries; isolation prevents modern harness dependencies from destabilizing the pinned consensus crate.

### 6.3 Proposed repository layout

All paths below are proposed implementation paths, not files added by this planning branch.

```text
tools/interchain-tests/
├── Cargo.toml                 # standalone nested workspace, explicit CI job
├── Cargo.lock
├── rust-toolchain.toml        # pinned stable harness compiler
├── README.md                  # public commands; no private topology or credentials
├── crates/
│   ├── harness/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── topology.rs        # typed topology and capability declaration
│   │   │   ├── readiness.rs       # bounded probes; no blind sleeps
│   │   │   ├── retry.rs           # classified eventual assertions
│   │   │   ├── artifacts.rs       # receipts, logs, redaction, checksums
│   │   │   ├── contract.rs        # Blocked/Enabled inventory; never fake pass
│   │   │   ├── driver/
│   │   │   │   ├── process.rs
│   │   │   │   ├── container.rs
│   │   │   │   ├── nomic.rs
│   │   │   │   ├── bitcoin.rs
│   │   │   │   ├── zakura.rs
│   │   │   │   ├── zakura_compat_wallet.rs
│   │   │   │   ├── zebra_oracle.rs
│   │   │   │   ├── zallet_oracle.rs
│   │   │   │   ├── bridge_worker.rs
│   │   │   │   ├── signer.rs
│   │   │   │   └── hermes.rs
│   │   │   ├── fault/
│   │   │   │   ├── network.rs
│   │   │   │   ├── process.rs
│   │   │   │   ├── disk.rs
│   │   │   │   └── chain.rs
│   │   │   └── scenario/
│   │   │       ├── btc.rs
│   │   │       ├── zec_deposit.rs
│   │   │       ├── zec_withdrawal.rs
│   │   │       ├── zec_headers.rs
│   │   │       ├── zec_signing.rs
│   │   │       ├── ibc.rs
│   │   │       └── multi_asset.rs
│   │   └── tests/
│   │       ├── manifest_completeness.rs
│   │       ├── protocol_vectors.rs
│   │       ├── golden_smoke.rs
│   │       ├── golden_full.rs
│   │       └── adversarial.rs
│   └── oracle/
│       ├── Cargo.toml              # dependency-denied from Nomic/bridge code
│       └── src/
│           ├── accounting.rs
│           ├── bitcoin.rs
│           ├── zcash.rs
│           └── ibc.rs
├── scenarios/
│   ├── manifest.toml          # IDs, phase, capabilities, topology, invariants
│   └── topology/
│       ├── smoke.toml
│       ├── full-quorum.toml
│       ├── ibc.toml
│       └── forked-zcash.toml
└── vectors/
    ├── source-lock.toml       # source commit/tag and license for each vector
    ├── headers/
    ├── transactions/
    ├── sighashes/
    ├── merkle/
    └── scripts/
```

### 6.4 Driver contracts

Do not create one god-trait named `Chain`.

- `LifecycleDriver`: initialize, start, readiness, stop, kill, restart, snapshot diagnostics.
- `NomicDriver`: start validators, submit/query typed app calls through a narrow test adapter, advance/observe blocks, query accounts/checkpoints/header state/events.
- `ZakuraDriver`: assert release/network/genesis/RPC capabilities; query tips, headers, raw blocks/transactions and UTXOs; submit transactions/blocks; configure peer stacks and activation heights; mine regtest blocks; invalidate/reconsider only within advertised limits; expose pruning horizon, metrics, and trace artifacts.
- `ZcashMinerDriver`: use Zakura `generate` for the fast PoW-disabled lane and `getblocktemplate`/`submitblock` for explicit competing branches; production-PoW validity remains a vector/replay lane.
- `ZcashWalletDriver`: drive the pinned Zakura compatibility sidecar for transparent/Sapling flows; use Zallet only for declared differential or post-NU6.3 shielded-factory cases; create ephemeral accounts, fund pools, build/sign payments, and report operation status.
- `ZcashOracleDriver`: compare Zakura-visible blocks, transaction effects, txids, sighashes, and policy results with official vectors and at least one maintained independent implementation.
- `BridgeWorkerDriver`: start/stop header, deposit, and checkpoint workers independently; expose only test-safe health and cursor state.
- `SignerDriver`: start/stop selected signers, inject invalid/withheld responses through a test-only build, inspect public signing state without exposing secrets.
- `HermesDriver`: create IBC path/channel, relay packets, stop/restart, query packet state.
- `FaultDriver`: network partition, latency/drop, process death, disk snapshot/restore, block invalidation, and controlled stale views.
- `ArtifactSink`: append structured events and persist diagnostics after failure.

The management plane is isolated from the service/data plane. It may start/stop processes, alter declared network rules, advance test-controlled clocks, and collect evidence; it may not fabricate a successful Nomic or Zakura RPC response for a primary golden path. Protocol-byte mutations use labeled adversarial fixtures/proxies and receipts, while a corresponding primary path runs unmodified release binaries end to end.

Primary golden paths run the exact release-feature Nomic/worker binaries whose hashes appear in the receipt. Test-only fault hooks are allowed only in labeled adversarial cases, must compile out of release artifacts, and cannot be the sole evidence for a failure path that can be injected at a process, network, disk, RPC, or signer boundary. Long-running relayer/signer code gains bounded readiness and cancellation/shutdown seams useful in production; the harness does not kill its way around missing lifecycle design and then call that graceful recovery.

### 6.5 Zakura-centered topologies

1. **Smoke:** one Nomic validator, one Zakura regtest node, one split `zcashd-compat` wallet sidecar pinned to that node, one worker, and one signer. Fast protocol and API feedback only; not a custody-quorum test.
2. **Full custody quorum:** exactly 15 eligible Nomic validators with 15 independently keyed Zcash signers under the Candidate-A hypothesis, independent workers, a dual-stack Zakura seed/producer, a Zakura-only relayer source, and a legacy-only Zakura observer/sidecar peer. A smaller `n` is a developer fixture, not evidence that `11-of-15` selection, liveness, signing cost, or rotation works.
3. **IBC:** full Nomic topology plus a minimal compatible Cosmos peer and Hermes.
4. **Forked Zcash:** two independently mineable Zakura views with controlled peering, explicit `submitblock` branches, and an observing Zakura node. `invalidateblock` is used only within its real non-finalized limits. Exercises fork choice and reorg behavior.
5. **Pruning/recovery:** one archive Zakura source and one pruned Zakura source with a deliberately bounded retention window. Exercises scans inside retention, outage beyond retention, source failover, and proof reconstruction.
6. **Compatibility/differential:** pinned Zakura stable, proposed-next Zakura, compatibility sidecar, and selected Zebra/Zallet oracle lanes. No floating `latest` tags.
7. **Protocol vectors:** no daemons. Pure deterministic parsing, hashing, Merkle, script, sighash, and header-verification vectors from pinned sources.

### 6.6 Lifecycle contract

Every scenario must execute this sequence:

1. Allocate an isolated run directory, network, dynamic endpoints, and ephemeral volumes.
2. Record Nomic and Zakura source commits, verified release checksums, OCI index and selected platform-manifest digests, feature flags, Zakura storage/P2P/upgrade configuration, topology, and scenario ID.
3. Generate regtest-only keys from the public semantic seed or inside isolated local volumes, label them impossible to reuse on production networks, and never print them into uploadable artifacts. Local retained environments may contain disposable test key material and are access-controlled and never uploaded.
4. Start components in dependency order using readiness probes, not sleeps.
5. Assert chain/network identity for every RPC endpoint before funding or submitting.
6. Execute arrange/act/assert steps with bounded, classified retries.
7. Record transactions, block hashes/heights, Nomic app heights, calls, events, balances, checkpoint states, and invariant results.
8. On success, retain the compact receipt and checksums.
9. On failure, additionally retain redacted configs, logs, state summaries, and process/container metadata.
10. Tear down unless `--retain-on-failure` is explicitly set; retained environments print a cleanup command.

#### Determinism, deadlines, and flake policy

- Every run has a public semantic root seed. The default corpus derives per-scenario seeds from the source lock, scenario ID, topology ID, and corpus index; `--seed` reproduces semantic actions and fault points.
- A separate random `run_nonce` affects only disposable resource names, endpoints, labels, and artifact directories. It never enters chain data, keys, action order, or expected outcomes, so two same-seed runs can execute in parallel without collision.
- All harness-generated keys, fixture choices, fault schedules, and order randomization use one specified deterministic RNG and record the derived seed. No production key material is involved.
- Zakura `generate` intentionally randomizes coinbase data. Receipts therefore normalize random block hashes/txids while preserving parent relations, heights, transaction effects, cumulative work, and raw artifacts. Byte-golden tests use fixed protocol vectors, not mined-regtest identities.
- A typed `ClockDriver` controls/observes Nomic consensus time, Zakura mining time, worker scheduling time, transaction expiry, rate-limit windows, and IBC timeout time. Tests use event/height/durability barriers, not wall-clock sleeps.
- Every readiness probe, RPC, process start/stop, eventual assertion, scenario, and teardown has a manifest deadline enforced by an outer supervisor watchdog. No unbounded loops and no blind startup sleeps are permitted.
- Retries are limited to typed transient states such as `NotReady`, `TipNotReached`, or a declared temporary transport failure. Invalid data, identity mismatch, consensus rejection, authorization failure, or resource exhaustion fails immediately.
- A scenario-level retry after failure is diagnostic only. It cannot turn a failed required CI result into a pass.
- PR lanes use one fixed seed corpus; nightly lanes use the fixed corpus plus a rotating recorded corpus. Release candidates replay every previously failing seed.
- A required flaky scenario blocks release. Quarantine is allowed only for non-release experimental compatibility lanes, with an owner, expiry, and linked evidence.
- Parallel runs use isolated dynamic endpoints, networks, volumes, labels, and artifact roots. The harness proves that cleanup removes only resources carrying the current run ID.

Each contract uses a schema equivalent to:

```toml
[[scenario]]
id = "ZDP-001"
case_id = "direct-exact-v5"
spec_version = 1
status = "blocked"                 # blocked | enabled; never interpreted as pass
release_required = true
requirement_ids = ["PROOF_BEFORE_MINT", "NZEC_CONSERVATION"]
phase = "I4"
topology = "full-quorum"
backend = "container"
fidelity = "real-release-binaries"
seed_domain = "nomic-zec/ZDP-001/v1"
overall_deadline_seconds = 1800
rpc_deadline_seconds = 30
max_rss_mib = 12288
max_artifact_mib = 2048
capabilities = ["zcash.headers.submit", "nzec.deposit.relay"]
fixtures = ["zakura-regtest-v1", "transparent-p2sh-v1"]
preconditions = ["deposit_terms_active", "canonical_tip_ready"]
steps = ["register", "fund", "mine", "relay", "challenge", "credit"]
barriers = ["registration_committed", "work_mature", "challenge_mature"]
terminal_predicates = ["credit_once", "reserve_matches"]
unchanged_projection = ["bitcoin.*", "nzec.unrelated_accounts"]
oracle = "independent-accounting-v1"
invariants = ["PROOF_BEFORE_MINT", "NZEC_CONSERVATION", "OUTPOINT_ONCE"]
fault_point = "none"
ci_lane = "full-quorum"
evidence = ["zakura.raw_block", "nomic.events", "accounting.ledger"]
```

### 6.7 Scenario contracts and true RED tests

The full ZEC catalog must exist before bridge code, but scaffolding is not allowed to impersonate TDD:

- Every scenario contract compiles and declares stable scenario/case/spec IDs, requirement traceability, backend/fidelity, topology, fixtures, derived seed, ordered setup/action/assert steps, named barriers, terminal predicates, unchanged-state projection, independent oracle, invariant IDs, evidence schema, first required capability, owner phase, deadlines, resource budget, CI lane, and final release requirement.
- `Blocked` is an inventory state emitted by `scenario list/run --json`; it is neither a passing test nor an ignored test. Pass counts exclude blocked contracts, and CI publishes the blocked count explicitly.
- Contract validation, fixture generation, protocol vectors, model-state transitions, topology primitives, and evidence schemas run before product code and must pass on their own merits.
- To begin an implementation slice, the developer changes exactly one bounded contract or coherent dependency set from `Blocked` to `Enabled` and records a **real failing assertion** against the unimplemented behavior. A runtime `MissingCapability` is failure once enabled.
- The implementation then adds the minimum production code required to make that assertion pass. The RED and GREEN commands and receipts are retained in the PR.
- Random RPC, timeout, parsing, process, or teardown failures are always failures; they can never satisfy an expected-RED condition.
- CI rejects a new or changed scenario that lacks an executable function, contract fields, oracle, evidence schema, budget, or release requirement. The blocked baseline may decrease but not increase without an explicit plan amendment.
- A required scenario that has once become `Enabled` cannot be demoted to `Blocked`, ignored, or optional to make CI green.
- The release gate requires zero blocked required contracts and zero ignored required tests.

This preserves a complete executable blueprint without declaring unimplemented behavior “green.”

### 6.8 Artifacts and receipts

Each run produces a machine-readable receipt containing:

- receipt-schema/runner versions, run ID, scenario/topology IDs, seed and corpus index;
- exact source commits and dependency/image digests;
- network identities and genesis hashes;
- normalized public addresses and transaction/block identifiers;
- ordered state transitions;
- before/after asset-bucket balances;
- accepted/rejected calls and stable error classes;
- reorg/fault actions;
- final invariant checks;
- per-component exit/timeout/resource status;
- canonical artifact manifest, log-file checksums, and redaction status.

Receipt JSON uses a versioned canonical serialization so checksums are reproducible. Schema migration tests read every retained fixture version; a new runner cannot silently reinterpret an old economic bucket or pass/fail class.

Artifacts are split by trust boundary:

- **Uploadable CI bundle:** always collected by an external supervisor before teardown—even on panic, timeout, or SIGKILL—then redacted and scanned. It never contains RPC passwords/cookies, key material, seed phrases, raw environment dumps, or internal topology. Test endpoints are component IDs, not copied host addresses.
- **Local retained environment:** opt-in, access-controlled, never uploaded, and may retain disposable regtest volumes/cookies/keys solely to reproduce the failure. Its receipt prints an expiry and cleanup command.

`HAR-010` injects canaries through arguments, environment, config, RPC, and logs, then scans the entire uploadable bundle. Collector failure is itself a failed test, not permission to upload raw state.

### 6.9 Oracle independence

The harness must not grade its own homework:

- Put protocol-vector and accounting oracles in separate packages/processes with dependency deny-lists preventing imports from `nomic`, proposed `src/zcash`, bridge workers, or production accounting helpers.
- Derive reserve truth from Zakura raw blocks/UTXOs and custody scripts, liability truth from an append-only scenario ledger plus independently decoded Nomic events/state, and IBC truth from both endpoint balances, commitments, acknowledgements, and timeouts.
- Production aggregate queries are observed outputs, not expected-value calculators.
- Cryptographic golden values require official vectors and, where available, an implementation path independent of the Zakura/Nomic code under test. “Two copies of the same fork agree” is not independence.
- Mutation tests must prove each oracle rejects a corrupted amount, asset tag, outpoint, Merkle sibling/index, branch ID, signature, fee, expiry, IBC denom trace, and event omission.
- The source lock records vector provenance, license, upstream file/hash, transform script/hash, and expected digest; hand-copied unexplained hex is forbidden.

---

## 7. Golden scenario catalog

The following is the **minimum** executable catalog to be built before production bridge implementation. “Golden” means the arrangement, state transition, and invariant are fixed. It does not mean brittle snapshots of random addresses or timestamps.

### 7.1 Harness and BTC characterization

| ID | Scenario | Required assertion/receipt |
|---|---|---|
| `HAR-001` | Clean topology bootstrap | Every component reports expected network identity and readiness; no fixed-port collision; teardown leaves no process/volume. |
| `HAR-002` | Retain on failure | Intentional assertion failure preserves redacted artifacts and emits a working cleanup receipt. |
| `HAR-003` | Process restart | Kill/restart each worker independently; cursor resumes without duplicate calls. |
| `HAR-004` | Network partition primitive | Isolate and rejoin declared peers; observed tips prove the partition rather than assuming it. |
| `HAR-005` | Seed replay and normalized receipts | Same seed reproduces fixture/fault/order choices; semantic receipt matches despite Zakura’s intentionally random coinbase identities. |
| `HAR-006` | Deadline/retry classifier | Typed transient state retries within budget; permanent error fails immediately; forced hang times out and teardown leaves no child process. |
| `HAR-007` | Parallel isolation and scoped cleanup | Concurrent runs have disjoint endpoints/networks/volumes/artifacts; one cleanup cannot remove another run’s resources. |
| `HAR-008` | Independent-oracle boundary | Dependency deny-list rejects production bridge imports; mutation corpus proves accounting/protocol oracles catch each corrupted field. |
| `HAR-009` | Backend conformance | The same probe/fixture contract passes on local-process and container backends; capability and error classes are identical after endpoint normalization. |
| `HAR-010` | Artifact redaction and collector survival | Secret-shaped canaries from argv/env/config/RPC/log paths are absent from the uploadable bundle; external collection still completes after child panic/kill and refuses upload on scan failure. |
| `BTC-001` | Existing direct BTC deposit | Confirmed registered output becomes pending then spendable nBTC only after checkpoint signing; supply oracle balances. |
| `BTC-002` | Existing BTC withdrawal | nBTC liability is reduced exactly once; checkpoint pays requested Bitcoin output and preserves reserve change. |
| `BTC-003` | Existing BTC interchain deposit | BTC deposit destination produces the expected ICS-20 balance after Hermes relay; no local double credit. |
| `BTC-004` | Existing IBC-origin withdrawal | Incoming nBTC voucher/memo results in one validated Bitcoin withdrawal; timeout/replay cannot duplicate it. |
| `BTC-005` | Existing signer threshold | Below-threshold signatures cannot complete; threshold completion produces the expected checkpoint transaction. |
| `BTC-006` | Existing relayer restart/rescan | Restart across a recognized deposit/checkpoint; outpoint and confirmation relay remain idempotent. |
| `BTC-007` | BTC behavior after each extraction | Receipt schema and economic state transitions remain equivalent to the pinned baseline. |
| `BTC-008` | Insufficient-confirmation BTC deposit | Current rejection/pending behavior and exact confirmation boundary are recorded without adding nBTC liability early. |
| `BTC-009` | Pending-transfer release boundary | Credit remains pending until the current code’s fully signed checkpoint boundary, then releases exactly once to local or IBC destination. |
| `BTC-010` | Signatory-set rotation and validator-power churn | Current xpub selection, voting-power threshold, old/new scripts, signer availability, and rate-limit behavior are characterized across a set change. |
| `BTC-011` | Old/expired/disabled sigset deposit and recovery | Current acceptance-age, disabled-deposit, sweep/recovery, and user-visible failure behavior are captured before any shared abstraction is extracted. |
| `BTC-012` | Emergency disbursal | Existing timelock, intermediate/final transactions, recovery scripts, fee bounds, and liability effects are recorded with no semantics silently dropped. |
| `BTC-013` | Shallow and deep Bitcoin reorg | Current bounded header-queue reorg and beyond-window rejection behavior are exercised and normalized into semantic receipts. |
| `BTC-014` | Existing IBC monolith decomposition | Direct deposit, interchain deposit, acknowledgement, timeout/refund, replay, and IBC-origin withdrawal are captured as separately attributable assertions even if the base test runs them together. |

### 7.2 Zcash protocol and node oracles

| ID | Scenario | Required assertion/receipt |
|---|---|---|
| `ZPV-001` | Parse/serialize source-locked transaction versions | V4, v5, and v6 byte-for-byte round trips and mined txids match pinned independent vectors; v5/v6 auth commitments also match; v1-v3 and unknown versions fail closed. |
| `ZPV-002` | V5 ZIP-244 transparent sighash vectors | Every v5 `SIGHASH_ALL` input digest matches pinned Zcash vectors, including all prevout amounts/P2SH scriptPubKeys, sequences, current input, outputs, branch ID, and negative alternate-flag cases. |
| `ZPV-003` | P2PKH/P2SH script vectors | Address, scriptPubKey, redeem script, scriptSig, and signature verification match Zakura plus independent Zcash vectors. |
| `ZPV-004` | Merkle inclusion and mutation vectors | Locally built path reconstructs a Zakura raw block’s txid root for odd/even counts, rejects malformed/detectable duplicate paths, and demonstrates the Bitcoin-inherited duplicate-last-leaf ambiguity that a header root plus compact proof cannot eliminate alone. |
| `ZPV-005` | Header serialization/hash vectors | Mainnet, testnet, and regtest sizes/hash display order match Zakura and official vectors. |
| `ZPV-006` | Equihash/difficulty vectors | Valid production headers pass; bad solution, target, linkage, and work values fail deterministically. Regtest’s disabled PoW is not accepted as this test. |
| `ZPV-007` | Network-upgrade boundary vectors | Transaction parser, branch ID, expiry, header-commitment field interpretation, and verified-versus-assumed rule matrix switch at exact activation heights; semantic block-body commitments are not mislabeled as header-only verified. |
| `ZPV-008` | ZIP-317 fee vectors | Logical-action counts and conventional fee match pinned policy vectors across input/output shapes. |
| `ZPV-009` | Difficulty/MTP context-retention vectors | Candidate validation matches independent implementations at normal, minimum-difficulty, activation, and reorg boundaries; pruning never removes the roughly 28-header predecessor context required by inspected current rules. |
| `ZPV-010` | Mixed-component mined txids | V4 transparent/Sprout/Sapling, v5 transparent/Sapling/Orchard, and v6 transparent/Sapling/Orchard/Ironwood combinations match official and independent mined-txid/auth vectors; malformed shielded encodings fail bounds even though SPV does not validate their proofs. |
| `ZPV-011` | Version/pool support matrix | Every v4/v5/v6 pool/activation combination is accepted or rejected exactly by the active source-locked matrix; v1-v3, impossible pool/version combinations, unknown versions, and unknown branch IDs fail closed. |
| `ZPV-012` | V4 ZIP-243 transparent sighash vectors | Legacy v4 `SIGHASH_ALL` preimages/digests match pinned independent vectors and cannot be routed through ZIP-244 by branch ID alone. |
| `ZPV-013` | V6 transaction and sighash vectors | Launch-pinned activation-applicable v6 mined txid/auth/sighash vectors include Ironwood and moved shielded-anchor placement; v5/v6 cross-routing fails; a Draft ZIP snapshot cannot satisfy this gate alone. |
| `ZPV-014` | Header-version consensus vectors | Canonical decode is followed by the explicit Zcash rule `nVersion >= 4` with signed interpretation/high bit clear; version 3, version 4, maximum positive, and high-bit-set values match independent consensus behavior. |
| `ZND-001` | Zakura regtest mining and propagation | Mine exact block counts with Zakura, mature funds, submit a transaction, and observe the same canonical tip through dual-stack, Zakura-only, and legacy-only peers. Hashes may vary because `generate` randomizes coinbase data; branch/effect invariants do not. |
| `ZND-002` | Zakura compatibility-sidecar transparent payment | Create/fund/spend a transparent output through the pinned sidecar and cross-check raw transaction effects directly in Zakura. Sidecar wallet state is never used as bridge proof. |
| `ZND-003` | Shielded-origin transparent output by named pool/version | Produce v4 Sprout/Sapling, v5 Sapling/Orchard, and v6 Sapling/Orchard/Ironwood transactions with the sidecar only where its capability snapshot allows and other pinned maintained factories/vectors elsewhere; Zakura detects the same transparent bridge output without exposing shielded keys. |
| `ZND-004` | Zakura fork/reorg controls | Build competing branches with controlled Zakura views and `submitblock`; test non-finalized invalidate/reconsider within documented limits; select greater work and retain fork receipts. |
| `ZND-005` | Zakura dependency differential | Bridge parser/hash/sighash/proof results agree with official vectors and at least one maintained implementation independent of the Zakura code path. |
| `ZND-006` | Zakura RPC capability contract | `rpc.discover`/method probes match the source lock; missing `gettxoutproof`, network identity, and error semantics are recorded explicitly. Unexpected capability drift fails the lane. |
| `ZND-007` | Archive/pruned proof reconstruction | Archive source serves the proof window; pruned source succeeds inside retention and fails over or halts clearly outside it without minting. |
| `ZND-008` | Zakura restart/catch-up/trace oracle | Empty-state and preserved-state restarts catch up, body/header frontiers converge, reorg state drains, and Zakura’s trace invariants pass with no leaked in-flight budget. |

### 7.3 ZEC deposits

| ID | Scenario | Required assertion/receipt |
|---|---|---|
| `ZDP-001` | Direct transparent ZEC deposit to Nomic account | Registered unique P2SH output, sufficient confirmations, valid proof, pending credit, checkpoint signing, and final nZEC balance all occur once; fees and reserve reconcile. |
| `ZDP-002` | Shielded-origin deposit to Nomic account | A Sapling/Orchard-funded transparent bridge output follows the same custody path; receipt explicitly marks transparent disclosure boundary. |
| `ZDP-003` | Deposit to IBC destination | Registered destination commitment leads to one ICS-20 nZEC credit on the remote chain after checkpoint completion and Hermes relay. |
| `ZDP-004` | Two deposits in one Zcash transaction | Distinct registered outputs/destinations are independently recognized and cannot alias. |
| `ZDP-005` | Same txid, different vout | Both valid outputs can be processed once; duplicate relay of either is rejected idempotently. |
| `ZDP-006` | Below minimum/dust deposit | Rejected before liability creation; reserve/accounting state is unchanged. |
| `ZDP-007` | Insufficient confirmation/work/challenge maturity | A direct app call returns a typed temporary `InsufficientMaturity` at every pre-boundary state and leaves the declared bridge projection unchanged; the relayer may retain local work, but Nomic creates no pending/spendable liability until the exact boundary. |
| `ZDP-008` | Wrong network | Testnet/regtest/mainnet address, header, and transaction mixing fails closed before state mutation. |
| `ZDP-009` | Wrong script or unregistered deposit index | Relayer-supplied destination cannot redirect credit; state remains unchanged. |
| `ZDP-010` | Wrong txid/vout/Merkle sibling/index/header root | Each independent corruption is rejected with stable error class and zero state delta. |
| `ZDP-011` | Duplicate/replayed proof | Processed outpoint is namespaced and idempotent across restarts and multiple relayers. |
| `ZDP-012` | Deposit to old but valid custody epoch | Accepted within configured age, swept by the documented old-set path, and credited once. |
| `ZDP-013` | Deposit to expired/disabled custody epoch | No normal mint. A valid late payment becomes a quarantined claimant obligation bound only to immutable `DepositTerms`; the single H3-selected recovery/refund path is exercised, and no sender address is inferred. |
| `ZDP-014` | Relayer crash between detection and call | Restart/rescan reaches one final outcome without lost or duplicate credit. |
| `ZDP-015` | Concurrent BTC and ZEC deposits | Outpoint maps, pending queues, fees, checkpoints, and assets do not cross-contaminate. |
| `ZDP-016` | Shallow pre-credit reorg | Orphaned deposit never becomes liability; replacement-chain deposit can later succeed. |
| `ZDP-017` | Reorg at confirmation boundary | Exactly defined canonical-height snapshot decides acceptance; no race-dependent mint. |
| `ZDP-018` | Deep post-credit reorg | ZEC bridge enters safety halt, emits insolvency exposure receipt, and does not silently claw back or fabricate reserves. |
| `ZDP-019` | Destination-commitment reveal/front-run | Wrong nonce/destination and copied reveal cannot redirect credit; the immutable original commitment decides the destination and only one claim completes. |
| `ZDP-020` | Child-index invalidity/exhaustion | Invalid BIP32 child results, counter bounds, and exhaustion follow one deterministic rule across every signer and never reuse an address. |
| `ZDP-021` | Late/unsolicited deposit with no canonical sender | Bridge never infers a refund target; only the committed destination or approved recovery state can receive value, and unsolicited ZEC cannot mask a deficit. |
| `ZDP-022` | Privacy-disclosure contract | Receipt proves exactly when registration, address, txid, amount, destination link, and IBC routing become public; prohibited privacy claims are absent from CLI/help/docs fixtures. |
| `ZDP-023` | Full maturity boundary table | Parameterized cases cover configurations `D=0`, `D=1`, and selected production `D`; tip depth before/at/after maturity, work just below/at/above threshold, challenge just before/at/after expiry, tip-age boundary, and height overflow from the same captured canonical snapshot. No early liability; `Active` mainnet rejects `D=0`. |
| `ZDP-024` | Mempool conflict and mined double spend | Mempool observation never creates liability; only the canonical mined outpoint may progress, and a conflicting spend/orphan cannot leave stale pending credit. |
| `ZDP-025` | Mixed relevant/irrelevant outputs and amount terms | Unregistered outputs are ignored; exact/min/max amount terms and multiple relevant outputs at each ±1 boundary produce the single H3-specified result without partial liability. |
| `ZDP-026` | Proof/transaction resource bounds ±1 | Transaction bytes, input/output count, script bytes, Merkle depth/count, call bytes, gas, and per-block budget pass at the bound and reject at bound+1 with no bridge-state delta. |
| `ZDP-027` | Two-relayer race and crash points | Competing relayers with stale cursors, crash-before-submit, crash-after-submit-before-record, and replay converge to one outpoint result and one local cursor per accepted app height. |
| `ZDP-028` | Zakura RPC outage/truncation/failover | Timeout, malformed/truncated raw block, wrong identity, pruned body, and source disagreement fail closed; only an identity/ancestry-matched source can resume. |
| `ZDP-029` | Registration expiry/cap reservation races | Same-block registrations and deposits at exact amount/count/value/rate/expiry boundaries cannot oversubscribe cap, revive terms, or reuse a derivation path. |

### 7.4 Checkpoints and custody

| ID | Scenario | Required assertion/receipt |
|---|---|---|
| `ZCP-001` | Build one-input checkpoint | Effects txid, expiry height, fee, output order, reserve change, and input digests match independent construction. |
| `ZCP-002` | Batch multiple deposits/withdrawals | Deterministic ordering, limits, logical-action fee, and conservation hold. |
| `ZCP-003` | Valid signer threshold | Below threshold cannot assemble; once at least 11 valid candidates exist, the builder deterministically selects exactly 11 in pubkey order and serializes only `OP_0`, those 11 signatures, and the redeem script. Later candidates remain signer-state evidence and do not overfill scriptSig. |
| `ZCP-004` | Invalid signature/share | Rejected without poisoning valid progress; signer identity and error are observable without exposing key data. |
| `ZCP-005` | Signer outage/restart | Nonce/signature state cannot be reused incorrectly; liveness resumes only through documented recovery. |
| `ZCP-006` | Custody epoch rotation | New deposits use new derived scripts; old reserve is swept; old/new key and liability buckets reconcile. |
| `ZCP-007` | Committee membership boundary | Script size, sigops, standardness, and scriptSig size are checked at configured maximum and fail closed above it. |
| `ZCP-008` | Expiry while building | Transaction is rebuilt before signing; stale version cannot collect signatures. |
| `ZCP-009` | Expiry during partial signing | Old signatures are invalidated; rebuilt transaction gets a new signing session; no nonce/signature reuse. |
| `ZCP-010` | Mempool fee/policy rejection | Worker classifies policy failure, rebuilds only under bounded fee policy, and never changes user amount invisibly. |
| `ZCP-011` | Broadcast/rebroadcast idempotence | wtxid/txid semantics are tracked correctly; rebroadcast cannot duplicate withdrawal or checkpoint confirmation. |
| `ZCP-012` | Checkpoint inclusion proof | Confirmed checkpoint is proven against accepted header and closes exactly the intended checkpoint. |
| `ZCP-013` | P2SH committee fee/policy geometry | Exact production 11-signature input (1,374 bytes/10 Revision-0 actions/50,000 zatoshis) and maximal 15-signature two-input probe (3,340 bytes/23 actions/115,000 zatoshis) match independent calculations and Zakura policy; every active fee revision recomputes rather than snapshots these constants. |
| `ZCP-014` | Signer anti-equivocation and fee-bump replacement | Crash/restart cannot sign conflicting spends; an authorized replacement preserves user outputs, links to the prior session, changes only permitted fee/change fields, and invalidates stale signatures. |
| `ZCP-015` | Permanent threshold loss | Loss of enough keys pauses deposits before exposure grows; unswept outputs and claims enter the approved recovery/deficit path. A test cannot pretend mandatory sweep works after threshold is gone. |
| `ZCP-016` | Validator exit with old-epoch exposure | Unbonding/removal does not end signing/evidence obligations or release enforceable bond before the epoch reserve reaches zero; old keys cannot be silently forgotten. |
| `ZCP-017` | Batch/action/fee arithmetic ±1 | Inputs, outputs, serialized bytes, logical actions, fee allocation, reserve change, and checked-value arithmetic pass at every limit and reject at ±1/overflow without partial journal entries. |
| `ZCP-018` | Reserve-input conflict and checkpoint reorg | A competing Zcash spend, mempool conflict, orphaned checkpoint, or replaced checkpoint cannot leave two canonical claims, release pending credit early, or close the wrong session. |

### 7.5 Signer-policy authorization

| ID | Scenario | Required assertion/receipt |
|---|---|---|
| `ZSG-001` | Authorized package reconstruction | Signer reconstructs the full H3-selected checkpoint version (v5 or launch-enabled v6) and its corresponding ZIP-244/activation-applicable v6 digests from local Nomic consensus state plus Zakura prevouts; the signed package equals the on-chain checkpoint/session commitment. V4 parsing for deposits never causes a signer to apply ZIP-244. |
| `ZSG-002` | Output/change/input/fee tampering | Attacker-controlled worker mutations to destination, amount, reserve change epoch/script, input set/order, fee, expiry, branch ID, or hash type are rejected before key use. |
| `ZSG-003` | Stale/unknown context | Wrong chain/network, unsupported height/version/branch, expired transaction, stale checkpoint/session, and unsupported fee revision fail closed. |
| `ZSG-004` | Membership/share abuse | Nonmember, wrong-epoch key, duplicate key/share, malformed/high-S signature, wrong input, and more-than-one share per signer/session are rejected and evidenced. |
| `ZSG-005` | Crash-safe anti-equivocation | Crash at every durable-write/sign/submit boundary cannot produce signatures for two conflicting effects under one input/session; restart resumes or halts deterministically. |
| `ZSG-006` | Authorized fee-bump replacement | Replacement links to prior session, preserves claimant outputs, changes only approved fee/change/expiry fields, invalidates stale signatures, and cannot become an arbitrary second spend. |
| `ZSG-007` | Nomic view disagreement | Signer trusts its local consensus-committed state; disagreement with a secondary Nomic endpoint or committed app root halts signing rather than selecting the convenient view. |
| `ZSG-008` | Key backup/restore/zeroization | Restored signer derives the same epoch/terms key, passes PoP, preserves anti-equivocation journal, never logs key material, and zeroizes retired material under the ceremony policy. |
| `ZSG-009` | Key registration/derivation abuse | Duplicate/shared xpub, invalid PoP, wrong domain/network, invalid child, derivation collision, index exhaustion, and attempted BTC/ZEC key reuse fail before epoch activation. |
| `ZSG-010` | Persistent policy failure | Repeated signer rejection pauses checkpoint intake and surfaces actionable evidence; it never triggers threshold weakening, guardian override, or unsigned accounting progress. |

### 7.6 Withdrawals

| ID | Scenario | Required assertion/receipt |
|---|---|---|
| `ZWD-001` | Direct transparent withdrawal | nZEC is consumed once; exact transparent output appears; reserve change and fees reconcile. |
| `ZWD-002` | P2PKH and P2SH withdrawal receivers | Supported receiver types serialize and validate correctly; wrong-network and shielded-only addresses fail. |
| `ZWD-003` | Unified-address input | CLI requires an explicit supported transparent receiver; it never silently picks a shielded or unintended receiver. |
| `ZWD-004` | Small withdrawal/fee boundary | Amount below safe output+fee policy is rejected before liability mutation. |
| `ZWD-005` | Withdrawal cancellation boundary | Cancellation succeeds only in `Queued` before assignment to a frozen checkpoint/signing session. After assignment it is irreversible except through an H3-authorized replacement preserving claimant/economic outputs or recovery transition. |
| `ZWD-006` | Emergency disbursal | Only the one H3-selected recovery construction and authority can trigger; timelock, claimant set, output set, fee, expiry/upgrade behavior, replacement relation, and final reserve/liability state match its normative transition table. |
| `ZWD-007` | Pause/halt/deficit action matrix | Deposit, withdrawal, signing, broadcast, sweep, and recovery actions are accepted or rejected exactly by safety state; guardian cannot unpause, raise caps, change anchor, or redirect funds. |

### 7.7 Headers, relayers, upgrades, and faults

| ID | Scenario | Required assertion/receipt |
|---|---|---|
| `ZHD-001` | Contiguous valid header batch | Canonical tip, cumulative work, local finality-risk boundary, and compact retained state advance correctly. |
| `ZHD-002` | Broken linkage | Batch rejected atomically; no partial canonical advancement. |
| `ZHD-003` | Invalid Equihash | Rejected even when hash/target/link fields are otherwise plausible. |
| `ZHD-004` | Invalid target/difficulty transition | Rejected at exact offending header. |
| `ZHD-005` | Lower-work fork | A valid lower-work branch inside the configured fork window and per-peer/branch quota is retained as noncanonical; one outside either bound is rejected without canonical-state mutation. |
| `ZHD-006` | Greater-work fork inside window | Canonical chain changes deterministically; affected unfinalized deposits/checkpoints are removed/re-evaluated. |
| `ZHD-007` | Fork beyond local finality-risk boundary | Authenticated greater work enters the H3-defined safety/deficit transition with explicit event; no automatic claim erasure or unauthenticated halt. |
| `ZHD-008` | Header flood/oversized batch | Bounded by count, encoded bytes, gas, and retained-window rules. |
| `ZHD-009` | Stale/eclipsed relayer | A second relayer supplies greater-work chain; no relayer identity has authority over fork choice. |
| `ZHD-010` | Header worker restart | Cursor derives from on-chain tip and resumes idempotently. |
| `ZHD-011` | Trusted-anchor change | Normal advancement proves ancestry/work from current state; emergency replacement runs halted with delayed governance/veto, independent observers, itemized state diff, and zero hidden liability/outpoint edits. |
| `ZHD-012` | Header-only semantic validity boundary | A synthetic PoW/header-valid but full-node-invalid fixture proves the client reports SPV eligibility rather than full validity; independent Zakura rejection raises the configured halt/alert and the risk is named in receipts. |
| `ZHD-013` | Equal-work fork tie | Deterministic source-locked tie behavior prevents relay-order flapping; repeated/duplicate headers are idempotent. |
| `ZHD-014` | Overlap/gap/out-of-order batches | Overlapping accepted prefixes are idempotent; gaps, reversed order, and mixed-network headers reject atomically at the exact element. |
| `ZHD-015` | Timestamp/target/work boundary table | MTP, deterministic future-time admission, compact-target canonicality/overflow, minimum difficulty, and cumulative-work arithmetic pass/reject at every ±1 boundary. |
| `ZHD-016` | Restart/pruning/state bound | Export/import/restart preserves canonical and side-branch work; pruning retains required 28-header context and proof window; state/gas stays under measured bounds. |
| `ZUP-001` | Known network upgrade activation | Header and transaction rules switch exactly at configured height; in-flight stale checkpoint is rebuilt. |
| `ZUP-002` | Unknown upgrade/version | New deposits/checkpoints pause and emit actionable incompatibility event; existing state is not corrupted. |
| `ZUP-003` | Support-lease drain boundary | Terms issuance stops before `max_supported_height` by the worst-case drain margin; existing claims follow declared recovery, and lease extension requires audited/timelocked activation. |
| `ZFT-001` | Nomic validator partition | No custody completion below Nomic consensus/signing threshold; recovery is deterministic after heal. |
| `ZFT-002` | Zakura producer/source loss | Worker fails over only to a network-identity-matched Zakura source; no new proof acceptance without accepted headers; Nomic base chain remains live. |
| `ZFT-003` | Disk-full/read-only worker | Worker fails safely and restart/rescan recovers; consensus state is authoritative. |
| `ZFT-004` | Corrupt local relayer index | Index rebuild from registered scripts and chain history produces no duplicate app effects. |
| `ZFT-005` | Zakura pruned-body gap | A relayer needing a deleted block body fails over to archive or halts with a typed retention error; it cannot substitute an address index or wallet assertion for an inclusion proof. |
| `ZFT-006` | Mixed invalid-call flood | Invalid headers/proofs/signatures across BTC and ZEC share a measured aggregate per-block admission budget; base-chain progress continues and no single bridge starves the other. |
| `ZFT-007` | Worker split-view/restart matrix | Each worker crashes at fetch/build/persist/submit/ack boundaries under stale and disagreeing Zakura/Nomic views; consensus state remains authoritative and the terminal result is unique. |

### 7.8 IBC, accounting, migration, and multi-asset isolation

| ID | Scenario | Required assertion/receipt |
|---|---|---|
| `IBC-Z-001` | Send nZEC over ICS-20 | Source escrow/burn and destination voucher balances reconcile for the selected path. |
| `IBC-Z-002` | Return nZEC voucher | Trace resolves to native nZEC exactly; no nBTC path accepts it. |
| `IBC-Z-003` | Timeout | Refund restores the correct nZEC bucket once; replay cannot duplicate. |
| `IBC-Z-004` | Malformed typed nZEC destination/memo | Receive returns an ICS-20 error acknowledgement atomically: no native nZEC burn/mint, no BTC/ZEC withdrawal, no queue entry; relaying the error acknowledgement restores the source-side sender balance exactly once. |
| `IBC-Z-005` | Cross-asset memo confusion | nBTC cannot invoke ZEC withdrawal and nZEC cannot invoke BTC withdrawal, even with adversarial denom text. |
| `IBC-Z-006` | Acknowledgement/timeout race and channel closure | Exactly one terminal packet outcome changes liability; escrow, remote voucher, refund, and any withdrawal claim cannot be double-counted across relayer crash/replay. |
| `ACC-001` | Every successful transition | Enumerated nZEC liabilities, reserves, queued obligations, and fee buckets conserve value. |
| `ACC-002` | Every rejected transition | The manifest’s bridge-state projection and all reserve/liability classes remain unchanged. Only explicitly named generic chain effects (for example caller sequence, gas/anti-spam fee, rejection event) may differ, and the oracle verifies that exact allowed envelope at matched app heights. |
| `ACC-003` | Genesis/migration | Existing BTC/nBTC/NOM state is byte/meaning preserving; nZEC starts at zero unless an explicit audited import says otherwise. |
| `ACC-004` | Pause/cap | Deposit, withdrawal, aggregate liability, rate, and per-tx controls act independently and cannot strand recovery actions. |
| `ACC-005` | Query/indexer consistency | CLI, app query, events, and accounting oracle agree on public state and units. |
| `ACC-006` | Disjoint economic partition | Every zatoshi and nZEC claim occupies one reserve/liability/equity/deficit class; old inputs/change, provisional outputs, IBC escrow/vouchers, burned withdrawals, and unsolicited deposits are not doubled or omitted. |
| `ACC-007` | Deficit transition and loss waterfall | Deep reorg, theft/key loss, and invalid-state fixtures atomically enter `Deficit`, freeze discretionary/preferential outflow, and execute only the approved claim-priority/pro-rata policy. |
| `ACC-008` | Atomic caps and authority | Same-block concurrent deposits cannot exceed caps; cap decrease below exposure preserves claims; compromised guardian can reduce risk only and cannot increase authority/value-at-risk. |
| `ACC-009` | Active-state migration/anchor diff | Halted migration itemizes and preserves anchors, outpoints, UTXOs, signing sessions, liabilities, IBC obligations, equity, and deficit; reset/reinitialization attempts fail. |
| `ACC-010` | Generated transition journal | Property-generated sequences covering every deposit/checkpoint/withdrawal/IBC/reorg/recovery state maintain balanced debit/credit entries and unique bucket membership at each app height. |
| `MIG-Z-001` | Existing migration closure | The base `InnerAppV6 → InnerAppV7` path is implemented and fixture-tested before any Zcash version is appended; existing `unreachable!()` assumptions are either proven or replaced with typed failure. |
| `MIG-Z-002` | Every durable ZEC state | Snapshot fixtures cover empty, registered, observed, pending, signing, complete, IBC in-flight, withdrawal queued, paused, deficit, recovery, old-epoch, and pruned-header states with itemized pre/post diffs. |
| `MIG-Z-003` | Two-binary upgrade restart | Old binary writes the fixture, new binary migrates/restarts all workers, continues signing/relaying exactly once, and export/import replay yields the same app root. |
| `MIG-Z-004` | Downgrade/rollback rejection | Once liability or new encoding exists, an old binary cannot silently open or mutate state; the operator gets a deterministic incompatible-version halt. |
| `MIG-Z-005` | Anchor/lease/authority continuity | Migration cannot reset trusted context, support lease, processed outpoints, safety state, authority delays, cap reservations, signer anti-equivocation, or deficit/loss claims. |

---

## 8. Proposed implementation architecture

### 8.1 Keep Bitcoin stable; add an explicit Zcash domain

Start with a separate module:

```text
src/zcash/
├── mod.rs
├── adapter.rs
├── types.rs
├── network.rs
├── header_queue.rs
├── merkle.rs
├── deposit_registry.rs
├── checkpoint.rs
├── custody.rs
├── multisig.rs
├── recovery.rs
├── relayer.rs          # full-node feature only
├── signer.rs           # full-node feature only
└── metrics.rs
```

Only after BTC and ZEC are both green should common code be extracted into narrowly named modules such as lifecycle retry helpers, bounded collections, accounting test oracles, or generic state-machine combinators.

Do **not** begin with `Bridge<C: UtxoChain>`. Bitcoin and Zcash disagree exactly where the bridge is security-critical.

### 8.2 Domain types

Introduce distinct, non-interchangeable types:

- `ZecTx`, `ZecMinedTxId`, `ZecUnminedTxId::{Legacy(ZecMinedTxId), V5Plus(ZecWtxId)}`, `ZecAuthDigest`, version-gated `ZecWtxId { txid, auth_digest }`, `ZecOutPoint`, `ZecVout`;
- `Zatoshis` with checked arithmetic;
- `Nzec` as a unique `Symbol` and `Coin<Nzec>`;
- `ZcashNetwork` and a genesis/network fingerprint;
- `ConsensusBranchId`/transaction-version representation;
- `ZcashHeight`, `ZcashBlockHash`, `ZcashMerkleRoot`, `ZcashWork`;
- `ZcashCustodyEpoch`, `DepositIndex`, `SigningSessionId`;
- `ZcashAddress` parsed to an explicit receiver enum.

Serialization must include stable version tags. No `Adapter<bitcoin::Transaction>` should appear in Zcash state.

### 8.3 Zcash header light client

The preferred light client should:

1. Bootstrap from an explicit trusted Zcash context containing network/genesis fingerprint, source provenance, and governance artifact: at least 28 contiguous linked headers ending at the anchor under inspected current rules, or a rigorously equivalent authenticated snapshot carrying every target, median-time-past, cumulative-work, activation, and commitment-interpretation input needed to validate the next header. A lone trusted header is insufficient. Post-launch ancestry and emergency replacement follow the gate below.
2. Parse network-specific headers with strict size bounds.
3. Enforce the normative signed header-version rule after decoding: `nVersion >= 4`, which also requires the high bit to be clear. Canonical byte decoding alone is insufficient.
4. Verify previous-hash linkage.
5. Verify Equihash solution for production-network headers.
6. Verify compact target, header hash threshold, expected difficulty transition, and cumulative work. Median-time-past uses retained chain context. Any local “too far in the future” admission check uses deterministic Nomic consensus block time—not validator wall clocks—and returns a temporary/resubmittable classification rather than permanently poisoning the header.
7. Select the valid chain with greatest cumulative work.
8. Track a bounded fork window and a local finality-risk boundary; never describe that frontier as Zcash finality.
9. Expose the transaction Merkle root and height for deposit/checkpoint inclusion verification.
10. Retain all predecessor context needed by difficulty and median-time rules before pruning. Under the inspected current rules, validating a candidate can require context reaching roughly 28 predecessor headers because the difficulty window itself depends on median-time-past; exact retention is derived and tested per network/upgrade rather than guessed from the reorg window.
11. Prune full Equihash solutions and noncanonical branches only after the safety window while retaining the compact summaries and contextual history needed for proofs/audit.
12. Maintain a per-upgrade verification matrix naming which header/body rules are fully verified, syntactically checked, or assumed under SPV.
13. Store a governance-renewable `max_supported_height` support lease and separate `FeePolicyRevision`. Unknown upgrades do not reliably announce themselves in headers, so the bridge stops issuing deposit terms far enough before the lease boundary to drain registrations/checkpoints and rejects new liability at or beyond it unless an audited, timelocked upgrade extends the horizon.

This is an SPV-style client, not a full Zakura implementation inside Nomic. Header PoW authenticates work over serialized fields, but a header-only verifier cannot semantically validate all block-body-derived commitment inputs or all transactions. The security ADR must enumerate exactly which Zcash consensus rules Nomic verifies, which are transitively assumed from the greatest-work chain, what an adversary with invalid-block hashpower can do, and whether a succinct full-validation proof is required at the intended cap.

Deposit maturity is not a bare block count. The H3 finality ADR defines:

`Observed → WorkMature → ChallengeMature → Credited`

- `WorkMature` requires both the unambiguous additional-block boundary and a minimum cumulative-work delta from the inclusion block under a canonical tip snapshot.
- `ChallengeMature` requires a Nomic-consensus-measured observation interval after that branch first becomes canonical, source-tip age within policy, and no higher-work conflicting branch. This reduces but does not remove eclipse/private-chain risk.
- Deep conflicting work is accepted only with enough retained context or authenticated history commitments and bounded chunked submission to verify it. If that is infeasible, the emergency observer/guardian trust assumption is named; an unauthenticated assertion cannot halt consensus.

#### Required feasibility spike

Before integrating Zakura-derived crates or Zcash cryptographic dependencies into consensus:

- implement the smallest pure verifier against official, Zakura, and independent vectors;
- benchmark parse, Equihash, difficulty, work, batch verification, state growth, and app-call gas under the actual pinned Nomic toolchain;
- verify deterministic results across supported architectures/builds;
- inspect dependency graph, unsafe code, `std` assumptions, allocation bounds, and license;
- test worst-case invalid input, not only valid headers;
- compare a minimal extracted implementation with the exact pinned `zakura-chain`/`equihash` behavior, official vectors, and an implementation without Zakura’s Zebra lineage where available (for example zcashd’s C++ consensus path or `librustzcash` transaction digests). Zebra is useful lineage-differential evidence, not sufficient independence by itself;
- set hard per-call count/byte/gas limits from measurements.

If this gate fails, stop. Evaluate a succinct proof design or explicitly weaker attestation mode under a hard cap. Do not silently remove Equihash checks to make benchmarks pass.

Finality is an economic policy layered above fork choice, not a consensus property of Zcash. A deposit progresses `Observed → WorkMature → ChallengeMature → Credited`; the first transition records inclusion header/hash/work, the second requires both a configured additional-block depth and cumulative-work delta, and the third requires a deterministic Nomic-block challenge/observation window plus a maximum tip-age bound. Relayers/observers publish Zakura and independent-node disagreements; authenticated greater work still controls fork choice, while observer disagreement can pause value-bearing actions under the authority matrix. This reduces eclipse/oracle risk but does not erase SPV or deep-reorg risk.

Admission economics are consensus rules, not only relayer settings. H3 benchmarks and fixes per-call and aggregate per-Nomic-block limits for header count/bytes, proof bytes/depth, transaction bytes/inputs/outputs, signature attempts, script work, state reads/writes, and CPU wall-equivalent gas under valid and worst-case-invalid data. It also defines sender fee/bond, refund, accepted-work reward, invalid/duplicate rejection charge, per-source/per-asset quotas, and same-block ordering so an invalid BTC/ZEC mix cannot starve base consensus or earn rewards. Limits pass at the bound and fail atomically at `bound+1`; raising them is delayed governance with a new benchmark receipt.

#### Trusted anchor change gate

- Normal anchor advancement is proven through the currently accepted header chain and cannot skip an unverified work interval.
- An emergency anchor replacement begins from `FullHalt`, disables deposits and normal withdrawals, publishes old/new anchor and cumulative-work evidence, reconciles every reserve/liability/outpoint bucket, and waits through governance delay plus veto.
- The migration exposes a machine-checkable state diff. It cannot delete processed outpoints, erase liabilities, relabel a deficit, or mark an unproven deposit canonical.
- Independent Zakura and non-Zakura observers must agree on network identity, chain ancestry/work, and the proposed anchor before activation.
- If ancestry cannot be proven from the old state, the action is explicitly a social recovery with named trust assumptions—not routine light-client maintenance.

#### Fast and production-valid test lanes

Zakura regtest can disable proof of work and its `generate` RPC requires PoW-disabled network parameters. Therefore:

- functional scenarios use fast regtest;
- protocol-vector scenarios verify real production-size headers and Equihash;
- optional slow lanes replay bounded real header ranges;
- no report may claim that a regtest-only run validated production PoW.

### 8.4 Merkle inclusion

The Zcash transaction tree is Bitcoin-inherited, but transaction identifiers differ by version.

- The relayer fetches the full raw block from Zakura using `getblock` verbosity `0`; verbose transaction IDs may be used only as an additional cross-check.
- It constructs a compact sibling path plus mandatory claimed transaction count and transaction index locally.
- Nomic recomputes the leaf txid under supported Zcash rules and folds the branch to the accepted header root.
- Proof encoding is bounded and rejects `index >= tx_count`, noncanonical path depth for the claimed count, extra/missing siblings, wrong odd-leaf duplication, wrong byte order, and detectable duplicate-branch mutations.
- The Bitcoin-inherited tree has an ambiguity where duplicating the final leaf can preserve the same Merkle root. The claimed transaction count constrains proof shape but is not independently authenticated by the header, so a compact inclusion proof cannot prove away every mutation elsewhere in the block. Zakura full-block acceptance and the SPV greatest-work-valid-chain assumption carry that semantic validity; the plan claims txid membership only, not global non-mutation.
- Official vectors plus a maintained implementation independent of Zakura are the cryptographic oracle; Zakura is the primary integration source.

Do not reuse Bitcoin’s `PartialMerkleTree` type merely because the internal parent hash is SHA-256d.

### 8.5 Deposit registration and destination binding

P2SH size constraints mean the first design should not depend on stuffing arbitrary destination bytes into the redeem script.

`DepositTermsV1` is the authorization object, not a mutable `(destination, index)` row. Its canonical encoding commits at least:

- Nomic chain ID, Zcash network/genesis fingerprint, custody epoch, and terms version;
- destination type/value and a local random nonce;
- exact accepted zatoshi amount for v1 (a later range form must reserve its maximum);
- registration/claim expiry, support-lease bound, fee-policy revision, and maximum user fee exposure;
- an explicit transparent refund/recovery receiver or an explicit `NoRefundReceiver` choice;
- derivation-spec version and any approved recovery-policy identifier.

Recommended flow:

1. The CLI builds the canonical terms locally, hashes them with a domain-separated commitment, and submits a public envelope containing network/epoch, exact or maximum amount, expiry/support bounds, fee revision, and `terms_commitment`. Destination, nonce, and refund receiver remain hidden until reveal. The CLI separately preserves the privacy-sensitive preimage/recovery receipt.
2. Consensus atomically reserves the advertised maximum against per-deposit, aggregate, outstanding-registration, and rate caps and allocates an immutable registration ID. Reservation expiry releases unused capacity but never makes the resulting address safe to reuse.
3. Each signer publishes only a dedicated epoch xpub. The P2SH child path is derived from a domain-separated hash of the full terms commitment using a fixed sequence of non-hardened 31-bit limbs, canonical retry counters for invalid BIP32 children, and an on-chain collision check. It is not derived only from a rollback-prone monotonic index.
4. Consensus constructs the exact P2SH redeem script/address and stores `script_hash → {registration_id, network, epoch, terms_commitment, reserved_amount, fee_revision, expiry, support_lease, derivation_path, status}` immutably.
5. The user pays that one-time address with the exact v1 amount. Underpayment, overpayment, duplicate relevant outputs, or payment after expiry follows one H3-defined quarantine/claim path; it is never silently rounded or partially credited.
6. Deposit proof reveals the canonical terms preimage. Consensus recomputes commitment/path/script, verifies amount/network/epoch/lease/fee policy and the unprocessed canonical outpoint, then converts the cap reservation into a liability exactly once.
7. A late deposit remains cryptographically bound to the original terms. Recovery may pay only the committed destination or explicit refund receiver under the selected policy. The bridge never infers a sender from transparent inputs, shielded spends, change, or wallet RPC.

Requirements:

- no address reuse across terms, destinations, amounts, fee revisions, epochs, or recovery policies;
- chain/network/epoch/full-terms domain separation in derivation and explicit collision/invalid-child handling;
- bounded outstanding registrations and anti-spam fee/deposit;
- atomic reservation of exposure at registration, including pending claims, queued withdrawals, IBC obligations, old-epoch reserve, and recovery obligations—not merely already minted spendable nZEC;
- immutable commitment mapping after allocation; deterministic handling of invalid BIP32 child indexes, retry counters, collisions, and exhaustion;
- explicit disclosure documentation: registration time/commitment/index and derived transparent address are public; deposit txid/output/amount/time are public; destination becomes publicly linked when revealed/credited; committee and reserve spends are public; IBC routing data is public on the involved chains;
- no use of “private,” “anonymous,” or “shielded bridge” for the v1 product. A shielded-origin transaction hides upstream history only to the extent Zcash itself provides; the bridge output and eventual destination link are transparent;
- registration transaction and deposit receipt are linked in the golden artifact.

### 8.6 Custody decision

#### Why the Bitcoin design cannot be copied

- Zcash transparent Script has no P2WSH/Taproot.
- The P2SH redeem script is a pushed element capped at 520 bytes.
- Nomic’s weighted script grows too quickly for a normal validator set.
- Existing FROST is Schnorr/Taproot-oriented; Zcash transparent `CHECKSIG` requires ECDSA.

#### Candidate A — rotating standard P2SH multisig committee

A standard compressed-key `m-of-n CHECKMULTISIG` with `n ≤ 15` fits established Zcash relay policy. The executable starting hypothesis is exactly `11-of-15`: 15 eligible bonded validators selected by a deterministic on-chain rule and 11 equal signature slots (`floor(2*15/3)+1`). Fewer than 15 eligible, proved-live Zcash keys means no new active epoch under this hypothesis—not an automatic weaker threshold. Member ordering and tie-breaking are canonical. Voting power may govern eligibility/selection and economic accountability, but it is not encoded as weighted Script authorization. H3 may replace this rule only with an ADR and new vectors.

The exact candidate script is canonical `OP_11 <15 sorted compressed pubkeys> OP_15 OP_CHECKMULTISIG`. Spending uses a push-only scriptSig with the CHECKMULTISIG dummy element, strict-DER/low-S ECDSA signatures plus the fixed hash-type byte in pubkey order, and the redeem script. Size alone is insufficient: null-dummy, clean-stack/policy flags, sigops, transaction limits, signature ordering, and ZIP-317 actions are all pinned in vectors against Zakura and an independent policy implementation.

Advantages:

- simple, inspectable, established transparent Script;
- no novel threshold-ECDSA protocol;
- current validator xpub/signature workflow can be adapted with separate Zcash keys;
- harness can prove standardness against Zakura and its pinned compatibility sidecar, then cross-check independent Zcash policy sources.

Costs/risks:

- only a bounded validator committee controls ZEC, not the full weighted set;
- equal signature slots do not encode Nomic voting power;
- committee selection/rotation becomes a critical governance and capture surface;
- liveness fails after enough members are unavailable;
- registration is needed because a full-size script leaves no room for destination data.

Required controls:

- deterministic, auditable selection from bonded validators at an epoch snapshot;
- a governance-approved fixed `m`/`n`, Nomic-committed selection snapshot height, eligibility/tenure rule, registration freeze, tie-break, and selection rule; no automatic threshold reduction or last-minute key replacement;
- exactly 15 distinct consensus-operator identities, parent xpubs, and derived keys; on-chain uniqueness cannot prove distinct beneficial ownership, so hidden common control remains an explicit residual risk;
- a per-epoch machine-verifiable security receipt enumerating every 11-member spending subset and every 5-member blocking subset, publishing at least the cheapest still-slashable 11-key coalition, the cheapest 5-key liveness coalition, stake concentration, unbonding horizons, and whether theft evidence is actually slashable;
- a quantitative economic-security ADR that either (a) adopts the independent assumption “at most four of the fifteen custody keys are Byzantine or unavailable,” (b) proves subset-power admission constraints against a named stake-fault assumption, or (c) rejects Candidate A. Deterministic selection and governance approval alone are not a security invariant;
- an aggregate exposure cap no greater than the more conservative of a fixed acceptable-loss budget and a discounted, realistically recoverable security budget from that receipt. If theft is not provably slashable and collectible, bonded stake is not marketed or counted as ZEC backing;
- custody duty and evidence/slashing horizon extending beyond validator exit until that epoch’s reserve is zero;
- a documented key ceremony generates a dedicated ZEC root in the signer/HSM, derives each custody-epoch parent through a hardened step unavailable to consensus, and publishes only that epoch’s canonical `ZecTransparentXpub`; no Bitcoin root/path/home is reused;
- proof-of-possession binds Nomic chain ID, Zcash network/genesis, custody epoch, validator identity, canonical xpub bytes, selection snapshot, and ceremony version. Duplicate parent/child keys, wrong depth/version/curve, and shared-key registrations fail before activation;
- only non-hardened public derivation below the published epoch xpub, using the `DepositTermsV1` commitment-derived path. Deterministic invalid-child/collision/exhaustion handling is shared by consensus and signers; child private keys are never exported because one leaked non-hardened child private key plus its xpub can compromise that epoch branch;
- short epochs with overlap/recovery and mandatory sweeps;
- automatic deposit pause when live eligible keys fall below threshold plus a reviewed safety margin;
- signer policy verification against local Nomic consensus state, crash-safe anti-equivocation, and explicit fee-bump replacement relations;
- HSM or equivalent isolated signing support, encrypted backups, restore drills, key-compromise rotation, and a lead-time freeze before an xpub becomes active;
- slashing/accountability design where technically and legally valid, without presenting slashing as a substitute for recoverable reserve;
- aggregate and rate caps, pause, and veto/timelock around parameter changes;
- public committee/epoch/reserve dashboards.

An ordinary `11-of-15 CHECKMULTISIG` output has no magic threshold-loss path. If five or more required key holders are permanently unavailable before a sweep, the funds can become permanently unspendable; if eleven collude, the funds can be stolen outside Nomic. Before Candidate A can hold value, H3 must do one of the following:

1. prove a standard, audited, size-bounded timelocked recovery branch that does not create a worse unilateral-control path;
2. establish an operational/economic design whose hard cap, bonded collateral or insurance, signer redundancy, exit delay, and loss waterfall explicitly price permanent threshold loss and theft; or
3. reject Candidate A and select another custody design.

“Mandatory sweep” is a liveness procedure, not recovery after the threshold is already gone.

#### Candidate B — threshold ECDSA to one P2PKH key

Advantages:

- constant-size on-chain custody and signatures;
- can potentially represent a larger signer set;
- unique child keys may be derived for registered deposits if the chosen threshold scheme safely supports public derivation/tweaks.

Costs/risks:

- new multi-round DKG, resharing, nonce management, blame, recovery, and signing protocol;
- far more catastrophic implementation failure modes;
- existing Taproot FROST is not reusable;
- library maturity, audit history, deterministic integration, and validator operations must be proven.

**Recommendation:** Use Candidate A only for a capped, explicitly committee-custodied first release after the threshold-loss/theft gate above, signer-authorization design, economic-security ADR, and loss waterfall are approved. Run Candidate B as a separate research/audit track. Do not block the harness on Candidate B, and do not market Candidate A as full-validator-set custody.

A prototype `3-of-5` P2SH topology may be used to exercise mechanics, but it is disposable test infrastructure—not evidence that mainnet custody is solved.

### 8.7 Transaction and checkpoint model

Implement Zcash checkpoints as a separate state machine sharing only reviewed transition concepts:

- `Building → Signing → Complete → Broadcast → Confirmed`, plus `Expired/Rebuild`, `Paused`, and `Recovery` states;
- transaction effects built before signature collection;
- supported Zcash transaction version and branch ID derived from network/target height;
- explicit nonzero expiry height derived from a canonical tip snapshot, signing/broadcast/confirmation budget, Zakura’s source-locked “expiring soon” policy, and the next network-upgrade boundary; if the safe window does not fit before activation, construction pauses and rebuilds under the next supported branch rather than crossing it;
- ZIP-317 logical-action fee calculation, not Bitcoin vbytes;
- deterministic input/output ordering;
- exact version-dispatched transparent sighash data: v4 follows ZIP-243’s own preimage/`scriptCode` rules, while v5 and launch-enabled v6 commit the required spent-output amounts and P2SH locking `scriptPubKey`s, including the current input’s P2SH `scriptPubKey`—not a blindly substituted redeem script. The redeem script is separately bound by its hash in that locking script. Bridge checkpoint policy fixes `SIGHASH_ALL` and rejects `ANYONECANPAY`, `NONE`, `SINGLE`, and unknown flags unless a later audited ADR changes that policy;
- effects txid and authorizing-data commitment tracked separately and combined only in a typed 64-byte witnessed identity;
- every rebuild creates a new signing-session ID and invalidates stale signatures/nonces;
- bounded inputs, outputs, encoded size, logical actions, and signing work;
- reserve change returns only to the intended current/transition custody epoch;
- confirmed checkpoint inclusion is proven against the Zcash light client before final closure.

Fee policy is an accounting ADR, not whatever one node happens to accept. `FeePolicyRevision` defines the active ZIP-317 revision/constants and computes whole-transaction transparent actions as `max(ceil(total_input_bytes / 150), ceil(total_output_bytes / 34))`, plus any shielded contributions; it is not “one action per input.” The ADR defines deterministic allocation of that whole fee among consolidation inputs, withdrawals, reserve change, and a protocol fee pool (for example source-ordered marginal contribution plus an explicit surplus bucket), along with quote/reservation lifetime, maximum fee and fee-bump delta, dust/minimum amounts, subsidy source, overpayment/refund treatment, and behavior when policy changes mid-checkpoint. It separately journals (1) the user-paid funding transaction fee already mined on Zcash, (2) the reserved future cost to spend a deposit UTXO, (3) the checkpoint/withdrawal miner fee, and (4) any bridge service fee. Zakura mempool acceptance is an integration check; independent fee vectors and double-entry transitions are the specification.

### 8.8 Relayer and signer separation

Run independently controllable workers:

- header relayer;
- deposit registration watcher/scanner;
- deposit proof relayer;
- checkpoint broadcaster/confirmation relayer;
- Zcash signer.

Properties:

- workers derive cursors from on-chain state and can rebuild local indexes;
- any operator can relay proofs;
- workers use bounded RPC responses and verify network identity at startup and periodically;
- no RPC response is trusted until locally parsed/validated and then independently checked by Nomic consensus;
- signers read an authorized package from their local Nomic consensus state at a named committed height/app hash: bridge/epoch/checkpoint/session IDs, complete transaction bytes, every prevout amount/script, allowed claimant and reserve-change outputs/amounts, fee-policy revision and bounds, expiry/upgrade context, hash type, and replacement parent if any. A remote worker-provided digest is never sufficient authority;
- signers independently recompute effects txid, all input sighashes, value conservation, policy limits, and destination/change scripts before touching the key;
- a crash-safe anti-equivocation database durably records/fsyncs each custody outpoint, committed app height/hash, full economic package hash, and authorized signing session **before** signature release. A conflicting spend is refused unless Nomic authorizes an explicit linked replacement preserving user economic outputs under bounded fee rules; backup/restore preserves this journal;
- querying multiple remote Nomic RPCs is only a diagnostic cross-check; it does not replace verification against the signer’s local consensus-committed state;
- worker logs use public IDs and stable error classes, never key material;
- metrics distinguish progress, stale source, rejection, expiry, reorg, and pause;
- Bitcoin and Zcash worker configuration cannot point at the other signer/key directory by accident.

### 8.9 nZEC accounting and IBC

Add a distinct `Nzec` symbol only after an ADR selects:

- base denomination string;
- display denomination `nZEC`;
- exponent of 8 for zatoshis;
- unused symbol index;
- IBC metadata and trace behavior;
- fee eligibility, if any.

Default safety posture: base Nomic fees remain independent of nZEC until economic policy is explicitly reviewed. Do not copy nBTC fee exceptions blindly.

The accounting oracle must enumerate at least:

- user spendable nZEC;
- pending deposit credits;
- withdrawal obligations;
- IBC escrow and in-flight packet obligations;
- bridge fee/reward pools;
- checkpoint inputs/change and recognized reserve UTXOs;
- old-epoch/recovery reserves;
- paused/quarantined amounts;
- confirmed burns and terminal releases.

The ADR must define a disjoint accounting partition and transition table. At minimum:

- **confirmed reserve assets:** canonical, sufficiently confirmed, unspent custody outputs by current/old/recovery epoch;
- **provisional assets:** mempool outputs, unconfirmed checkpoint change, and reorg-exposed outputs—reported but never counted as confirmed backing;
- **native liabilities:** spendable and pending nZEC, with Nomic-side IBC escrow counted once as the backing representation for remote vouchers rather than counting both escrow and vouchers as two Nomic liabilities;
- **withdrawal claims:** nZEC burned/locked but not yet paid on Zcash remains a liability until canonical payment or an approved claim resolution;
- **protocol equity:** only realized fees/rewards legally and semantically owned by the protocol, never user claims or unsolicited deposits;
- **unrecognized/unsolicited ZEC:** not backing and not protocol equity until a reviewed state transition recognizes or recovers it;
- **explicit deficit/surplus:** a first-class value with cause and incident ID, never hidden by relabeling a bucket.

For every durable transition, the oracle proves both (a) each item appears in exactly one class and (b) the class-delta equation balances. It reconstructs values from raw Zakura outputs, decoded Nomic state/events, and both IBC endpoints rather than calling production aggregate helpers.

H3 must commit a versioned double-entry journal schema before scenario outcomes freeze. At minimum, the normative transition table includes:

| Transition | Required journal effect |
|---|---|
| Register terms | Reserve cap/exposure memorandum only; no ZEC asset or nZEC liability. |
| Observe unconfirmed output | Provisional evidence only; no confirmed asset/liability. |
| Mature and accept deposit | Debit confirmed reserve asset; credit pending-user liability, explicit future-fee obligation, and any disclosed service-fee equity so the zatoshi total balances. |
| Move pending to spendable | Debit pending liability; credit spendable/IBC-destination liability; no reserve change. |
| Queue withdrawal | Debit spendable liability; credit withdrawal claim; no reserve change. |
| Pay miner/checkpoint fee | Debit the fee obligation/equity account selected by policy; credit confirmed reserve when the canonical spend is recognized. |
| Confirm withdrawal | Debit withdrawal claim; credit confirmed reserve for the paid amount; provisional change becomes confirmed only under the checkpoint rule. |
| IBC send/receive/timeout | Move liability between user, source escrow, remote-voucher-observation, and refund states without counting source escrow and remote voucher twice. |
| Reorg/theft/key loss | Credit lost confirmed reserve and debit protocol equity then explicit deficit under the approved waterfall; never erase user claims to force balance. |
| Recovery/sweep | Move only named claimant/reserve classes; unsolicited ZEC or recovered change cannot become anonymous surplus. |

Consensus emits a typed journal event for every bridge transition, including rejection/compensation. The independent oracle recreates entries from raw inputs and compares per-entry and cumulative balances; state aggregate queries are merely another observed output.

Every call path in `src/app.rs`, protobuf dispatch, queries, CLI, and `src/cosmos.rs` that assumes only `Nbtc` must be reviewed explicitly.

### 8.10 Typed IBC destination and recovery

Use a versioned enum, never an untyped string/memo:

```text
NzecDestinationV1 =
  | NomicAccount { address }
  | Ics20 { port, channel, receiver, timeout_height, timeout_timestamp }
```

Parsing and chain/port/channel/receiver bounds happen before liability creation. Cross-asset dispatch matches the native denom/type plus destination variant, not memo text. A malformed/inapplicable packet must produce an error acknowledgement that preserves/refunds the source obligation; existing `src/app.rs` paths that log-and-swallow transfer errors are unacceptable for nZEC and must be refactored under BTC non-regression tests.

IBC accounting records packet commitment, source escrow, acknowledgement/timeout terminal state, remote voucher trace, refund, and any downstream withdrawal claim as one state machine. ICS-20 nZEC remains disabled in L1 until acknowledgement, timeout, channel closure, malformed receive, two-relayer race, and remote-chain recovery/operator procedures are golden. A user must not be able to create a Zcash withdrawal through an arbitrary memo outside the typed route.

### 8.11 Migration

**Precondition:** close the pinned tree’s `InnerAppV6 → InnerAppV7` `todo!()` and prove every retained `unreachable!()` with fixtures or replace it with typed failure. No Zcash state version may be appended atop an unexecu...[truncated]

1. preserve every existing field and meaning for NOM, nBTC, Bitcoin headers, checkpoints, accounts, IBC, Ethereum/Babylon features, and staking;
2. initialize Zcash state disabled with zero liabilities and no custody epoch unless an explicit launch upgrade supplies reviewed parameters;
3. allocate new encoding indexes without collision;
4. support export/import and deterministic replay;
5. include old-state fixtures from real representative snapshots with secrets removed;
6. prove pre/post totals and root-level semantic equivalence for all existing assets;
7. make rollback limitations explicit: once ZEC deposits are accepted, binary rollback cannot erase liabilities.

Every later migration touching active Zcash state must additionally run with the bridge halted; verify old/new canonical anchors, processed outpoints, custody UTXOs, signing sessions, pending credits, withdrawals, IBC obligations, fees/equity, and deficit; and produce a deterministic itemized diff reviewed before activation. Migration code has no implicit right to reset a header queue or reinitialize bridge state.

---

## 9. Delivery sequence and acceptance gates

No calendar estimates are asserted here. Each milestone has an evidence gate.

### H0 — Harness skeleton

**Work**

- Create isolated harness package, topology, lifecycle, dynamic endpoint allocation, readiness, typed clocks, outer watchdog, retries, artifact sink, redaction, manifest, and fixture daemons that intentionally crash, hang, and emit redaction canaries.
- Pin binaries/images by version and digest in a source lock.
- Add smoke topology and CI cache strategy.

**Gate**

- `HAR-001`, `HAR-002`, and `HAR-005` through `HAR-010` pass against fixture daemons locally and in CI. This proves the harness, not yet Nomic/Zakura product behavior.
- Failure artifacts are sufficient to reproduce diagnosis and contain no secrets.
- Repeated runs prove cleanup and port isolation.

### H1 — BTC characterization extraction

**Work**

- Move process-control code out of the monolithic ignored Bitcoin test without changing bridge behavior.
- Add behavior-preserving production seams for configurable bind/RPC endpoints, cancellation, one-shot iteration, readiness, and durable cursors so BTC relayer/signer components can run as independently supervised services. Do not satisfy this with `cfg(test)` shortcuts.
- Implement BTC drivers and normalized accounting receipts.
- Keep the existing test until equivalence is proven.

**Gate**

- `BTC-001` through `BTC-014` pass at the pinned base behavior.
- `HAR-003` passes against the real Nomic BTC relayer/signer and `HAR-004` passes against a real Nomic validator partition.
- No change in BTC transaction/checkpoint economic outputs except normalized nondeterminism.

### H2 — Zakura local network and Zcash protocol oracle

**Work**

- Add the pinned Zakura regtest driver, split compatibility-sidecar wallet driver, multi-peer legacy/Zakura/dual-stack topologies, mining/maturity helpers, bounded fork controls, pruning tests, and named-pool shielded-origin funding.
- Port or invoke Zakura’s existing regtest trace oracle and preserve its JSONL traces beside Nomic receipts.
- Add Zebra/Zallet only as explicitly independent differential lanes; no primary scenario may silently bypass Zakura.
- Import pinned, licensed official vectors with provenance.
- Implement independent transaction/header/script/sighash/Merkle oracle checks.

**Gate**

- `ZPV-*` and `ZND-*` pass.
- `HAR-004` also passes against the Zakura legacy/v2/dual-stack partition controller, proving both backends implement the same fault contract.
- Stable/current Zakura compatibility lanes and exact RPC capability snapshots are explicit.
- The Zakura compatibility sidecar may run only as a labeled wallet/factory and compatibility oracle, never the proof or consensus lane.
- Archive and pruned retention behavior is proven, including failover or typed halt when required raw blocks have been deleted.

### H3 — Feasibility spikes and normative ADRs

**Work**

- Native Zcash header/Equihash benchmark and deterministic dependency review.
- P2SH committee script-size/standardness/signing/rotation spike plus per-subset stake/liveness security receipts.
- Threshold-loss recovery feasibility spike covering permanent key loss, old-epoch late deposits, upgrade/expiry interaction, claimant payout, and any lower post-timelock theft threshold.
- Signer-policy, key-ceremony, anti-equivocation, backup/restore, and replacement-transaction spikes.
- Canonical `DepositTerms`, cap reservation, derivation binding, refund/late-payment, and commitment-disclosure spike.
- Threshold-ECDSA research spike kept separate.
- Fee/expiry/checkpoint transaction construction spike.
- Finality/work/challenge/support-horizon and deep-conflict-proof spike.
- Normative double-entry accounting, deficit/loss-waterfall, safety-authority, typed IBC recovery, and trust-root/migration ADRs.
- State-size/gas and worst-case proof bounds, including aggregate per-block admission budgets under mixed invalid BTC/ZEC traffic.

**Gate**

Reviewed ADRs select or reject:

- production light-client trust mode;
- first custody model and hard TVL/rate caps;
- quantitative committee economic security, signer authorization/anti-equivocation, threshold-loss/theft handling, exit duty/bond horizon, and approved loss waterfall;
- supported Zcash network/version/upgrade window;
- nZEC denom/index/metadata;
- work/challenge/finality-risk/deep-reorg policy and support lease;
- canonical deposit terms, cap reservation, late-payment/refund policy, and privacy disclosures;
- versioned fee allocation and double-entry reserve/liability/deficit journal;
- pause/recovery/anchor/migration/IBC authority, timelock, veto, and loss settlement.

If native verification or acceptable custody fails, implementation stops at this gate.

### H4 — Complete executable scenario contract

**Work**

- Freeze policy-dependent expected outcomes from the accepted H3 ADRs; unresolved choices remain explicit no-go records rather than “golden” ambiguity.
- Implement every `ZDP-*`, `ZCP-*`, `ZSG-*`, `ZWD-*`, `ZHD-*`, `ZUP-*`, `MIG-Z-*`, `ZFT-*`, `IBC-Z-*`, and `ACC-*` contract/function with typed steps, barriers, fixtures, independent oracle, budgets, invariant/requirement IDs, unchanged-state projections, and evidence schema.
- Mark product-dependent contracts `Blocked` in the catalog without registering them as passing or ignored tests. A driver-side early return cannot satisfy a contract.
- Run all pre-product contract validation, model/oracle, fixture, topology, and protocol checks.
- Add manifest completeness, blocked-baseline monotonicity, enabled-state monotonicity, and zero-blocked release-gate checks.

**Gate**

- Every catalog ID maps to compiled contract code and exact ADR-backed invariants.
- Contract validation and all product-independent steps pass.
- CI reports `Pass`, `Fail`, and `Blocked` separately; blocked contracts contribute nothing to the pass count.
- No enabled scenario is ignored or demoted.
- Each implementation slice changes one coherent contract set to `Enabled`, records a real failing assertion, then implements the minimum GREEN path.
- Topology/process noise or driver-side capability shortcuts cannot masquerade as RED or GREEN.

### I1 — Consensus protocol types and light client

**Test-first activation**

Enable `ZHD-001` through `ZHD-016`, `ZUP-*`, `ZPV-*`, and support/finality boundary tests one slice at a time.

**Work**

- Bounded Zcash types/serialization.
- Header validation, cumulative work, fork window, finality-risk boundary, pruning, parameters, and calls/queries.
- Genesis/migration-disabled state.

**Gate**

- All header scenarios green.
- Benchmarks stay below reviewed state/gas/resource bounds.
- Differential vectors and deterministic multi-node replay pass.

### I2 — Custody registry, scripts, and signing

**Test-first activation**

Enable `ZCP-003` through `ZCP-009`, `ZCP-014` through `ZCP-016`, all `ZSG-*`, and the custody/terms cases `ZDP-009`, `ZDP-012`, `ZDP-013`, `ZDP-019` through `ZDP-022`, and `ZDP-029` as their primitives land.

**Work**

- Separate Zcash xpub registration/proof-of-possession.
- Committee selection/epoch state.
- Per-registration child derivation and P2SH construction.
- Signing-session state, scriptSig assembly, bad-signature handling, rotation, and recovery.

**Gate**

- Maximum configured committee scripts are accepted under the pinned Zakura mempool/policy path and compatibility sidecar, with an independent policy cross-check.
- Separate-key enforcement tests pass.
- Rotation and below-threshold behavior are green.
- External cryptographic review has no unresolved critical finding.

### I3 — Checkpoint transaction builder and fees

**Test-first activation**

Enable `ZCP-001`, `ZCP-002`, `ZCP-008` through `ZCP-013`, `ZCP-017`, `ZCP-018`, and `ZPV-002`/`ZPV-008`/`ZPV-012`/`ZPV-013` integration.

**Work**

- Transaction effects, txid/auth digest, ZIP-244 signatures, ZIP-317 fees, expiry/rebuild, deterministic batching, change, limits, broadcast receipts.

**Gate**

- Every constructed transaction is accepted by the pinned Zakura policy/mempool lane and decoded consistently by an independent implementation.
- Independent parser/builder agrees on effects and signatures.
- Expiry and fee rejection cannot lose or duplicate liabilities.

### I4 — Deposit relay and nZEC minting

**Test-first activation**

Enable `ZDP-001` through `ZDP-029` incrementally.

**Work**

- Deposit registration calls/queries.
- Block scanning, local Merkle construction, proof call.
- On-chain verification, processed outpoints, pending credit, checkpoint input, fees, stale epoch/recovery.
- `Nzec` accounts and accounting events.

**Gate**

- Every deposit/reorg/replay scenario is green.
- Conservation oracle passes after each transition.
- Two independent relayers produce the same accepted state.

### I5 — Withdrawals, IBC, CLI, queries, and operators

**Test-first activation**

Enable `ZWD-*`, `IBC-Z-*`, and multi-asset scenarios.

**Work**

- Explicit receiver parsing and withdrawal calls.
- CLI commands for ZEC registration, status, nZEC transfer, withdrawal, relayer, signer.
- IBC denom/packet/memo handling with strict asset routing.
- Queries, events, metrics, runbook-safe public diagnostics.

**Gate**

- Cross-asset confusion tests pass.
- No CLI accepts ambiguous network/address/receiver silently.
- IBC timeout/replay/refund conserves nZEC.

### I6 — Migration, adversarial hardening, and audit candidate

**Work**

- Full migration fixtures and export/import replay.
- `MIG-Z-*` two-binary upgrade/downgrade fixtures for every durable ZEC state and closure of the pre-existing base migration gap.
- Fuzz/property tests for parsing, proofs, scripts, fees, state transitions, and packet routing.
- Dependency/license/SBOM review.
- Fault and soak suites.
- Independent security review and remediation.

**Gate**

- All catalog scenarios expect `Enabled` and pass.
- No ignored required tests.
- No unresolved critical/high security issue.
- Existing BTC suite remains green.
- Reproducible release artifacts and checksums exist.

### L0 — Shadow and observation release

- Deploy code with Zcash state disabled.
- Run header relayers and observers without accepting deposits.
- Compare Nomic light-client tip/work with multiple Zakura sources, a lineage-differential Zebra observer, and a non-Zakura-lineage zcashd/official-vector validation path where available.
- Exercise pause/upgrade telemetry.

**Gate:** sustained agreement and no resource-budget violation under real header flow.

### L1 — Capped mainnet experiment

- Governance explicitly enables deposits with small aggregate, per-deposit, and growth-rate caps.
- Withdrawal/recovery paths are enabled and funded operationally before deposits.
- The custody economic-security ADR, signer authorization/anti-equivocation design, threshold-loss/theft treatment, exit-duty horizon, deficit accounting, and loss waterfall are approved and publicly reviewable.
- Public dashboard shows reserve UTXOs, liabilities, committee epoch, light-client tip lag, caps, and pause state.
- Veto/timelock applies to cap and custody-parameter increases.
- ICS-20 nZEC remains disabled until its full error-acknowledgement, timeout, channel-closure, remote-chain recovery, and accounting scenarios are green in the exact launch binary; direct Nomic credit may launch independently.

**Gate:** defined observation window and incident drills complete before any cap increase.

### L2 — Expansion

Cap increases require new evidence, not optimism:

- real receipts and accounting history;
- signer/relayer reliability;
- audit closure;
- no unresolved reorg/upgrade incident;
- governance acceptance of custody mode;
- repeated recovery drills.

Shielded custody or threshold ECDSA remains a separate proposal and audit surface.

---

## 10. CI and test lanes

| Lane | Trigger | Contents | Failure policy |
|---|---|---|---|
| Unit/protocol | Every PR | Pure types, parsing, vectors, scripts, sighash, Merkle, accounting properties | Required |
| Harness smoke | Every PR touching bridge/harness | One-node Zakura topology, compatibility-sidecar transparent wallet flow, BTC characterization subset, selected ZEC scenarios | Required |
| Full quorum | Merge queue/nightly | Multi-validator/signers, all golden deposit/withdraw/checkpoint scenarios | Required for release branch |
| Fault/reorg | Nightly | Zakura competing branches, non-finalized invalidation/reconsideration, partitions, pruning gaps, worker death, signer outage, expiry, deep-reorg halt | Required for release candidate |
| Compatibility | Scheduled and dependency PRs | Pinned stable plus proposed-next Zakura; compatibility sidecar; independent Zebra/Zallet/transaction-library oracles | Blocks dependency promotion |
| Soak | Scheduled/manual release | Repeated mixed BTC/ZEC/IBC load and rotations under resource limits | Required for mainnet enablement |
| Migration/replay | Every state change | Historical fixtures, export/import, deterministic replay | Required |
| Security/fuzz | Continuous/scheduled | Parsers, proof bounds, scripts, state-machine and packet fuzzing | No unresolved reproducible crash/invariant break |

CI must print exact versions and digests, retain redacted artifacts on failure, and never pull floating image tags.

### Explicit build integration

The base repository is not a Cargo workspace, so a nested package is invisible to existing root jobs unless CI names it explicitly.

- `tools/interchain-tests` owns a committed lockfile and its own pinned stable `rust-toolchain.toml`; invoking Cargo from that directory must not inherit Nomic’s `nightly-2024-07-21` accidentally.
- Nomic itself continues to build with the base repository’s pinned nightly/Nix path. The harness launches the resulting binary as a process/container and does not drag modern orchestration dependencies into consensus code.
- CI adds dedicated harness format, clippy, unit/contract, smoke, and source-lock jobs with `working-directory: tools/interchain-tests` or an explicit `--manifest-path`.
- The named commands are committed rather than implied: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features`, `cargo run -p nomic-bridge-harness -- contract validate --manifest scenarios/manifest.toml`, and the selected `scenario run` lane. The nested toolchain file resolves the exact stable compiler.
- CI proves the harness crate appears in dependency/license/SBOM and coverage reports; a successful root `cargo test` is not evidence that harness code compiled.
- Images are pulled by digest, capability-checked before scenarios, and cached by digest. A cache miss cannot fall back to `latest`.
- Job timeout handling kills the complete labeled process/container tree and still runs redacted artifact collection.
- Scenario matrices use `fail-fast: false`; every matrix cell uploads a small receipt/manifest on success and the redacted diagnostic bundle on failure with a finite retention period. Retry-after-failure is diagnostic and never changes the original failed result.

H0 first records cold-cache and warm-cache `p50`, `p95`, and maximum wall time, aggregate RSS, disk, and artifact bytes over the fixed seed corpus. The table below is a provisional **upper envelope**, not evidence that the jobs fit. H0 must commit lower or justified adjusted admission ceilings with headroom from those measurements before the corresponding lane is enabled; later increases require a reviewed performance receipt.

| Lane | Wall-clock ceiling | Peak aggregate memory | Ephemeral disk/artifacts | Parallelism |
|---|---:|---:|---:|---:|
| Unit/contract/protocol | 10 minutes | 4 GiB | 2 GiB | normal test parallelism |
| PR Zakura smoke | 30 minutes | 12 GiB | 20 GiB / 2 GiB retained | one topology per runner |
| Full quorum/IBC | 90 minutes | 32 GiB | 80 GiB / 5 GiB retained | one topology per runner |
| Fault/reorg/pruning | 120 minutes | 32 GiB | 100 GiB / 10 GiB retained | one topology per runner |
| Soak/replay | 180 minutes per declared seed shard | 32 GiB | 120 GiB / 10 GiB retained | isolated runner, fixed shard count |

Crossing a resource ceiling is a test failure with per-component CPU/RSS/disk diagnostics. It is not retried until green.

---

## 11. Observability and public accountability

### Consensus/query state

Expose:

- Zcash network/genesis fingerprint;
- canonical tip and local finality-risk-boundary height/hash/work;
- relayer-independent lag estimate inputs;
- accepted fork window and halt reason;
- custody epoch, public members/keys, threshold, activation/expiry;
- open deposit registrations and public status;
- checkpoint state, effects txid, expiry, fee, signature count, broadcast/confirmation status;
- aggregate nZEC liabilities by bucket;
- recognized reserve by bucket;
- caps, rate limits, and pause flags;
- active support lease (`max_supported_height`), next known upgrade, required drain margin, and time-to-terms-issuance stop;

### Worker metrics

At minimum:

- source tip and on-chain tip;
- accepted/rejected header/deposit/checkpoint calls by stable reason;
- rescan cursor and lag;
- checkpoint age/expiry margin;
- signer participation and invalid responses;
- RPC identity mismatch;
- reorg depth/work;
- accounting invariant status;
- paused/cap utilization.

Metrics must not contain user memos beyond necessary public identifiers, RPC credentials, keys, or internal topology.

### Alerts

Page-worthy conditions include:

- authenticated greater-work conflict beyond the local finality-risk boundary;
- accounting mismatch;
- unknown network upgrade/version;
- custody threshold unavailable;
- checkpoint approaching expiry repeatedly;
- source/on-chain work divergence across independent observers;
- invalid Equihash/difficulty flood;
- cap exceeded or pause changed;
- reserve/liability discrepancy;
- migration/replay divergence.

---

## 12. Review and audit requirements

Before mainnet enablement, independent reviewers must cover:

1. Zcash consensus/header/difficulty/Equihash implementation.
2. Transaction parser, txid/auth digest, ZIP-244 sighash, script, and ZIP-317 fee logic.
3. Custody committee selection, quantitative economic security, key registration/derivation, exit-duty horizon, rotation, threshold loss/theft, recovery, and compromise assumptions.
4. Signer local-state authorization, policy reconstruction, anti-equivocation, replacement rules, HSM/backup/restore, and secret boundaries.
5. Nomic state encoding, trusted-anchor governance, active-state migration, itemized diffs, and deterministic execution.
6. Deposit commitment/reveal binding, replay protection, privacy disclosure, late/unsolicited deposits, and reorg handling.
7. Disjoint nZEC reserve/liability/equity/deficit accounting, every mint/burn/escrow/withdrawal-claim path, and loss waterfall.
8. IBC denom/memo/acknowledgement/timeout/channel routing and cross-asset confusion.
9. Relayer operational hardening, Zakura source/pruning assumptions, and failover boundaries.
10. Harness/oracle independence: prove it is not merely restating the implementation under test.
11. Safety-state authority, atomic caps, guardian limits, pause/unpause, veto/timelock, deficit, and incident authority.

At least one reviewer should be Zcash-protocol competent and one should be independent of the implementation team.

---

## 13. Decision register

These decisions must become reviewed ADRs during H3. Recommended defaults are hypotheses to test, not permission to skip evidence.

| Decision | Recommended starting hypothesis | Evidence required |
|---|---|---|
| Harness implementation | Isolated Rust package with process/container drivers | Repeated local/CI bootstrap, cleanup, fault, and artifact tests |
| Production node | Zakura `v1.0.5` source/release/OCI digests pinned; workers use direct Zakura RPC | RPC capability lock, archive/pruning policy, multi-source agreement, version-promotion suite, independent observer tests |
| Test wallet/factory | Zakura `zcashd-compat` sidecar pinned as the default transparent/Sapling factory; Zallet only for declared differential or sidecar-descope cases | Required operations by pool/upgrade, sole-peer enforcement, and raw-transaction cross-checks in Zakura |
| Initial custody | Explicitly capped `11-of-15` standard P2SH validator committee | Script/policy vectors, selection ADR, rotation/recovery tests, audit, governance acceptance |
| Long-term custody | Separately audited threshold ECDSA research | Library/protocol audit, DKG/resharing/nonces, deterministic integration, drills |
| Committee economic security | Bonded-validator selection with duty/exit horizon beyond old-epoch exposure | Collusion/key-loss model, bond/insurance versus cap, concentration, slashing evidence, sweep completion |
| Threshold-loss/theft response | No value until recovery or explicit capped-loss model exists | Standard script proof or approved permanent-loss/theft cap, deficit/loss waterfall, drills |
| Signer authorization | Local Nomic state verification plus crash-safe anti-equivocation | Conflicting-spend, restart, endpoint compromise, fee-bump replacement, HSM/restore tests |
| Light client | Native PoW verification | Benchmark, deterministic replay, dependency audit, state/gas bounds |
| Deposit binding | Commitment-first on-chain registration plus unique child-key P2SH script | Reveal/front-run, redirect/replay/expiry/rotation, privacy-disclosure, child-index tests |
| Supported custody surface | Transparent outputs only | Honest UX, address/parser and shielded-origin tests |
| Withdrawals | Explicit transparent receiver only | Address/network/UA ambiguity tests |
| Fee policy | ZIP-317 logical actions, versioned | Independent vectors, Zakura mempool acceptance, and differential policy checks |
| Finality | Configured confirmations plus bounded fork window and deep-reorg halt | Fork/reorg scenarios and governance incident policy |
| Safety authority | Guardian can only pause/lower risk; delayed governance plus veto for risk increases/recovery | Full action/state matrix, cap races, compromised guardian, deficit tests |
| Loss allocation | Pre-approved explicit deficit and non-preferential claim waterfall | Deep reorg, theft/key loss, invalid mint/migration fixtures and public policy |
| nZEC denom/index | New distinct symbol; exact string/index selected by ADR | Encoding collision check, chain metadata/IBC review, migration fixture |
| Launch | Disabled → shadow → hard-capped enablement | Audit, all-green catalog, public receipts/dashboard, incident drill |

---

## 14. Stop conditions

Stop implementation or launch if any of these is true:

- native Zcash verification is nondeterministic or exceeds accepted consensus resource bounds and no reviewed proof alternative exists;
- custody cannot meet the explicitly approved security model;
- the configured P2SH script is nonstandard or above size/sigop limits;
- the selected committee cap is not quantitatively bounded by credible economic security, or old-epoch duty can end before reserve exposure;
- permanent threshold loss/theft has neither a reviewed recovery construction nor an explicitly capped, funded loss treatment;
- signers can sign from a remote digest without local policy reconstruction or can equivocate across crashes/replacements;
- threshold signing requires unaudited cryptography for uncapped funds;
- migration cannot prove preservation of existing BTC/nBTC state;
- accounting invariants are incomplete or fail under fault/reorg tests;
- a required golden scenario is blocked, skipped, demoted after enablement, or flaky;
- safety authority permits an emergency actor to unpause, raise caps, change anchors/committee, redirect funds, or prefer creditors;
- trusted-anchor or migration changes can delete proof history, processed outpoints, liabilities, or deficits without a halted itemized diff;
- no pre-approved deficit/loss waterfall exists;
- Zakura/sidecar/network-upgrade compatibility is unknown, required raw-block retention is unbounded or insufficient, or the primary lane can pass by falling back to Zebra/Zallet;
- pause/recovery/withdrawal cannot be exercised before deposits;
- public documentation implies shielded custody/privacy that the system does not provide;
- the release process cannot reproduce exact binaries/images and source commits.

A stopped unsafe bridge is an engineering result. A live bridge built on vibes is a liability.

---

## 15. Independent-review resolution ledger

Three independent reviews were run against the pre-reconciliation draft: Nomic/protocol architecture, harness determinism/CI, and adversarial custody/economics. Their verdict was correctly **no-go for value-bearing implementation at the old H4 gate**. The Zakura-first rewrite and this reconciliation resolve plan defects; they do not constitute implementation or audit closure.

| Review finding | Resolution in this plan | Remaining executable gate |
|---|---|---|
| BTC design must not be genericized first | Sibling `src/zcash` domain; BTC characterization and extraction equivalence first | `BTC-001..014`, H1 |
| Existing app migration seam is unfinished | Explicit precondition closes `InnerAppV6 → InnerAppV7` `todo!()` before any Zcash state version | `MIG-Z-*`, H1/H3 |
| Equal-slot `11-of-15` lacks a stake invariant | Enumerate cheapest 11-key theft and 5-key halt coalitions; bind cap/bond/exit horizon or reject Candidate A | H3 custody ADR, `ZCP-014..018` |
| Threshold loss has no recovery transaction | Separate byte-level recovery/late-deposit/upgrade spike; permanent-loss acceptance is explicit, never implied away | H3 recovery ADR, `ZCP-016`, `ZDP-025` |
| Header bootstrap/context/time semantics under-specified | Checkpoint plus at least 28 contiguous predecessor headers/equivalent summary; deterministic Nomic time and temporary `TooNew` class | H3, `ZPV-009`, `ZHD-015` |
| Support for future Zcash upgrades could become undefined | Consensus `max_supported_height` lease and pre-horizon stop/drain rules | `ZUP-003`, compatibility lane |
| Valid active transaction versions/pools were narrowed to v5 | Deposit recognition covers active v4/v5/v6 and exact Sprout/Sapling/Orchard/Ironwood combinations; construction remains one explicitly pinned version | `ZPV-001`, `ZPV-010..013` |
| Header decoding omitted the signed version rule | Explicit `nVersion >= 4`/high-bit-clear validation and boundary vectors | `ZPV-014`, I1 |
| Zero depth could underflow ambiguous confirmation arithmetic | `maturity_height = inclusion + D`; `D=0` defined for regtest/shadow and forbidden in Active mainnet | `ZDP-023`, H3 |
| Fee model counted inputs too loosely | Whole-transaction ZIP-317 action calculation, versioned allocation, four fee classes, exact 11-signature production and 15-signature/two-input policy vectors | `ZPV-008`, `ZCP-013`, H3 fee ADR |
| Version-specific P2SH sighash/preimage unclear | Fixed `SIGHASH_ALL`; v4 ZIP-243, v5 ZIP-244, launch-pinned activation-applicable v6 rules; prior output P2SH `scriptPubKey` and amount; independent vectors | `ZPV-002`, `ZPV-012`, `ZPV-013`, `ZSG-*` |
| Compact Merkle proof omitted mandatory shape context | Mandatory `transaction_count`, exact path-depth/index checks, explicit mutation ambiguity/SPV limit | `ZPV-004`, `ZDP-010` |
| “Expected MissingCapability = green” was fake TDD | H3 ADRs precede H4 exact contracts; Blocked is inventory, RED is a real enabled assertion | H4 and every I-phase |
| Harness was not bounded/reproducible enough | Semantic seed plus separate run nonce, typed clocks/barriers, deadlines, kill tree, measured budgets, no green retry | `HAR-001..010`, H0 |
| Oracles could share production bugs | Separate oracle package/process, dependency deny-list, raw-state derivation, mutation corpus | `HAR-008`, `ACC-010` |
| Scenario outcomes and coverage were ambiguous | ADR-backed terminal predicates, unchanged projections, signing/migration/boundary/race/outage families | H4 zero-ambiguous contract gate |
| Signers lacked a complete authority boundary | Local consensus-committed package, full economic reconstruction, durable anti-equivocation and replacement linkage | `ZSG-001..010`, H3 |
| Deposit destination alone did not bind economic terms | Canonical `DepositTermsV1`, cap reservation, terms-bound derivation, explicit amount/expiry/refund/fees | `ZDP-019..029`, H3 |
| Reserve accounting and loss allocation were non-normative | Versioned double-entry journal, disjoint buckets, explicit deficit and approved loss waterfall | `ACC-*`, H3/L1 |
| “Confirmations” overstated finality | Work + depth + Nomic-block challenge/observation state machine and SPV limitation | `ZHD-006..012`, H3 |
| IBC memo/error behavior could orphan value | Typed destination enum, error acknowledgement/timeout recovery, ICS-20 launch independently gated | `IBC-Z-*`, L1 |
| Artifacts and CI could leak keys or hide failures | External collector, upload/local trust split, canary scan, `fail-fast: false`, finite retention | `HAR-002`, `HAR-010`, H0 |

Any reviewer disagreement with a “resolution” reopens the H3/H4 gate; the ledger is not a waiver.

## 16. Primary sources and pinned research inputs

Accessed on 2026-08-02.

### Nomic

- Pinned fork/upstream base: <https://github.com/n0sn0de/nomic/tree/3dccaf5d6349430148fa490cc4a0bddbf2ef433e>
- Existing Bitcoin test: <https://github.com/n0sn0de/nomic/blob/3dccaf5d6349430148fa490cc4a0bddbf2ef433e/tests/bitcoin.rs>
- Bitcoin bridge state/calls: <https://github.com/n0sn0de/nomic/blob/3dccaf5d6349430148fa490cc4a0bddbf2ef433e/src/bitcoin/mod.rs>
- Header queue: <https://github.com/n0sn0de/nomic/blob/3dccaf5d6349430148fa490cc4a0bddbf2ef433e/src/bitcoin/header_queue.rs>
- Checkpoints: <https://github.com/n0sn0de/nomic/blob/3dccaf5d6349430148fa490cc4a0bddbf2ef433e/src/bitcoin/checkpoint.rs>
- Signatory scripts: <https://github.com/n0sn0de/nomic/blob/3dccaf5d6349430148fa490cc4a0bddbf2ef433e/src/bitcoin/signatory.rs>
- Signer/relayer: <https://github.com/n0sn0de/nomic/tree/3dccaf5d6349430148fa490cc4a0bddbf2ef433e/src/bitcoin>
- Application wiring: <https://github.com/n0sn0de/nomic/blob/3dccaf5d6349430148fa490cc4a0bddbf2ef433e/src/app.rs>

### Zakura primary node and compatibility sidecar

- Zakura project site: <https://zakura.com/>
- Zakura `v1.0.5` release: <https://github.com/zakura-core/zakura/releases/tag/v1.0.5>
- Zakura pinned source commit: <https://github.com/zakura-core/zakura/tree/57c2898f1e2202a05215d2f0bc92c1e3478c8cda>
- Zakura RPC trait, including regtest mining and bounded reorg controls: <https://github.com/zakura-core/zakura/blob/57c2898f1e2202a05215d2f0bc92c1e3478c8cda/zakura-rpc/src/methods.rs>
- Zakura configured-regtest parameters and activation heights: <https://github.com/zakura-core/zakura/blob/57c2898f1e2202a05215d2f0bc92c1e3478c8cda/zakura-network/src/config.rs>
- Zakura four-node regtest e2e: <https://github.com/zakura-core/zakura/tree/57c2898f1e2202a05215d2f0bc92c1e3478c8cda/docker/zakura-regtest-e2e>
- Zakura zcashd-compat architecture and pruning constraints: <https://github.com/zakura-core/zakura/blob/57c2898f1e2202a05215d2f0bc92c1e3478c8cda/book/src/user/zcashd-compat.md>
- Zakura chain types, transaction IDs, headers, Equihash, and difficulty: <https://github.com/zakura-core/zakura/tree/57c2898f1e2202a05215d2f0bc92c1e3478c8cda/zakura-chain/src>
- Zakura fork-aware header-chain specification and its explicit header-only validity boundary: <https://github.com/zakura-core/zakura/blob/57c2898f1e2202a05215d2f0bc92c1e3478c8cda/docs/specs/fork-aware-header-chain-engine.md>
- Compatibility sidecar `v1.1.0`: <https://github.com/valargroup/zcashd/releases/tag/v1.1.0>
- Compatibility sidecar pinned source commit: <https://github.com/valargroup/zcashd/tree/c44e282a1b72ceeaef7fb494a8757cfde0a4d5fc>
- Compatibility sidecar source license and dependency caveat: <https://github.com/valargroup/zcashd/blob/c44e282a1b72ceeaef7fb494a8757cfde0a4d5fc/COPYING>
- Zakura OCI tags (digest must still be locked, never inferred from tag): <https://hub.docker.com/r/zakuracore/zakura/tags>
- Compatibility-sidecar OCI tags: <https://hub.docker.com/r/zakuracore/zcashd/tags>

The plan’s initial OCI index digests and release checksums are recorded in Section 6.1. A platform-specific manifest digest is resolved into the future harness lock on each supported architecture.

### Zcash protocol and independent/differential implementations

- Zcash protocol specification: <https://zips.z.cash/protocol/protocol.pdf>
- ZIP-243, version-4 transaction signature digest: <https://zips.z.cash/zip-0243>
- ZIP-244, version-5 transaction identifiers and signature digest: <https://zips.z.cash/zip-0244>
- ZIP-229 inspected Draft snapshot, version-6 format (evidence only; H3 must pin activation-applicable launch authority): <https://github.com/zcash/zips/blob/c5d14d6a84a3c3090a23420ba7010d2263c31ca0/zips/zip-0229.md>
- ZIP-258 inspected NU6.3 activation/pool rules: <https://github.com/zcash/zips/blob/c5d14d6a84a3c3090a23420ba7010d2263c31ca0/zips/zip-0258.md>
- ZIP-316, unified addresses and transparent receiver types: <https://zips.z.cash/zip-0316>
- ZIP-317, fee mechanism: <https://zips.z.cash/zip-0317>
- ZIP-320, transparent-source restrictions/TEX addresses: <https://zips.z.cash/zip-0320>
- Pinned ZIP repository research commit: <https://github.com/zcash/zips/tree/c5d14d6a84a3c3090a23420ba7010d2263c31ca0>
- Zebra pinned differential research commit: <https://github.com/ZcashFoundation/zebra/tree/d141092a5abaa55a795460f63787ac90b61554ba>
- Zebra regtest documentation, used as a differential harness input rather than the primary node contract: <https://github.com/ZcashFoundation/zebra/blob/d141092a5abaa55a795460f63787ac90b61554ba/book/src/user/regtest.md>
- Zebra header/Equihash/work implementations, used as lineage-differential evidence but not sufficient independence from Zakura by themselves: <https://github.com/ZcashFoundation/zebra/tree/d141092a5abaa55a795460f63787ac90b61554ba/zebra-chain/src>
- `librustzcash` pinned research commit: <https://github.com/zcash/librustzcash/tree/24ed8eaa3961cf8f1385d7eafebd1e1c7bbf5efe>
- Zallet pinned differential/wallet-factory research commit: <https://github.com/zcash/zallet/tree/a457ddb30c2420c7e65a908eb0df2c5303d84886>
- Archived `zcashd` source, used only for compatibility/script-policy evidence: <https://github.com/zcash/zcash/tree/558f686599586f55def3db86955d74d3be44605e>
- Zcash Script element limit: <https://github.com/zcash/zcash/blob/558f686599586f55def3db86955d74d3be44605e/src/script/script.h>
- Zcash standard P2SH policy: <https://github.com/zcash/zcash/blob/558f686599586f55def3db86955d74d3be44605e/src/policy/policy.cpp>

### Harness inspirations

- Interchaintest pinned research commit: <https://github.com/strangelove-ventures/interchaintest/tree/34a13b0e5f1117f63a3d90a961abfeeede2e98af>
- Starship pinned research commit: <https://github.com/hyperweb-io/starship/tree/2475efb85165a561b9b87a76c8f60462efba84aa>
- Hermes IBC test framework pinned research commit: <https://github.com/informalsystems/hermes/tree/5e78a424904ff669a787a6c0eda53353c8096c11/tools/test-framework>

---

## 17. Definition of done for this plan

This plan has been executed—not merely discussed—when:

- the harness milestones H0–H4 are implemented and reviewed;
- every listed scenario ID maps to executable code and a structured receipt;
- all required capabilities are enabled and scenarios pass before release;
- the selected light-client and custody ADRs have measured evidence and external review;
- BTC behavior remains characterized and green;
- nZEC conservation holds across deposits, checkpoints, withdrawals, IBC, faults, upgrades, and migration;
- mainnet starts disabled, then shadowed, then hard-capped;
- operators and users can independently inspect reserve, liabilities, headers, custody epoch, caps, and incident state.

That is the standard. Anything less is not “adding ZEC to Nomic.” It is adding unpriced custody risk with a ticker.
