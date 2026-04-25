/// # Streak Preservation Tests
///
/// ## Streak Management Logic
///
/// The streak counter lives in `GameState.streak` and follows these rules:
///
/// | Event                        | Effect on streak                        |
/// |------------------------------|-----------------------------------------|
/// | `start_game`                 | Initialized to 0                        |
/// | `reveal` → win               | `streak = streak.saturating_add(1)`     |
/// | `reveal` → loss              | Game deleted (streak irrelevant)        |
/// | `continue_streak`            | Streak **preserved** (unchanged)        |
/// | `cash_out` / `claim_winnings`| Game deleted (streak irrelevant)        |
///
/// ## Invariants
///
/// 1. Streak starts at 0 for every new game.
/// 2. Each win increments streak by exactly 1.
/// 3. `continue_streak` never modifies streak.
/// 4. Loss deletes the game record; no streak survives a loss.
/// 5. Streak is monotonically non-decreasing within a single game lifetime.
/// 6. Multiplier tier is determined by streak at settlement time.
use super::*;
use soroban_sdk::testutils::Address as _;

// ── Harness ───────────────────────────────────────────────────────────────────

const WAGER: i128 = 10_000_000;

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

fn fund(env: &Env, contract_id: &Address) {
    env.as_contract(contract_id, || {
        let mut stats = CoinflipContract::load_stats(env);
        stats.reserve_balance = 1_000_000_000;
        CoinflipContract::save_stats(env, &stats);
    });
}

/// Inject a GameState with a specific streak directly into storage.
fn inject(env: &Env, contract_id: &Address, player: &Address, phase: GamePhase, streak: u32) {
    // commitment = sha256([1u8;32]) — matches secret [1u8;32]
    let commitment: BytesN<32> = env
        .crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(env, &[1u8; 32]))
        .into();
    let contract_random: BytesN<32> = env
        .crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(env, &[2u8; 32]))
        .into();
    let game = GameState {
        wager: WAGER,
        side: Side::Heads,
        streak,
        commitment,
        contract_random,
        fee_bps: 300,
        phase,
        start_ledger: env.ledger().sequence(),
    
        vrf_input: env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into(),
    };
    env.as_contract(contract_id, || {
        CoinflipContract::save_player_game(env, player, &game);
    });
}

fn streak_of(env: &Env, contract_id: &Address, player: &Address) -> u32 {
    env.as_contract(contract_id, || {
        CoinflipContract::load_player_game(env, player)
            .unwrap()
            .unwrap()
            .streak
    })
}

fn new_commitment(env: &Env, seed: u8) -> BytesN<32> {
    env.crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(env, &[seed; 32]))
        .into()
}

// ── Streak initialization ─────────────────────────────────────────────────────

/// start_game always initializes streak to 0.
#[test]
fn start_game_initializes_streak_to_zero() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id);
    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &WAGER, &new_commitment(&env, 7, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    assert_eq!(streak_of(&env, &contract_id, &player), 0);
}

// ── Streak increment on win ───────────────────────────────────────────────────

/// reveal win from streak 0 → streak 1.
#[test]
fn reveal_win_increments_streak_from_zero() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed, 0);
    let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    let won = client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));
    if won {
        assert_eq!(streak_of(&env, &contract_id, &player), 1);
    }
}

/// reveal win from streak N → streak N+1 for N in {1, 2, 3}.
#[test]
fn reveal_win_increments_streak_by_exactly_one() {
    for initial in [1u32, 2, 3] {
        let (env, client, contract_id) = setup();
        fund(&env, &contract_id);
        let player = Address::generate(&env);
        inject(&env, &contract_id, &player, GamePhase::Committed, initial);
        let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);
        env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
        let won = client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));
        if won {
            assert_eq!(
                streak_of(&env, &contract_id, &player),
                initial + 1,
                "win from streak {initial} must yield streak {}",
                initial + 1
            );
        }
    }
}

/// reveal win from streak 4 → streak 5 (counter keeps incrementing past tier cap).
#[test]
fn reveal_win_increments_streak_past_tier_cap() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed, 4);
    let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    let won = client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));
    if won {
        assert_eq!(streak_of(&env, &contract_id, &player), 5);
    }
}

// ── Streak preservation across continue_streak ────────────────────────────────

/// continue_streak does not modify streak.
#[test]
fn continue_streak_preserves_streak() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Revealed, 2);
    client.continue_streak(&player, &new_commitment(&env, 42));
    assert_eq!(streak_of(&env, &contract_id, &player), 2);
}

