use soroban_sdk::{contracttype, Address, BytesN, Env, String, Vec};

use crate::errors::Error;

/// Keys for contract storage.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// The employer / admin address.
    Admin,
    /// The payout token (e.g. a USDC Stellar Asset Contract address).
    Token,
    /// A worker's configured salary, keyed by their address.
    Salary(Address),
    /// The registry: every worker address this treasury knows, enumerable.
    Workers,
    /// A run id that has been paid, with the total it moved. Makes
    /// `pay_batch` idempotent: a retry of the same run pays nothing.
    RunPaid(BytesN<32>),
    /// The code version this contract runs (set at deploy, bumped by `upgrade`).
    Version,
    /// The version whose post-upgrade `migrate` has already run.
    MigratedTo,
}

pub fn set_version(env: &Env, version: &String) {
    env.storage().instance().set(&DataKey::Version, version);
}

pub fn get_version(env: &Env) -> Option<String> {
    env.storage().instance().get(&DataKey::Version)
}

pub fn set_migrated_to(env: &Env, version: &String) {
    env.storage().instance().set(&DataKey::MigratedTo, version);
}

pub fn get_migrated_to(env: &Env) -> Option<String> {
    env.storage().instance().get(&DataKey::MigratedTo)
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn get_admin(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)
}

pub fn set_token(env: &Env, token: &Address) {
    env.storage().instance().set(&DataKey::Token, token);
}

pub fn get_token(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Token)
        .ok_or(Error::NotInitialized)
}

pub fn set_salary(env: &Env, worker: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::Salary(worker.clone()), &amount);
}

pub fn get_salary(env: &Env, worker: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Salary(worker.clone()))
        .unwrap_or(0)
}

pub fn remove_salary(env: &Env, worker: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::Salary(worker.clone()));
}

pub fn workers(env: &Env) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&DataKey::Workers)
        .unwrap_or_else(|| Vec::new(env))
}

fn set_workers(env: &Env, workers: &Vec<Address>) {
    env.storage().persistent().set(&DataKey::Workers, workers);
}

/// Add a worker to the registry if absent. Returns true if it was added.
pub fn add_worker(env: &Env, worker: &Address) -> bool {
    let mut all = workers(env);
    if all.contains(worker) {
        return false;
    }
    all.push_back(worker.clone());
    set_workers(env, &all);
    true
}

/// Remove a worker from the registry. Returns true if it was present.
pub fn remove_worker(env: &Env, worker: &Address) -> bool {
    let all = workers(env);
    match all.first_index_of(worker) {
        Some(i) => {
            let mut next = all;
            next.remove(i);
            set_workers(env, &next);
            true
        }
        None => false,
    }
}

pub fn is_worker(env: &Env, worker: &Address) -> bool {
    workers(env).contains(worker)
}

pub fn run_paid(env: &Env, run_id: &BytesN<32>) -> Option<i128> {
    env.storage()
        .persistent()
        .get(&DataKey::RunPaid(run_id.clone()))
}

pub fn set_run_paid(env: &Env, run_id: &BytesN<32>, total: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::RunPaid(run_id.clone()), &total);
}
