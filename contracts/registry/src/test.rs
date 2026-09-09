#![cfg(test)]

use super::{RegistryContract, RegistryContractClient};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String};

fn setup() -> (Env, RegistryContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(RegistryContract, ());
    let client = RegistryContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    (env, client, admin)
}

#[test]
fn initializes_with_admin() {
    let (_, client, admin) = setup();
    assert_eq!(client.get_admin(), admin);
}

#[test]
#[should_panic(expected = "already initialized")]
fn cannot_initialize_twice() {
    let (_, client, admin) = setup();
    client.initialize(&admin);
}

#[test]
fn admin_can_write_and_read_an_attestation() {
    let (env, client, _admin) = setup();
    let domain = String::from_str(&env, "quoteanchor.example.com");
    let hash = BytesN::from_array(&env, &[7u8; 32]);

    client.attest(&domain, &true, &hash);

    let attestation = client.get_attestation(&domain).unwrap();
    assert!(attestation.passed);
    assert_eq!(attestation.result_hash, hash);
}

#[test]
fn missing_domain_returns_none() {
    let (env, client, _admin) = setup();
    let domain = String::from_str(&env, "never-checked.example.com");
    assert_eq!(client.get_attestation(&domain), None);
}

#[test]
fn a_later_attestation_overwrites_the_earlier_one() {
    let (env, client, _admin) = setup();
    let domain = String::from_str(&env, "anchor.example.com");
    let hash_a = BytesN::from_array(&env, &[1u8; 32]);
    let hash_b = BytesN::from_array(&env, &[2u8; 32]);

    client.attest(&domain, &true, &hash_a);
    client.attest(&domain, &false, &hash_b);

    let attestation = client.get_attestation(&domain).unwrap();
    assert!(!attestation.passed);
    assert_eq!(attestation.result_hash, hash_b);
}

#[test]
fn admin_rotation_transfers_write_access() {
    let (env, client, _admin) = setup();
    let new_admin = Address::generate(&env);

    client.set_admin(&new_admin);

    assert_eq!(client.get_admin(), new_admin);
}

/// `setup()` calls `mock_all_auths()`, which bypasses every `require_auth`
/// check regardless of caller — useful above for testing storage logic, but
/// it can't prove the auth gate itself works. This test builds a fresh env
/// with no blanket auth mock, so `attest`'s `admin.require_auth()` has
/// nothing backing it and must fail.
#[test]
#[should_panic]
fn attest_fails_without_the_admins_authorization() {
    let env = Env::default();
    let contract_id = env.register(RegistryContract, ());
    let client = RegistryContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let domain = String::from_str(&env, "example.com");
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    client.attest(&domain, &true, &hash);
}
