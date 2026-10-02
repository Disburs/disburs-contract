#![no_std]
//! Disburs payroll contract.
//!
//! One contract per client (deployed through the factory). An employer (admin)
//! keeps a registry of workers and pays a whole run in one atomic call
//! (`pay_batch`), either from funds parked in the contract or pulled from the
//! employer's wallet in the same transaction. Every payment emits an event the
//! backend reconciles against. Per-worker salaries and single `pay` remain for
//! simple use. Conditional release (escrow) and zero-knowledge privacy come in
//! later phases and build on this.

mod errors;
mod storage;

#[cfg(test)]
mod test;
#[cfg(test)]
mod test_upgrade;

use soroban_sdk::{
    contract, contractimpl, contractmeta, contracttype, symbol_short, token, Address, BytesN, Env,
    String, Vec,
};

use crate::errors::Error;

/// The code version. Bump it with every release; `upgrade` records the new
/// label on chain so the backend can see which version each client runs.
pub const VERSION: &str = "0.3.0";

contractmeta!(key = "version", val = "0.3.0");
contractmeta!(
    key = "description",
    val = "Disburs payroll: fund a treasury and pay workers in a token on Stellar."
);
contractmeta!(key = "license", val = "MIT");

/// The most payments one `pay_batch` may carry. Stellar's per-transaction
/// resource limits bound this well below the number; 100 keeps a batch
/// comfortably inside them and matches the backend's chunk size.
pub const MAX_BATCH: u32 = 100;

/// One line of a run: who gets paid and how much (token smallest unit).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Payment {
    pub worker: Address,
    pub amount: i128,
}

#[contract]
pub struct PayrollContract;

#[contractimpl]
impl PayrollContract {
    /// Initialize with the employer/admin and the payout token address.
    pub fn __constructor(env: Env, admin: Address, token: Address) {
        storage::set_admin(&env, &admin);
        storage::set_token(&env, &token);
        storage::set_version(&env, &String::from_str(&env, VERSION));
    }

    /// Fund the treasury: transfer `amount` of the token from `from` into the
    /// contract. Anyone can top up, but they must authorize the transfer.
    pub fn deposit(env: Env, from: Address, amount: i128) -> Result<(), Error> {
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        from.require_auth();
        let token = storage::get_token(&env)?;
        token::TokenClient::new(&env, &token).transfer(
            &from,
            &env.current_contract_address(),
            &amount,
        );
        env.events()
            .publish((symbol_short!("deposit"), from), amount);
        Ok(())
    }

    /// Admin-only: set (or update) a worker's salary. Registers the worker.
    pub fn set_salary(env: Env, worker: Address, amount: i128) -> Result<(), Error> {
        storage::get_admin(&env)?.require_auth();
        if amount < 0 {
            return Err(Error::InvalidAmount);
        }
        storage::set_salary(&env, &worker, amount);
        storage::add_worker(&env, &worker);
        Ok(())
    }

    /* ------------------------------ registry ------------------------------ */

    /// Admin-only: add a worker to the registry. Returns true if newly added.
    pub fn add_worker(env: Env, worker: Address) -> Result<bool, Error> {
        storage::get_admin(&env)?.require_auth();
        let added = storage::add_worker(&env, &worker);
        if added {
            env.events()
                .publish((symbol_short!("worker"), symbol_short!("added")), worker);
        }
        Ok(added)
    }

    /// Admin-only: remove a worker and clear their salary. Returns true if
    /// they were registered.
    pub fn remove_worker(env: Env, worker: Address) -> Result<bool, Error> {
        storage::get_admin(&env)?.require_auth();
        storage::remove_salary(&env, &worker);
        let removed = storage::remove_worker(&env, &worker);
        if removed {
            env.events()
                .publish((symbol_short!("worker"), symbol_short!("removed")), worker);
        }
        Ok(removed)
    }

    /// Every registered worker, in registration order.
    pub fn workers(env: Env) -> Vec<Address> {
        storage::workers(&env)
    }

    /// Whether an address is a registered worker.
    pub fn is_worker(env: Env, worker: Address) -> bool {
        storage::is_worker(&env, &worker)
    }

    /* ------------------------------ batch pay ----------------------------- */

