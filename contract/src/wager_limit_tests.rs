/// # Wager Limit Enforcement Tests
///
/// ## Strategy
///
/// Wager limits are enforced in `start_game` using **inclusive** bounds:
///
/// ```text
/// Accepted: wager >= config.min_wager && wager <= config.max_wager
/// Rejected: wager < config.min_wager  → Error::WagerBelowMinimum  (code 1)
/// Rejected: wager > config.max_wager  → Error::WagerAboveMaximum  (code 2)
/// ```
///
/// The guards use strict inequalities (`<` / `>`), so exactly `min_wager` and
/// `max_wager` are **accepted**.  This is the "inclusive boundary" contract.
///
/// ## Coverage
///
/// | Test                                    | Boundary point        |
/// |-----------------------------------------|-----------------------|
/// | `min_minus_one_rejected`                | min − 1               |
/// | `min_accepted`                          | min (inclusive)       |
/// | `min_plus_one_accepted`                 | min + 1               |
/// | `max_minus_one_accepted`                | max − 1               |
/// | `max_accepted`                          | max (inclusive)       |
/// | `max_plus_one_rejected`                 | max + 1               |
/// | `zero_rejected`                         | 0 (below any min > 0) |
/// | `negative_rejected`                     | −1                    |
/// | `i128_min_rejected`                     | i128::MIN             |
/// | `i128_max_rejected`                     | i128::MAX             |
/// | `limit_update_takes_effect`             | post-update boundary  |
/// | `limit_update_old_min_now_rejected`     | old min after update  |
/// | `limit_update_old_max_now_rejected`     | old max after update  |
/// | `limit_update_unauthorized`             | admin guard           |
/// | `limit_update_invalid_min_gte_max`      | min >= max guard      |
use super::*;
use soroban_sdk::testutils::Address as _;

// ── Harness ───────────────────────────────────────────────────────────────────

const MIN: i128 = 1_000_000;   // 1 XLM in stroops
const MAX: i128 = 100_000_000; // 100 XLM in stroops

fn setup() -> (Env, CoinflipContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(&admin, &treasury, &token, &300, &MIN, &MAX, &BytesN::from_array(&env, &[0u8; 32]));
    (env, client, contract_id, admin)
}

fn fund(env: &Env, contract_id: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let mut stats = CoinflipContract::load_stats(env);
        stats.reserve_balance = amount;
        CoinflipContract::save_stats(env, &stats);
    });
}

fn commitment(env: &Env) -> BytesN<32> {
    env.crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(env, &[7u8; 32]))
        .into()
}

// ── Boundary tests ────────────────────────────────────────────────────────────

#[test]
fn min_minus_one_rejected() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, i128::MAX / 2);
    let player = Address::generate(&env);
    let result = client.try_start_game(&player, &Side::Heads, &(MIN - 1), &commitment(&env));
    assert_eq!(result, Err(Ok(Error::WagerBelowMinimum)));
}

#[test]
fn min_accepted() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, i128::MAX / 2);
    let player = Address::generate(&env);
    // Exactly at min must succeed (inclusive lower bound).
    assert!(client.try_start_game(&player, &Side::Heads, &MIN, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into())).is_ok());
}

#[test]
fn min_plus_one_accepted() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, i128::MAX / 2);
    let player = Address::generate(&env);
    assert!(client.try_start_game(&player, &Side::Heads, &(MIN + 1), &commitment(&env)).is_ok());
}

#[test]
fn max_minus_one_accepted() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, i128::MAX / 2);
    let player = Address::generate(&env);
    assert!(client.try_start_game(&player, &Side::Heads, &(MAX - 1), &commitment(&env)).is_ok());
}

#[test]
fn max_accepted() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, i128::MAX / 2);
    let player = Address::generate(&env);
    // Exactly at max must succeed (inclusive upper bound).
    assert!(client.try_start_game(&player, &Side::Heads, &MAX, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into())).is_ok());
}

