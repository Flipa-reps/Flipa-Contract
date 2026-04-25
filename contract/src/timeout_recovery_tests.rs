/// Integration tests for timeout wager reclaim functionality.
///
/// Issue: add-integration-tests-timeout-recovery-mechanisms
///
/// Covers:
///   - Test game timeout detection in Committed phase
///   - Test reclaim_wager function after timeout
///   - Test wager is returned to player (reserve credited)
///   - Test game state is cleared after reclaim
///   - Test timeout doesn't trigger prematurely
///   - Verify correct error handling
///   - Property 26: Timeout wager reclaim
///   - Edge cases (just before/after timeout)
use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use proptest::prelude::*;

const TIMEOUT: u32 = 100; // REVEAL_TIMEOUT_LEDGERS

// ── Test harness ──────────────────────────────────────────────────────────────

fn setup() -> (Env, CoinflipContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000, &BytesN::from_array(&env, &[0u8; 32]));
    (env, client, contract_id)
}

fn fund(env: &Env, contract_id: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let mut stats = CoinflipContract::load_stats(env);
        stats.reserve_balance = amount;
        CoinflipContract::save_stats(env, &stats);
    });
}

fn make_secret(env: &Env, seed: u8) -> Bytes {
    let mut b = Bytes::new(env);
    for _ in 0..32 {
        b.push_back(seed);
    }
    b
}

fn make_commitment(env: &Env, seed: u8) -> BytesN<32> {
    env.crypto().sha256(&make_secret(env, seed)).into()
}

fn advance_ledger(env: &Env, by: u32) {
    env.ledger().with_mut(|l| l.sequence_number += by);
}

fn inject_committed_game(
    env: &Env,
    contract_id: &Address,
    player: &Address,
    wager: i128,
    start_ledger: u32,
) {
    let game = GameState {
        wager,
        side: Side::Heads,
        streak: 0,
        commitment: make_commitment(env, 1),
        contract_random: make_commitment(env, 2),
        fee_bps: 300,
        phase: GamePhase::Committed,
        start_ledger,
    };
    env.as_contract(contract_id, || {
        CoinflipContract::save_player_game(env, player, &game);
    });
}

// ── Guard: NoActiveGame ───────────────────────────────────────────────────────

#[test]
fn test_reclaim_wager_no_active_game_rejected() {
    let (env, client, _contract_id) = setup();
    let player = Address::generate(&env);
    assert_eq!(
        client.try_reclaim_wager(&player),
        Err(Ok(Error::NoActiveGame))
    );
}

// ── Guard: InvalidPhase (Revealed) ───────────────────────────────────────────

#[test]
fn test_reclaim_wager_revealed_phase_rejected() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    // Inject a Revealed game — only Committed games can time out
    let game = GameState {
        wager: 5_000_000,
        side: Side::Heads,
        streak: 1,
        commitment: make_commitment(&env, 1),
        contract_random: make_commitment(&env, 2),
        fee_bps: 300,
        phase: GamePhase::Revealed,
        start_ledger: 0,
    
        vrf_input: env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into(),
    };
    env.as_contract(&contract_id, || {
        CoinflipContract::save_player_game(&env, &player, &game);
    });
    // Advance past timeout
    advance_ledger(&env, TIMEOUT + 1);
    assert_eq!(
        client.try_reclaim_wager(&player),
        Err(Ok(Error::InvalidPhase))
    );
}

// ── Guard: RevealTimeout (too early) ─────────────────────────────────────────

#[test]
fn test_reclaim_wager_before_timeout_rejected() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    let start = env.ledger().sequence();
    inject_committed_game(&env, &contract_id, &player, 5_000_000, start);
    // Still within timeout window
    assert_eq!(
        client.try_reclaim_wager(&player),
        Err(Ok(Error::RevealTimeout))
    );
}

#[test]
fn test_reclaim_wager_one_ledger_before_timeout_rejected() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    let start = env.ledger().sequence();
    inject_committed_game(&env, &contract_id, &player, 5_000_000, start);
    // Advance to exactly one ledger before expiry
    advance_ledger(&env, TIMEOUT - 1);
    assert_eq!(
        client.try_reclaim_wager(&player),
        Err(Ok(Error::RevealTimeout))
    );
}

// ── Happy path: reclaim after timeout ────────────────────────────────────────

#[test]
fn test_reclaim_wager_at_exact_timeout_succeeds() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let wager = 5_000_000i128;
    let player = Address::generate(&env);
    let start = env.ledger().sequence();
    inject_committed_game(&env, &contract_id, &player, wager, start);
    // Advance to exactly the timeout boundary
    advance_ledger(&env, TIMEOUT);
    let result = client.try_reclaim_wager(&player);
    assert_eq!(result, Ok(wager));
}

#[test]
fn test_reclaim_wager_after_timeout_succeeds() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let wager = 10_000_000i128;
    let player = Address::generate(&env);
    let start = env.ledger().sequence();
    inject_committed_game(&env, &contract_id, &player, wager, start);
    advance_ledger(&env, TIMEOUT + 50);
    let result = client.try_reclaim_wager(&player);
    assert_eq!(result, Ok(wager));
}

// ── State cleanup after reclaim ───────────────────────────────────────────────