    /// Admin-only: pay a whole run in one atomic call.
    ///
    /// `from` is where the money comes from. If it is this contract, the run
    /// is paid from funds parked here by `deposit`. Otherwise `from` (the
    /// employer's wallet) authorizes the call too and the run's total is
    /// pulled in first, so nothing is parked between runs. Either way the
    /// whole batch settles or none of it does. Each payment emits
    /// `("pay", run_id, worker) amount`; the run emits `("run", run_id) total`.
    /// `run_id` makes the call idempotent: paying the same run twice fails
    /// with `RunAlreadyPaid`. Workers paid are added to the registry.
    pub fn pay_batch(
        env: Env,
        from: Address,
        run_id: BytesN<32>,
        payments: Vec<Payment>,
    ) -> Result<i128, Error> {
        storage::get_admin(&env)?.require_auth();
        if payments.is_empty() {
            return Err(Error::EmptyBatch);
        }
        if payments.len() > MAX_BATCH {
            return Err(Error::BatchTooLarge);
        }
        if storage::run_paid(&env, &run_id).is_some() {
            return Err(Error::RunAlreadyPaid);
        }

        let mut total: i128 = 0;
        for p in payments.iter() {
            if p.amount <= 0 {
                return Err(Error::InvalidAmount);
            }
            total = total.checked_add(p.amount).ok_or(Error::InvalidAmount)?;
        }

        let token = storage::get_token(&env)?;
        let client = token::TokenClient::new(&env, &token);
        let treasury = env.current_contract_address();

        if from != treasury {
            from.require_auth();
            if client.balance(&from) < total {
                return Err(Error::InsufficientTreasury);
            }
            client.transfer(&from, &treasury, &total);
        }
        if client.balance(&treasury) < total {
            return Err(Error::InsufficientTreasury);
        }

        for p in payments.iter() {
            client.transfer(&treasury, &p.worker, &p.amount);
            storage::add_worker(&env, &p.worker);
            env.events().publish(
                (symbol_short!("pay"), run_id.clone(), p.worker.clone()),
                p.amount,
            );
        }
        storage::set_run_paid(&env, &run_id, total);
        env.events().publish((symbol_short!("run"), run_id), total);
        Ok(total)
    }

    /// The total a run id moved, if it has been paid.
    pub fn run_paid(env: Env, run_id: BytesN<32>) -> Option<i128> {
        storage::run_paid(&env, &run_id)
    }

    /// Admin-only: pay a worker their configured salary from the treasury.
    pub fn pay(env: Env, worker: Address) -> Result<(), Error> {
        storage::get_admin(&env)?.require_auth();

        let amount = storage::get_salary(&env, &worker);
        if amount <= 0 {
            return Err(Error::NoSalarySet);
        }
        let token = storage::get_token(&env)?;
        let client = token::TokenClient::new(&env, &token);
        let treasury = env.current_contract_address();
        if client.balance(&treasury) < amount {
            return Err(Error::InsufficientTreasury);
        }
        client.transfer(&treasury, &worker, &amount);
        env.events().publish((symbol_short!("pay"), worker), amount);
        Ok(())
    }

    /// Admin-only: pull unused funds back out of the treasury to `to`.
    pub fn withdraw(env: Env, to: Address, amount: i128) -> Result<(), Error> {
        storage::get_admin(&env)?.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let token = storage::get_token(&env)?;
        let client = token::TokenClient::new(&env, &token);
        let treasury = env.current_contract_address();
        if client.balance(&treasury) < amount {
            return Err(Error::InsufficientTreasury);
        }
        client.transfer(&treasury, &to, &amount);
        env.events()
            .publish((symbol_short!("withdraw"), to), amount);
        Ok(())
    }

    /// Token balance held by the treasury.
    pub fn treasury_balance(env: Env) -> Result<i128, Error> {
        let token = storage::get_token(&env)?;
        Ok(token::TokenClient::new(&env, &token).balance(&env.current_contract_address()))
    }

    /// A worker's configured salary (0 if unset).
    pub fn salary_of(env: Env, worker: Address) -> i128 {
        storage::get_salary(&env, &worker)
    }

    /// The current admin address.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env)
    }

    /// Admin-only: hand admin rights to a new address.
    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        storage::get_admin(&env)?.require_auth();
        storage::set_admin(&env, &new_admin);
        Ok(())
    }

    /* ------------------------------- upgrade ------------------------------ */

    /// The code version this contract runs. Contracts deployed before
    /// versions were recorded report "0.2.0".
    pub fn version(env: Env) -> String {
        storage::get_version(&env).unwrap_or_else(|| String::from_str(&env, "0.2.0"))
    }

    /// Admin-only: replace this contract's code in place with an uploaded
    /// wasm, keeping every balance, worker, salary and paid run. The new
    /// code takes effect from the next invocation. `new_version` is the
    /// label the upgraded contract reports; it is written here so the new
    /// code finds it. Emits `("upgraded", version) wasm_hash`.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>, new_version: String) -> Result<(), Error> {
        storage::get_admin(&env)?.require_auth();
        if new_version.is_empty() {
            return Err(Error::InvalidVersion);
        }
        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());
        storage::set_version(&env, &new_version);
        env.events()
            .publish((symbol_short!("upgraded"), new_version), new_wasm_hash);
        Ok(())
    }

    /// Admin-only, once per version: run the data changes a new version
    /// needs after `upgrade`. A version with nothing to migrate still records
    /// that it ran, so a second call fails with `AlreadyMigrated`. Emits
    /// `("migrated", version)`.
    pub fn migrate(env: Env) -> Result<(), Error> {
        storage::get_admin(&env)?.require_auth();
        let current = Self::version(env.clone());
        if storage::get_migrated_to(&env).as_ref() == Some(&current) {
            return Err(Error::AlreadyMigrated);
        }
        // Per-version data migrations go here, keyed on `current`. None so far.
        storage::set_migrated_to(&env, &current);
        env.events()
            .publish((symbol_short!("migrated"), current), ());
        Ok(())
    }
}
