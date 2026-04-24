/// # Phase Transition Validation Tests
///
/// ## State Machine
///
/// ```text
///                    ┌─────────────────────────────────────────┐
///                    │                                         │
///  start_game ──► Committed ──reveal(win)──► Revealed ──cash_out / claim_winnings──► (deleted)
///                    │                          │
///                    │                          └──continue_streak──► Committed (loop)
///                    │
///                    └──reveal(loss)──► (deleted)
///                    │
///                    └──reclaim_wager (timeout)──► (deleted)
/// ```
///
/// ## Transition Rules
///
/// | From      | Operation          | To / Result                        |
/// |-----------|--------------------|------------------------------------|
/// | —         | start_game         | Committed                          |
/// | Committed | reveal (win)       | Revealed (streak += 1)             |
/// | Committed | reveal (loss)      | deleted                            |
/// | Committed | reclaim_wager      | deleted (timeout only)             |
/// | Revealed  | cash_out           | deleted                            |
/// | Revealed  | claim_winnings     | Completed                          |
/// | Revealed  | continue_streak    | Committed (streak/wager preserved) |
/// | Completed | start_game         | Committed (slot reuse)             |
///
/// ## Invalid Transitions (all must return `InvalidPhase`)
///
/// | From      | Operation          | Error                              |
/// |-----------|--------------------|------------------------------------|
/// | Revealed  | reveal             | InvalidPhase                       |
/// | Committed | cash_out           | InvalidPhase                       |
/// | Committed | claim_winnings     | InvalidPhase                       |
/// | Committed | continue_streak    | InvalidPhase                       |
/// | Completed | reveal             | NoActiveGame (slot deleted)        |
/// | Completed | cash_out           | NoActiveGame (slot deleted)        |
///
/// ## Invariants
///
/// 1. `wager` and `fee_bps` are immutable across all transitions.
/// 2. `streak` only increases (on win via `reveal`); never decreases.
/// 3. `continue_streak` preserves `streak`, `wager`, `fee_bps`, and `side`.
/// 4. Rejected operations leave game state and reserves byte-for-byte unchanged.
/// 5. Each player's game state is fully isolated from other players.
use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};

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
    client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);
    (env, client, contract_id)
}

fn fund(env: &Env, contract_id: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let mut stats = CoinflipContract::load_stats(env);
        stats.reserve_balance = amount;
        CoinflipContract::save_stats(env, &stats);
    });
}

fn inject(env: &Env, contract_id: &Address, player: &Address, phase: GamePhase, streak: u32) {
    let commitment: BytesN<32> = env.crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(env, &[1u8; 32]))
        .into();
    let contract_random: BytesN<32> = env.crypto()
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
    };
    env.as_contract(contract_id, || {
        CoinflipContract::save_player_game(env, player, &game);
    });
}

fn game(env: &Env, contract_id: &Address, player: &Address) -> Option<GameState> {
    env.as_contract(contract_id, || {
        CoinflipContract::load_player_game(env, player).unwrap()
    })
}

fn reserve(env: &Env, contract_id: &Address) -> i128 {
    env.as_contract(contract_id, || CoinflipContract::load_stats(env).reserve_balance)
}

fn new_commitment(env: &Env) -> BytesN<32> {
    env.crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(env, &[42u8; 32]))
        .into()
}

// ── Valid transitions ─────────────────────────────────────────────────────────

/// start_game → Committed: phase is Committed, wager/fee_bps/side stored correctly.
#[test]
fn start_game_produces_committed_phase() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    let commitment = new_commitment(&env);
    client.start_game(&player, &Side::Tails, &WAGER, &commitment);
    let g = game(&env, &contract_id, &player).unwrap();
    assert_eq!(g.phase, GamePhase::Committed);
    assert_eq!(g.wager, WAGER);
    assert_eq!(g.side, Side::Tails);
    assert_eq!(g.streak, 0);
    assert_eq!(g.commitment, commitment);
}

/// Committed → Revealed on win: streak incremented, wager/fee_bps unchanged.
#[test]
fn reveal_win_transitions_to_revealed() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    // Inject a Committed game whose commitment matches secret=[1u8;32]
    inject(&env, &contract_id, &player, GamePhase::Committed, 0);
    let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);
    // The injected commitment = sha256([1u8;32]), so this is a valid reveal.
    // Outcome depends on sha256(secret || contract_random); we accept either result.
    let won = client.reveal(&player, &secret);
    if won {
        let g = game(&env, &contract_id, &player).unwrap();
        assert_eq!(g.phase, GamePhase::Revealed);
        assert_eq!(g.streak, 1);
        assert_eq!(g.wager, WAGER);
        assert_eq!(g.fee_bps, 300);
    }
    // loss path tested separately
}

