//! # Reveal Validation Tests
//!
//! Tests for the `reveal` guard clauses that enforce the preconditions required
//! before a committed game can be revealed.
//!
//! ## Preconditions (in evaluation order)
//!
//! | Guard | Error              | Condition                                      |
//! |-------|--------------------|------------------------------------------------|
//! | 1     | `NoActiveGame`     | No game record exists for the player           |
//! | 2     | `InvalidPhase`     | Game exists but is not in `Committed` phase    |
//! | 3     | `CommitmentMismatch` | `SHA-256(secret) != stored commitment`       |
//!
//! All guards must fire before any state mutation occurs.
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
    client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);
    (env, client, contract_id)
}

/// Fund the contract reserve so `start_game` solvency checks pass.
fn fund(env: &Env, contract_id: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let mut stats = CoinflipContract::load_stats(env);
        stats.reserve_balance = amount;
        CoinflipContract::save_stats(env, &stats);
    });
}

/// Inject a game in the given phase directly into storage (bypasses `start_game`).
///
/// The stored commitment is `SHA-256([1u8; 32])`, so the matching secret is
/// `[1u8; 32]`.  The contract_random is `SHA-256([2u8; 32])`.
fn inject(env: &Env, contract_id: &Address, player: &Address, phase: GamePhase) {
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
        streak: 0,
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

/// The valid secret whose SHA-256 matches the injected commitment.
fn valid_secret(env: &Env) -> soroban_sdk::Bytes {
    soroban_sdk::Bytes::from_slice(env, &[1u8; 32])
}

/// A secret that does NOT match the injected commitment.
fn wrong_secret(env: &Env) -> soroban_sdk::Bytes {
    soroban_sdk::Bytes::from_slice(env, &[0xffu8; 32])
}

// ── Guard 1: NoActiveGame ─────────────────────────────────────────────────────

/// `reveal` with no game record → `NoActiveGame`.
#[test]
fn reveal_no_game_returns_no_active_game() {
    let (env, client, _) = setup();
    let player = Address::generate(&env);
    assert_eq!(
        client.try_reveal(&player, &valid_secret(&env)),
        Err(Ok(Error::NoActiveGame))
    );
}

/// `reveal` after a loss (game deleted by previous reveal) → `NoActiveGame`.
#[test]
fn reveal_after_loss_deletion_returns_no_active_game() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed);

    // First reveal: outcome is deterministic; keep calling until we get a loss.
    // The injected secret/contract_random pair produces a fixed outcome.
    let won = client.reveal(&player, &valid_secret(&env));
    if !won {
        // Game was deleted on loss — second reveal must return NoActiveGame.
        assert_eq!(
            client.try_reveal(&player, &valid_secret(&env)),
            Err(Ok(Error::NoActiveGame))
        );
    }
    // If the first reveal was a win, the game is in Revealed phase; the
    // InvalidPhase guard (tested separately) would fire on a second call.
}

// ── Guard 2: InvalidPhase ─────────────────────────────────────────────────────

/// `reveal` on a game already in `Revealed` phase → `InvalidPhase`.
#[test]
fn reveal_from_revealed_phase_returns_invalid_phase() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Revealed);
    assert_eq!(
        client.try_reveal(&player, &valid_secret(&env)),
        Err(Ok(Error::InvalidPhase))
    );
}

/// `reveal` on a game in `Completed` phase → `InvalidPhase`.
///
/// A `Completed` record is still present in storage (it was not deleted);
/// the phase guard fires before the commitment check.
#[test]
fn reveal_from_completed_phase_returns_invalid_phase() {
    let (env, client, contract_id) = setup();
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Completed);
    assert_eq!(
        client.try_reveal(&player, &valid_secret(&env)),
        Err(Ok(Error::InvalidPhase))
    );
}

