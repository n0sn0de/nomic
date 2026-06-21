You are the lead security researcher for an explicitly authorized, defensive
audit of Nomic's open-source Bitcoin custody and IBC implementation.

TARGET
------
Repository: https://github.com/nomic-io/nomic
Authorized revision: <COMMIT_SHA_OR_TAG>
Authorized dependency scope:
  - nomic-io/nomic
  - Exact pinned Orga revision and enabled Orga features
  - Exact pinned Bitcoin, ICS-23, Cosmos protobuf, FROST, serialization,
    storage, cryptography, networking, and consensus dependencies
  - Other repositories only when their code is reached by a security-sensitive
    call path from the target revision

Primary language: Rust
Primary systems:
  - Bitcoin header and transaction verification
  - Bitcoin reserve custody
  - Checkpoint construction and signing
  - Threshold signatures and FROST
  - nBTC issuance, transfer, burning, and redemption
  - Cosmos IBC and ICS-20 behavior
  - ICS-23 proof verification
  - Relayer-facing APIs
  - Consensus-critical state transitions
  - Genesis, migrations, upgrades, and feature-gated builds

AUTHORIZED ENVIRONMENT
----------------------
Use only:
  - Local source checkouts
  - Local unit, integration, property, fuzz, and model-based tests
  - Bitcoin Core regtest or a fully isolated Bitcoin simulator
  - Locally instantiated Nomic/Cosmos chains
  - Local mock relayers or isolated Hermes-compatible relayers
  - Disposable containers or virtual machines
  - Synthetic keys and valueless test assets

Never:
  - Connect vulnerability probes to Nomic mainnet
  - Test public Nomic testnets
  - Test public frontends, RPCs, relayers, explorers, validators, seed nodes,
    signers, or third-party infrastructure
  - Use real BTC, real private keys, or real user information
  - Broadcast crafted transactions or packets to public networks
  - degrade, disrupt, or deny service to a public system
  - Publish unpatched vulnerability details
  - Produce a mainnet-ready exploit or asset-theft tool

A proof of concept must be a minimal local regression test, fuzz corpus entry,
model trace, or isolated harness. Remove unnecessary weaponization. Report
findings privately through the disclosure channel in the repository's current
SECURITY.md.

MISSION
-------
Determine whether an untrusted user, depositor, relayer, validator, signer,
operator, IBC counterparty, Bitcoin miner, dependency, or upgrade can violate
Nomic's safety, solvency, consensus, authorization, confidentiality, or
liveness guarantees.

Prioritize vulnerabilities that could cause:

  1. Unbacked nBTC creation
  2. Unauthorized Bitcoin expenditure
  3. Double-crediting of a Bitcoin UTXO
  4. Loss or permanent locking of reserve BTC
  5. Invalid or conflicting checkpoint signatures
  6. Threshold-signature or FROST key compromise
  7. Acceptance of invalid Bitcoin headers or Merkle proofs
  8. Reorg-related solvency failures
  9. IBC packet replay, double refund, double mint, or incorrect burn
 10. Invalid ICS-23 proof acceptance
 11. Consensus divergence or nondeterministic state
 12. Chain halt, validator crash, or unbounded resource consumption
 13. Privilege escalation or authorization bypass
 14. Migration-induced state corruption or supply mismatch
 15. Emergency recovery or disbursal failure

Do not assume documentation, comments, tests, or configuration are correct.
Treat source code at the authorized commit as ground truth, while identifying
and reporting discrepancies between code, documentation, network parameters,
and intended protocol behavior.

PHASE 0 — RECORD THE EXACT TARGET
---------------------------------
Before analysis:

  1. Record:
       git rev-parse HEAD
       git status --short
       rustc --version --verbose
       cargo --version
       cargo metadata --format-version 1
       cargo tree --all-features
       cargo tree -d

  2. Record all:
       - Enabled Cargo features
       - cfg(test), testnet, mainnet, development, and experimental branches
       - Git dependencies and exact revisions
       - Build scripts
       - Generated protobuf code
       - Patched crates
       - Workspace members
       - Lockfile state
       - Compiler profiles
       - Panic and overflow settings

  3. Build a feature/build matrix covering at least:
       - Default features
       - All features
       - No-default-features where supported
       - Mainnet-equivalent configuration
       - Testnet-equivalent configuration
       - Release mode
       - Debug mode

  4. Identify semantic differences between configurations, especially:
       - Signing thresholds
       - Confirmation requirements
       - Checkpoint behavior
       - FROST activation
       - Recovery paths
       - Fee and capacity settings
       - Validation shortcuts
       - Debug assertions
       - Overflow behavior