#[test]
fn max_plus_one_rejected() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, i128::MAX / 2);
    let player = Address::generate(&env);
    let result = client.try_start_game(&player, &Side::Heads, &(MAX + 1), &commitment(&env));
    assert_eq!(result, Err(Ok(Error::WagerAboveMaximum)));
}

#[test]
fn zero_rejected() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, i128::MAX / 2);
    let player = Address::generate(&env);
    let result = client.try_start_game(&player, &Side::Heads, &0, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    assert_eq!(result, Err(Ok(Error::WagerBelowMinimum)));
}

#[test]
fn negative_rejected() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, i128::MAX / 2);
    let player = Address::generate(&env);
    let result = client.try_start_game(&player, &Side::Heads, &(-1), &commitment(&env));
    assert_eq!(result, Err(Ok(Error::WagerBelowMinimum)));
}

#[test]
fn i128_min_rejected() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, i128::MAX / 2);
    let player = Address::generate(&env);
    let result = client.try_start_game(&player, &Side::Heads, &i128::MIN, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    assert_eq!(result, Err(Ok(Error::WagerBelowMinimum)));
}

#[test]
fn i128_max_rejected() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, i128::MAX / 2);
    let player = Address::generate(&env);
    // i128::MAX far exceeds max_wager; must be rejected as above maximum.
    let result = client.try_start_game(&player, &Side::Heads, &i128::MAX, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    assert_eq!(result, Err(Ok(Error::WagerAboveMaximum)));
}

// ── Limit update tests ────────────────────────────────────────────────────────

#[test]
fn limit_update_takes_effect_immediately() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, i128::MAX / 2);

    // Tighten limits: new range [5_000_000, 10_000_000]
    let new_min: i128 = 5_000_000;
    let new_max: i128 = 10_000_000;
    client.set_wager_limits(&admin, &new_min, &new_max);

    let player = Address::generate(&env);

    // Exactly new_min accepted
    assert!(client
        .try_start_game(&player, &Side::Heads, &new_min, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()))
        .is_ok());

    // Exactly new_max accepted (need a fresh player — no active game)
    let player2 = Address::generate(&env);
    assert!(client
        .try_start_game(&player2, &Side::Heads, &new_max, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()))
        .is_ok());
}

#[test]
fn limit_update_old_min_now_rejected() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, i128::MAX / 2);

    // Raise the minimum above the old MIN
    let new_min: i128 = MIN + 1_000_000;
    client.set_wager_limits(&admin, &new_min, &MAX);

    let player = Address::generate(&env);
    // Old MIN is now below the new minimum → rejected
    let result = client.try_start_game(&player, &Side::Heads, &MIN, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    assert_eq!(result, Err(Ok(Error::WagerBelowMinimum)));
}

#[test]
fn limit_update_old_max_now_rejected() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, i128::MAX / 2);

    // Lower the maximum below the old MAX
    let new_max: i128 = MAX - 1_000_000;
    client.set_wager_limits(&admin, &MIN, &new_max);

    let player = Address::generate(&env);
    // Old MAX is now above the new maximum → rejected
    let result = client.try_start_game(&player, &Side::Heads, &MAX, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    assert_eq!(result, Err(Ok(Error::WagerAboveMaximum)));
}

#[test]
fn limit_update_unauthorized() {
    let (env, client, _, _) = setup();
    let non_admin = Address::generate(&env);
    let result = client.try_set_wager_limits(&non_admin, &MIN, &MAX);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn limit_update_invalid_min_gte_max() {
    let (env, client, _, admin) = setup();

    // min == max
    let result = client.try_set_wager_limits(&admin, &MIN, &MIN);
    assert_eq!(result, Err(Ok(Error::InvalidWagerLimits)));

    // min > max
    let result = client.try_set_wager_limits(&admin, &MAX, &MIN);
    assert_eq!(result, Err(Ok(Error::InvalidWagerLimits)));
}
