#![cfg(test)]

use soroban_sdk::{testutils::Address as _, token, Address, Env};

use crate::{PayrollContract, PayrollContractClient};

/// Deploy a fresh payroll contract with a test token; mint 1_000 to the admin.
/// Returns (env, admin, contract_id, token_address).
fn setup() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let issuer = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(issuer);
    let token_address = sac.address();
    token::StellarAssetClient::new(&env, &token_address).mint(&admin, &1_000);

    let contract_id = env.register(PayrollContract, (admin.clone(), token_address.clone()));
    (env, admin, contract_id, token_address)
}

#[test]
fn deposit_set_salary_and_pay() {
    let (env, admin, contract_id, token_address) = setup();
    let client = PayrollContractClient::new(&env, &contract_id);
    let token = token::TokenClient::new(&env, &token_address);
    let worker = Address::generate(&env);

    client.deposit(&admin, &500);
    assert_eq!(client.treasury_balance(), 500);

    client.set_salary(&worker, &200);
    assert_eq!(client.salary_of(&worker), 200);

    client.pay(&worker);
    assert_eq!(token.balance(&worker), 200);
    assert_eq!(client.treasury_balance(), 300);
}

#[test]
fn withdraw_returns_unused_funds() {
    let (env, admin, contract_id, token_address) = setup();
    let client = PayrollContractClient::new(&env, &contract_id);
    let token = token::TokenClient::new(&env, &token_address);

    client.deposit(&admin, &500);
    assert_eq!(client.treasury_balance(), 500);

    client.withdraw(&admin, &300);
    assert_eq!(client.treasury_balance(), 200);
    assert_eq!(token.balance(&admin), 800);
}

#[test]
fn withdraw_more_than_treasury_fails() {
    let (env, admin, contract_id, _token) = setup();
    let client = PayrollContractClient::new(&env, &contract_id);

    client.deposit(&admin, &100);
    assert!(client.try_withdraw(&admin, &500).is_err());
}

#[test]
fn pay_without_salary_fails() {
    let (env, _admin, contract_id, _token) = setup();
    let client = PayrollContractClient::new(&env, &contract_id);
    let worker = Address::generate(&env);

    assert!(client.try_pay(&worker).is_err());
}

#[test]
fn pay_with_empty_treasury_fails() {
    let (env, _admin, contract_id, _token) = setup();
    let client = PayrollContractClient::new(&env, &contract_id);
    let worker = Address::generate(&env);

    client.set_salary(&worker, &100);
    assert!(client.try_pay(&worker).is_err());
}

/* ------------------------------ registry ------------------------------ */

#[test]
fn registry_add_remove_and_enumerate() {
    let (env, _admin, contract_id, _token) = setup();
    let client = PayrollContractClient::new(&env, &contract_id);
    let a = Address::generate(&env);
    let b = Address::generate(&env);

    assert!(client.add_worker(&a));
    assert!(!client.add_worker(&a), "adding twice is a no-op");
    client.set_salary(&b, &50);
    assert_eq!(client.workers().len(), 2);
    assert!(client.is_worker(&a));
    assert!(client.is_worker(&b));

    assert!(client.remove_worker(&b));
    assert!(!client.remove_worker(&b));
    assert_eq!(client.workers().len(), 1);
    assert_eq!(client.salary_of(&b), 0, "removing clears the salary");
}

/* ------------------------------ batch pay ----------------------------- */

use crate::Payment;
use soroban_sdk::{vec, BytesN, Vec};

fn run_id(env: &Env, n: u8) -> BytesN<32> {
    BytesN::from_array(env, &[n; 32])
}

#[test]
fn pay_batch_from_parked_funds() {
    let (env, admin, contract_id, token_address) = setup();
    let client = PayrollContractClient::new(&env, &contract_id);
    let token = token::TokenClient::new(&env, &token_address);
    let w1 = Address::generate(&env);
    let w2 = Address::generate(&env);

    client.deposit(&admin, &500);
    let payments: Vec<Payment> = vec![
        &env,
        Payment {
            worker: w1.clone(),
            amount: 200,
        },
        Payment {
            worker: w2.clone(),
            amount: 150,
        },
    ];
    let total = client.pay_batch(&contract_id, &run_id(&env, 1), &payments);

    assert_eq!(total, 350);
    assert_eq!(token.balance(&w1), 200);
    assert_eq!(token.balance(&w2), 150);
    assert_eq!(client.treasury_balance(), 150);
    assert_eq!(client.run_paid(&run_id(&env, 1)), Some(350));
    assert_eq!(client.workers().len(), 2, "paid workers join the registry");
}

