#![cfg(test)]

use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env};

use crate::{FactoryContract, FactoryContractClient};

/// The payroll contract as built wasm, so the factory deploys the real thing.
/// Build it first: `cargo build -p disburs-payroll --target wasm32v1-none --release`.
mod payroll {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/disburs_payroll.wasm");
}

fn setup() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let issuer = Address::generate(&env);
    let token_address = env.register_stellar_asset_contract_v2(issuer).address();
    let wasm_hash = env.deployer().upload_contract_wasm(payroll::WASM);
    let factory = env.register(FactoryContract, (owner.clone(), wasm_hash));
    (env, owner, factory, token_address)
}

fn salt(env: &Env, n: u8) -> BytesN<32> {
    BytesN::from_array(env, &[n; 32])
}

#[test]
fn deploys_an_isolated_payroll_contract_per_client() {
    let (env, _owner, factory, token_address) = setup();
    let client = FactoryContractClient::new(&env, &factory);
    let acme = Address::generate(&env);
    let globex = Address::generate(&env);

    let acme_payroll = client.deploy(&salt(&env, 1), &acme, &token_address);
    let globex_payroll = client.deploy(&salt(&env, 2), &globex, &token_address);
    assert_ne!(acme_payroll, globex_payroll);
    assert_eq!(client.deployed_count(), 2);
    assert_eq!(client.deployed_at(&0), Some(acme_payroll.clone()));
    assert_eq!(client.deployed_at(&1), Some(globex_payroll.clone()));
    assert_eq!(client.deployed_at(&2), None);

    // Each contract is a real, initialized payroll contract with its own admin
    // and its own treasury.
    let acme_client = payroll::Client::new(&env, &acme_payroll);
    let globex_client = payroll::Client::new(&env, &globex_payroll);
    assert_eq!(acme_client.get_admin(), acme);
    assert_eq!(globex_client.get_admin(), globex);

    token::StellarAssetClient::new(&env, &token_address).mint(&acme, &1_000);
    acme_client.deposit(&acme, &400);
    assert_eq!(acme_client.treasury_balance(), 400);
    assert_eq!(
        globex_client.treasury_balance(),
        0,
        "treasuries are isolated"
    );
}

#[test]
fn the_same_salt_cannot_deploy_twice() {
    let (env, _owner, factory, token_address) = setup();
    let client = FactoryContractClient::new(&env, &factory);
    let acme = Address::generate(&env);
    client.deploy(&salt(&env, 9), &acme, &token_address);
    assert!(client
        .try_deploy(&salt(&env, 9), &acme, &token_address)
        .is_err());
}

#[test]
fn only_the_owner_deploys() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let issuer = Address::generate(&env);
    let token_address = env.register_stellar_asset_contract_v2(issuer).address();
    let wasm_hash = env.deployer().upload_contract_wasm(payroll::WASM);
    let factory = env.register(FactoryContract, (owner.clone(), wasm_hash));
    let client = FactoryContractClient::new(&env, &factory);
    // No auths mocked: the owner's authorization is missing, so deploy fails.
    assert!(client
        .try_deploy(&salt(&env, 1), &Address::generate(&env), &token_address)
        .is_err());
}

#[test]
fn the_wasm_hash_can_be_updated_by_the_owner() {
    let (env, owner, factory, _token) = setup();
    let client = FactoryContractClient::new(&env, &factory);
    let before = client.wasm_hash();
    let next = BytesN::from_array(&env, &[7u8; 32]);
    client.set_wasm_hash(&next);
    assert_ne!(before, next);
    assert_eq!(client.wasm_hash(), next);
    assert_eq!(client.owner(), owner);
}
