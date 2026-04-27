/// # Referral and Affiliate Tracking Tests (#465)
///
/// Tests for the referral system covering:
/// - Referral code generation (idempotent, unique per player)
/// - Referral registration: valid, self-referral rejection, unknown code rejection
/// - Commission calculation and payout on game settlement
/// - Admin controls: set_referral_commission validation and authorization
/// - Referral stats tracking (referrals_count, total_referral_rewards)
use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Ledger;

// ── Harness ───────────────────────────────────────────────────────────────────

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
        &1_000_000,
        &100_000_000,
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

fn secret(env: &Env, seed: u8) -> Bytes {
    Bytes::from_slice(env, &[seed; 32])
}

fn commitment(env: &Env, seed: u8) -> BytesN<32> {
    env.crypto().sha256(&secret(env, seed)).into()
}

fn load_referral_stats(
    env: &Env,
    contract_id: &Address,
    player: &Address,
) -> ReferralStats {
    env.as_contract(contract_id, || {
        CoinflipContract::load_referral_stats(env, player)
    })
}

// ── Referral code generation ──────────────────────────────────────────────────

/// get_or_create_referral_code returns a non-empty code.
#[test]
fn referral_code_is_non_empty() {
    let (env, client, _, _) = setup();
    let player = Address::generate(&env);
    let code = client.get_or_create_referral_code(&player);
    assert!(!code.is_empty());
}

/// Calling get_or_create_referral_code twice returns the same code (idempotent).
#[test]
fn referral_code_generation_is_idempotent() {
    let (env, client, _, _) = setup();
    let player = Address::generate(&env);
    let code1 = client.get_or_create_referral_code(&player);
    let code2 = client.get_or_create_referral_code(&player);
    assert_eq!(code1, code2);
}

/// Two different players get different referral codes.
#[test]
fn different_players_get_different_referral_codes() {
    let (env, client, _, _) = setup();
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);
    let code1 = client.get_or_create_referral_code(&player1);
    let code2 = client.get_or_create_referral_code(&player2);
    assert_ne!(code1, code2);
}

// ── Referral registration ─────────────────────────────────────────────────────

/// A player can register using a valid referral code.
#[test]
fn register_referral_succeeds_with_valid_code() {
    let (env, client, contract_id, _) = setup();
    let referrer = Address::generate(&env);
    let new_player = Address::generate(&env);

    let code = client.get_or_create_referral_code(&referrer);
    client.register_referral(&new_player, &code).unwrap();

    let stats = load_referral_stats(&env, &contract_id, &new_player);
    assert_eq!(stats.referrer, Some(referrer));
}

/// Registering with an unknown code returns InvalidCommitment.
#[test]
fn register_referral_rejects_unknown_code() {
    let (env, client, _, _) = setup();
    let player = Address::generate(&env);
    let fake_code = Bytes::from_slice(&env, b"not_a_real_code_xyzxyz");

    let result = client.try_register_referral(&player, &fake_code);
    assert_eq!(result, Err(Ok(Error::InvalidCommitment)));
}

/// A player cannot use their own referral code (self-referral).
#[test]
fn register_referral_rejects_self_referral() {
    let (env, client, _, _) = setup();
    let player = Address::generate(&env);
    let code = client.get_or_create_referral_code(&player);

    let result = client.try_register_referral(&player, &code);
    assert_eq!(result, Err(Ok(Error::InvalidCommitment)));
}

/// Registering a referral is idempotent — second call is a no-op.
#[test]
fn register_referral_is_idempotent() {
    let (env, client, contract_id, _) = setup();
    let referrer1 = Address::generate(&env);
    let referrer2 = Address::generate(&env);
    let player = Address::generate(&env);

    let code1 = client.get_or_create_referral_code(&referrer1);
    let code2 = client.get_or_create_referral_code(&referrer2);

    client.register_referral(&player, &code1).unwrap();
    // Second registration with a different code must be ignored
    client.register_referral(&player, &code2).unwrap();

    let stats = load_referral_stats(&env, &contract_id, &player);
    assert_eq!(stats.referrer, Some(referrer1), "referrer must not change after first registration");
}

// ── Referral stats tracking ───────────────────────────────────────────────────

/// Referrer's referrals_count increments when a new player registers.
#[test]
fn referrer_referrals_count_increments_on_registration() {
    let (env, client, contract_id, _) = setup();
    let referrer = Address::generate(&env);
    let code = client.get_or_create_referral_code(&referrer);

    for _ in 0..3 {
        let player = Address::generate(&env);
        client.register_referral(&player, &code).unwrap();
    }

    let stats = load_referral_stats(&env, &contract_id, &referrer);
    assert_eq!(stats.referrals_count, 3);
}

