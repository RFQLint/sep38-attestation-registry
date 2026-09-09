# sep38-attestation-registry

A minimal Soroban smart contract that stores on-chain attestations of
[SEP-38](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0038.md)
quote server conformance checks — a durable, publicly queryable answer to
"is this anchor's SEP-38 quote server currently verified?" that doesn't
depend on trusting whoever runs the checker.

Part of a four-repo project mirroring
[`sep24-attestation-registry`](https://github.com/SEP-24-conform/sep24-attestation-registry)
and
[`sep31-attestation-registry`](https://github.com/sep31-conformance/sep31-attestation-registry)'s
exact pattern:

- [`sep38-conformance`](https://github.com/RFQLint/sep38-conformance) — the checking library + CLI. Produces the results this contract stores.
- **This repo** — the on-chain record.
- `sep38-conformance-backend` — the API service that runs the checker and writes to this contract.
- `sep38-conformance-frontend` — dashboard over that backend.

```mermaid
flowchart LR
    Anchor[(Anchor under test)]
    Lib[sep38-conformance<br/>library + CLI]
    BE[sep38-conformance-backend]
    subgraph This repo
        Contract[sep38-attestation-registry<br/>Soroban contract]
    end
    FE[sep38-conformance-frontend]

    Lib -->|GET stellar.toml, /info, /prices, /price| Anchor
    BE -->|runs| Lib
    BE -->|attest domain, passed, hash<br/>admin-signed| Contract
    Contract -->|get_attestation domain<br/>no auth required| BE
    FE -->|POST /api/checks, GET /api/registry/:domain/onchain| BE
```

This repo depends on nothing else in the project — pure Soroban contract
code with no knowledge of the checker or backend beyond the shape of the
data it's handed.

## Table of contents

- [Why this exists](#why-this-exists)
- [This contract vs. its two siblings](#this-contract-vs-its-two-siblings)
- [Trust model](#trust-model)
- [Data model](#data-model)
- [Interface reference](#interface-reference)
- [Sequence: from check to on-chain record](#sequence-from-check-to-on-chain-record)
- [Deployed instances](#deployed-instances)
- [Storage and TTL considerations](#storage-and-ttl-considerations)
- [Security considerations](#security-considerations)
- [Testing philosophy](#testing-philosophy)
- [Development](#development)
- [Deploying your own instance](#deploying-your-own-instance)
- [Design decisions](#design-decisions)
- [What this deliberately does not do](#what-this-deliberately-does-not-do)
- [FAQ](#faq)
- [Contributing](#contributing)
- [License](#license)

## Why this exists

`sep38-conformance` can tell you, right now, whether an anchor's SEP-38
quote server matches spec. But that result only exists wherever the check
happened to run. A wallet or another anchor deciding whether to trust a
quote server needs a durable, independently queryable answer instead of a
one-off terminal output. This contract is that: an off-chain backend runs
the real check, and only on a pass, submits a signed attestation here.

## This contract vs. its two siblings

The underlying problem — "durably record a pass/fail + hash for a domain,
signed by one admin key" — is identical regardless of which SEP is being
attested to, so this contract's Rust source is structurally the same as
[`sep24-attestation-registry`](https://github.com/SEP-24-conform/sep24-attestation-registry)'s
and
[`sep31-attestation-registry`](https://github.com/sep31-conformance/sep31-attestation-registry)'s.
Deliberate reuse of a proven, already-audited-in-full pattern across all
three, not duplicated effort by accident. What differs each time is only
the surrounding context: a separate deployment, a separate admin key, and
attestations that mean a different specific claim ("this domain's SEP-38
quote server conforms," here).

## Trust model

This contract does **not** run any checks itself and has no opinion on
what "conformant" means. It's a signed, timestamped bulletin board with
exactly one poster:

- **Trust the admin key** to only submit attestations reflecting real
  conformance runs. The admin is a single Stellar account, currently held
  by `sep38-conformance-backend`.
- Every write is a permanent, signed, publicly-visible Stellar
  transaction — the admin can overwrite what the *current* attestation
  for a domain says, but cannot rewrite the historical record of what it
  submitted and when.
- Because `sep38-conformance` is open source, anyone can independently
  re-run the same check and compare against `result_hash` — the admin's
  claims are falsifiable, not just asserted.
- If the admin key were compromised, an attacker could write false
  attestations until `set_admin` rotates to a new key — no multi-sig or
  governance layer over the admin role in this version, same as both
  sibling contracts.

```mermaid
flowchart LR
    subgraph Trusted
        A[Admin key]
    end
    subgraph Verifiable by anyone
        B[sep38-conformance source code]
        C[This contract's on-chain state]
        D[Ledger history of every attest tx]
    end
    A -->|writes| C
    B -.->|anyone can re-run the same check and compare| C
    C -.->|every write is a permanent, signed tx| D
```

## Data model

```rust
pub struct Attestation {
    pub timestamp: u64,       // ledger close time (unix seconds) when written
    pub passed: bool,         // whether the conformance run had zero failures
    pub result_hash: BytesN<32>,  // hash of the full report, e.g. SHA-256
}
```

A single map from domain (`String`) to its most recent `Attestation`,
overwritten on each new `attest` call — current status, not a history log.

## Interface reference

| Function | Auth required | Description |
|---|---|---|
| `initialize(admin: Address)` | — | One-time setup. Panics if already initialized. |
| `get_admin() -> Address` | — | Returns the current admin address. |
| `set_admin(new_admin: Address)` | current admin | Rotates the admin key. |
| `attest(domain: String, passed: bool, result_hash: BytesN<32>)` | admin | Records a conformance result for `domain`, overwriting any prior attestation for the same domain. |
| `get_attestation(domain: String) -> Option<Attestation>` | — | Reads the latest attestation for `domain`. `None` if never attested. |

## Sequence: from check to on-chain record

```mermaid
sequenceDiagram
    participant Backend as sep38-conformance-backend
    participant Checker as sep38-conformance (library)
    participant Anchor
    participant Contract as sep38-attestation-registry

    Backend->>Checker: runConformanceSuite(domain)
    Checker->>Anchor: GET stellar.toml, /info, /prices, /price
    Anchor-->>Checker: responses
    Checker-->>Backend: ConformanceReport
    alt every check passed
        Backend->>Backend: sha256(report) -> result_hash
        Backend->>Contract: attest(domain, true, result_hash)  [signed, admin auth]
        Contract-->>Backend: tx hash
    else any check failed
        Backend->>Backend: record failure locally, no on-chain write
    end
    Note over Contract: anyone can now call get_attestation(domain)<br/>and independently verify result_hash against a re-run report
```

## Deployed instances

| Network | Contract ID |
|---|---|
| Testnet | [`CDWRHACVHZ7EDIUKKA64H34KNNQBISWWF2CZWYL3DG36IR2BPJ2M3UBD`](https://stellar.expert/explorer/testnet/contract/CDWRHACVHZ7EDIUKKA64H34KNNQBISWWF2CZWYL3DG36IR2BPJ2M3UBD) |
| Mainnet | not yet deployed |

Round-trip verified for real on this testnet deployment. Worth noting
what was actually attested: `testanchor.stellar.org` was written with
`passed: false` — an honest reflection of the real, currently-reproducible
`/sep38/price` failure documented in
[`sep38-conformance`'s README](https://github.com/RFQLint/sep38-conformance#a-real-bug-this-tool-found-in-sdfs-own-reference-anchor),
not a placeholder value chosen for the demo.

## Storage and TTL considerations

Soroban ledger entries — including this contract's persistent storage —
are subject to a rent/TTL model: an entry that isn't extended can expire
and be archived, requiring an explicit restore to read again. This
contract uses `env.storage().persistent()` for attestation records, so
entries for domains that stop being re-checked will eventually approach
their TTL. No automatic extension happens here —
[`soroban-ttl-doctor`](https://github.com/soroban-doc-ttl/soroban-ttl-doctor),
a separate project built specifically for this risk, is a direct fit for
monitoring this contract's own instance and persistent entries.

## Security considerations

- **Single admin key** — the contract's main centralization point, by
  design, keeping it small enough to audit in full.
- **No re-entrancy or asset-custody surface** — this contract never
  holds, transfers, or has authority over any asset.
- **`domain` is an unvalidated string** — validation happens in
  `sep38-conformance-backend` before `attest` is ever called.
- **Admin rotation has no timelock** — same open trade-off as both
  sibling contracts.

## Testing philosophy

7 unit tests. The one worth calling out: `attest_fails_without_the_admins_authorization`.
`env.mock_all_auths()` (used in every other test here) bypasses every
`require_auth()` check regardless of caller, which means a test suite
that only ever uses it can reach 100% coverage on `attest()` without
proving the admin gate actually works. This test builds a fresh `Env`
with no blanket auth mock, so `attest`'s `admin.require_auth()` has
nothing backing it and must panic — the only way to prove the
authorization check is load-bearing rather than untested-but-assumed.

## Development

```sh
cargo test              # 7 unit tests, including the negative auth test above
stellar contract build   # -> target/wasm32v1-none/release/registry.wasm (~2.9KB)
```

## Deploying your own instance

```sh
stellar keys generate sep38-registry-admin --network testnet --fund
stellar contract build
stellar contract deploy \
  --wasm target/wasm32v1-none/release/registry.wasm \
  --source sep38-registry-admin --network testnet --alias registry
stellar contract invoke --id registry --source sep38-registry-admin --network testnet -- \
  initialize --admin "$(stellar keys address sep38-registry-admin)"
```

As with both sibling projects, the deploying key isn't necessarily the
key that should sign attestations day to day — rotate admin via
`set_admin` to a dedicated key held only by whatever backend operates
this instance.

## Design decisions

**Why Soroban and not a regular database?** A database the backend
controls is exactly the "trust a centralized list" problem this project
exists to avoid — putting the record on Stellar means the write is a
public, signed, timestamped transaction anyone can audit independently.

**Why not store the full report on-chain instead of a hash?** Cost and
boundedness — a hash is enough to verify a specific report matches what
was attested, at fixed, minimal cost.

**Why does this contract's source deliberately match both sibling
contracts instead of being written fresh?** The underlying problem is
identical regardless of which SEP is being attested to — see
[This contract vs. its two siblings](#this-contract-vs-its-two-siblings).
Writing a "fresh" version each time would only introduce a chance for all
three to silently diverge in behavior for no functional reason.

## What this deliberately does not do

- Run conformance checks itself (that's `sep38-conformance`'s job).
- Store more than the latest attestation per domain.
- Provide any reputation, scoring, or ranking beyond a single pass/fail
  bit.
- Enforce anything about what a "domain" string looks like.
- Provide governance, multi-sig, or timelock protection over the admin
  role.

## FAQ

**Why not reuse one of the sibling contracts' deployed instances for
SEP-38 attestations too, since the code is identical?** Because the three
record fundamentally different claims — a wallet or anchor querying this
contract needs to know unambiguously which SEP's conformance it's
reading. Separate deployments keep that unambiguous.

**What happens if the admin key is lost?** Nothing already-written is
lost — existing attestations remain readable forever. But no *new*
attestations can be written, since `set_admin` itself requires the
current admin's signature.

**Can anyone call `get_attestation`?** Yes — no authentication required.
See `sep38-conformance-backend`'s `/api/registry/:domain/onchain`
endpoint for a worked example of a trustless read.

## Contributing

Same extension points as both sibling repos (attestation history, richer
result data, admin governance) — worth linking issues across all three
rather than solving the same design question independently three times.

## License

Apache-2.0
