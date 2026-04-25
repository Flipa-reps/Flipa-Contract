/// Unit tests for contract pause/unpause functionality.
///
/// Issue: add-unit-tests-pause-functionality
///
/// Covers:
///   - Test set_paused function (admin only)
///   - Test new games are rejected when paused
///   - Test existing games can reveal when paused
///   - Test existing games can cash out when paused
///   - Test existing games can continue when paused
///   - Test unpause allows new games again
///   - Property 28: Pause behavior test
///   - Admin access control
use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use proptest::prelude::*;

// ── Harness ───────────────────────────────────────────────────────────────────

fn setup() -> (Env, CoinflipContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000, &BytesN::from_array(&env, &[0u8; 32]));
    (env, client, contract_id, admin)
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

fn inject_game(
    env: &Env,
    contract_id: &Address,
    player: &Address,
    phase: GamePhase,
    streak: u32,
    wager: i128,
) {
    let game = GameState {
        wager,
        side: Side::Heads,
        streak,
        commitment: make_commitment(env, 1),
        contract_random: make_commitment(env, 2),
        fee_bps: 300,
        phase,
        start_ledger: env.ledger().sequence(),
    
        vrf_input: env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into(),
    };
    env.as_contract(contract_id, || {
        CoinflipContract::save_player_game(env, player, &game);
    });
}

fn is_paused(env: &Env, contract_id: &Address) -> bool {
    env.as_contract(contract_id, || CoinflipContract::load_config(env).paused)
}

// ── set_paused: admin only ────────────────────────────────────────────────────

#[test]
fn test_set_paused_true_by_admin_succeeds() {
    let (env, client, contract_id, admin) = setup();
    assert!(client.try_set_paused(&admin, &true).is_ok());
    assert!(is_paused(&env, &contract_id));
}

#[test]
fn test_set_paused_false_by_admin_succeeds() {
    let (env, client, contract_id, admin) = setup();
    client.set_paused(&admin, &true);
    assert!(client.try_set_paused(&admin, &false).is_ok());
    assert!(!is_paused(&env, &contract_id));
}

#[test]
fn test_set_paused_by_non_admin_rejected() {
    let (env, client, _contract_id, _admin) = setup();
    let stranger = Address::generate(&env);
    assert_eq!(
        client.try_set_paused(&stranger, &true),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn test_set_paused_non_admin_does_not_mutate_config() {
    let (env, client, contract_id, _admin) = setup();
    let stranger = Address::generate(&env);
    let before: ContractConfig = env.as_contract(&contract_id, || {
        env.storage().persistent().get(&StorageKey::Config).unwrap().unwrap()
    });
    let _ = client.try_set_paused(&stranger, &true);
    let after: ContractConfig = env.as_contract(&contract_id, || {
        env.storage().persistent().get(&StorageKey::Config).unwrap().unwrap()
    });
    assert_eq!(before, after);
}

#[test]
fn test_set_paused_idempotent_pause() {
    let (env, client, contract_id, admin) = setup();
    client.set_paused(&admin, &true);
    client.set_paused(&admin, &true); // second pause is a no-op
    assert!(is_paused(&env, &contract_id));
}

#[test]
fn test_set_paused_idempotent_unpause() {
    let (env, client, contract_id, admin) = setup();
    client.set_paused(&admin, &false); // already unpaused
    assert!(!is_paused(&env, &contract_id));
}

// ── New games rejected when paused ───────────────────────────────────────────

#[test]
fn test_start_game_rejected_when_paused() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    client.set_paused(&admin, &true);
    let player = Address::generate(&env);
    assert_eq!(
        client.try_start_game(&player, &Side::Heads, &5_000_000, &make_commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into())),
        Err(Ok(Error::ContractPaused))
    );
}