/// get_referral_stats returns default values for a player with no referral activity.
#[test]
fn get_referral_stats_returns_defaults_for_new_player() {
    let (env, client, _, _) = setup();
    let player = Address::generate(&env);
    let stats = client.get_referral_stats(&player);
    assert_eq!(stats.referrer, None);
    assert_eq!(stats.referrals_count, 0);
    assert_eq!(stats.total_referral_rewards, 0);
}

// ── Commission calculation ────────────────────────────────────────────────────

/// Referrer accumulates total_referral_rewards after a referred player wins and cashes out.
#[test]
fn referrer_earns_commission_on_referred_player_win() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 1_000_000_000);

    // Set commission to 100 bps (1%)
    client.set_referral_commission(&admin, &100).unwrap();

    let referrer = Address::generate(&env);
    let player = Address::generate(&env);
    let code = client.get_or_create_referral_code(&referrer);
    client.register_referral(&player, &code).unwrap();

    // seed 1 → Heads → win
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, 1));
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    client.reveal(&player, &secret(&env, 1));
    client.cash_out(&player);

    let stats = load_referral_stats(&env, &contract_id, &referrer);
    assert!(
        stats.total_referral_rewards > 0,
        "referrer must earn commission after referred player wins"
    );
}

/// With commission set to 0, no rewards are distributed.
#[test]
fn zero_commission_distributes_no_rewards() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 1_000_000_000);

    client.set_referral_commission(&admin, &0).unwrap();

    let referrer = Address::generate(&env);
    let player = Address::generate(&env);
    let code = client.get_or_create_referral_code(&referrer);
    client.register_referral(&player, &code).unwrap();

    // seed 1 → win
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, 1));
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    client.reveal(&player, &secret(&env, 1));
    client.cash_out(&player);

    let stats = load_referral_stats(&env, &contract_id, &referrer);
    assert_eq!(stats.total_referral_rewards, 0);
}

/// No commission is paid when the referred player loses.
#[test]
fn no_commission_on_referred_player_loss() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 1_000_000_000);

    client.set_referral_commission(&admin, &100).unwrap();

    let referrer = Address::generate(&env);
    let player = Address::generate(&env);
    let code = client.get_or_create_referral_code(&referrer);
    client.register_referral(&player, &code).unwrap();

    // seed 3 → Tails → loss for Heads player
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, 3));
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    client.reveal(&player, &secret(&env, 3));

    let stats = load_referral_stats(&env, &contract_id, &referrer);
    assert_eq!(stats.total_referral_rewards, 0);
}

// ── Admin controls ────────────────────────────────────────────────────────────

/// set_referral_commission succeeds for valid rates (0–1000 bps).
#[test]
fn set_referral_commission_accepts_valid_rates() {
    let (env, client, _, admin) = setup();
    assert!(client.try_set_referral_commission(&admin, &0).is_ok());
    assert!(client.try_set_referral_commission(&admin, &500).is_ok());
    assert!(client.try_set_referral_commission(&admin, &1000).is_ok());
}

/// set_referral_commission rejects rates above 1000 bps.
#[test]
fn set_referral_commission_rejects_rate_above_1000() {
    let (env, client, _, admin) = setup();
    let result = client.try_set_referral_commission(&admin, &1001);
    assert_eq!(result, Err(Ok(Error::InvalidFeePercentage)));
}

/// set_referral_commission rejects non-admin callers.
#[test]
fn set_referral_commission_rejects_non_admin() {
    let (env, client, _, _) = setup();
    let non_admin = Address::generate(&env);
    let result = client.try_set_referral_commission(&non_admin, &100);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

/// get_referral_commission returns the updated rate after set_referral_commission.
#[test]
fn get_referral_commission_reflects_updated_rate() {
    let (env, client, _, admin) = setup();
    client.set_referral_commission(&admin, &250).unwrap();
    assert_eq!(client.get_referral_commission(), 250);
}

/// Default commission rate is 0 before any admin update.
#[test]
fn default_referral_commission_is_zero() {
    let (env, client, _, _) = setup();
    assert_eq!(client.get_referral_commission(), 0);
}

// ── Payout accuracy ───────────────────────────────────────────────────────────

/// Commission is proportional to the wager at the configured rate.
#[test]
fn commission_is_proportional_to_wager() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 1_000_000_000);

    // 200 bps = 2%
    client.set_referral_commission(&admin, &200).unwrap();

    let referrer = Address::generate(&env);
    let player = Address::generate(&env);
    let code = client.get_or_create_referral_code(&referrer);
    client.register_referral(&player, &code).unwrap();

    let wager = 10_000_000i128;
    let expected_commission = wager * 200 / 10_000; // 200_000

    // seed 1 → win
    client.start_game(&player, &Side::Heads, &wager, &commitment(&env, 1));
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    client.reveal(&player, &secret(&env, 1));
    client.cash_out(&player);

    let stats = load_referral_stats(&env, &contract_id, &referrer);
    assert_eq!(
        stats.total_referral_rewards, expected_commission,
        "commission must equal wager * commission_bps / 10_000"
    );
}
