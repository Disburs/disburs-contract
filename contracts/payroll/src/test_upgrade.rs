#![cfg(test)]
//! In-place upgrades: code changes, data stays.
//!
//! `fixtures/payroll-<version>.wasm` is the previous release as built. Every
//! release copies its wasm there after bumping `VERSION`, so this test always
//! upgrades from the version clients actually run to the current build.

use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env, String, Vec};

use crate::{Payment, PayrollContract, PayrollContractClient};

/// The previous release, as deployed to clients.
mod previous {
    soroban_sdk::contractimport!(file = "fixtures/payroll-0.3.0.wasm");
}

/// The current build, as it will be uploaded.
mod current {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/disburs_payroll.wasm");
}

fn env_with_token() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let issuer = Address::generate(&env);
    let token_address = env.register_stellar_asset_contract_v2(issuer).address();
    token::StellarAssetClient::new(&env, &token_address).mint(&admin, &10_000);
    (env, admin, token_address)
}

fn run_id(env: &Env, n: u8) -> BytesN<32> {
    BytesN::from_array(env, &[n; 32])
}

#[test]
fn upgrade_from_previous_release_keeps_every_record() {
    let (env, admin, token_address) = env_with_token();
    // Deploy the previous release exactly as a client has it.
    let contract_id = env.register(previous::WASM, (admin.clone(), token_address.clone()));
    let client = PayrollContractClient::new(&env, &contract_id);

    let worker = Address::generate(&env);
    let other = Address::generate(&env);
    client.set_salary(&worker, &250);
    client.add_worker(&other);
    client.deposit(&admin, &1_000);
    let payments = Vec::from_array(
        &env,
        [
            Payment {
                worker: worker.clone(),
                amount: 250,
            },
            Payment {
                worker: other.clone(),
                amount: 100,
            },
        ],
    );
    client.pay_batch(&contract_id, &run_id(&env, 1), &payments);
    let balance_before = client.treasury_balance();

    // Upgrade to the current build.
    let new_hash = env.deployer().upload_contract_wasm(current::WASM);
    client.upgrade(&new_hash, &String::from_str(&env, "9.9.9"));

    // Same address, new code, every record intact.
    assert_eq!(client.version(), String::from_str(&env, "9.9.9"));
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.salary_of(&worker), 250);
    assert_eq!(client.workers().len(), 2);
    assert!(client.is_worker(&other));
    assert_eq!(client.run_paid(&run_id(&env, 1)), Some(350));
    assert_eq!(client.treasury_balance(), balance_before);
    // Still refuses to pay the same run twice after the upgrade.
    assert!(client
        .try_pay_batch(&contract_id, &run_id(&env, 1), &payments)
        .is_err());
    // And still works for new runs.
    client.pay_batch(&contract_id, &run_id(&env, 2), &payments);
    assert_eq!(client.treasury_balance(), balance_before - 350);
}

#[test]
fn a_fresh_contract_reports_the_current_version() {
    let (env, admin, token_address) = env_with_token();
    let contract_id = env.register(PayrollContract, (admin, token_address));
    let client = PayrollContractClient::new(&env, &contract_id);
    assert_eq!(client.version(), String::from_str(&env, crate::VERSION));
}

#[test]
fn migrate_runs_once_per_version() {
    let (env, admin, token_address) = env_with_token();
    let contract_id = env.register(PayrollContract, (admin, token_address));
    let client = PayrollContractClient::new(&env, &contract_id);

    client.migrate();
    assert_eq!(
        client.try_migrate().err().unwrap().unwrap(),
        crate::errors::Error::AlreadyMigrated
    );

    // A new version opens a new migration window.
    let new_hash = env.deployer().upload_contract_wasm(current::WASM);
    client.upgrade(&new_hash, &String::from_str(&env, "9.9.9"));
    client.migrate();
    assert!(client.try_migrate().is_err());
}

#[test]
fn upgrade_rejects_an_empty_version() {
    let (env, admin, token_address) = env_with_token();
    let contract_id = env.register(PayrollContract, (admin, token_address));
    let client = PayrollContractClient::new(&env, &contract_id);
    let new_hash = env.deployer().upload_contract_wasm(current::WASM);
    assert_eq!(
        client
            .try_upgrade(&new_hash, &String::from_str(&env, ""))
            .err()
            .unwrap()
            .unwrap(),
        crate::errors::Error::InvalidVersion
    );
    assert_eq!(client.version(), String::from_str(&env, crate::VERSION));
}

#[test]
#[should_panic]
fn only_the_admin_can_upgrade() {
    let env = Env::default();
    // No auth is mocked: an upgrade without the admin's signature must fail.
    let admin = Address::generate(&env);
    let issuer = Address::generate(&env);
    let token_address = env.register_stellar_asset_contract_v2(issuer).address();
    let contract_id = env.register(PayrollContract, (admin, token_address));
    let client = PayrollContractClient::new(&env, &contract_id);
    let new_hash = env.deployer().upload_contract_wasm(current::WASM);
    client.upgrade(&new_hash, &String::from_str(&env, "9.9.9"));
}