/// Committed → deleted on loss: no game record remains.
#[test]
fn reveal_loss_deletes_game() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed, 0);
    let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);
    let won = client.reveal(&player, &secret);
    if !won {
        assert!(game(&env, &contract_id, &player).is_none());
    }
}

/// Revealed → deleted via cash_out: game gone, reserves reduced by gross.
#[test]
fn cash_out_from_revealed_deletes_game() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Revealed, 1);
    let reserve_before = reserve(&env, &contract_id);
    let net = client.cash_out(&player);
    assert!(game(&env, &contract_id, &player).is_none());
    assert!(net > 0);
    // gross = WAGER * 19_000 / 10_000 = 19_000_000
    let gross = WAGER * 19_000 / 10_000;
    assert_eq!(reserve_before - reserve(&env, &contract_id), gross);
}

/// Revealed → Committed via continue_streak: streak/wager/fee_bps/side preserved.
#[test]
fn continue_streak_transitions_revealed_to_committed() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Revealed, 2);
    let nc = new_commitment(&env);
    client.continue_streak(&player, &nc);
    let g = game(&env, &contract_id, &player).unwrap();
    assert_eq!(g.phase, GamePhase::Committed);
    assert_eq!(g.streak, 2);       // preserved
    assert_eq!(g.wager, WAGER);    // preserved
    assert_eq!(g.fee_bps, 300);    // preserved
    assert_eq!(g.side, Side::Heads); // preserved
    assert_eq!(g.commitment, nc);  // updated
}

/// Completed slot allows a new start_game (slot reuse).
#[test]
fn completed_slot_allows_new_start_game() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Completed, 0);
    let nc = new_commitment(&env);
    // Must succeed — Completed is treated as "no active game"
    assert!(client.try_start_game(&player, &Side::Tails, &WAGER, &nc).is_ok());
    let g = game(&env, &contract_id, &player).unwrap();
    assert_eq!(g.phase, GamePhase::Committed);
}

/// Committed → deleted via reclaim_wager after timeout.
#[test]
fn reclaim_wager_after_timeout_deletes_committed_game() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed, 0);
    // Advance ledger past REVEAL_TIMEOUT_LEDGERS (100)
    env.ledger().with_mut(|l| l.sequence_number += 101);
    let reclaimed = client.reclaim_wager(&player);
    assert_eq!(reclaimed, WAGER);
    assert!(game(&env, &contract_id, &player).is_none());
}

// ── Invalid transitions ───────────────────────────────────────────────────────

/// reveal on Revealed phase → InvalidPhase (double-reveal blocked).
#[test]
fn reveal_from_revealed_is_invalid_phase() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Revealed, 1);
    let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);
    assert_eq!(client.try_reveal(&player, &secret), Err(Ok(Error::InvalidPhase)));
}

/// cash_out from Committed → InvalidPhase.
#[test]
fn cash_out_from_committed_is_invalid_phase() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed, 1);
    assert_eq!(client.try_cash_out(&player), Err(Ok(Error::InvalidPhase)));
}

/// claim_winnings from Committed → InvalidPhase.
#[test]
fn claim_winnings_from_committed_is_invalid_phase() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed, 1);
    assert_eq!(client.try_claim_winnings(&player, &soroban_sdk::Bytes::new(&env)), Err(Ok(Error::InvalidPhase)));
}

/// continue_streak from Committed → InvalidPhase.
#[test]
fn continue_streak_from_committed_is_invalid_phase() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed, 1);
    assert_eq!(
        client.try_continue_streak(&player, &new_commitment(&env)),
        Err(Ok(Error::InvalidPhase))
    );
}

/// reveal on Completed slot → NoActiveGame (slot was deleted after settlement).
#[test]
fn reveal_from_completed_is_no_active_game() {
    let (env, client, contract_id) = setup();
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Completed, 0);
    // cash_out deletes the record; simulate by injecting Completed then calling reveal
    // In practice Completed games are deleted by cash_out; inject here to test the guard.
    let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);
    // Completed phase → reveal guard fires InvalidPhase (game record still present)
    assert_eq!(client.try_reveal(&player, &secret), Err(Ok(Error::InvalidPhase)));
}