#[test]
fn test_reclaim_wager_deletes_game_state() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    let start = env.ledger().sequence();
    inject_committed_game(&env, &contract_id, &player, 5_000_000, start);
    advance_ledger(&env, TIMEOUT);
    client.reclaim_wager(&player);
    let game = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });
    assert!(game.is_none(), "game state must be deleted after reclaim");
}

#[test]
fn test_reclaim_wager_credits_reserve_balance() {
    let (env, client, contract_id) = setup();
    let initial_reserve = 1_000_000_000i128;
    fund(&env, &contract_id, initial_reserve);
    let wager = 7_000_000i128;
    let player = Address::generate(&env);
    let start = env.ledger().sequence();
    inject_committed_game(&env, &contract_id, &player, wager, start);
    advance_ledger(&env, TIMEOUT);
    client.reclaim_wager(&player);
    let stats = env.as_contract(&contract_id, || CoinflipContract::load_stats(&env));
    assert_eq!(
        stats.reserve_balance,
        initial_reserve + wager,
        "reserve must increase by wager amount after reclaim"
    );
}

#[test]
fn test_reclaim_wager_allows_new_game_after_cleanup() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    let start = env.ledger().sequence();
    inject_committed_game(&env, &contract_id, &player, 5_000_000, start);
    advance_ledger(&env, TIMEOUT);
    client.reclaim_wager(&player);
    // Player should be able to start a new game after reclaim
    let result = client.try_start_game(
        &player,
        &Side::Heads,
        &5_000_000,
        &make_commitment(&env, 42, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()),
    );
    assert!(result.is_ok(), "player must be able to start a new game after reclaim");
}

// ── Timeout doesn't trigger prematurely ──────────────────────────────────────

#[test]
fn test_reclaim_wager_cannot_be_called_immediately_after_start_game() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &5_000_000, &make_commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    // No ledger advance — should be rejected
    assert_eq!(
        client.try_reclaim_wager(&player),
        Err(Ok(Error::RevealTimeout))
    );
}

#[test]
fn test_reveal_before_timeout_prevents_reclaim() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    let secret = make_secret(&env, 1);
    let commitment = make_commitment(&env, 1);
    client.start_game(&player, &Side::Heads, &5_000_000, &commitment).sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into());
    // Reveal before timeout
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));
    // Advance past timeout
    advance_ledger(&env, TIMEOUT + 10);
    // Game is now in Revealed phase — reclaim must be rejected
    assert_eq!(
        client.try_reclaim_wager(&player),
        Err(Ok(Error::InvalidPhase))
    );
}

// ── Property 26: Timeout wager reclaim ───────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// PROPERTY 26: For any wager in [min, max], reclaim_wager succeeds after
    /// REVEAL_TIMEOUT_LEDGERS and returns exactly the original wager amount.
    #[test]
    fn prop_26_reclaim_returns_exact_wager(
        wager in 1_000_000i128..=100_000_000i128,
        extra_ledgers in 0u32..=1000u32,
    ) {
        let (env, client, contract_id) = setup();
        fund(&env, &contract_id, 1_000_000_000_000i128);
        let player = Address::generate(&env);
        let start = env.ledger().sequence();
        inject_committed_game(&env, &contract_id, &player, wager, start);
        advance_ledger(&env, TIMEOUT + extra_ledgers);
        let result = client.try_reclaim_wager(&player);
        prop_assert_eq!(result, Ok(wager));
    }

    /// PROPERTY 26b: reclaim_wager always fails before the timeout window expires.
    #[test]
    fn prop_26b_reclaim_rejected_before_timeout(
        wager in 1_000_000i128..=100_000_000i128,
        advance in 0u32..TIMEOUT,
    ) {
        let (env, client, contract_id) = setup();
        fund(&env, &contract_id, 1_000_000_000_000i128);
        let player = Address::generate(&env);
        let start = env.ledger().sequence();
        inject_committed_game(&env, &contract_id, &player, wager, start);
        advance_ledger(&env, advance);
        let result = client.try_reclaim_wager(&player);
        prop_assert_eq!(result, Err(Ok(Error::RevealTimeout)));
    }

    /// PROPERTY 26c: reserve_balance increases by exactly the wager after reclaim.
    #[test]
    fn prop_26c_reserve_increases_by_wager_on_reclaim(
        wager in 1_000_000i128..=100_000_000i128,
    ) {
        let (env, client, contract_id) = setup();
        let initial_reserve = 500_000_000i128;
        fund(&env, &contract_id, initial_reserve);
        let player = Address::generate(&env);
        let start = env.ledger().sequence();
        inject_committed_game(&env, &contract_id, &player, wager, start);
        advance_ledger(&env, TIMEOUT);
        client.reclaim_wager(&player);
        let stats = env.as_contract(&contract_id, || CoinflipContract::load_stats(&env));
        prop_assert_eq!(stats.reserve_balance, initial_reserve + wager);
    }

    /// PROPERTY 26d: game state is always deleted after a successful reclaim.
    #[test]
    fn prop_26d_game_deleted_after_reclaim(
        wager in 1_000_000i128..=100_000_000i128,
    ) {
        let (env, client, contract_id) = setup();
        fund(&env, &contract_id, 1_000_000_000_000i128);
        let player = Address::generate(&env);
        let start = env.ledger().sequence();
        inject_committed_game(&env, &contract_id, &player, wager, start);
        advance_ledger(&env, TIMEOUT);
        client.reclaim_wager(&player);
        let game = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player).unwrap()
        });
        prop_assert!(game.is_none());
    }
}
