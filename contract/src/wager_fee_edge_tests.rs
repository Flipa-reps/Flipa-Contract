/// # Wager and Fee Edge Case Tests — Issue #155
///
/// Focused unit tests for spec boundaries not covered by the existing
/// `wager_limit_tests` and `fee_calculation_tests` suites.
///
/// ## What each group protects
///
/// | Group                              | Invariant protected                                      |
/// |------------------------------------|----------------------------------------------------------|
/// | Exact min/max wager + reserve      | Solvency check uses gross payout (pre-fee), not net      |
/// | Zero reserves + min wager          | No game starts when reserves are empty, even at min bet  |
/// | Fee boundary via `set_fee`         | Fee range [200, 500] bps is enforced on updates          |
/// | Fee boundary via `initialize`      | Fee range enforced at contract creation                  |
/// | Solvency uses gross, not net       | Reserve check is conservative (pre-fee worst case)       |
///
/// ## Solvency formula (contract Guard 6)
///   max_payout = wager × streak4_plus_bps / 10_000
///   accepted   iff reserve_balance >= max_payout
///
/// With default streak4_plus = 100_000 bps (10×):
///   max_payout(min_wager=1_000_000)   =  10_000_000
///   max_payout(max_wager=100_000_000) = 1_000_000_000
use super::*;
use soroban_sdk::testutils::Address as _;

// ── Harness ───────────────────────────────────────────────────────────────────

const MIN: i128 = 1_000_000;
const MAX: i128 = 100_000_000;
// streak4_plus default = 100_000 bps → 10× multiplier
const STREAK4_MULT: i128 = 100_000;

/// Minimum reserve needed to accept a wager (gross worst-case payout).
fn min_reserve_for(wager: i128) -> i128 {
    wager * STREAK4_MULT / 10_000
}

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

// ── Exact min wager + reserve boundary ───────────────────────────────────────

/// Exact min wager accepted when reserve equals the gross worst-case payout.
///
/// reserve = min_wager × 10 = 10_000_000
/// This is the tightest possible reserve that still allows a min-wager game.
#[test]
fn min_wager_accepted_at_exact_reserve_boundary() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, min_reserve_for(MIN));
    let player = Address::generate(&env);
    assert!(
        client.try_start_game(&player, &Side::Heads, &MIN, &commitment(&env)).is_ok(),
        "min wager must be accepted when reserve == gross worst-case payout"
    );
}

/// Exact min wager rejected when reserve is one stroop below the gross worst-case payout.
///
/// reserve = min_wager × 10 − 1 = 9_999_999
#[test]
fn min_wager_rejected_one_stroop_below_reserve_boundary() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, min_reserve_for(MIN) - 1);
    let player = Address::generate(&env);
    assert_eq!(
        client.try_start_game(&player, &Side::Heads, &MIN, &commitment(&env)),
        Err(Ok(Error::InsufficientReserves)),
        "min wager must be rejected one stroop below reserve boundary"
    );
}

// ── Exact max wager + reserve boundary ───────────────────────────────────────

/// Exact max wager accepted when reserve equals the gross worst-case payout.
///
/// reserve = max_wager × 10 = 1_000_000_000
#[test]
fn max_wager_accepted_at_exact_reserve_boundary() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, min_reserve_for(MAX));
    let player = Address::generate(&env);
    assert!(
        client.try_start_game(&player, &Side::Heads, &MAX, &commitment(&env)).is_ok(),
        "max wager must be accepted when reserve == gross worst-case payout"
    );
}

/// Exact max wager rejected when reserve is one stroop below the gross worst-case payout.
///
/// reserve = max_wager × 10 − 1 = 999_999_999
#[test]
fn max_wager_rejected_one_stroop_below_reserve_boundary() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, min_reserve_for(MAX) - 1);
    let player = Address::generate(&env);
    assert_eq!(
        client.try_start_game(&player, &Side::Heads, &MAX, &commitment(&env)),
        Err(Ok(Error::InsufficientReserves)),
        "max wager must be rejected one stroop below reserve boundary"
    );
}

// ── Zero reserves ─────────────────────────────────────────────────────────────

/// Zero reserves rejects even the minimum wager.
///
/// Protects: no game can start when the house has no funds, regardless of wager size.
#[test]
fn zero_reserves_rejects_min_wager() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, 0);
    let player = Address::generate(&env);
    assert_eq!(
        client.try_start_game(&player, &Side::Heads, &MIN, &commitment(&env)),
        Err(Ok(Error::InsufficientReserves)),
        "zero reserves must reject even the minimum wager"
    );
}

