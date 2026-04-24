//! Tests for the entropy pool: accumulation, mixing, and quality metrics.

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};

// ── helpers ──────────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (Address, CoinflipContractClient) {
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let treasury = Address::generate(env);
    let token = Address::generate(env);
    client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000, &BytesN::from_array(&env, &[0u8; 32]));
    (contract_id, client)
}

fn fund(env: &Env, contract_id: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let mut stats = CoinflipContract::load_stats(env);
        stats.reserve_balance = amount;
        CoinflipContract::save_stats(env, &stats);
    });
}

fn load_entropy(env: &Env, contract_id: &Address) -> EntropyPool {
    env.as_contract(contract_id, || CoinflipContract::load_entropy_pool(env))
}

fn load_stats(env: &Env, contract_id: &Address) -> ContractStats {
    env.as_contract(contract_id, || CoinflipContract::load_stats(env))
}

fn commitment(env: &Env, seed: u8) -> BytesN<32> {
    env.crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(env, &[seed; 32]))
        .into()
}

// ── Accumulation tests ────────────────────────────────────────────────────────

/// initialize seeds the entropy pool with pool_size == 1.
#[test]
fn test_entropy_pool_initialized_on_contract_init() {
    let env = Env::default();
    let (contract_id, _client) = setup(&env);

    let entropy = load_entropy(&env, &contract_id);
    assert_eq!(entropy.pool_size, 1, "pool_size must be 1 after initialize");
    assert_eq!(entropy.mix_count, 0, "mix_count must be 0 after initialize");
    // Pool must not be all-zero (seeded from ledger sequence hash).
    assert_ne!(entropy.pool, BytesN::from_array(&env, &[0u8; 32]));
}

/// start_game accumulates entropy: pool_size increments by 1.
#[test]
fn test_start_game_accumulates_entropy() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let before = load_entropy(&env, &contract_id);
    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    let after = load_entropy(&env, &contract_id);

    assert_eq!(after.pool_size, before.pool_size + 1);
}

/// start_game mixes entropy: mix_count increments by 1.
#[test]
fn test_start_game_mixes_entropy() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let before = load_entropy(&env, &contract_id);
    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    let after = load_entropy(&env, &contract_id);

    assert_eq!(after.mix_count, before.mix_count + 1);
}

/// continue_streak also accumulates and mixes entropy.
#[test]
fn test_continue_streak_accumulates_and_mixes_entropy() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    // Start and win a game (seed 1 → Heads win).
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    assert_eq!(client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64])), true);

    let before = load_entropy(&env, &contract_id);
    env.ledger().with_mut(|l| l.sequence_number += 1);
    client.continue_streak(&player, &commitment(&env, 2));
    let after = load_entropy(&env, &contract_id);

    assert_eq!(after.pool_size, before.pool_size + 1, "pool_size must increment on continue");
    assert_eq!(after.mix_count, before.mix_count + 1, "mix_count must increment on continue");
}

/// Pool value changes after each accumulation (XOR with distinct contributions).
#[test]
fn test_entropy_pool_value_changes_across_games() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000_000);

    let pool_after_init = load_entropy(&env, &contract_id).pool;

    // Advance ledger so each game gets a distinct contribution.
    env.ledger().with_mut(|l| l.sequence_number += 1);
    let p1 = Address::generate(&env);
    client.start_game(&p1, &Side::Heads, &10_000_000, &commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    let pool_after_game1 = load_entropy(&env, &contract_id).pool;

    env.ledger().with_mut(|l| l.sequence_number += 1);
    let p2 = Address::generate(&env);
    client.start_game(&p2, &Side::Heads, &10_000_000, &commitment(&env, 2, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    let pool_after_game2 = load_entropy(&env, &contract_id).pool;

    assert_ne!(pool_after_init, pool_after_game1, "pool must change after first game");
    assert_ne!(pool_after_game1, pool_after_game2, "pool must change after second game");
}

// ── Mixing tests ──────────────────────────────────────────────────────────────

/// contract_random stored in GameState differs from the raw SHA-256(sequence)
/// because the entropy pool is XOR-mixed in.
#[test]
fn test_contract_random_is_entropy_mixed() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));

    let game: GameState = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });

    // Raw SHA-256 of the ledger sequence (what the old code would have stored).
    let seq_bytes = env.ledger().sequence().to_be_bytes();
    let raw_random: BytesN<32> = env
        .crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(&env, &seq_bytes))
        .into();

    // The stored contract_random must differ from the raw value because the
    // entropy pool was XOR-mixed in.
    assert_ne!(
        game.contract_random, raw_random,
        "contract_random must be entropy-mixed, not raw SHA-256(sequence)"
    );
}

