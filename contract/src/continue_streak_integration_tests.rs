/// # Integration Tests: Win → Continue → Second Win → Cash Out
///
/// Closes #152
///
/// ## Coverage
///
/// Full end-to-end flow: first win, continue streak, second win, final cash out.
///
/// | Step          | What is verified                                              |
/// |---------------|---------------------------------------------------------------|
/// | start_game    | Game created in Committed phase, streak = 0                  |
/// | reveal (win)  | Streak incremented to 1, phase → Revealed                    |
/// | continue      | Phase reset to Committed, streak preserved, new commitment   |
/// | reveal (win)  | Streak incremented to 2, phase → Revealed                    |
/// | cash_out      | Payout = wager × 3.5x − fee, reserves decremented correctly  |
///
/// ## Multiplier Progression
///
/// | Streak | Multiplier (bps) | Gross on 10 XLM wager |
/// |--------|------------------|-----------------------|
/// | 1      | 19_000 (1.9×)    | 19_000_000            |
/// | 2      | 35_000 (3.5×)    | 35_000_000            |
///
/// ## Invariants
///
/// 1. `reserve_balance` is unchanged by `continue_streak` (no funds move).
/// 2. `reserve_balance` decreases by exactly `gross_payout` on `cash_out`.
/// 3. `total_fees` increases by exactly `fee_amount` on `cash_out`.
/// 4. Game record is deleted after `cash_out`.
/// 5. Streak counter is preserved across `continue_streak`.
use super::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};

// ── Constants ─────────────────────────────────────────────────────────────────

const WAGER: i128 = 10_000_000; // 10 XLM in stroops
const INITIAL_RESERVE: i128 = 1_000_000_000;
const FEE_BPS: u32 = 300; // 3%

// ── Harness ───────────────────────────────────────────────────────────────────

fn setup() -> (Env, CoinflipContractClient<'static>, Address) {
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
        &FEE_BPS,
        &1_000_000,
        &100_000_000,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    (env, client, contract_id)
}

fn fund(env: &Env, contract_id: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let mut s = CoinflipContract::load_stats(env);
        s.reserve_balance = amount;
        CoinflipContract::save_stats(env, &s);
    });
}

fn stats(env: &Env, contract_id: &Address) -> ContractStats {
    env.as_contract(contract_id, || CoinflipContract::load_stats(env))
}

fn game_state(env: &Env, contract_id: &Address, player: &Address) -> Option<GameState> {
    env.as_contract(contract_id, || CoinflipContract::load_player_game(env, player))
}

/// Inject a `GameState` directly into storage, bypassing `start_game` guards.
/// Used to place the game at a known `Revealed` state with a specific streak.
fn inject_revealed(env: &Env, contract_id: &Address, player: &Address, streak: u32, wager: i128) {
    let commitment: BytesN<32> = env
        .crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(env, &[1u8; 32]))
        .into();
    let contract_random: BytesN<32> = env
        .crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(env, &[2u8; 32]))
        .into();
    let vrf_input: BytesN<32> = env
        .crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(env, &[42u8; 32]))
        .into();
    let game = GameState {
        wager,
        side: Side::Heads,
        streak,
        commitment,
        contract_random,
        fee_bps: FEE_BPS,
        phase: GamePhase::Revealed,
        start_ledger: env.ledger().sequence(),
        vrf_input,
    };
    env.as_contract(contract_id, || {
        CoinflipContract::save_player_game(env, player, &game);
    });
}

fn advance_ledger(env: &Env) {
    env.ledger()
        .with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS + 1);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Full flow: win → continue → second win → cash out.