/// Zero reserves rejects the maximum wager.
#[test]
fn zero_reserves_rejects_max_wager() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id, 0);
    let player = Address::generate(&env);
    assert_eq!(
        client.try_start_game(&player, &Side::Heads, &MAX, &commitment(&env)),
        Err(Ok(Error::InsufficientReserves)),
        "zero reserves must reject the maximum wager"
    );
}

// ── Solvency check uses gross payout, not net ─────────────────────────────────

/// The solvency check is conservative: it uses the gross payout (pre-fee),
/// not the net payout (post-fee).  This means the reserve requirement is
/// higher than what the player actually receives, providing a safety buffer.
///
/// At fee=500 bps (5%), net = gross × 0.95.
/// If the check used net, reserve = max_wager × 10 × 0.95 = 950_000_000 would suffice.
/// Since it uses gross, reserve = max_wager × 10 = 1_000_000_000 is required.
///
/// This test verifies that 950_000_000 (net boundary) is NOT sufficient for max wager.
#[test]
fn solvency_check_uses_gross_not_net_payout() {
    let (env, client, contract_id, _) = setup();
    // Net worst-case at 5% fee: 1_000_000_000 × 0.95 = 950_000_000
    // Gross worst-case:         1_000_000_000
    // Fund at net boundary — should still be rejected because check uses gross.
    fund(&env, &contract_id, 950_000_000);
    let player = Address::generate(&env);
    assert_eq!(
        client.try_start_game(&player, &Side::Heads, &MAX, &commitment(&env)),
        Err(Ok(Error::InsufficientReserves)),
        "reserve at net boundary must be rejected; solvency check uses gross payout"
    );
}

// ── Fee boundary: set_fee ─────────────────────────────────────────────────────

/// `set_fee` accepts the minimum valid fee (200 bps = 2%).
#[test]
fn set_fee_accepts_200_bps_minimum() {
    let (_, client, _, admin) = setup();
    assert!(
        client.try_set_fee(&admin, &200, &None).is_ok(),
        "set_fee must accept 200 bps (minimum valid fee)"
    );
}

/// `set_fee` accepts the maximum valid fee (500 bps = 5%).
#[test]
fn set_fee_accepts_500_bps_maximum() {
    let (_, client, _, admin) = setup();
    assert!(
        client.try_set_fee(&admin, &500, &None).is_ok(),
        "set_fee must accept 500 bps (maximum valid fee)"
    );
}

/// `set_fee` rejects 199 bps (one below minimum).
#[test]
fn set_fee_rejects_199_bps_below_minimum() {
    let (_, client, _, admin) = setup();
    assert_eq!(
        client.try_set_fee(&admin, &199, &None),
        Err(Ok(Error::InvalidFeePercentage)),
        "set_fee must reject 199 bps (below minimum)"
    );
}

/// `set_fee` rejects 501 bps (one above maximum).
#[test]
fn set_fee_rejects_501_bps_above_maximum() {
    let (_, client, _, admin) = setup();
    assert_eq!(
        client.try_set_fee(&admin, &501, &None),
        Err(Ok(Error::InvalidFeePercentage)),
        "set_fee must reject 501 bps (above maximum)"
    );
}

/// `set_fee` rejects 0 bps (zero fee).
#[test]
fn set_fee_rejects_zero_bps() {
    let (_, client, _, admin) = setup();
    assert_eq!(
        client.try_set_fee(&admin, &0, &None),
        Err(Ok(Error::InvalidFeePercentage)),
        "set_fee must reject 0 bps"
    );
}

/// `set_fee` rejects u32::MAX.
#[test]
fn set_fee_rejects_u32_max() {
    let (_, client, _, admin) = setup();
    assert_eq!(
        client.try_set_fee(&admin, &u32::MAX, &None),
        Err(Ok(Error::InvalidFeePercentage)),
        "set_fee must reject u32::MAX"
    );
}

// ── Fee boundary: initialize ──────────────────────────────────────────────────

/// `initialize` rejects fee_bps below 200.
#[test]
fn initialize_rejects_fee_below_200_bps() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);
    let result = client.try_initialize(
        &admin,
        &treasury,
        &token,
        &199,
        &MIN,
        &MAX,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    assert_eq!(
        result,
        Err(Ok(Error::InvalidFeePercentage)),
        "initialize must reject fee_bps=199"
    );
}

/// `initialize` rejects fee_bps above 500.
#[test]
fn initialize_rejects_fee_above_500_bps() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);
    let result = client.try_initialize(
        &admin,
        &treasury,
        &token,
        &501,
        &MIN,
        &MAX,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    assert_eq!(
        result,
        Err(Ok(Error::InvalidFeePercentage)),
        "initialize must reject fee_bps=501"
    );
}