/// Two games at the same ledger sequence but different pool states produce
/// different contract_random values.
#[test]
fn test_same_sequence_different_pool_yields_different_random() {
    let env = Env::default();
    env.mock_all_auths();

    // Contract A: one game started.
    let cid_a = env.register(CoinflipContract, ());
    let client_a = CoinflipContractClient::new(&env, &cid_a);
    let admin_a = Address::generate(&env);
    let treasury_a = Address::generate(&env);
    let token_a = Address::generate(&env);
    client_a.initialize(&admin_a, &treasury_a, &token_a, &300, &1_000_000, &100_000_000, &BytesN::from_array(&env, &[0u8; 32]));
    fund(&env, &cid_a, 1_000_000_000);

    // Contract B: two games started before the game we care about.
    let cid_b = env.register(CoinflipContract, ());
    let client_b = CoinflipContractClient::new(&env, &cid_b);
    let admin_b = Address::generate(&env);
    let treasury_b = Address::generate(&env);
    let token_b = Address::generate(&env);
    client_b.initialize(&admin_b, &treasury_b, &token_b, &300, &1_000_000, &100_000_000, &BytesN::from_array(&env, &[0u8; 32]));
    fund(&env, &cid_b, 1_000_000_000);

    // Warm up contract B's pool with an extra game.
    let warmup = Address::generate(&env);
    client_b.start_game(&warmup, &Side::Heads, &1_000_000, &commitment(&env, 99, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));

    // Now both contracts start a game at the same ledger sequence.
    let player_a = Address::generate(&env);
    let player_b = Address::generate(&env);
    client_a.start_game(&player_a, &Side::Heads, &10_000_000, &commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    client_b.start_game(&player_b, &Side::Heads, &10_000_000, &commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));

    let game_a: GameState = env.as_contract(&cid_a, || {
        CoinflipContract::load_player_game(&env, &player_a).unwrap()
    });
    let game_b: GameState = env.as_contract(&cid_b, || {
        CoinflipContract::load_player_game(&env, &player_b).unwrap()
    });

    assert_ne!(
        game_a.contract_random, game_b.contract_random,
        "different pool states must produce different contract_random values"
    );
}

// ── Quality metric tests ──────────────────────────────────────────────────────

/// stats.pool_size reflects the total entropy contributions.
#[test]
fn test_stats_pool_size_tracks_accumulations() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000_000);

    // After initialize: pool_size == 1 (seeded in initialize).
    let stats = load_stats(&env, &contract_id);
    assert_eq!(stats.pool_size, 1);

    // Each start_game adds 1.
    for i in 0..3u32 {
        env.ledger().with_mut(|l| l.sequence_number += 1);
        let player = Address::generate(&env);
        client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, i as u8 + 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    }

    let stats = load_stats(&env, &contract_id);
    assert_eq!(stats.pool_size, 4, "pool_size must be 1 (init) + 3 (games)");
}

/// stats.mix_count reflects the total mix operations.
#[test]
fn test_stats_mix_count_tracks_mixes() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000_000);

    // After initialize: mix_count == 0.
    let stats = load_stats(&env, &contract_id);
    assert_eq!(stats.mix_count, 0);

    // Each start_game mixes once.
    for i in 0..3u32 {
        env.ledger().with_mut(|l| l.sequence_number += 1);
        let player = Address::generate(&env);
        client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, i as u8 + 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    }

    let stats = load_stats(&env, &contract_id);
    assert_eq!(stats.mix_count, 3, "mix_count must equal number of games started");
}

/// pool_size and mix_count are monotonically non-decreasing.
#[test]
fn test_quality_metrics_are_monotonically_non_decreasing() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000_000);

    let mut prev_pool_size = load_stats(&env, &contract_id).pool_size;
    let mut prev_mix_count = load_stats(&env, &contract_id).mix_count;

    for i in 0..5u32 {
        env.ledger().with_mut(|l| l.sequence_number += 1);
        let player = Address::generate(&env);
        client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, i as u8 + 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));

        let stats = load_stats(&env, &contract_id);
        assert!(stats.pool_size >= prev_pool_size, "pool_size must not decrease");
        assert!(stats.mix_count >= prev_mix_count, "mix_count must not decrease");
        prev_pool_size = stats.pool_size;
        prev_mix_count = stats.mix_count;
    }
}

/// mix_count increments on continue_streak as well as start_game.
#[test]
fn test_mix_count_increments_on_continue_streak() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    assert_eq!(client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64])), true);

    let before = load_stats(&env, &contract_id).mix_count;
    env.ledger().with_mut(|l| l.sequence_number += 1);
    client.continue_streak(&player, &commitment(&env, 2));
    let after = load_stats(&env, &contract_id).mix_count;

    assert_eq!(after, before + 1, "mix_count must increment on continue_streak");
}
