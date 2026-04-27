/// # Game History Tests (#466)
///
/// Tests for immutable game history storage and deterministic replay:
/// - History entries appended on win (cash_out) and loss (reveal)
/// - Ring-buffer cap at HISTORY_LIMIT (100 entries, oldest evicted)
/// - Paginated `get_game_history` queries
/// - Ledger-range queries via `get_history_by_ledger_range`
/// - `prune_history` reduces storage and returns removed count
/// - `verify_past_game` confirms deterministic replay accuracy
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

fn secret(env: &Env, seed: u8) -> Bytes {
    Bytes::from_slice(env, &[seed; 32])
}

fn commitment(env: &Env, seed: u8) -> BytesN<32> {
    env.crypto().sha256(&secret(env, seed)).into()
}

fn load_history(
    env: &Env,
    contract_id: &Address,
    player: &Address,
) -> soroban_sdk::Vec<HistoryEntry> {
    env.as_contract(contract_id, || {
        CoinflipContract::load_player_history(env, player)
    })
}

// ── Storage: entries appended correctly ──────────────────────────────────────

/// A loss appends one entry with won=false and payout=0.
#[test]
fn loss_appends_history_entry_with_correct_fields() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);

    // seed 3 → Tails outcome → loss for Heads player
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, 3));
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    client.reveal(&player, &secret(&env, 3));

    let history = load_history(&env, &contract_id, &player);
    assert_eq!(history.len(), 1);
    let entry = history.get(0).unwrap();
    assert!(!entry.won);
    assert_eq!(entry.wager, 10_000_000);
    assert_eq!(entry.payout, 0);
    assert_eq!(entry.streak, 0);
    assert_eq!(entry.side, Side::Heads);
}

/// A win followed by cash_out appends one entry with won=true and payout > 0.
#[test]
fn win_cash_out_appends_history_entry_with_correct_fields() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);

    // seed 1 → Heads outcome → win for Heads player
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, 1));
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    client.reveal(&player, &secret(&env, 1));
    client.cash_out(&player);

    let history = load_history(&env, &contract_id, &player);
    assert_eq!(history.len(), 1);
    let entry = history.get(0).unwrap();
    assert!(entry.won);
    assert_eq!(entry.wager, 10_000_000);
    assert!(entry.payout > 0);
    assert_eq!(entry.streak, 1);
}

/// Multiple games accumulate in chronological order (oldest first).
#[test]
fn history_accumulates_in_chronological_order() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);

    // Game 1: win (seed 1 → Heads)
    client.start_game(&player, &Side::Heads, &5_000_000, &commitment(&env, 1));
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    client.reveal(&player, &secret(&env, 1));
    client.cash_out(&player);

    // Game 2: loss (seed 3 → Tails)
    client.start_game(&player, &Side::Heads, &3_000_000, &commitment(&env, 3));
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    client.reveal(&player, &secret(&env, 3));

    let history = load_history(&env, &contract_id, &player);
    assert_eq!(history.len(), 2);
    assert!(history.get(0).unwrap().won, "first entry must be the win");
    assert!(!history.get(1).unwrap().won, "second entry must be the loss");
}

// ── Ring-buffer cap ───────────────────────────────────────────────────────────

/// After HISTORY_LIMIT + 1 games the buffer holds exactly HISTORY_LIMIT entries.
#[test]
fn history_capped_at_history_limit() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 100_000_000_000i128);
    let player = Address::generate(&env);

    for _ in 0..(HISTORY_LIMIT + 1) {
        // seed 3 → loss, so no cash_out needed
        client.start_game(&player, &Side::Heads, &1_000_000, &commitment(&env, 3));
        env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
        client.reveal(&player, &secret(&env, 3));
    }

    let history = load_history(&env, &contract_id, &player);
    assert_eq!(history.len(), HISTORY_LIMIT);
}

/// The oldest entry is evicted when the cap is exceeded.
#[test]
fn history_evicts_oldest_entry_on_overflow() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 100_000_000_000i128);
    let player = Address::generate(&env);

    // Fill to cap with losses (seed 3)
    for _ in 0..HISTORY_LIMIT {
        client.start_game(&player, &Side::Heads, &1_000_000, &commitment(&env, 3));
        env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
        client.reveal(&player, &secret(&env, 3));
    }

    // One more win (seed 1) — should evict the oldest loss
    client.start_game(&player, &Side::Heads, &1_000_000, &commitment(&env, 1));
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    client.reveal(&player, &secret(&env, 1));
    client.cash_out(&player);

    let history = load_history(&env, &contract_id, &player);
    assert_eq!(history.len(), HISTORY_LIMIT);
    // The last entry must be the win we just added
    assert!(history.get(HISTORY_LIMIT - 1).unwrap().won);
}

// ── get_game_history: pagination ──────────────────────────────────────────────

/// get_game_history returns an empty vec for a player with no history.
#[test]
fn get_game_history_returns_empty_for_unknown_player() {
    let (env, client, _) = setup();
    let player = Address::generate(&env);
    let result = client.get_game_history(&player, &0, &10);
    assert_eq!(result.len(), 0);
}

/// get_game_history respects the limit parameter.
#[test]
fn get_game_history_respects_limit() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 100_000_000_000i128);
    let player = Address::generate(&env);

    for _ in 0..10 {
        client.start_game(&player, &Side::Heads, &1_000_000, &commitment(&env, 3));
        env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
        client.reveal(&player, &secret(&env, 3));
    }

    let page = client.get_game_history(&player, &0, &5);
    assert_eq!(page.len(), 5);
}

