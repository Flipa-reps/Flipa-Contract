/// # Batch Operations Tests (#463)
///
/// Tests for `batch_reveal`, `batch_cash_out`, and `batch_settle` covering:
/// - Empty and oversized batch rejection
/// - Atomicity: all-or-nothing on validation failure
/// - Partial-failure handling in `batch_settle`
/// - Correct payout and stats updates across 10+ games
use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Ledger;

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
        &300,
        &1_000_000,
        &100_000_000,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
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
    Bytes::from_slice(env, &[seed; 32])
}

fn make_commitment(env: &Env, seed: u8) -> BytesN<32> {
    env.crypto().sha256(&make_secret(env, seed)).into()
}

fn zero_vrf() -> [u8; 64] {
    [0u8; 64]
}

/// Inject a game in Revealed phase with a given streak directly into storage.
fn inject_revealed(env: &Env, contract_id: &Address, player: &Address, streak: u32, wager: i128) {
    let game = GameState {
        wager,
        side: Side::Heads,
        streak,
        commitment: make_commitment(env, 1),
        contract_random: make_commitment(env, 2),
        fee_bps: 300,
        phase: GamePhase::Revealed,
        start_ledger: 0,
        vrf_input: env.crypto().sha256(&Bytes::from_slice(env, &[42u8; 32])).into(),
    };
    env.as_contract(contract_id, || {
        CoinflipContract::save_player_game(env, player, &game);
    });
}

// ── batch_reveal: guard tests ─────────────────────────────────────────────────

#[test]
fn batch_reveal_rejects_empty_batch() {
    let (env, client, _) = setup();
    let empty: soroban_sdk::Vec<BatchRevealInput> = soroban_sdk::Vec::new(&env);
    let result = client.try_batch_reveal(&empty);
    assert_eq!(result, Err(Ok(Error::BatchEmpty)));
}

#[test]
fn batch_reveal_rejects_oversized_batch() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 10_000_000_000);

    let mut reveals: soroban_sdk::Vec<BatchRevealInput> = soroban_sdk::Vec::new(&env);
    for i in 0..(MAX_BATCH_SIZE + 1) {
        let player = Address::generate(&env);
        reveals.push_back(BatchRevealInput {
            player,
            secret: make_secret(&env, (i % 255) as u8 + 1),
            vrf_proof: BytesN::from_array(&env, &zero_vrf()),
        });
    }
    let result = client.try_batch_reveal(&reveals);
    assert_eq!(result, Err(Ok(Error::BatchTooLarge)));
}

#[test]
fn batch_reveal_fails_atomically_when_one_player_has_no_game() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 10_000_000_000);

    // Start a game for player1 only
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env); // no game
    client.start_game(&player1, &Side::Heads, &5_000_000, &make_commitment(&env, 1));
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);

    let mut reveals: soroban_sdk::Vec<BatchRevealInput> = soroban_sdk::Vec::new(&env);
    reveals.push_back(BatchRevealInput {
        player: player1.clone(),
        secret: make_secret(&env, 1),
        vrf_proof: BytesN::from_array(&env, &zero_vrf()),
    });
    reveals.push_back(BatchRevealInput {
        player: player2.clone(),
        secret: make_secret(&env, 1),
        vrf_proof: BytesN::from_array(&env, &zero_vrf()),
    });

    // Entire batch must fail because player2 has no game
    let result = client.try_batch_reveal(&reveals);
    assert_eq!(result, Err(Ok(Error::BatchOperationFailed)));
}

// ── batch_reveal: success path ────────────────────────────────────────────────

#[test]
fn batch_reveal_processes_10_games() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 100_000_000_000);

    let mut players: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    // seed 1 → Heads outcome → win for Heads player
    for _ in 0..10 {
        let player = Address::generate(&env);
        client.start_game(&player, &Side::Heads, &5_000_000, &make_commitment(&env, 1));
        players.push_back(player);
    }
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);

    let mut reveals: soroban_sdk::Vec<BatchRevealInput> = soroban_sdk::Vec::new(&env);
    for i in 0..players.len() {
        reveals.push_back(BatchRevealInput {
            player: players.get(i).unwrap(),
            secret: make_secret(&env, 1),
            vrf_proof: BytesN::from_array(&env, &zero_vrf()),
        });
    }

    let results = client.batch_reveal(&reveals);
    assert_eq!(results.len(), 10);
    // All results must be Ok (win or loss — no errors)
    for i in 0..results.len() {
        let r = results.get(i).unwrap();
        assert!(r.result.is_ok());
    }
}

// ── batch_cash_out: guard tests ───────────────────────────────────────────────

#[test]
fn batch_cash_out_rejects_empty_batch() {
    let (env, client, _) = setup();
    let empty: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    let result = client.try_batch_cash_out(&empty);
    assert_eq!(result, Err(Ok(Error::BatchEmpty)));
}

#[test]
fn batch_cash_out_rejects_oversized_batch() {
    let (env, client, _) = setup();
    let mut players: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    for _ in 0..(MAX_BATCH_SIZE + 1) {
        players.push_back(Address::generate(&env));
    }
    let result = client.try_batch_cash_out(&players);
    assert_eq!(result, Err(Ok(Error::BatchTooLarge)));
}