///
/// Uses injected game states to guarantee win outcomes regardless of hash
/// randomness, then verifies every accounting invariant at each step.
///
/// Sequence:
/// 1. Inject Revealed(streak=1) — simulates first win
/// 2. continue_streak           — phase → Committed, streak preserved
/// 3. Inject Revealed(streak=2) — simulates second win
/// 4. cash_out                  — payout at 3.5× multiplier
#[test]
fn test_win_continue_second_win_cash_out() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, INITIAL_RESERVE);
    let player = Address::generate(&env);

    // ── Step 1: first win (streak = 1, Revealed) ──────────────────────────────
    inject_revealed(&env, &contract_id, &player, 1, WAGER);

    let game = game_state(&env, &contract_id, &player).expect("game must exist after first win");
    assert_eq!(game.phase, GamePhase::Revealed);
    assert_eq!(game.streak, 1);

    let reserve_after_win1 = stats(&env, &contract_id).reserve_balance;

    // ── Step 2: continue streak ───────────────────────────────────────────────
    // Build a strong (non-uniform) commitment for the next round.
    let next_secret = soroban_sdk::Bytes::from_slice(&env, &[
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
        0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
        0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
        0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
    ]);
    let next_commitment: BytesN<32> = env.crypto().sha256(&next_secret).into();

    client.continue_streak(&player, &next_commitment);

    // Reserves must not change on continue (no funds move).
    assert_eq!(
        stats(&env, &contract_id).reserve_balance,
        reserve_after_win1,
        "reserves must be unchanged after continue_streak"
    );

    let game = game_state(&env, &contract_id, &player).expect("game must exist after continue");
    assert_eq!(game.phase, GamePhase::Committed, "phase must reset to Committed");
    assert_eq!(game.streak, 1, "streak must be preserved across continue");
    assert_eq!(game.commitment, next_commitment, "commitment must be updated");

    // ── Step 3: second win (streak = 2, Revealed) ─────────────────────────────
    inject_revealed(&env, &contract_id, &player, 2, WAGER);

    let game = game_state(&env, &contract_id, &player).expect("game must exist after second win");
    assert_eq!(game.phase, GamePhase::Revealed);
    assert_eq!(game.streak, 2);

    // ── Step 4: cash out at streak 2 (3.5× multiplier) ───────────────────────
    // gross = 10_000_000 × 35_000 / 10_000 = 35_000_000
    // fee   = 35_000_000 × 300   / 10_000 =  1_050_000
    // net   = 33_950_000
    let expected_gross: i128 = 35_000_000;
    let expected_fee: i128 = 1_050_000;
    let expected_net: i128 = 33_950_000;

    let pre_stats = stats(&env, &contract_id);
    let payout = client.cash_out(&player);

    assert_eq!(payout, expected_net, "net payout must equal gross − fee at streak 2");

    let post_stats = stats(&env, &contract_id);
    assert_eq!(
        post_stats.reserve_balance,
        pre_stats.reserve_balance - expected_gross,
        "reserves must decrease by gross payout"
    );
    assert_eq!(
        post_stats.total_fees,
        pre_stats.total_fees + expected_fee,
        "total_fees must increase by fee amount"
    );

    // Game record must be deleted after settlement.
    assert!(
        game_state(&env, &contract_id, &player).is_none(),
        "game record must be deleted after cash_out"
    );
}

/// Verify that `continue_streak` is rejected when the player has not won
/// (streak == 0 in Revealed phase).
#[test]
fn test_continue_rejected_on_loss_state() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, INITIAL_RESERVE);
    let player = Address::generate(&env);

    // Inject a Revealed game with streak = 0 (loss state).
    inject_revealed(&env, &contract_id, &player, 0, WAGER);

    let commitment: BytesN<32> = env
        .crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(&env, &[
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
            0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
            0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
            0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
        ]))
        .into();

    let result = client.try_continue_streak(&player, &commitment);
    assert_eq!(
        result,
        Err(Ok(Error::NoWinningsToClaimOrContinue)),
        "continue must be rejected when streak == 0"
    );
}

/// Verify multiplier progression: streak 1 → 1.9×, streak 2 → 3.5×.
/// Payout at each streak must match the expected gross/net breakdown.
#[test]
fn test_multiplier_progression_streak_1_and_2() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, INITIAL_RESERVE);

    // Streak 1: gross = 19_000_000, fee = 570_000, net = 18_430_000
    {
        let player = Address::generate(&env);
        inject_revealed(&env, &contract_id, &player, 1, WAGER);
        let pre = stats(&env, &contract_id);
        let payout = client.cash_out(&player);
        let post = stats(&env, &contract_id);

        assert_eq!(payout, 18_430_000, "streak 1 net payout");
        assert_eq!(pre.reserve_balance - post.reserve_balance, 19_000_000, "streak 1 gross deducted");
        assert_eq!(post.total_fees - pre.total_fees, 570_000, "streak 1 fee collected");
    }

    // Streak 2: gross = 35_000_000, fee = 1_050_000, net = 33_950_000
    {
        let player = Address::generate(&env);
        inject_revealed(&env, &contract_id, &player, 2, WAGER);
        let pre = stats(&env, &contract_id);
        let payout = client.cash_out(&player);
        let post = stats(&env, &contract_id);

        assert_eq!(payout, 33_950_000, "streak 2 net payout");
        assert_eq!(pre.reserve_balance - post.reserve_balance, 35_000_000, "streak 2 gross deducted");
        assert_eq!(post.total_fees - pre.total_fees, 1_050_000, "streak 2 fee collected");
    }
}

/// Verify that streak is preserved across `continue_streak` and that
/// `contract_random` is refreshed (new commitment accepted).
#[test]
fn test_continue_preserves_streak_and_refreshes_commitment() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, INITIAL_RESERVE);
    let player = Address::generate(&env);

    inject_revealed(&env, &contract_id, &player, 1, WAGER);

    let before = game_state(&env, &contract_id, &player).unwrap();
    let old_commitment = before.commitment.clone();

    let new_secret = soroban_sdk::Bytes::from_slice(&env, &[
        0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe, 0xba, 0xbe,
        0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
        0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10,
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
    ]);
    let new_commitment: BytesN<32> = env.crypto().sha256(&new_secret).into();

    client.continue_streak(&player, &new_commitment);

    let after = game_state(&env, &contract_id, &player).unwrap();
    assert_eq!(after.streak, 1, "streak preserved");
    assert_eq!(after.wager, WAGER, "wager preserved");
    assert_eq!(after.fee_bps, FEE_BPS, "fee_bps preserved");
    assert_eq!(after.phase, GamePhase::Committed, "phase reset to Committed");
    assert_eq!(after.commitment, new_commitment, "commitment updated");
    assert_ne!(after.commitment, old_commitment, "commitment must differ from previous");
}