#[test]
fn test_start_game_rejected_when_paused_no_state_mutation() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    client.set_paused(&admin, &true);
    let player = Address::generate(&env);
    let before_stats: ContractStats = env.as_contract(&contract_id, || {
        env.storage().persistent().get(&StorageKey::Stats).unwrap().unwrap()
    });
    let _ = client.try_start_game(&player, &Side::Heads, &5_000_000, &make_commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    let after_stats: ContractStats = env.as_contract(&contract_id, || {
        env.storage().persistent().get(&StorageKey::Stats).unwrap().unwrap()
    });
    assert_eq!(before_stats, after_stats);
    let game = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });
    assert!(game.is_none());
}

// ── Existing games can reveal when paused ────────────────────────────────────

#[test]
fn test_reveal_succeeds_when_paused() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    let secret = make_secret(&env, 1);
    let commitment = make_commitment(&env, 1);
    client.start_game(&player, &Side::Heads, &5_000_000, &commitment).sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into());
    client.set_paused(&admin, &true);
    // reveal must still work
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    let result = client.try_reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));
    assert_eq!(result, Ok(true));
    let game = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });
    assert_eq!(game.phase, GamePhase::Revealed);
}

// ── Existing games can cash out when paused ───────────────────────────────────

#[test]
fn test_cash_out_succeeds_when_paused() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject_game(&env, &contract_id, &player, GamePhase::Revealed, 1, 5_000_000);
    client.set_paused(&admin, &true);
    let result = client.try_cash_out(&player);
    assert!(result.is_ok(), "cash_out must succeed while paused");
}

// ── Existing games can continue when paused ───────────────────────────────────

#[test]
fn test_continue_streak_succeeds_when_paused() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject_game(&env, &contract_id, &player, GamePhase::Revealed, 1, 5_000_000);
    client.set_paused(&admin, &true);
    let result = client.try_continue_streak(&player, &make_commitment(&env, 42));
    assert!(result.is_ok(), "continue_streak must succeed while paused");
    let game = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });
    assert_eq!(game.phase, GamePhase::Committed);
}

// ── Unpause restores new game creation ───────────────────────────────────────

#[test]
fn test_unpause_allows_new_games() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    client.set_paused(&admin, &true);
    // Confirm paused
    assert_eq!(
        client.try_start_game(
            &Address::generate(&env),
            &Side::Heads,
            &5_000_000,
            &make_commitment(&env, 1)
        ),
        Err(Ok(Error::ContractPaused))
    );
    // Unpause
    client.set_paused(&admin, &false);
    let player = Address::generate(&env);
    let result = client.try_start_game(&player, &Side::Heads, &5_000_000, &make_commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    assert!(result.is_ok(), "start_game must succeed after unpause");
}

#[test]
fn test_pause_unpause_cycle_multiple_times() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    for _ in 0..3 {
        client.set_paused(&admin, &true);
        assert!(is_paused(&env, &contract_id));
        client.set_paused(&admin, &false);
        assert!(!is_paused(&env, &contract_id));
    }
}

// ── Full in-flight game lifecycle while paused ────────────────────────────────

#[test]
fn test_full_game_lifecycle_while_paused() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    let secret = make_secret(&env, 1);
    let commitment = make_commitment(&env, 1);

    // Start game before pause
    client.start_game(&player, &Side::Heads, &5_000_000, &commitment).sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into());

    // Pause
    client.set_paused(&admin, &true);

    // Reveal while paused
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    let won = client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));
    assert!(won);

    // Continue while paused
    let next_commitment = make_commitment(&env, 42);
    client.continue_streak(&player, &next_commitment);
    let game = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });
    assert_eq!(game.phase, GamePhase::Committed);
    assert_eq!(game.streak, 1);

    // Reveal again while paused
    let secret2 = make_secret(&env, 42);
    let commitment2 = make_commitment(&env, 42);
    // Update commitment to match secret2
    let mut g = game.clone();
    g.commitment = commitment2;
    env.as_contract(&contract_id, || {
        CoinflipContract::save_player_game(&env, &player, &g);
    });
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    let won2 = client.reveal(&player, &secret2, &BytesN::from_array(&env, &[0u8; 64]));
    assert!(won2);

    // Cash out while paused
    let payout = client.cash_out(&player);
    assert!(payout > 0);
}