#[test]
fn pay_batch_pulls_from_the_employer_wallet_in_the_same_call() {
    let (env, admin, contract_id, token_address) = setup();
    let client = PayrollContractClient::new(&env, &contract_id);
    let token = token::TokenClient::new(&env, &token_address);
    let w1 = Address::generate(&env);

    let payments: Vec<Payment> = vec![
        &env,
        Payment {
            worker: w1.clone(),
            amount: 300,
        },
    ];
    client.pay_batch(&admin, &run_id(&env, 2), &payments);

    assert_eq!(token.balance(&w1), 300);
    assert_eq!(token.balance(&admin), 700, "pulled exactly the total");
    assert_eq!(client.treasury_balance(), 0, "nothing parked between runs");
}

#[test]
fn pay_batch_is_atomic_when_the_treasury_cannot_cover_it() {
    let (env, admin, contract_id, token_address) = setup();
    let client = PayrollContractClient::new(&env, &contract_id);
    let token = token::TokenClient::new(&env, &token_address);
    let w1 = Address::generate(&env);
    let w2 = Address::generate(&env);

    client.deposit(&admin, &250);
    let payments: Vec<Payment> = vec![
        &env,
        Payment {
            worker: w1.clone(),
            amount: 200,
        },
        Payment {
            worker: w2.clone(),
            amount: 100,
        },
    ];
    assert!(client
        .try_pay_batch(&contract_id, &run_id(&env, 3), &payments)
        .is_err());
    assert_eq!(token.balance(&w1), 0, "the first line was not paid either");
    assert_eq!(client.treasury_balance(), 250);
    assert_eq!(client.run_paid(&run_id(&env, 3)), None);
}

#[test]
fn pay_batch_rejects_bad_lines_before_moving_anything() {
    let (env, admin, contract_id, token_address) = setup();
    let client = PayrollContractClient::new(&env, &contract_id);
    let token = token::TokenClient::new(&env, &token_address);
    let w1 = Address::generate(&env);
    let w2 = Address::generate(&env);
    client.deposit(&admin, &500);

    let bad: Vec<Payment> = vec![
        &env,
        Payment {
            worker: w1.clone(),
            amount: 100,
        },
        Payment {
            worker: w2.clone(),
            amount: 0,
        },
    ];
    assert!(client
        .try_pay_batch(&contract_id, &run_id(&env, 4), &bad)
        .is_err());
    assert_eq!(token.balance(&w1), 0);

    let empty: Vec<Payment> = vec![&env];
    assert!(client
        .try_pay_batch(&contract_id, &run_id(&env, 5), &empty)
        .is_err());
}

#[test]
fn pay_batch_is_idempotent_per_run_id() {
    let (env, admin, contract_id, token_address) = setup();
    let client = PayrollContractClient::new(&env, &contract_id);
    let token = token::TokenClient::new(&env, &token_address);
    let w1 = Address::generate(&env);
    client.deposit(&admin, &500);

    let payments: Vec<Payment> = vec![
        &env,
        Payment {
            worker: w1.clone(),
            amount: 100,
        },
    ];
    client.pay_batch(&contract_id, &run_id(&env, 6), &payments);
    assert!(client
        .try_pay_batch(&contract_id, &run_id(&env, 6), &payments)
        .is_err());
    assert_eq!(token.balance(&w1), 100, "the retry paid nothing");
}

#[test]
fn pay_batch_caps_the_batch_size() {
    let (env, admin, contract_id, _token) = setup();
    let client = PayrollContractClient::new(&env, &contract_id);
    client.deposit(&admin, &1_000);
    let mut payments: Vec<Payment> = vec![&env];
    for _ in 0..(crate::MAX_BATCH + 1) {
        payments.push_back(Payment {
            worker: Address::generate(&env),
            amount: 1,
        });
    }
    assert!(client
        .try_pay_batch(&contract_id, &run_id(&env, 7), &payments)
        .is_err());
}