Never infer the production threshold or other security parameter from old
documentation. Trace each value from configuration through execution and into
the resulting Bitcoin script or consensus decision.

PHASE 1 — ARCHITECTURE AND TRUST MAP
------------------------------------
Map the complete security architecture. Begin with, but do not limit review to:

  src/app.rs
  src/app/
  src/app/migrations.rs
  src/bitcoin/mod.rs
  src/bitcoin/adapter.rs
  src/bitcoin/checkpoint.rs
  src/bitcoin/deposit_index.rs
  src/bitcoin/header_queue.rs
  src/bitcoin/outpoint_set.rs
  src/bitcoin/recovery.rs
  src/bitcoin/relayer.rs
  src/bitcoin/signatory.rs
  src/bitcoin/signer.rs
  src/bitcoin/threshold_sig.rs
  src/cosmos.rs
  src/frost/
  network and genesis configuration
  build.rs
  Cargo.toml
  Cargo.lock
  tests/bitcoin.rs
  tests/header_queue.rs
  tests/ibc.rs
  tests/relayer.rs
  tests/node.rs

Follow security-sensitive calls into Orga and all relevant pinned dependencies.

Produce:

  - Module and call graph
  - State-transition diagram
  - Bitcoin deposit lifecycle
  - Bitcoin withdrawal lifecycle
  - Checkpoint lifecycle
  - Emergency recovery lifecycle
  - Threshold-signing lifecycle
  - FROST DKG and signing lifecycle
  - IBC packet lifecycle
  - ICS-23 proof-verification lifecycle
  - Upgrade and migration lifecycle
  - Privilege and capability map
  - External-input map
  - Cross-chain finality map
  - Persistence and crash-recovery map

For every trust boundary, identify:

  - Input source
  - Authentication mechanism
  - Authorization mechanism
  - Replay protection
  - Freshness mechanism
  - Domain separation
  - Validation performed
  - State mutated
  - Economic effect
  - Failure behavior
  - Resource bound
  - Whether the path is consensus-critical

THREAT ACTORS
-------------
Model at least the following adversaries, individually and in combinations:

  - Anonymous transaction submitter
  - Malicious Bitcoin depositor
  - Malicious withdrawal requester
  - Malicious or buggy relayer
  - Malicious IBC counterparty chain
  - Byzantine minority of validators
  - Threshold-sized validator or signer coalition
  - Malicious newly entering or exiting validator
  - Validator withholding signatures
  - Signer equivocating across rounds
  - Signer restoring stale disk state
  - Compromised signer host
  - Malicious Bitcoin miner or temporary majority hash-power adversary
  - Attacker able to induce shallow or deep Bitcoin reorganizations
  - Attacker controlling transaction ordering
  - Malicious governance or upgrade proposal
  - Supply-chain attacker
  - Network attacker able to delay, reorder, duplicate, or drop messages
  - Resource-exhaustion attacker with many low-cost transactions
  - Honest components running different feature combinations or architectures

Also analyze collusion boundaries. For example:

  - Relayer plus depositor
  - Relayer plus IBC counterparty
  - Minority signer plus transaction requester
  - Validator plus miner
  - Upgrade authority plus dependency compromise
  - Stale signer plus current signer set

SECURITY INVARIANTS
-------------------
Turn each invariant into one or more executable tests, assertions, state-machine
properties, or formal specifications.

A. RESERVE AND SUPPLY

  A1. Total redeemable nBTC must never exceed verified, spendable reserve BTC,
      accounting explicitly for protocol fees, pending withdrawals, recovery
      transactions, dust, and any defined liabilities.

  A2. Every unit of nBTC issuance must be attributable to exactly one accepted
      Bitcoin output or another explicitly authorized state transition.

  A3. Every accepted Bitcoin outpoint can be credited at most once across:
        - Duplicate relay attempts
        - Alternate Merkle proofs
        - Reorganizations
        - State pruning
        - Migration
        - Restart
        - Legacy and current commitment formats
        - Concurrent or differently ordered transactions

  A4. Burning, escrowing, sending, refunding, and redeeming nBTC must conserve
      value under all success, error, timeout, acknowledgement, and rollback
      paths.

  A5. Integer conversion, fee subtraction, rounding, dust handling, and unit
      conversion must not create or destroy value unexpectedly.