// ── Property 28: Pause behavior ──────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// PROPERTY 28a: start_game is always rejected when paused, regardless of wager or side.
    #[test]
    fn prop_28a_start_game_always_rejected_when_paused(
        wager in 1_000_000i128..=100_000_000i128,
        side_pick in any::<bool>(),
        commitment_bytes in prop::array::uniform32(any::<u8>()),
    ) {
        let (env, client, contract_id, admin) = setup();
        fund(&env, &contract_id, 1_000_000_000_000i128);
        client.set_paused(&admin, &true);
        let player = Address::generate(&env);
        let side = if side_pick { Side::Heads } else { Side::Tails };
        let commitment = BytesN::from_array(&env, &commitment_bytes);
        let result = client.try_start_game(&player, &side, &wager, &commitment).sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into());
        prop_assert_eq!(result, Err(Ok(Error::ContractPaused)));
    }

    /// PROPERTY 28b: cash_out always succeeds for a valid won game while paused.
    #[test]
    fn prop_28b_cash_out_succeeds_while_paused(
        wager in 1_000_000i128..=100_000_000i128,
        streak in 1u32..=4u32,
    ) {
        let (env, client, contract_id, admin) = setup();
        fund(&env, &contract_id, 1_000_000_000_000i128);
        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, streak, wager);
        client.set_paused(&admin, &true);
        let result = client.try_cash_out(&player);
        prop_assert!(result.is_ok(), "cash_out must succeed while paused (streak={})", streak);
    }

    /// PROPERTY 28c: pause flag is the only config field mutated by set_paused.
    #[test]
    fn prop_28c_set_paused_only_mutates_pause_flag(
        pause_target in any::<bool>(),
    ) {
        let (env, client, contract_id, admin) = setup();
        let before: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });
        client.set_paused(&admin, &pause_target);
        let after: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });
        prop_assert_eq!(before.admin, after.admin);
        prop_assert_eq!(before.treasury, after.treasury);
        prop_assert_eq!(before.token, after.token);
        prop_assert_eq!(before.fee_bps, after.fee_bps);
        prop_assert_eq!(before.min_wager, after.min_wager);
        prop_assert_eq!(before.max_wager, after.max_wager);
        prop_assert_eq!(after.paused, pause_target);
    }

    /// PROPERTY 28d: unpause always re-enables start_game for valid wagers.
    #[test]
    fn prop_28d_unpause_reenables_start_game(
        wager in 1_000_000i128..=100_000_000i128,
    ) {
        let (env, client, contract_id, admin) = setup();
        fund(&env, &contract_id, 1_000_000_000_000i128);
        client.set_paused(&admin, &true);
        client.set_paused(&admin, &false);
        let player = Address::generate(&env);
        let commitment_bytes = [42u8; 32];
        let commitment = BytesN::from_array(&env, &commitment_bytes);
        let result = client.try_start_game(&player, &Side::Heads, &wager, &commitment).sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into());
        prop_assert!(result.is_ok(), "start_game must succeed after unpause (wager={})", wager);
    }
}


// ── Pause security validation (Issue #411) ────────────────────────────────────

/// Test unauthorized pause attempts are rejected.
#[test]
fn test_unauthorized_pause_attempt_rejected() {
    let (env, client, _contract_id, _admin) = setup();
    let unauthorized = Address::generate(&env);
    let result = client.try_set_paused(&unauthorized, &true);
    assert_eq!(result, Err(Ok(Error::Unauthorized)),
        "unauthorized caller should not be able to pause");
}

/// Test unauthorized unpause attempts are rejected.
#[test]
fn test_unauthorized_unpause_attempt_rejected() {
    let (env, client, contract_id, admin) = setup();
    client.set_paused(&admin, &true);
    
    let unauthorized = Address::generate(&env);
    let result = client.try_set_paused(&unauthorized, &false);
    assert_eq!(result, Err(Ok(Error::Unauthorized)),
        "unauthorized caller should not be able to unpause");
    
    // Verify pause state unchanged
    assert!(is_paused(&env, &contract_id));
}