#[test]
fn batch_cash_out_fails_atomically_when_player_has_no_game() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 10_000_000_000);

    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env); // no game
    inject_revealed(&env, &contract_id, &player1, 1, 5_000_000);

    let mut players: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    players.push_back(player1);
    players.push_back(player2);

    let result = client.try_batch_cash_out(&players);
    assert_eq!(result, Err(Ok(Error::BatchOperationFailed)));
}

// ── batch_cash_out: success path ──────────────────────────────────────────────

#[test]
fn batch_cash_out_settles_10_revealed_games() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 100_000_000_000);

    let mut players: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    for _ in 0..10 {
        let player = Address::generate(&env);
        inject_revealed(&env, &contract_id, &player, 1, 5_000_000);
        players.push_back(player);
    }

    let results = client.batch_cash_out(&players);
    assert_eq!(results.len(), 10);
    for i in 0..results.len() {
        let r = results.get(i).unwrap();
        let payout = r.result.unwrap();
        assert!(payout > 0, "each settled game must yield a positive payout");
    }
}

#[test]
fn batch_cash_out_updates_reserve_balance() {
    let (env, client, contract_id) = setup();
    let initial_reserve = 100_000_000_000i128;
    fund(&env, &contract_id, initial_reserve);

    let wager = 5_000_000i128;
    let mut players: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    for _ in 0..5 {
        let player = Address::generate(&env);
        inject_revealed(&env, &contract_id, &player, 1, wager);
        players.push_back(player);
    }

    client.batch_cash_out(&players);

    let stats = env.as_contract(&contract_id, || CoinflipContract::load_stats(&env));
    assert!(
        stats.reserve_balance < initial_reserve,
        "reserve must decrease after payouts"
    );
}

// ── batch_settle: partial failure ─────────────────────────────────────────────

#[test]
fn batch_settle_rejects_empty_batch() {
    let (env, client, _) = setup();
    let empty: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    let result = client.try_batch_settle(&empty);
    assert_eq!(result, Err(Ok(Error::BatchEmpty)));
}

#[test]
fn batch_settle_rejects_oversized_batch() {
    let (env, client, _) = setup();
    let mut players: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    for _ in 0..(MAX_BATCH_SIZE + 1) {
        players.push_back(Address::generate(&env));
    }
    let result = client.try_batch_settle(&players);
    assert_eq!(result, Err(Ok(Error::BatchTooLarge)));
}

#[test]
fn batch_settle_records_error_for_player_with_no_game_without_aborting() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 100_000_000_000);

    let good_player = Address::generate(&env);
    let bad_player = Address::generate(&env); // no game
    inject_revealed(&env, &contract_id, &good_player, 1, 5_000_000);

    let mut players: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    players.push_back(good_player.clone());
    players.push_back(bad_player.clone());

    let results = client.batch_settle(&players);
    assert_eq!(results.len(), 2);

    let r0 = results.get(0).unwrap();
    let r1 = results.get(1).unwrap();
    assert!(r0.result.is_ok(), "good player must settle successfully");
    assert_eq!(r1.result, Err(Error::NoActiveGame), "bad player must record NoActiveGame");
}

#[test]
fn batch_settle_processes_mixed_valid_and_invalid_entries() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 100_000_000_000);

    let mut players: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    // 5 valid revealed games
    for _ in 0..5 {
        let p = Address::generate(&env);
        inject_revealed(&env, &contract_id, &p, 1, 5_000_000);
        players.push_back(p);
    }
    // 5 players with no game
    for _ in 0..5 {
        players.push_back(Address::generate(&env));
    }

    let results = client.batch_settle(&players);
    assert_eq!(results.len(), 10);

    let successes = (0..results.len())
        .filter(|&i| results.get(i).unwrap().result.is_ok())
        .count();
    let failures = (0..results.len())
        .filter(|&i| results.get(i).unwrap().result.is_err())
        .count();

    assert_eq!(successes, 5);
    assert_eq!(failures, 5);
}

#[test]
fn batch_settle_processes_12_games() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 100_000_000_000);

    let mut players: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    for _ in 0..12 {
        let p = Address::generate(&env);
        inject_revealed(&env, &contract_id, &p, 1, 5_000_000);
        players.push_back(p);
    }

    let results = client.batch_settle(&players);
    assert_eq!(results.len(), 12);
    for i in 0..results.len() {
        assert!(results.get(i).unwrap().result.is_ok());
    }
}

#[test]
fn batch_settle_flushes_stats_once_for_entire_batch() {
    let (env, client, contract_id) = setup();
    let initial_reserve = 100_000_000_000i128;
    fund(&env, &contract_id, initial_reserve);

    let mut players: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    for _ in 0..5 {
        let p = Address::generate(&env);
        inject_revealed(&env, &contract_id, &p, 1, 5_000_000);
        players.push_back(p);
    }

    client.batch_settle(&players);

    let stats = env.as_contract(&contract_id, || CoinflipContract::load_stats(&env));
    assert!(stats.reserve_balance < initial_reserve);
    assert!(stats.total_fees > 0);
}