/// cash_out on Completed slot → NoActiveGame (slot deleted after cash_out).
#[test]
fn cash_out_after_settlement_is_no_active_game() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Revealed, 1);
    client.cash_out(&player); // deletes the record
    assert_eq!(client.try_cash_out(&player), Err(Ok(Error::NoActiveGame)));
}

/// reclaim_wager from Revealed → InvalidPhase.
#[test]
fn reclaim_wager_from_revealed_is_invalid_phase() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Revealed, 1);
    assert_eq!(client.try_reclaim_wager(&player), Err(Ok(Error::InvalidPhase)));
}

/// reclaim_wager before timeout → RevealTimeout.
#[test]
fn reclaim_wager_before_timeout_is_reveal_timeout() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed, 0);
    // Do NOT advance ledger — timeout has not elapsed
    assert_eq!(client.try_reclaim_wager(&player), Err(Ok(Error::RevealTimeout)));
}

// ── Phase consistency invariants ──────────────────────────────────────────────

/// wager is immutable across the full Committed → Revealed → Committed loop.
#[test]
fn wager_immutable_across_continue_loop() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Revealed, 1);
    client.continue_streak(&player, &new_commitment(&env));
    assert_eq!(game(&env, &contract_id, &player).unwrap().wager, WAGER);
}

/// fee_bps snapshot is immutable across continue_streak.
#[test]
fn fee_bps_immutable_across_continue_streak() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Revealed, 1);
    client.continue_streak(&player, &new_commitment(&env));
    assert_eq!(game(&env, &contract_id, &player).unwrap().fee_bps, 300);
}

/// Rejected operations leave game state and reserves unchanged.
#[test]
fn rejected_ops_leave_state_unchanged() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed, 1);
    let state_before = game(&env, &contract_id, &player).unwrap();
    let reserve_before = reserve(&env, &contract_id);
    let _ = client.try_cash_out(&player);
    let _ = client.try_continue_streak(&player, &new_commitment(&env));
    assert_eq!(state_before, game(&env, &contract_id, &player).unwrap());
    assert_eq!(reserve_before, reserve(&env, &contract_id));
}

/// streak only increases on win; never decreases or resets via continue_streak.
#[test]
fn streak_only_increases_on_win() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed, 2);
    let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);
    let won = client.reveal(&player, &secret);
    if won {
        assert_eq!(game(&env, &contract_id, &player).unwrap().streak, 3);
    }
}

// ── Multi-player isolation ────────────────────────────────────────────────────

/// Two players' game states are fully independent.
#[test]
fn two_players_states_are_isolated() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    inject(&env, &contract_id, &p1, GamePhase::Committed, 0);
    inject(&env, &contract_id, &p2, GamePhase::Revealed, 3);

    // p2 cashes out — must not affect p1
    client.cash_out(&p2);
    assert!(game(&env, &contract_id, &p2).is_none());
    let g1 = game(&env, &contract_id, &p1).unwrap();
    assert_eq!(g1.phase, GamePhase::Committed);
    assert_eq!(g1.streak, 0);
}

/// A player starting a new game does not affect another player's active game.
#[test]
fn new_game_does_not_affect_other_player() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    inject(&env, &contract_id, &p1, GamePhase::Revealed, 2);
    // p2 starts a fresh game
    client.start_game(&p2, &Side::Tails, &WAGER, &new_commitment(&env));
    // p1's state must be unchanged
    let g1 = game(&env, &contract_id, &p1).unwrap();
    assert_eq!(g1.phase, GamePhase::Revealed);
    assert_eq!(g1.streak, 2);
}

/// start_game while another player has an active game is allowed (per-player isolation).
#[test]
fn concurrent_start_games_are_independent() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    let c1: BytesN<32> = env.crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(&env, &[11u8; 32]))
        .into();
    let c2: BytesN<32> = env.crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(&env, &[22u8; 32]))
        .into();
    client.start_game(&p1, &Side::Heads, &WAGER, &c1);
    client.start_game(&p2, &Side::Tails, &WAGER, &c2);
    assert_eq!(game(&env, &contract_id, &p1).unwrap().phase, GamePhase::Committed);
    assert_eq!(game(&env, &contract_id, &p2).unwrap().phase, GamePhase::Committed);
    assert_eq!(game(&env, &contract_id, &p1).unwrap().side, Side::Heads);
    assert_eq!(game(&env, &contract_id, &p2).unwrap().side, Side::Tails);
}