B. BITCOIN DEPOSIT VALIDATION

  B1. A deposit is accepted only when:
        - The claimed transaction ID is correct
        - The transaction is actually committed by the proved block
        - The block belongs to the accepted canonical header chain
        - The required confirmation rule is satisfied without off-by-one error
        - The claimed vout exists
        - The output value is correctly parsed
        - The output script exactly commits to the intended destination and
          intended signer set
        - The signer set is active for that deposit
        - Deposit age and expiry constraints are satisfied
        - Minimum amount and capacity constraints are satisfied
        - The outpoint has not previously been processed

  B2. No alternative encoding, malformed script, duplicate transaction ID,
      partial-Merkle-tree ambiguity, endianness confusion, or legacy-format
      fallback can redirect a deposit or bypass validation.

  B3. Validation and state mutation are atomic. A failed deposit must not:
        - Consume the outpoint improperly
        - Partially mint nBTC
        - Alter checkpoint state
        - Create an unintended recovery transaction
        - Leave a state-dependent denial-of-service condition

  B4. A deposit near expiry, confirmation, epoch, checkpoint, or capacity
      boundaries has one unambiguous outcome independent of transaction order.

C. BITCOIN HEADER CHAIN

  C1. Every accepted header satisfies:
        - Previous-header linkage
        - Proof of work
        - Target encoding rules
        - Difficulty adjustment rules
        - Median-time-past rules
        - Network-specific special rules
        - Cumulative-work comparison

  C2. Chain selection uses cumulative work correctly and cannot be manipulated
      through overflow, truncation, signedness, target conversion, or malformed
      compact difficulty encodings.

  C3. Reorganization logic correctly:
        - Identifies the common ancestor
        - Reverts all affected state
        - Reapplies canonical state
        - Handles queue pruning
        - Rejects unsupported deep reorgs safely
        - Avoids retaining orphan-derived credits
        - Avoids double-crediting after re-inclusion

  C4. Confirmation counts are correct at genesis, queue boundaries, current tip,
      and immediately before and after a reorg.

  C5. Header pruning cannot remove evidence required to preserve solvency,
      replay protection, emergency recovery, or checkpoint correctness.

D. CHECKPOINTS AND BITCOIN TRANSACTIONS

  D1. Every checkpoint transaction spends only authorized reserve inputs.

  D2. Its outputs, fees, change, recovery path, locktime, sequence, sighash,
      script, witness, and transaction version match the state-approved plan.

  D3. Value is conserved:
        sum(inputs) = sum(outputs) + fee
      without overflow, underflow, truncation, or inconsistent units.

  D4. The exact transaction approved by consensus is the transaction signed by
      validators. No serialization difference, mutable field, alternate
      sighash, or stale checkpoint can change signing meaning.

  D5. Checkpoint batching respects:
        - Maximum inputs
        - Maximum outputs
        - Weight and virtual-size limits
        - Dust rules
        - Standardness assumptions
        - Fee bounds
        - Mempool replacement behavior
        - Dependency chains
        - Bitcoin consensus rules

  D6. A failed, dropped, replaced, delayed, or reorganized checkpoint cannot
      strand funds or allow inconsistent state.

  D7. Fees cannot be manipulated to:
        - Drain reserves
        - Censor withdrawals indefinitely
        - Create dust
        - Cause arithmetic failure
        - Force pathological checkpoint growth
        - Shift losses from an attacker to other holders

E. THRESHOLD SIGNATURES AND SIGNER SETS

  E1. The implemented threshold is identical across:
        - Configuration
        - Rust threshold calculation
        - Voting-power calculation
        - Bitcoin script
        - Signature-completion predicate
        - FROST threshold
        - Documentation and operator assumptions

  E2. Explicitly test `>`, `>=`, floor, ceiling, rational multiplication, and
      integer-rounding behavior for every validator-set size and power
      distribution, especially very small sets.

  E3. Duplicate keys, duplicate voting power, zero power, extreme power,
      malformed extended public keys, invalid child derivation, and index wrap
      cannot reduce the effective threshold or redirect funds.

  E4. Signatures are bound to:
        - Correct chain
        - Correct checkpoint
        - Correct input
        - Correct transaction
        - Correct signer-set epoch
        - Correct protocol version
        - Correct signing algorithm

  E5. A signer cannot safely be induced to sign conflicting transactions for
      the same reserve state.

  E6. Offline, jailed, newly bonded, unbonding, or removed validators cannot
      create a discrepancy between consensus voting power and Bitcoin spending
      power.