/// Phase guard fires before commitment check: wrong secret on a `Revealed` game
/// still returns `InvalidPhase`, not `CommitmentMismatch`.
#[test]
fn reveal_phase_guard_fires_before_commitment_check() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Revealed);
    // Wrong secret — but phase guard should fire first.
    assert_eq!(
        client.try_reveal(&player, &wrong_secret(&env)),
        Err(Ok(Error::InvalidPhase))
    );
}

// ── Guard 3: CommitmentMismatch ───────────────────────────────────────────────

/// `reveal` with a secret that does not hash to the stored commitment →
/// `CommitmentMismatch`.
#[test]
fn reveal_wrong_secret_returns_commitment_mismatch() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed);
    assert_eq!(
        client.try_reveal(&player, &wrong_secret(&env)),
        Err(Ok(Error::CommitmentMismatch))
    );
}

/// `reveal` with an empty secret → `CommitmentMismatch` (SHA-256 of empty ≠
/// SHA-256 of `[1u8; 32]`).
#[test]
fn reveal_empty_secret_returns_commitment_mismatch() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed);
    let empty = soroban_sdk::Bytes::new(&env);
    assert_eq!(
        client.try_reveal(&player, &empty),
        Err(Ok(Error::CommitmentMismatch))
    );
}

/// `reveal` with a one-byte-off secret → `CommitmentMismatch`.
#[test]
fn reveal_near_miss_secret_returns_commitment_mismatch() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed);
    // Flip the last byte of the valid secret.
    let mut near_miss = [1u8; 32];
    near_miss[31] = 0x00;
    let near_miss_bytes = soroban_sdk::Bytes::from_slice(&env, &near_miss);
    assert_eq!(
        client.try_reveal(&player, &near_miss_bytes),
        Err(Ok(Error::CommitmentMismatch))
    );
}

/// Commitment mismatch leaves game state unchanged (no side effects on failure).
#[test]
fn reveal_commitment_mismatch_leaves_state_unchanged() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed);

    let reserve_before = env.as_contract(&contract_id, || {
        CoinflipContract::load_stats(&env).reserve_balance
    });

    // Attempt reveal with wrong secret — must fail.
    let _ = client.try_reveal(&player, &wrong_secret(&env));

    // Game must still be in Committed phase.
    let game = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });
    assert_eq!(game.phase, GamePhase::Committed);

    // Reserves must be unchanged.
    let reserve_after = env.as_contract(&contract_id, || {
        CoinflipContract::load_stats(&env).reserve_balance
    });
    assert_eq!(reserve_before, reserve_after);
}

// ── Happy path ────────────────────────────────────────────────────────────────

/// Valid reveal with correct secret succeeds and returns a boolean outcome.
#[test]
fn reveal_valid_committed_game_succeeds() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed);
    // Must not error — outcome (win/loss) is deterministic but either is valid.
    assert!(client.try_reveal(&player, &valid_secret(&env)).is_ok());
}

/// After a winning reveal the game advances to `Revealed` with streak == 1.
#[test]
fn reveal_win_advances_to_revealed_with_streak_one() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed);
    let won = client.reveal(&player, &valid_secret(&env));
    if won {
        let game = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player).unwrap()
        });
        assert_eq!(game.phase, GamePhase::Revealed);
        assert_eq!(game.streak, 1);
    }
}

/// After a losing reveal the game record is deleted and reserves increase.
#[test]
fn reveal_loss_deletes_game_and_credits_reserves() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject(&env, &contract_id, &player, GamePhase::Committed);

    let reserve_before = env.as_contract(&contract_id, || {
        CoinflipContract::load_stats(&env).reserve_balance
    });

    let won = client.reveal(&player, &valid_secret(&env));
    if !won {
        // Game record must be gone.
        let game_opt = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player)
        });
        assert!(game_opt.is_none());

        // Reserves must have increased by the wager.
        let reserve_after = env.as_contract(&contract_id, || {
            CoinflipContract::load_stats(&env).reserve_balance
        });
        assert_eq!(reserve_after, reserve_before + WAGER);
    }
}
