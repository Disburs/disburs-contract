#![cfg(test)]
//! The factory upgrades in place too, keeping its owner, wasm hash and the
//! record of deployed client contracts. `fixtures/factory-<version>.wasm` is
//! the previous release; copy each release's wasm there after bumping VERSION.

use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String};

use crate::{FactoryContract, FactoryContractClient};

mod payroll {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/disburs_payroll.wasm");
}
mod previous {
    soroban_sdk::contractimport!(file = "fixtures/factory-0.2.0.wasm");
}
mod current {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/disburs_factory.wasm");
}

#[test]
fn upgrade_from_previous_release_keeps_owner_hash_and_deployments() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let issuer = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(issuer).address();
    let payroll_hash = env.deployer().upload_contract_wasm(payroll::WASM);
    let factory = env.register(previous::WASM, (owner.clone(), payroll_hash.clone()));
    let client = FactoryContractClient::new(&env, &factory);

    let admin = Address::generate(&env);
    let deployed = client.deploy(&BytesN::from_array(&env, &[7; 32]), &admin, &token);

    let new_hash = env.deployer().upload_contract_wasm(current::WASM);
    client.upgrade(&new_hash, &String::from_str(&env, "9.9.9"));

    assert_eq!(client.version(), String::from_str(&env, "9.9.9"));
    assert_eq!(client.owner(), owner);
    assert_eq!(client.wasm_hash(), payroll_hash);
    assert_eq!(client.deployed_count(), 1);
    assert_eq!(client.deployed_at(&0), Some(deployed));
    // Still deploys after the upgrade.
    client.deploy(&BytesN::from_array(&env, &[8; 32]), &admin, &token);
    assert_eq!(client.deployed_count(), 2);
}

#[test]
fn fresh_factory_reports_version_and_migrates_once() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let payroll_hash = env.deployer().upload_contract_wasm(payroll::WASM);
    let factory = env.register(FactoryContract, (owner, payroll_hash));
    let client = FactoryContractClient::new(&env, &factory);
    assert_eq!(client.version(), String::from_str(&env, crate::VERSION));
    client.migrate();
    assert_eq!(
        client.try_migrate().err().unwrap().unwrap(),
        crate::errors::Error::AlreadyMigrated
    );
}