F. FROST AND DISTRIBUTED KEY GENERATION

  F1. DKG identifiers, commitments, shares, transcripts, and resulting public
      keys are domain-separated by chain, epoch, round, and protocol purpose.

  F2. A participant cannot:
        - Equivocate between recipients undetected
        - Submit malformed shares that corrupt the group key
        - Perform a rogue-key attack
        - Bias the resulting key
        - Impersonate another participant
        - Replay messages from another DKG
        - Exploit missing-participant or timeout handling

  F3. Signing nonces are:
        - Generated with a secure RNG
        - Never reused
        - Persisted or invalidated safely across crashes
        - Deleted only after safe completion
        - Bound to one message and one signing session
        - Protected against rollback and backup restoration

  F4. Signing commitments and shares are validated before state mutation.

  F5. Invalid shares are attributable where intended and cannot make honest
      participants accept an invalid aggregate signature.

  F6. Timeout, retry, abort, and restart behavior cannot cause nonce reuse,
      state desynchronization, or cross-session replay.

  F7. Secret material is not exposed through:
        - Logs
        - Debug formatting
        - Panic messages
        - Metrics
        - Serialization
        - Temporary files
        - Core dumps
        - Insecure file permissions
        - Unzeroized buffers

G. EMERGENCY RECOVERY AND DISBURSAL

  G1. Emergency transactions are valid, sufficiently funded, correctly timed,
      and spendable by the intended security policy.

  G2. Every new checkpoint correctly supersedes or invalidates obsolete
      emergency paths without leaving an unintended alternative spend.

  G3. The most recent confirmed reserve state remains recoverable under the
      documented failure model.

  G4. Locktime and sequence semantics are tested against Bitcoin consensus,
      not merely assumed from library types.

  G5. Validator churn, key rotation, chain halt, signer loss, and deep reorg do
      not make all recovery paths simultaneously unusable.

  G6. No actor can trigger emergency recovery early or redirect its outputs.

H. IBC AND ICS-20

  H1. Packet send, receive, acknowledgement, timeout, refund, escrow, burn,
      mint, and channel-close paths are each exactly-once.

  H2. Packet replay, duplicate acknowledgement, timeout-after-success,
      acknowledgement-after-timeout, reordered delivery, and relayer retry do
      not duplicate or destroy value.

  H3. Source/sink determination and denomination traces are correct for:
        - Native nBTC
        - Voucher nBTC
        - Multi-hop routes
        - Channel upgrades or replacements
        - Reverse transfers
        - Closed and reopened channels

  H4. Channel ID, port ID, connection, counterparty, sequence, timeout height,
      timeout timestamp, revision number, and client state are all bound to the
      intended operation.

  H5. A malicious counterparty cannot forge a success acknowledgement, exploit
      malformed acknowledgement decoding, or cause both a remote mint and a
      local refund.

  H6. Interchain deposit data commits unambiguously to the intended ICS-20
      transfer and cannot be repurposed for a different:
        - Destination chain
        - Channel
        - Receiver
        - Amount
        - Denomination
        - Memo
        - Signer set

  H7. Withdrawal memo and destination parsing must reject:
        - Ambiguous prefixes
        - Trailing data
        - Mixed encodings
        - Invalid Bech32 variants
        - Wrong-network addresses
        - Oversized scripts
        - Invalid scripts
        - Unicode or normalization tricks
        - Hex-decoding ambiguity
        - Empty or partial destinations

I. ICS-23 AND COSMOS PROOFS

  I1. Every proof is bound to the intended:
        - Trusted IBC client
        - Counterparty chain
        - Revision and height
        - Consensus state
        - Store name
        - Key path
        - Serialized key
        - Expected value
        - Proof specification

  I2. Reject proofs against:
        - Expired clients
        - Frozen clients
        - Incorrect heights
        - Future heights
        - Wrong revisions
        - Wrong stores
        - Neighboring keys
        - Non-membership where membership is required
        - Malformed or non-canonical protobuf encodings

  I3. Prove that operator-key and validator-set association cannot be forged by
      key-path confusion, type confusion, protobuf `Any` confusion, or an
      incorrectly trusted client.

  I4. Confirm all `latest_height - 1` or similar arithmetic is safe at revision
      boundaries, genesis, and minimum heights.

  I5. Validator ordering, consensus-key conversion, voting-power summation, and
      operator-key mapping are deterministic and duplicate-safe.

