//! # Input Validation Tests (#471)
//!
//! Tests for the input sanitization and validation layer.

use super::*;
use soroban_sdk::{testutils::Address as _, Bytes, BytesN, Env};

fn setup() -> (Env, CoinflipContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(
        &admin, &treasury, &token,
        &300, &1_000_000, &100_000_000,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    (env, client, admin)
}

// ── validate_wager ────────────────────────────────────────────────────────────

#[test]
fn test_validate_wager_zero_rejected() {
    let (_, client, _) = setup();
    let config = client.get_config();
    let result = CoinflipContract::validate_wager(0, &config);
    assert_eq!(result, Err(Error::InvalidWagerValue));
}

#[test]
fn test_validate_wager_negative_rejected() {
    let (_, client, _) = setup();
    let config = client.get_config();
    assert_eq!(CoinflipContract::validate_wager(-1, &config), Err(Error::InvalidWagerValue));
}

#[test]
fn test_validate_wager_below_minimum_rejected() {
    let (_, client, _) = setup();
    let config = client.get_config();
    assert_eq!(CoinflipContract::validate_wager(1, &config), Err(Error::WagerBelowMinimum));
}

#[test]
fn test_validate_wager_above_maximum_rejected() {
    let (_, client, _) = setup();
    let config = client.get_config();
    assert_eq!(CoinflipContract::validate_wager(200_000_000, &config), Err(Error::WagerAboveMaximum));
}

#[test]
fn test_validate_wager_at_minimum_accepted() {
    let (_, client, _) = setup();
    let config = client.get_config();
    assert!(CoinflipContract::validate_wager(config.min_wager, &config).is_ok());
}

#[test]
fn test_validate_wager_at_maximum_accepted() {
    let (_, client, _) = setup();
    let config = client.get_config();
    assert!(CoinflipContract::validate_wager(config.max_wager, &config).is_ok());
}

#[test]
fn test_validate_wager_valid_midrange() {
    let (_, client, _) = setup();
    let config = client.get_config();
    let result = CoinflipContract::validate_wager(5_000_000, &config);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().0, 5_000_000);
}

// ── validate_commitment_input ─────────────────────────────────────────────────

#[test]
fn test_validate_commitment_valid() {
    let env = Env::default();
    // Use a commitment with varied bytes (not all-same).
    let mut arr = [0u8; 32];
    for (i, b) in arr.iter_mut().enumerate() {
        *b = i as u8;
    }
    let commitment = BytesN::from_array(&env, &arr);
    assert!(CoinflipContract::validate_commitment_input(&commitment).is_ok());
}

#[test]
fn test_validate_commitment_all_zero_rejected() {
    let env = Env::default();
    let commitment = BytesN::from_array(&env, &[0u8; 32]);
    assert_eq!(
        CoinflipContract::validate_commitment_input(&commitment),
        Err(Error::InvalidCommitment)
    );
}

#[test]
fn test_validate_commitment_all_same_byte_rejected() {
    let env = Env::default();
    let commitment = BytesN::from_array(&env, &[0xABu8; 32]);
    assert_eq!(
        CoinflipContract::validate_commitment_input(&commitment),
        Err(Error::WeakCommitment)
    );
}

// ── validate_secret ───────────────────────────────────────────────────────────

#[test]
fn test_validate_secret_valid() {
    let env = Env::default();
    let secret = Bytes::from_slice(&env, b"my_secret_value_with_enough_entropy");
    assert!(CoinflipContract::validate_secret(&secret).is_ok());
}

#[test]
fn test_validate_secret_empty_rejected() {
    let env = Env::default();
    let secret = Bytes::new(&env);
    assert_eq!(CoinflipContract::validate_secret(&secret), Err(Error::InvalidSecretLength));
}

#[test]
fn test_validate_secret_too_long_rejected() {
    let env = Env::default();
    let data = [0u8; 257];
    let secret = Bytes::from_slice(&env, &data);
    assert_eq!(CoinflipContract::validate_secret(&secret), Err(Error::InvalidSecretLength));
}

#[test]
fn test_validate_secret_max_length_accepted() {
    let env = Env::default();
    let data = [0u8; 256];
    let secret = Bytes::from_slice(&env, &data);
    assert!(CoinflipContract::validate_secret(&secret).is_ok());
}

// ── validate_fee_bps ──────────────────────────────────────────────────────────

#[test]
fn test_validate_fee_bps_valid_range() {
    assert!(CoinflipContract::validate_fee_bps(300).is_ok());
    assert!(CoinflipContract::validate_fee_bps(200).is_ok());
    assert!(CoinflipContract::validate_fee_bps(500).is_ok());
}

#[test]
fn test_validate_fee_bps_too_low_rejected() {
    assert_eq!(CoinflipContract::validate_fee_bps(100), Err(Error::InvalidFeePercentage));
    assert_eq!(CoinflipContract::validate_fee_bps(0), Err(Error::InvalidFeePercentage));
}

#[test]
fn test_validate_fee_bps_too_high_rejected() {
    assert_eq!(CoinflipContract::validate_fee_bps(501), Err(Error::InvalidFeePercentage));
    assert_eq!(CoinflipContract::validate_fee_bps(10_000), Err(Error::InvalidFeePercentage));
}

// ── validate_wager_limits ─────────────────────────────────────────────────────

#[test]
fn test_validate_wager_limits_valid() {
    assert!(CoinflipContract::validate_wager_limits(1_000_000, 100_000_000).is_ok());
}

#[test]
fn test_validate_wager_limits_min_zero_rejected() {
    assert_eq!(
        CoinflipContract::validate_wager_limits(0, 100_000_000),
        Err(Error::InvalidWagerLimits)
    );
}

#[test]
fn test_validate_wager_limits_min_negative_rejected() {
    assert_eq!(
        CoinflipContract::validate_wager_limits(-1, 100_000_000),
        Err(Error::InvalidWagerLimits)
    );
}

#[test]
fn test_validate_wager_limits_min_equals_max_rejected() {
    assert_eq!(
        CoinflipContract::validate_wager_limits(1_000_000, 1_000_000),
        Err(Error::InvalidWagerLimits)
    );
}

#[test]
fn test_validate_wager_limits_min_greater_than_max_rejected() {
    assert_eq!(
        CoinflipContract::validate_wager_limits(100_000_000, 1_000_000),
        Err(Error::InvalidWagerLimits)
    );
}
