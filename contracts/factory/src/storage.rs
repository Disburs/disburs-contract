use soroban_sdk::{contracttype, Address, BytesN, Env, String};

use crate::errors::Error;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Who may deploy and update the wasm hash (Disburs).
    Owner,
    /// Hash of the uploaded payroll wasm to deploy from.
    WasmHash,
    /// Number of client contracts deployed so far.
    DeployedCount,
    /// The i-th deployed client contract address.
    Deployed(u32),
    /// The code version this factory runs (set at deploy, bumped by `upgrade`).
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

pub fn set_owner(env: &Env, owner: &Address) {
    env.storage().instance().set(&DataKey::Owner, owner);
}

pub fn get_owner(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Owner)
        .ok_or(Error::NotInitialized)
}

pub fn set_wasm_hash(env: &Env, hash: &BytesN<32>) {
    env.storage().instance().set(&DataKey::WasmHash, hash);
}

pub fn get_wasm_hash(env: &Env) -> Result<BytesN<32>, Error> {
    env.storage()
        .instance()
        .get(&DataKey::WasmHash)
        .ok_or(Error::NotInitialized)
}

pub fn deployed_count(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::DeployedCount)
        .unwrap_or(0)
}

pub fn deployed_at(env: &Env, index: u32) -> Option<Address> {
    env.storage().persistent().get(&DataKey::Deployed(index))
}

pub fn record_deployment(env: &Env, deployed: &Address) {
    let n = deployed_count(env);
    env.storage()
        .persistent()
        .set(&DataKey::Deployed(n), deployed);
    env.storage()
        .instance()
        .set(&DataKey::DeployedCount, &(n + 1));
}