J. CONSENSUS DETERMINISM

  J1. Consensus results must not depend on:
        - Hash-map or hash-set iteration order
        - Filesystem ordering
        - Thread scheduling
        - Wall-clock time
        - Local timezone
        - Randomness not committed to state
        - Floating-point behavior
        - CPU architecture
        - Endianness
        - Compiler version
        - Debug versus release arithmetic
        - Optional feature differences
        - Nondeterministic serialization

  J2. Locate every use of:
        f32, f64, HashMap, HashSet, system time, randomness, parallelism,
        platform casts, `usize`, unchecked iteration, and host filesystem I/O
      that can influence consensus or persisted state.

  J3. Replace floating-point protocol calculations with exact rational or
      integer arithmetic in test models and compare all boundary results.

  J4. Run differential state-transition tests with randomized message order on
      multiple clean processes. State roots must match after every block.

K. RUST MEMORY, TYPE, AND ERROR SAFETY

  Audit for:

    - `unsafe` code in target and reachable dependencies
    - `unwrap`, `expect`, indexing, assertion, and panic in attacker-reachable
      or consensus paths
    - Arithmetic overflow and underflow
    - Narrowing casts and `as` conversions
    - `usize` portability
    - Incorrect `Option` or `Result` handling
    - State mutation before validation completion
    - Error suppression or overly broad error conversion
    - Partial decoding
    - Trailing-byte acceptance
    - Non-canonical encoding
    - Serde defaults and skipped fields
    - Enum discriminant/version confusion
    - Unbounded vectors, strings, maps, proofs, scripts, or transactions
    - Recursive parsing
    - Algorithmic-complexity denial of service
    - Large clone or allocation behavior
    - Lock inversion or deadlock
    - Secret copying
    - Accidental `Debug` exposure
    - TOCTOU issues
    - Path traversal or unsafe local secret storage
    - Improper zeroization
    - Undefined behavior in dependencies

  Treat panic-induced deterministic chain halt as a security issue even where
  Rust memory safety is preserved.

L. STATE, STORAGE, AND ATOMICITY

  L1. Identify transaction boundaries and rollback guarantees provided by
      Orga/storage layers.

  L2. Inject failures after every security-sensitive state write and verify
      that execution either commits a complete valid transition or preserves
      the exact prior state.

  L3. Test crash/restart at each checkpoint, signing, DKG, IBC, and migration
      phase.

  L4. Ensure replay-protection records are never pruned before all relevant
      cross-chain reorganization and recovery windows expire.

  L5. Check prefix collisions, key encoding, namespace separation, iteration
      bounds, stale indexes, tombstones, and inconsistent secondary indexes.

  L6. Compare logical supply, account balances, reserve UTXOs, checkpoint
      inputs, processed outpoints, pending withdrawals, and recovery state after
      every generated transition.

M. MIGRATIONS, GENESIS, AND UPGRADES

  M1. Review every historical and current migration.

  M2. Prove migrations preserve:
        - Total supply
        - Account balances
        - Processed outpoints
        - Reserve ownership
        - Checkpoint state
        - Signer epochs
        - Signing sessions
        - Recovery transactions
        - IBC sequences and escrow
        - Header-chain work and tip
        - Configuration invariants

  M3. Test:
        - Migration applied once
        - Migration accidentally applied twice
        - Interrupted migration
        - Empty state
        - Maximum-size state
        - Legacy malformed-but-accepted state
        - Upgrade immediately during checkpoint signing
        - Upgrade during DKG
        - Upgrade with pending IBC packets
        - Upgrade during a Bitcoin reorg

  M4. Validate genesis cannot introduce:
        - Unbacked initial balances
        - Duplicate signers
        - Invalid thresholds
        - Invalid header checkpoints
        - Incorrect network parameters
        - Inconsistent supply
        - Privileged accounts not intended by policy

N. RELAYER, RPC, AND DENIAL OF SERVICE

  Assume relayers and all submitted data are untrusted.

  Determine the cheapest attacker cost for causing:

    - Header verification
    - Merkle-proof verification
    - Transaction decoding
    - Script parsing
    - ICS-23 verification
    - Protobuf decoding
    - Checkpoint reconstruction
    - Signature verification
    - DKG processing
    - Storage growth
    - Event or log amplification
    - Repeated failing state transitions

  For each externally supplied field, establish a strict maximum:
    - Byte length
    - Element count
    - Nesting depth
    - Verification work
    - Storage impact
    - Error/log volume

  Search for asymmetry where a tiny transaction causes large CPU, memory, disk,
  state, or signing work.

  Test queues and indexes under adversarial ordering, duplicates, sparse keys,
  maximum sizes, malformed values, and repeated retries.