/// Test that multiple unauthorized attempts don't affect pause state.
#[test]
fn test_multiple_unauthorized_attempts_dont_mutate_state() {
    let (env, client, contract_id, admin) = setup();
    let initial_paused = is_paused(&env, &contract_id);
    
    for _ in 0..5 {
        let unauthorized = Address::generate(&env);
        let _ = client.try_set_paused(&unauthorized, &true);
        let _ = client.try_set_paused(&unauthorized, &false);
    }
    
    let final_paused = is_paused(&env, &contract_id);
    assert_eq!(initial_paused, final_paused,
        "unauthorized attempts should not change pause state");
}

/// Test start_game rejection when paused with various wagers.
#[test]
fn test_start_game_rejection_various_wagers_when_paused() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 10_000_000_000);
    client.set_paused(&admin, &true);
    
    for wager in [1_000_000, 10_000_000, 50_000_000, 100_000_000] {
        let player = Address::generate(&env);
        let result = client.try_start_game(
            &player,
            &Side::Heads,
            &wager,
            &make_commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()),
        );
        assert_eq!(result, Err(Ok(Error::ContractPaused)),
            "start_game should be rejected for wager {wager} when paused");
    }
}

/// Test start_game rejection when paused with both sides.
#[test]
fn test_start_game_rejection_both_sides_when_paused() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 10_000_000_000);
    client.set_paused(&admin, &true);
    
    for side in [Side::Heads, Side::Tails] {
        let player = Address::generate(&env);
        let result = client.try_start_game(
            &player,
            &side,
            &10_000_000,
            &make_commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()),
        );
        assert_eq!(result, Err(Ok(Error::ContractPaused)),
            "start_game should be rejected for side {side:?} when paused");
    }
}

/// Test reveal succeeds when paused (existing game can continue).
#[test]
fn test_reveal_succeeds_when_paused_existing_game() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    
    let player = Address::generate(&env);
    let secret = make_secret(&env, 1);
    let commitment = make_commitment(&env, 1);
    
    client.start_game(&player, &Side::Heads, &5_000_000, &commitment).sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into());
    client.set_paused(&admin, &true);
    
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    let result = client.try_reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));
    assert!(result.is_ok(), "reveal should succeed when paused");
    
    let game = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });
    assert_eq!(game.phase, GamePhase::Revealed);
}

/// Test cash_out succeeds when paused (existing game can settle).
#[test]
fn test_cash_out_succeeds_when_paused_existing_game() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    
    let player = Address::generate(&env);
    inject_game(&env, &contract_id, &player, GamePhase::Revealed, 2, 5_000_000);
    
    client.set_paused(&admin, &true);
    
    let result = client.try_cash_out(&player);
    assert!(result.is_ok(), "cash_out should succeed when paused");
    
    let payout = result.unwrap();
    assert!(payout > 0, "payout should be positive");
}

/// Test continue_streak succeeds when paused (existing game can continue).
#[test]
fn test_continue_streak_succeeds_when_paused_existing_game() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    
    let player = Address::generate(&env);
    inject_game(&env, &contract_id, &player, GamePhase::Revealed, 1, 5_000_000);
    
    client.set_paused(&admin, &true);
    
    let result = client.try_continue_streak(&player, &make_commitment(&env, 42));
    assert!(result.is_ok(), "continue_streak should succeed when paused");
    
    let game = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });
    assert_eq!(game.phase, GamePhase::Committed);
    assert_eq!(game.streak, 1);
}

/// Test pause state persistence across multiple operations.
#[test]
fn test_pause_state_persistence_across_operations() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    
    // Pause
    client.set_paused(&admin, &true);
    assert!(is_paused(&env, &contract_id));
    
    // Try to start game (should fail)
    let player1 = Address::generate(&env);
    let result1 = client.try_start_game(
        &player1,
        &Side::Heads,
        &5_000_000,
        &make_commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()),
    );
    assert_eq!(result1, Err(Ok(Error::ContractPaused)));
    
    // Pause state should still be true
    assert!(is_paused(&env, &contract_id));
    
    // Try another start_game (should also fail)
    let player2 = Address::generate(&env);
    let result2 = client.try_start_game(
        &player2,
        &Side::Heads,
        &5_000_000,
        &make_commitment(&env, 2, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()),
    );
    assert_eq!(result2, Err(Ok(Error::ContractPaused)));
    
    // Pause state should still be true
    assert!(is_paused(&env, &contract_id));
}