/// get_game_history with offset beyond history length returns empty.
#[test]
fn get_game_history_returns_empty_when_offset_exceeds_length() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 10_000_000_000i128);
    let player = Address::generate(&env);

    client.start_game(&player, &Side::Heads, &1_000_000, &commitment(&env, 3));
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    client.reveal(&player, &secret(&env, 3));

    let result = client.get_game_history(&player, &100, &10);
    assert_eq!(result.len(), 0);
}

/// Paginating through history returns all entries without duplicates.
#[test]
fn get_game_history_pagination_covers_all_entries() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 100_000_000_000i128);
    let player = Address::generate(&env);

    for _ in 0..15 {
        client.start_game(&player, &Side::Heads, &1_000_000, &commitment(&env, 3));
        env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
        client.reveal(&player, &secret(&env, 3));
    }

    let page1 = client.get_game_history(&player, &0, &10);
    let page2 = client.get_game_history(&player, &10, &10);
    assert_eq!(page1.len(), 10);
    assert_eq!(page2.len(), 5);
}

// ── get_history_by_ledger_range ───────────────────────────────────────────────

/// Ledger-range query returns only entries within the range.
#[test]
fn get_history_by_ledger_range_filters_correctly() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 100_000_000_000i128);
    let player = Address::generate(&env);

    // Game at ledger ~10
    env.ledger().with_mut(|l| l.sequence_number = 10);
    client.start_game(&player, &Side::Heads, &1_000_000, &commitment(&env, 3));
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    client.reveal(&player, &secret(&env, 3));

    // Game at ledger ~100
    env.ledger().with_mut(|l| l.sequence_number = 100);
    client.start_game(&player, &Side::Heads, &1_000_000, &commitment(&env, 3));
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    client.reveal(&player, &secret(&env, 3));

    // Query only the first game's range
    let result = client.get_history_by_ledger_range(&player, &0, &50);
    assert_eq!(result.len(), 1);
    assert!(result.get(0).unwrap().ledger <= 50);
}

/// Ledger-range query returns empty when no entries fall in range.
#[test]
fn get_history_by_ledger_range_returns_empty_when_no_match() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 10_000_000_000i128);
    let player = Address::generate(&env);

    client.start_game(&player, &Side::Heads, &1_000_000, &commitment(&env, 3));
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    client.reveal(&player, &secret(&env, 3));

    // Query a range that doesn't include the game's ledger
    let result = client.get_history_by_ledger_range(&player, &9000, &9999);
    assert_eq!(result.len(), 0);
}

// ── prune_history ─────────────────────────────────────────────────────────────

/// prune_history removes old entries and returns the count removed.
#[test]
fn prune_history_removes_old_entries_and_returns_count() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 100_000_000_000i128);
    let player = Address::generate(&env);

    for _ in 0..10 {
        client.start_game(&player, &Side::Heads, &1_000_000, &commitment(&env, 3));
        env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
        client.reveal(&player, &secret(&env, 3));
    }

    let removed = client.prune_history(&player, &5);
    assert_eq!(removed, 5);

    let history = load_history(&env, &contract_id, &player);
    assert_eq!(history.len(), 5);
}

/// prune_history with keep >= current length is a no-op returning 0.
#[test]
fn prune_history_noop_when_keep_exceeds_length() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 10_000_000_000i128);
    let player = Address::generate(&env);

    for _ in 0..3 {
        client.start_game(&player, &Side::Heads, &1_000_000, &commitment(&env, 3));
        env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
        client.reveal(&player, &secret(&env, 3));
    }

    let removed = client.prune_history(&player, &10);
    assert_eq!(removed, 0);

    let history = load_history(&env, &contract_id, &player);
    assert_eq!(history.len(), 3);
}

/// prune_history retains the most recent entries (not the oldest).
#[test]
fn prune_history_retains_most_recent_entries() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 100_000_000_000i128);
    let player = Address::generate(&env);

    // 5 losses then 1 win
    for _ in 0..5 {
        client.start_game(&player, &Side::Heads, &1_000_000, &commitment(&env, 3));
        env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
        client.reveal(&player, &secret(&env, 3));
    }
    client.start_game(&player, &Side::Heads, &1_000_000, &commitment(&env, 1));
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    client.reveal(&player, &secret(&env, 1));
    client.cash_out(&player);

    // Keep only the last 1 entry — must be the win
    client.prune_history(&player, &1);

    let history = load_history(&env, &contract_id, &player);
    assert_eq!(history.len(), 1);
    assert!(history.get(0).unwrap().won, "retained entry must be the most recent win");
}

// ── verify_past_game: replay accuracy ────────────────────────────────────────

/// verify_past_game returns true for a valid history entry.
#[test]
fn verify_past_game_returns_true_for_valid_entry() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);

    // seed 3 → loss (secret stored in history)
    client.start_game(&player, &Side::Heads, &5_000_000, &commitment(&env, 3));
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    client.reveal(&player, &secret(&env, 3));

    let valid = client.verify_past_game(&player, &0);
    assert!(valid, "history entry must verify correctly");
}

/// verify_past_game returns an error for an out-of-range index.
#[test]
fn verify_past_game_errors_on_out_of_range_index() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);

    client.start_game(&player, &Side::Heads, &5_000_000, &commitment(&env, 3));
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    client.reveal(&player, &secret(&env, 3));

    let result = client.try_verify_past_game(&player, &99);
    assert!(result.is_err());
}