O. ECONOMIC AND LIVENESS ATTACKS

  Analyze:

    - Dust attacks
    - Reserve fragmentation
    - UTXO exhaustion
    - Fee-rate manipulation
    - Capacity-limit races
    - Checkpoint starvation
    - Withdrawal censorship
    - Signature withholding
    - Validator churn around signing thresholds
    - Deposit expiry manipulation
    - Reorg timing attacks
    - Griefing through invalid proofs
    - Free state growth
    - Cross-chain timeout manipulation
    - Front-running of administrative or checkpoint operations
    - Incentive incompatibility between validators, relayers, and nBTC holders

  Separate:
    - Safety under partial synchrony
    - Liveness under partial synchrony
    - Safety under chain halt
    - Safety under Bitcoin reorganization
    - Safety when the threshold will not sign
    - Recovery assumptions after catastrophic signer loss

P. SUPPLY-CHAIN REVIEW

  For every reachable dependency:

    - Record exact version or Git revision
    - Identify unmaintained, yanked, vulnerable, or forked packages
    - Review build scripts and proc macros
    - Identify unsafe code
    - Check enabled and disabled security features
    - Look for duplicate cryptography or serialization versions
    - Compare pinned Git code to published releases where relevant
    - Review changes since the last known independent audit
    - Determine whether a dependency is consensus-critical

  Run appropriate tools such as:

    cargo audit
    cargo deny check
    cargo tree -d
    cargo geiger
    cargo vet, when configuration exists
    cargo clippy --workspace --all-targets --all-features
    cargo fmt --all -- --check

  Do not report scanner output as a vulnerability without demonstrating reachability
  and security impact.

MANDATORY TEST STRATEGY
-----------------------
Use layered testing rather than relying on code reading alone.

1. EXISTING TESTS

   Run all existing tests under each relevant feature profile. Determine which
   security invariants are absent, weakly asserted, or mocked away.

2. NEGATIVE UNIT TESTS

   For every validation branch, create a paired test:
     - Smallest valid input
     - Smallest invalid input
     - Boundary minus one
     - Boundary
     - Boundary plus one
     - Duplicate
     - Reordered
     - Stale
     - Future
     - Wrong chain
     - Wrong epoch
     - Wrong signer set
     - Malformed encoding
     - Oversized input

3. PROPERTY TESTING

   Use proptest or an equivalent framework for:
     - Amount conservation
     - Threshold calculations
     - Header-chain transitions
     - Reorganization handling
     - Outpoint uniqueness
     - Checkpoint input/output conservation
     - Destination commitment parsing
     - IBC packet state machines
     - ICS-23 path construction
     - Serialization round trips
     - Migration idempotence where expected

4. COVERAGE-GUIDED FUZZING

   Create isolated fuzz targets for:
     - Bitcoin header decoding and validation
     - Compact-target conversion
     - Partial Merkle proofs
     - Bitcoin transaction decoding
     - Deposit proof processing
     - Destination commitment extraction
     - Bitcoin script/address parsing
     - Checkpoint transaction generation
     - Threshold-signature input parsing
     - FROST DKG messages
     - FROST signing messages
     - IBC packet and acknowledgement decoding
     - ICS-23 proofs
     - Protobuf `Any`
     - Migration decoding
     - All custom binary/state encodings

   Add semantic oracles. A crash-only fuzz target is insufficient.

5. STATE-MACHINE TESTING

   Model commands including:
     - Relay header
     - Reorganize Bitcoin chain
     - Relay deposit
     - Replay deposit
     - Create withdrawal
     - Advance checkpoint
     - Add or remove validator
     - Submit signature
     - Abort signing
     - Restart signer
     - Start or complete DKG
     - Send IBC packet
     - Receive packet
     - Deliver acknowledgement
     - Deliver timeout
     - Upgrade application
     - Migrate state
     - Restart node

   After every command, assert the complete invariant set.

6. DIFFERENTIAL TESTING

   Compare security-sensitive behavior with independent implementations or
   authoritative local nodes where practical:

     - Bitcoin header, target, transaction, script, and locktime behavior
       against Bitcoin Core regtest
     - Merkle proof results against an independent Bitcoin implementation
     - Generated transaction IDs and sighashes against independent libraries
     - ICS-23 proof behavior against another standards-conformant verifier
     - IBC packet semantics against a minimal ibc-go or ibc-rs reference chain
     - FROST signatures against an independent standards-conformant verifier

   Investigate every discrepancy. Do not automatically assume the reference or
   Nomic is correct.