/// Test pause state recovery after unpause.
#[test]
fn test_pause_state_recovery_after_unpause() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    
    // Pause
    client.set_paused(&admin, &true);
    assert!(is_paused(&env, &contract_id));
    
    // Unpause
    client.set_paused(&admin, &false);
    assert!(!is_paused(&env, &contract_id));
    
    // start_game should now succeed
    let player = Address::generate(&env);
    let result = client.try_start_game(
        &player,
        &Side::Heads,
        &5_000_000,
        &make_commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()),
    );
    assert!(result.is_ok(), "start_game should succeed after unpause");
}

/// Test pause state consistency with config storage.
#[test]
fn test_pause_state_consistency_with_config() {
    let (env, client, contract_id, admin) = setup();
    
    // Get initial config
    let config_before: ContractConfig = env.as_contract(&contract_id, || {
        env.storage().persistent().get(&StorageKey::Config).unwrap().unwrap()
    });
    assert!(!config_before.paused);
    
    // Pause
    client.set_paused(&admin, &true);
    
    // Get config after pause
    let config_after: ContractConfig = env.as_contract(&contract_id, || {
        env.storage().persistent().get(&StorageKey::Config).unwrap().unwrap()
    });
    assert!(config_after.paused);
    
    // Verify other fields unchanged
    assert_eq!(config_before.admin, config_after.admin);
    assert_eq!(config_before.treasury, config_after.treasury);
    assert_eq!(config_before.token, config_after.token);
    assert_eq!(config_before.fee_bps, config_after.fee_bps);
    assert_eq!(config_before.min_wager, config_after.min_wager);
    assert_eq!(config_before.max_wager, config_after.max_wager);
}

/// Test that pause doesn't affect existing game state.
#[test]
fn test_pause_doesnt_affect_existing_game_state() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    
    let player = Address::generate(&env);
    inject_game(&env, &contract_id, &player, GamePhase::Revealed, 2, 5_000_000);
    
    let game_before = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });
    
    client.set_paused(&admin, &true);
    
    let game_after = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });
    
    assert_eq!(game_before, game_after,
        "pause should not affect existing game state");
}

/// Test full game lifecycle with pause/unpause cycles.
#[test]
fn test_full_game_lifecycle_with_pause_unpause_cycles() {
    let (env, client, contract_id, admin) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    
    let player = Address::generate(&env);
    let secret = make_secret(&env, 1);
    let commitment = make_commitment(&env, 1);
    
    // Start game
    client.start_game(&player, &Side::Heads, &5_000_000, &commitment).sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into());
    
    // Pause
    client.set_paused(&admin, &true);
    
    // Reveal while paused
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    let won = client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));
    assert!(won);
    
    // Unpause
    client.set_paused(&admin, &false);
    
    // Cash out after unpause
    let payout = client.cash_out(&player);
    assert!(payout > 0);
    
    // Verify game is completed
    let game = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player)
    });
    assert!(game.is_none(), "game should be deleted after cash_out");
}

/// Test authorization enforcement is independent of pause state.
#[test]
fn test_authorization_enforcement_independent_of_pause() {
    let (env, client, _contract_id, admin) = setup();
    let unauthorized = Address::generate(&env);
    
    // Try to pause as unauthorized (should fail)
    let result1 = client.try_set_paused(&unauthorized, &true);
    assert_eq!(result1, Err(Ok(Error::Unauthorized)));
    
    // Pause as admin
    client.set_paused(&admin, &true);
    
    // Try to unpause as unauthorized (should still fail)
    let result2 = client.try_set_paused(&unauthorized, &false);
    assert_eq!(result2, Err(Ok(Error::Unauthorized)));
    
    // Unpause as admin (should succeed)
    let result3 = client.try_set_paused(&admin, &false);
    assert!(result3.is_ok());
}
