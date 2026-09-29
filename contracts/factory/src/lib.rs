#![no_std]
//! Disburs payroll factory.
//!
//! Deploys one isolated payroll contract per client from a single uploaded
//! wasm, so every client's treasury is its own contract with its own admin.
//! The factory's owner (Disburs) is the only one who may deploy, and can point
//! the factory at a newer payroll wasm as the contract evolves; existing
//! client contracts are untouched by that. Every deployment is recorded so
//! the set of client contracts is enumerable on chain.

mod errors;
mod storage;

#[cfg(test)]
mod test;

use soroban_sdk::{contract, contractimpl, contractmeta, symbol_short, Address, BytesN, Env};

use crate::errors::Error;

contractmeta!(key = "version", val = "0.1.0");
contractmeta!(
    key = "description",
    val = "Disburs factory: deploy one payroll contract per client."
);
contractmeta!(key = "license", val = "MIT");

#[contract]
pub struct FactoryContract;

#[contractimpl]
impl FactoryContract {
    /// Initialize with the owner (who may deploy) and the hash of the
    /// uploaded payroll wasm to deploy from.
    pub fn __constructor(env: Env, owner: Address, payroll_wasm_hash: BytesN<32>) {
        storage::set_owner(&env, &owner);
        storage::set_wasm_hash(&env, &payroll_wasm_hash);
    }

    /// Owner-only: deploy a payroll contract for a client. `salt` makes the
    /// address deterministic per client (the backend uses a hash of the
    /// organization id); `admin` and `token` are passed to the payroll
    /// contract's constructor. Returns the new contract's address.
    pub fn deploy(
        env: Env,
        salt: BytesN<32>,
        admin: Address,
        token: Address,
    ) -> Result<Address, Error> {
        storage::get_owner(&env)?.require_auth();
        let wasm_hash = storage::get_wasm_hash(&env)?;
        let deployed = env
            .deployer()
            .with_current_contract(salt)
            .deploy_v2(wasm_hash, (admin.clone(), token));
        storage::record_deployment(&env, &deployed);
        env.events()
            .publish((symbol_short!("deployed"), admin), deployed.clone());
        Ok(deployed)
    }

    /// Owner-only: deploy from a newer payroll wasm from now on.
    pub fn set_wasm_hash(env: Env, payroll_wasm_hash: BytesN<32>) -> Result<(), Error> {
        storage::get_owner(&env)?.require_auth();
        storage::set_wasm_hash(&env, &payroll_wasm_hash);
        Ok(())
    }

    /// The payroll wasm hash new deployments use.
    pub fn wasm_hash(env: Env) -> Result<BytesN<32>, Error> {
        storage::get_wasm_hash(&env)
    }

    /// The factory owner.
    pub fn owner(env: Env) -> Result<Address, Error> {
        storage::get_owner(&env)
    }

    /// Owner-only: hand the factory to a new owner.
    pub fn set_owner(env: Env, new_owner: Address) -> Result<(), Error> {
        storage::get_owner(&env)?.require_auth();
        storage::set_owner(&env, &new_owner);
        Ok(())
    }

    /// How many client contracts this factory has deployed.
    pub fn deployed_count(env: Env) -> u32 {
        storage::deployed_count(&env)
    }

    /// The i-th deployed client contract (0-based), if any.
    pub fn deployed_at(env: Env, index: u32) -> Option<Address> {
        storage::deployed_at(&env, index)
    }
}