7. FORMAL OR SYMBOLIC METHODS

   Apply bounded verification, symbolic execution, or explicit-state modeling
   where it provides value, particularly to:
     - Threshold arithmetic
     - Value conservation
     - Checkpoint state transitions
     - Packet acknowledgement/timeout exclusivity
     - DKG round transitions
     - Crash-recovery state
     - Migration invariants

   Tools may include Kani, Loom, Miri, TLA+, Apalache, Alloy, Creusot, Prusti,
   or a purpose-built executable model. Clearly state each model's abstraction
   limits.

HIGH-PRIORITY ADVERSARIAL SCENARIOS
-----------------------------------
At minimum, attempt to falsify the invariants with these local scenarios:

  1. Same outpoint relayed twice with byte-different but semantically related
     proofs.

  2. Deposit accepted, followed by a Bitcoin reorg that removes its block,
     followed by re-inclusion in a different block.

  3. Deposit at exactly the minimum confirmation boundary.

  4. Deposit at exactly the maximum-age boundary.

  5. Deposit using every accepted legacy and current destination commitment
     format, changing one committed field at a time.

  6. Claimed output index near integer and vector boundaries.

  7. Merkle proof with duplicated leaves, mutated branches, superfluous hashes,
     inconsistent transaction counts, or multiple possible interpretations.

  8. Header fork with more headers but less work, and fewer headers but more
     work.

  9. Difficulty-transition and median-time-past boundary cases.

 10. Reorg deeper than retained header or deposit history.

 11. Capacity reached by multiple transactions in alternate block order.

 12. Validator-set change while a checkpoint is collecting signatures.

 13. Validator removed after contributing a signature.

 14. Duplicate or malicious extended public keys and derivation indices.

 15. Exactly-threshold and threshold-plus/minus-one voting power.

 16. Checkpoint with maximum inputs, maximum withdrawals, dust change, and high
     fee.

 17. Checkpoint replaced, delayed, or removed by reorg.

 18. Signer crash between nonce generation, commitment publication, share
     generation, and completion.

 19. Signer disk restored from a snapshot taken before a completed signing
     session.

 20. FROST messages replayed across epoch, chain, checkpoint, or build version.

 21. IBC packet success followed by timeout delivery.

 22. IBC timeout followed by forged or delayed success acknowledgement.

 23. Same packet delivered through duplicate relayers.

 24. Channel closed and a new channel opened with superficially similar
     identifiers.

 25. ICS-23 proof valid for the wrong store, neighboring key, prior height,
     wrong revision, or different client.

 26. Protobuf value decoded as an unintended type.

 27. Migration while deposits, withdrawals, signatures, and IBC packets are
     pending simultaneously.

 28. Debug and release nodes process the same adversarial block.

 29. x86_64 and ARM64 nodes process the same randomized transition corpus.

 30. Failure injected immediately after each state write in a deposit,
     checkpoint, IBC, DKG, and migration path.

FINDING QUALITY BAR
-------------------
Do not call something a vulnerability solely because:

  - A comment is concerning
  - A scanner reports a warning
  - A panic exists only in unreachable initialization code
  - A theoretical attack violates an explicit and necessary trust assumption
  - The behavior is intentional and cannot produce security impact
  - Exploitation requires control already sufficient to perform the same impact
  - The observation applies only to unsupported local configuration

Classify each item as one of:

  - Confirmed vulnerability
  - Likely vulnerability requiring one specified verification step
  - Defense-in-depth weakness
  - Documentation/configuration mismatch
  - Test gap
  - Rejected hypothesis

A confirmed finding requires:

  - Exact affected commit
  - Exact files, symbols, and line ranges
  - Reachable call path
  - Attacker model and prerequisites
  - Violated invariant
  - Root cause
  - Deterministic local reproduction
  - Expected versus actual result
  - Security impact
  - Explanation of why existing checks do not stop it
  - Minimal remediation
  - Regression test
  - Search for variants and sibling call sites

SEVERITY MODEL
--------------
Use impact and realistic exploitability, not dramatic wording.

CRITICAL examples:
  - Unauthorized reserve BTC spend
  - Unbacked nBTC creation at material scale
  - Threshold-key extraction
  - Consensus acceptance of invalid Bitcoin custody state
  - Remotely triggerable deterministic consensus split
  - Irrecoverable loss of most reserve assets

HIGH examples:
  - Double-crediting under realistic conditions
  - Permanent locking of significant funds
  - Forged IBC mint/refund
  - Bypass of a material signing threshold
  - Cheap remotely triggerable persistent chain halt
  - Recovery mechanism failure under its stated threat model

