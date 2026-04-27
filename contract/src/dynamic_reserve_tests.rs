/// # Dynamic Reserve Management Tests
///
/// Covers:
/// - `compute_dynamic_max_wager` tier boundaries
/// - `get_reserve_health` query correctness
/// - `get_dynamic_max_wager` query correctness
/// - `start_game` rejection when wager exceeds dynamic cap
/// - Solvency maintained across all tiers
use super::*;
use soroban_sdk::testutils::Address as _;

// ── Harness ───────────────────────────────────────────────────────────────────

const MIN: i128 = 1_000_000;
const MAX: i128 = 100_000_000;
// streak4_plus multiplier default = 100_000 bps (10x)
// max_worst_case_payout = MAX * 100_000 / 10_000 = MAX * 10 = 1_000_000_000

fn setup() -> (Env, CoinflipContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(
        &admin,
        &treasury,
        &token,
        &300,
        &MIN,
        &MAX,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
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

// ── compute_dynamic_max_wager unit tests ─────────────────────────────────────

#[test]
fn healthy_tier_full_max_wager() {
    // coverage_ratio = 10 → 100% of max_wager
    // reserve = 10 * max_worst_case = 10 * (MAX * 10) = 10_000_000_000
    let reserve = 10_000_000_000i128;
    let (dynamic_max, ratio, worst) = compute_dynamic_max_wager(reserve, MAX, 100_000);
    assert_eq!(worst, MAX * 10);
    assert_eq!(ratio, 10);
    assert_eq!(dynamic_max, MAX);
}

#[test]
fn moderate_tier_half_max_wager() {
    // coverage_ratio = 5 → 50% of max_wager
    let reserve = 5_000_000_000i128;
    let (dynamic_max, ratio, _) = compute_dynamic_max_wager(reserve, MAX, 100_000);
    assert_eq!(ratio, 5);
    assert_eq!(dynamic_max, MAX / 2);
}

#[test]
fn low_tier_twenty_percent_max_wager() {
    // coverage_ratio = 2 → 20% of max_wager
    let reserve = 2_000_000_000i128;
    let (dynamic_max, ratio, _) = compute_dynamic_max_wager(reserve, MAX, 100_000);
    assert_eq!(ratio, 2);
    assert_eq!(dynamic_max, MAX / 5);
}

#[test]
fn critical_tier_zero_max_wager() {
    // coverage_ratio = 1 → 0 (no new games)
    let reserve = 1_000_000_000i128;
    let (dynamic_max, ratio, _) = compute_dynamic_max_wager(reserve, MAX, 100_000);
    assert_eq!(ratio, 1);
    assert_eq!(dynamic_max, 0);
}

#[test]
fn zero_reserve_critical() {
    let (dynamic_max, ratio, _) = compute_dynamic_max_wager(0, MAX, 100_000);
    assert_eq!(ratio, 0);
    assert_eq!(dynamic_max, 0);
}

#[test]
fn negative_reserve_critical() {
    let (dynamic_max, ratio, _) = compute_dynamic_max_wager(-1, MAX, 100_000);
    assert_eq!(ratio, 0);
    assert_eq!(dynamic_max, 0);
}

// ── get_reserve_health query tests ───────────────────────────────────────────

#[test]
fn reserve_health_healthy_tier() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, 10_000_000_000);
    let health = client.get_reserve_health();
    assert_eq!(health.coverage_ratio, 10);
    assert_eq!(health.dynamic_max_wager, MAX);
    assert_eq!(health.tier, soroban_sdk::String::from_str(&env, "healthy"));
}

#[test]
fn reserve_health_moderate_tier() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, 5_000_000_000);
    let health = client.get_reserve_health();
    assert_eq!(health.coverage_ratio, 5);
    assert_eq!(health.dynamic_max_wager, MAX / 2);
    assert_eq!(health.tier, soroban_sdk::String::from_str(&env, "moderate"));
}

#[test]
fn reserve_health_low_tier() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, 2_000_000_000);
    let health = client.get_reserve_health();
    assert_eq!(health.coverage_ratio, 2);
    assert_eq!(health.dynamic_max_wager, MAX / 5);
    assert_eq!(health.tier, soroban_sdk::String::from_str(&env, "low"));
}

#[test]
fn reserve_health_critical_tier() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, 500_000_000);
    let health = client.get_reserve_health();
    assert_eq!(health.coverage_ratio, 0);
    assert_eq!(health.dynamic_max_wager, 0);
    assert_eq!(health.tier, soroban_sdk::String::from_str(&env, "critical"));
}

// ── get_dynamic_max_wager query tests ────────────────────────────────────────

#[test]
fn dynamic_max_wager_healthy() {
    let (_, client, contract_id, _) = setup();
    let env = soroban_sdk::Env::default();
    // Re-use the client's env via the contract
    let (env2, client2, cid2, _) = setup();
    fund(&env2, &cid2, 10_000_000_000);
    assert_eq!(client2.get_dynamic_max_wager(), MAX);
}

#[test]
fn dynamic_max_wager_critical_is_zero() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, 100_000); // tiny reserves
    assert_eq!(client.get_dynamic_max_wager(), 0);
}

// ── start_game dynamic cap enforcement ───────────────────────────────────────

#[test]
fn start_game_rejected_when_wager_exceeds_dynamic_cap() {
    let (env, client, contract_id, _) = setup();
    // Moderate tier: dynamic_max = MAX / 2 = 50_000_000
    fund(&env, &contract_id, 5_000_000_000);
    let player = Address::generate(&env);
    // Wager = MAX (100_000_000) > dynamic_max (50_000_000) → InsufficientReserves
    let result = client.try_start_game(&player, &Side::Heads, &MAX, &commitment(&env));
    assert_eq!(result, Err(Ok(Error::InsufficientReserves)));
}

#[test]
fn start_game_accepted_at_dynamic_cap() {
    let (env, client, contract_id, _) = setup();
    // Moderate tier: dynamic_max = MAX / 2 = 50_000_000
    fund(&env, &contract_id, 5_000_000_000);
    let player = Address::generate(&env);
    // Wager = MAX / 2 = dynamic_max → accepted
    assert!(client
        .try_start_game(&player, &Side::Heads, &(MAX / 2), &commitment(&env))
        .is_ok());
}

#[test]
fn start_game_rejected_in_critical_tier() {
    let (env, client, contract_id, _) = setup();
    // Critical tier: dynamic_max = 0
    fund(&env, &contract_id, 500_000_000);
    let player = Address::generate(&env);
    let result = client.try_start_game(&player, &Side::Heads, &MIN, &commitment(&env));
    assert_eq!(result, Err(Ok(Error::InsufficientReserves)));
}

#[test]
fn start_game_accepted_in_healthy_tier_at_max() {
    let (env, client, contract_id, _) = setup();
    // Healthy tier: dynamic_max = MAX
    fund(&env, &contract_id, 10_000_000_000);
    let player = Address::generate(&env);
    assert!(client
        .try_start_game(&player, &Side::Heads, &MAX, &commitment(&env))
        .is_ok());
}