/// Streak is preserved across multiple continue_streak calls.
#[test]
fn streak_preserved_across_multiple_continues() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Revealed, 3);
    // Simulate three continue cycles: Revealed → Committed → Revealed → ...
    for seed in [10u8, 11, 12] {
        // Re-inject as Revealed with same streak to simulate winning the next flip
        inject(&env, &contract_id, &player, GamePhase::Revealed, 3);
        client.continue_streak(&player, &new_commitment(&env, seed));
        assert_eq!(streak_of(&env, &contract_id, &player), 3);
    }
}

// ── Streak reset on loss ──────────────────────────────────────────────────────

/// Loss deletes the game record — no streak survives.
#[test]
fn loss_deletes_game_no_streak_survives() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed, 3);
    let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    let won = client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));
    if !won {
        // Game must be gone — no streak to read
        let game = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player).unwrap()
        });
        assert!(game.is_none(), "game record must be deleted after loss");
    }
}

/// After a loss, a new game starts fresh at streak 0.
#[test]
fn new_game_after_loss_starts_at_streak_zero() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed, 5);
    let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    let won = client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));
    if !won {
        // Start a fresh game — must begin at streak 0
        client.start_game(&player, &Side::Heads, &WAGER, &new_commitment(&env, 99, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
        assert_eq!(streak_of(&env, &contract_id, &player), 0);
    }
}

// ── Long streak sequences ─────────────────────────────────────────────────────

/// Simulate 10 consecutive wins via inject+reveal; streak reaches 10.
#[test]
fn ten_consecutive_wins_reach_streak_ten() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id);
    let player = Address::generate(&env);
    let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);

    let mut streak = 0u32;
    let mut wins = 0u32;

    // Drive up to 10 wins; each iteration injects a fresh Committed game at
    // the current streak so the commitment always matches the known secret.
    while wins < 10 {
        inject(&env, &contract_id, &player, GamePhase::Committed, streak);
        env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
        let won = client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));
        if won {
            streak += 1;
            wins += 1;
            assert_eq!(streak_of(&env, &contract_id, &player), streak);
        }
        // On loss the game is deleted; loop re-injects at same streak.
    }

    assert_eq!(streak, 10, "must accumulate exactly 10 wins");
}

/// Streak at boundary 0: reveal win → 1, multiplier is 1.9x tier.
#[test]
fn streak_boundary_zero_win_reaches_tier_1() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed, 0);
    let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    let won = client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));
    if won {
        let s = streak_of(&env, &contract_id, &player);
        assert_eq!(s, 1);
        assert_eq!(get_multiplier(s), 19_000); // 1.9x
    }
}

/// Streak at boundary 3: reveal win → 4, multiplier hits 10x cap.
#[test]
fn streak_boundary_three_win_reaches_cap() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed, 3);
    let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    let won = client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));
    if won {
        let s = streak_of(&env, &contract_id, &player);
        assert_eq!(s, 4);
        assert_eq!(get_multiplier(s), 100_000); // 10x cap
    }
}

/// Streak at boundary 4+: multiplier stays capped, counter still increments.
#[test]
fn streak_boundary_above_cap_multiplier_stays_capped() {
    for initial in [4u32, 9, 10] {
        let (env, client, contract_id) = setup();
        fund(&env, &contract_id);
        let player = Address::generate(&env);
        inject(&env, &contract_id, &player, GamePhase::Committed, initial);
        let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);
        env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
        let won = client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));
        if won {
            let s = streak_of(&env, &contract_id, &player);
            assert_eq!(s, initial + 1);
            assert_eq!(get_multiplier(s), 100_000, "multiplier must stay at 10x for streak {s}");
        }
    }
}

// ── Monotonicity ──────────────────────────────────────────────────────────────

/// Streak never decreases: after a win it is strictly greater than before.
#[test]
fn streak_never_decreases_after_win() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id);
    let player = Address::generate(&env);
    let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);

    let mut prev = 0u32;
    for initial in [0u32, 1, 2, 3, 4] {
        inject(&env, &contract_id, &player, GamePhase::Committed, initial);
        env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
        let won = client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));
        if won {
            let current = streak_of(&env, &contract_id, &player);
            assert!(
                current > prev || current == initial + 1,
                "streak must not decrease: was {prev}, now {current}"
            );
            prev = current;
        }
    }
}