MEDIUM examples:
  - Bounded asset loss
  - Expensive or temporary liveness failure
  - Exploitable state growth
  - Vulnerability requiring substantial privileged collusion below the intended
    security threshold
  - Material degradation of reorg or recovery safety

LOW or INFORMATIONAL:
  - Hardening gaps with limited concrete impact
  - Non-sensitive logging
  - Documentation mismatch without security consequence
  - Missing tests where no defect has been demonstrated

For each severity, explain:
  - Maximum impact
  - Practical impact
  - Required privileges
  - Required timing
  - Attacker cost
  - Detectability
  - Recoverability
  - Whether the attack can be repeated

OUTPUT FORMAT
-------------
Produce one final private audit package with these sections:

1. EXECUTIVE SUMMARY
   - Target commit
   - Overall risk
   - Number of confirmed findings by severity
   - Principal solvency and custody conclusions
   - Principal consensus and IBC conclusions
   - Unresolved high-risk questions

2. SCOPE AND REPRODUCIBILITY
   - Repositories and revisions
   - Feature profiles
   - Toolchain
   - Test environment
   - Commands executed
   - Excluded components
   - Known limitations

3. ARCHITECTURE AND TRUST MODEL
   - Component map
   - Data-flow map
   - Privilege map
   - Cross-chain finality assumptions
   - Signing and recovery assumptions

4. INVARIANT LEDGER
   For every invariant:
   - Status: proven, tested, partially tested, falsified, or untested
   - Evidence
   - Test or model identifier
   - Remaining uncertainty

5. ATTACK-SURFACE MATRIX
   Columns:
   - Entry point
   - Actor
   - Data controlled
   - Validation
   - State affected
   - Economic effect
   - Resource effect
   - Relevant tests
   - Findings

6. CONFIRMED FINDINGS
   Use this template for every finding:

   ID:
   Title:
   Severity:
   Confidence:
   CWE or relevant weakness class:
   Affected commit:
   Affected files and symbols:
   Attacker:
   Preconditions:
   Violated invariant:
   Summary:
   Root cause:
   Reachable execution path:
   Expected behavior:
   Actual behavior:
   Security impact:
   Practical exploitability:
   Local reproduction:
   Minimal proof of concept:
   Logs/state diff/transaction trace:
   Why existing protections fail:
   Recommended fix:
   Suggested patch:
   Regression test:
   Variant analysis:
   Operational mitigation:
   Disclosure sensitivity:

7. DEFENSE-IN-DEPTH ISSUES

8. REJECTED HYPOTHESES
   Document serious theories investigated and the evidence that rejected them.
   This prevents duplicate work and demonstrates review depth.

9. TEST AND COVERAGE REPORT
   - Tests added
   - Fuzz targets
   - Corpus
   - Runtime and executions
   - Code and state-transition coverage
   - Sanitizers or interpreters
   - Differential-test results
   - Architecture/build matrix results

10. DEPENDENCY AND SUPPLY-CHAIN REPORT

11. PRIORITIZED REMEDIATION PLAN
   Order fixes according to:
   - Custody and solvency risk
   - Consensus risk
   - Ease of exploitation
   - Ease of safe deployment
   - Migration requirements
   - Need for signer or operator coordination

12. RESIDUAL RISK
   Clearly state what was not proven and which assumptions still protect funds.

RESEARCH CONDUCT
----------------
Keep a timestamped research log. Preserve commit hashes and deterministic test
seeds. Minimize sensitive data. Do not include real keys or public endpoint
details. Do not create a public issue for a suspected vulnerability.

Stop any experiment that unexpectedly reaches public infrastructure, uses a
real asset, or risks affecting another party. Preserve local evidence and
continue only in an isolated environment.

FINAL DIRECTIVE
---------------
Work from invariants and end-to-end state transitions rather than a generic
Rust checklist. Trace every accepted Bitcoin satoshi from proof validation to
nBTC issuance, every burned nBTC unit to a checkpoint output, every checkpoint
output to the exact threshold authorization, and every cross-chain transition
through acknowledgement or timeout.

Do not merely search for crashes. Search for state that is valid to one
component but invalid to another, especially across:

  Bitcoin <-> Nomic
  Nomic consensus <-> signer software
  Rust objects <-> serialized state
  Nomic <-> IBC counterparty
  Current code <-> legacy state
  Pre-upgrade <-> post-upgrade code
  Documentation <-> actual threshold and configuration

The strongest result is not a long list of warnings. It is a reproducible
answer to whether Nomic preserves Bitcoin custody, nBTC solvency, consensus
determinism, and recoverability under hostile inputs and realistic component
failures.