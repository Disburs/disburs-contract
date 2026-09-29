# Disburs Contracts

The on-chain money layer for Disburs. An employer funds a treasury that the
contract custodies, records what each contractor is owed, and releases those
funds in a token (USDC) on Stellar.

This repo starts small on purpose. The `payroll` contract is the whole thing
today; the privacy work — keeping salary amounts off the public ledger with
zero-knowledge proofs — is a later phase, described under [Where this is
going](#where-this-is-going).

---

## Getting set up

### What you need installed

Only the first two rows are needed to build, test, and deploy what's in this
repo today. Node.js and Circom belong to the zero-knowledge phase and aren't
used yet — they're listed so the full toolchain is in one place.

| Tool | Minimum Version | Used for |
|------|-----------------|----------|
| [Rust](https://rustup.rs/) | 1.81+ | Building and testing the contracts. The stable toolchain is pinned in `rust-toolchain.toml`, so rustup installs the right version (and the `wasm32v1-none` target) the first time you build. |
| [Stellar CLI](https://developers.stellar.org/docs/tools/stellar-cli) | v22+ | Compiling to wasm, deploying, and invoking on the network. This is the tool that used to be called the Soroban CLI. |
| [Node.js](https://nodejs.org/) | 18+ | Zero-knowledge phase only: runtime for the circom / snarkjs proof tooling. |
| [Circom](https://docs.circom.io/getting-started/installation/) | 2.1+ | Zero-knowledge phase only: compiling the proof circuits. |

Installing the two you need now:

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Stellar CLI — pick one
brew install stellar-cli            # macOS
cargo install --locked stellar-cli  # any platform
```

### Build and test

```bash
git clone <this-repo-url> disburs-contract
cd disburs-contract

cargo test            # runs the payroll unit tests
stellar contract build   # compiles to target/wasm32v1-none/release/disburs_payroll.wasm
```

`Cargo.lock` is committed, so everyone resolves the same dependency tree and the
build is reproducible. Common tasks are wrapped in the `Makefile` (`make build`,
`make test`, `make fmt`, `make clippy`, `make clean`).

The tests in `contracts/payroll/src/test.rs` run against a local ledger and a
real Stellar Asset Contract token with auth mocked — no network required. They
cover the deposit → set salary → pay path and the two failure cases (no salary
set, treasury too low).

---

## The contracts

Two contracts. The **factory** (`contracts/factory`) deploys one **payroll**
contract (`contracts/payroll`) per client from a single uploaded wasm, so
every client's treasury is its own contract with its own admin. Disburs owns
the factory; each client's treasury wallet is the admin of its own payroll
contract.

### Factory

| Function | Who can call | What it does |
|----------|--------------|--------------|
| `__constructor(owner, payroll_wasm_hash)` | deployer | Sets the owner and the payroll wasm to deploy from. |
| `deploy(salt, admin, token)` | owner | Deploys a payroll contract for a client (deterministic address per salt) and records it. Returns its address. |
| `set_wasm_hash(hash)` | owner | Deploy from a newer payroll wasm from now on; existing client contracts are untouched. |
| `wasm_hash()` / `owner()` / `set_owner(new)` | anyone / anyone / owner | Read the wasm hash, read or transfer ownership. |
| `deployed_count()` / `deployed_at(i)` | anyone | Enumerate every client contract deployed. |

### Payroll

One employer (the **admin**) and many workers. State lives on-chain: the admin
address, the payout token, a registry of workers, each worker's salary, and
which run ids have been paid.

| Function | Who can call | What it does |
|----------|--------------|--------------|
| `__constructor(admin, token)` | deployer | Sets the admin and payout token. Runs once, at deploy. |
| `deposit(from, amount)` | anyone | Moves `amount` of the token from `from` into the treasury. `from` authorizes the transfer. |
| `set_salary(worker, amount)` | admin | Sets or updates a worker's salary. |
| `pay(worker)` | admin | Pays the worker their salary from the treasury. Fails if no salary is set or the treasury is short. |
| `pay_batch(from, run_id, payments)` | admin (+ `from`) | Pays a whole run atomically: up to 100 `{worker, amount}` lines. If `from` is the employer's wallet, the total is pulled in first (nothing parked between runs); if it is the contract, parked funds are used. Emits `("pay", run_id, worker) amount` per line and `("run", run_id) total`. A `run_id` can only be paid once. |
| `add_worker(worker)` / `remove_worker(worker)` | admin | Maintain the registry (removal clears the salary). `set_salary` and `pay_batch` register workers too. |
| `workers()` / `is_worker(worker)` | anyone | Enumerate the registry / check membership. |
| `run_paid(run_id)` | anyone | The total a run id moved, if paid. |
| `withdraw(to, amount)` | admin | Reclaims unused treasury funds. |
| `treasury_balance()` | anyone | Token balance held by the contract. |
| `salary_of(worker)` | anyone | A worker's configured salary (0 if unset). |
| `get_admin()` | anyone | The current admin. |
| `set_admin(new_admin)` | admin | Transfers admin rights to a new address. |

Money moves through the SEP-41 token interface, so any compliant token works; in
production that's the USDC Stellar Asset Contract. Every state-changing call
authorizes the party that must consent — the admin for payroll operations, the
sender for deposits. Errors surface as stable `u32` codes: `NotInitialized (1)`,
`InvalidAmount (2)`, `NoSalarySet (3)`, `InsufficientTreasury (4)`,
`BatchTooLarge (5)`, `EmptyBatch (6)`, `RunAlreadyPaid (7)`.

`pay_batch` is atomic: if any line cannot be paid, nothing moves and the run id
stays unpaid, so the caller can retry. The Disburs backend calls it with the
treasury wallet as both admin and `from`, built on the wallet's own account and
fee-bumped by the sponsor, so wallets never need XLM.

---

## Deploying to testnet

The production shape is: upload the payroll wasm once, deploy the factory
pointing at it, then let the factory deploy one payroll contract per client.

```bash
# 1. an identity, funded by friendbot (Disburs uses its sponsor key here)
stellar keys generate deployer --network testnet
stellar keys fund deployer --network testnet

# 2. build and shrink both wasms
stellar contract build
for w in disburs_payroll disburs_factory; do
  stellar contract optimize --wasm target/wasm32v1-none/release/$w.wasm
done

# 3. upload the payroll wasm (prints its hash)
stellar contract upload \
  --wasm target/wasm32v1-none/release/disburs_payroll.optimized.wasm \
  --source deployer --network testnet

# 4. deploy the factory — constructor args come after the `--`
stellar contract deploy \
  --wasm target/wasm32v1-none/release/disburs_factory.optimized.wasm \
  --source deployer --network testnet \
  -- \
  --owner "$(stellar keys address deployer)" \
  --payroll_wasm_hash <WASM_HASH>

# 5. deploy a client's payroll contract through the factory
stellar contract invoke --id $FACTORY --source deployer --network testnet \
  -- deploy --salt <32_BYTES_HEX> --admin <CLIENT_TREASURY_ADDRESS> --token <TOKEN_CONTRACT_ADDRESS>
```

The current testnet factory is `CALV34YING3FXQW6MYFQHB2ZL53SWS6S3CMXQOYTG4NJ2SKAFETSMI43`
(payroll wasm `99012ba6…3841`); the backend reads it as `FACTORY_CONTRACT_ID`.

A payroll contract can also be deployed on its own for experiments:
`stellar contract deploy --wasm …/disburs_payroll.optimized.wasm … -- --admin <ADMIN> --token <TOKEN>`.
With a contract id exported as `CONTRACT`, a full run looks like:

```bash
# fund the treasury — amounts are in the token's smallest unit (USDC = 7 decimals, so 5000000 = 0.5 USDC)
stellar contract invoke --id $CONTRACT --source deployer --network testnet \
  -- deposit --from "$(stellar keys address deployer)" --amount 5000000

stellar contract invoke --id $CONTRACT --source deployer --network testnet \
  -- set_salary --worker <WORKER_ADDRESS> --amount 2000000

stellar contract invoke --id $CONTRACT --source deployer --network testnet \
  -- pay --worker <WORKER_ADDRESS>

stellar contract invoke --id $CONTRACT --source deployer --network testnet \
  -- treasury_balance

# or pay a whole run in one call, pulling the total from the admin's wallet
stellar contract invoke --id $CONTRACT --source deployer --network testnet \
  -- pay_batch --from "$(stellar keys address deployer)" --run_id <32_BYTES_HEX> \
  --payments '[{"worker":"<WORKER_ADDRESS>","amount":"2000000"}]'
```

You'll need a token contract address for `--token`: on testnet, use the USDC SAC
or deploy a throwaway token to test against.

---

## Repository layout

```
disburs-contract/
├── Cargo.toml              workspace: members = contracts/*
├── Cargo.lock              committed for reproducible builds
├── rust-toolchain.toml     pinned toolchain + wasm target
├── Makefile                build / test / fmt / clippy / clean
├── contracts/payroll/      one per client: treasury, registry, salaries, pay_batch
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs          entry points
│       ├── errors.rs       typed error codes
│       ├── storage.rs      storage keys + accessors
│       └── test.rs         unit tests
└── contracts/factory/      deploys a payroll contract per client from one wasm
    ├── Cargo.toml
    └── src/
        ├── lib.rs
        ├── errors.rs
        ├── storage.rs
        └── test.rs         deploys the real payroll wasm (build it first: make test)
```

Each contract keeps a thin `lib.rs` with `errors.rs`, `storage.rs`, and
`test.rs` alongside it. Contracts are `no_std`, and the release profile is tuned
for small wasm (`opt-level = "z"`, `lto`, `panic = "abort"`, stripped, with
`overflow-checks` left on).

---

## Phases to completion

The build ships in phases. Each one is usable on its own, and each new contract
lives under `contracts/*` and composes with the ones before it, so the pieces
stay small and independently testable. "Complete" means private payroll running
end to end.

### Phase 1 — Payroll core &nbsp;·&nbsp; ✅ shipped

The `payroll` contract in this repo: an employer funds a treasury, sets each
worker's salary, and pays them in a token. Auth on every mutation, typed errors,
tested. This is the foundation everything else builds on.

### Phase 2 — Registry, batch runs & per-client deployment &nbsp;·&nbsp; ✅ shipped

A worker registry (`add_worker`, `remove_worker`, `workers`), `pay_batch` that
settles a whole run atomically with a per-worker event and an idempotent run
id, and the `factory` contract that deploys one isolated payroll contract per
client. Tested for atomicity, partial-failure rejection, replay safety and the
batch cap.

### Phase 3 — Conditional releases

Release funds against milestones or approvals instead of a flat salary — the
money moves only when a condition is met, not just when the admin says so.

### Phase 4 — Private salaries &nbsp;·&nbsp; completion

The point of the whole thing: salary amounts as commitments rather than
plaintext, payouts proven valid without revealing the amount, and auditor
keys for selective disclosure. The default path is Stellar's Confidential
Token wrapper (OpenZeppelin; Noir circuits, UltraHonk verifier over the
BN254/Poseidon host functions), with our own shielded pool as the fallback
behind the same settlement interface. When this lands, payroll runs privately
end to end and the contract system is complete.

---

## A note on dependency pinning

`soroban-sdk` is held at `22.x` (builds on current stable Rust) and
`ed25519-dalek` at `2.2.0` — its `3.0.0` release breaks the SDK's test
utilities. Both are locked in `Cargo.lock`; keep it in git and `cargo build`
just works.

## License

MIT — see [LICENSE](LICENSE).
