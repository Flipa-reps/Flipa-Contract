//! # Player Performance Analytics Tests
//!
//! Tests for `get_player_performance` and `get_cohort_stats`.

use super::*;
use soroban_sdk::testutils::Address as _;

// ── helpers ───────────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (Address, CoinflipContractClient) {
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let treasury = Address::generate(env);
    let token = env.register_stellar_asset_contract(admin.clone());
    client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);
    (contract_id, client)
}

fn dummy_commitment(env: &Env) -> BytesN<32> {
    env.crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(env, &[1u8; 32]))
        .into()
}

fn inject_stats(
    env: &Env,
    contract_id: &Address,
    player: &Address,
    games: u64,
    wins: u64,
    wagered: i128,
    net: i128,
    max_streak: u32,
) {
    env.as_contract(contract_id, || {
        CoinflipContract::save_player_stats(env, player, &PlayerStats {
            games_played: games,
            wins,
            losses: games - wins,
            max_streak,
            current_streak: 0,
            total_wagered: wagered,
            net_winnings: net,
        });
    });
}

fn inject_history_entry(
    env: &Env,
    contract_id: &Address,
    player: &Address,
    won: bool,
    streak: u32,
    wager: i128,
    payout: i128,
    ledger: u32,
) {
    let dummy = dummy_commitment(env);
    env.as_contract(contract_id, || {
        CoinflipContract::save_history_entry(env, player, HistoryEntry {
            wager,
            side: Side::Heads,
            outcome: if won { Side::Heads } else { Side::Tails },
            won,
            streak,
            commitment: dummy.clone(),
            secret: soroban_sdk::Bytes::from_slice(env, &[1u8; 32]),
            contract_random: dummy,
            payout,
            ledger,
            vrf_proof: BytesN::from_array(env, &[0u8; 64]),
        });
    });
}

// ── get_player_performance: zero state ───────────────────────────────────────

#[test]
fn test_performance_zero_state() {
    let env = Env::default();
    let (_, client) = setup(&env);
    let player = Address::generate(&env);
    let report = client.get_player_performance(&player);
    assert_eq!(report.games_played, 0);
    assert_eq!(report.win_rate_bps, 0);
    assert_eq!(report.roi_bps, 0);
    assert_eq!(report.avg_wager, 0);
    assert_eq!(report.max_streak, 0);
    assert_eq!(report.multi_streak_wins, 0);
    assert_eq!(report.total_payout, 0);
    assert_eq!(report.recent_games, 0);
    assert_eq!(report.retention_window, RETENTION_WINDOW_LEDGERS);
}

// ── win_rate_bps ──────────────────────────────────────────────────────────────

#[test]
fn test_performance_win_rate_50_percent() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    let player = Address::generate(&env);
    inject_stats(&env, &contract_id, &player, 10, 5, 100_000_000, 0, 1);
    let report = client.get_player_performance(&player);
    assert_eq!(report.win_rate_bps, 5_000);
}

#[test]
fn test_performance_win_rate_100_percent() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    let player = Address::generate(&env);
    inject_stats(&env, &contract_id, &player, 4, 4, 40_000_000, 36_000_000, 4);
    let report = client.get_player_performance(&player);
    assert_eq!(report.win_rate_bps, 10_000);
}

// ── roi_bps ───────────────────────────────────────────────────────────────────

#[test]
fn test_performance_roi_positive() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    let player = Address::generate(&env);
    // net = +10_000_000, wagered = 100_000_000 → ROI = 10% = 1_000 bps
    inject_stats(&env, &contract_id, &player, 10, 6, 100_000_000, 10_000_000, 2);
    let report = client.get_player_performance(&player);
    assert_eq!(report.roi_bps, 1_000);
}

#[test]
fn test_performance_roi_negative() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    let player = Address::generate(&env);
    // net = -20_000_000, wagered = 100_000_000 → ROI = -20% = -2_000 bps
    inject_stats(&env, &contract_id, &player, 10, 4, 100_000_000, -20_000_000, 1);
    let report = client.get_player_performance(&player);
    assert_eq!(report.roi_bps, -2_000);
}

// ── avg_wager ─────────────────────────────────────────────────────────────────

#[test]
fn test_performance_avg_wager() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    let player = Address::generate(&env);
    inject_stats(&env, &contract_id, &player, 5, 3, 50_000_000, 5_000_000, 2);
    let report = client.get_player_performance(&player);
    assert_eq!(report.avg_wager, 10_000_000);
}

// ── history-derived metrics ───────────────────────────────────────────────────

#[test]
fn test_performance_total_payout_from_history() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    let player = Address::generate(&env);
    inject_stats(&env, &contract_id, &player, 3, 2, 30_000_000, 10_000_000, 2);
    inject_history_entry(&env, &contract_id, &player, true, 1, 10_000_000, 19_000_000, 100);
    inject_history_entry(&env, &contract_id, &player, true, 2, 10_000_000, 35_000_000, 200);
    inject_history_entry(&env, &contract_id, &player, false, 0, 10_000_000, 0, 300);
    let report = client.get_player_performance(&player);
    assert_eq!(report.total_payout, 54_000_000);
}

#[test]
fn test_performance_multi_streak_wins() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    let player = Address::generate(&env);
    inject_stats(&env, &contract_id, &player, 4, 3, 40_000_000, 0, 3);
    inject_history_entry(&env, &contract_id, &player, true, 1, 10_000_000, 19_000_000, 100);
    inject_history_entry(&env, &contract_id, &player, true, 2, 10_000_000, 35_000_000, 200); // streak ≥ 2
    inject_history_entry(&env, &contract_id, &player, true, 3, 10_000_000, 60_000_000, 300); // streak ≥ 2
    inject_history_entry(&env, &contract_id, &player, false, 0, 10_000_000, 0, 400);
    let report = client.get_player_performance(&player);
    assert_eq!(report.multi_streak_wins, 2);
}

// ── retention ─────────────────────────────────────────────────────────────────

#[test]
fn test_performance_recent_games_within_window() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    let player = Address::generate(&env);
    inject_stats(&env, &contract_id, &player, 2, 1, 20_000_000, 0, 1);

    // Set current ledger to 200_000.
    env.ledger().with_mut(|l| l.sequence_number = 200_000);

    // One entry within window (ledger 150_000 > 200_000 - 120_960 = 79_040).
    inject_history_entry(&env, &contract_id, &player, true, 1, 10_000_000, 19_000_000, 150_000);
    // One entry outside window (ledger 10_000 < 79_040).
    inject_history_entry(&env, &contract_id, &player, false, 0, 10_000_000, 0, 10_000);

    let report = client.get_player_performance(&player);
    assert_eq!(report.recent_games, 1);
}

#[test]
fn test_performance_no_recent_games_outside_window() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    let player = Address::generate(&env);
    inject_stats(&env, &contract_id, &player, 1, 0, 10_000_000, -10_000_000, 0);
    env.ledger().with_mut(|l| l.sequence_number = 200_000);
    inject_history_entry(&env, &contract_id, &player, false, 0, 10_000_000, 0, 1_000);
    let report = client.get_player_performance(&player);
    assert_eq!(report.recent_games, 0);
}

// ── get_cohort_stats ──────────────────────────────────────────────────────────

#[test]
fn test_cohort_empty() {
    let env = Env::default();
    let (_, client) = setup(&env);
    let players = soroban_sdk::Vec::new(&env);
    let cohort = client.get_cohort_stats(&players);
    assert_eq!(cohort.cohort_size, 0);
    assert_eq!(cohort.active_players, 0);
    assert_eq!(cohort.avg_win_rate_bps, 0);
    assert_eq!(cohort.avg_roi_bps, 0);
    assert_eq!(cohort.retained_players, 0);
}

#[test]
fn test_cohort_excludes_inactive_players() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env); // no games
    inject_stats(&env, &contract_id, &p1, 10, 6, 100_000_000, 5_000_000, 2);
    let mut players = soroban_sdk::Vec::new(&env);
    players.push_back(p1);
    players.push_back(p2);
    let cohort = client.get_cohort_stats(&players);
    assert_eq!(cohort.cohort_size, 2);
    assert_eq!(cohort.active_players, 1);
}

#[test]
fn test_cohort_avg_win_rate() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    // p1: 8/10 wins = 8_000 bps; p2: 2/10 wins = 2_000 bps → avg = 5_000
    inject_stats(&env, &contract_id, &p1, 10, 8, 100_000_000, 0, 3);
    inject_stats(&env, &contract_id, &p2, 10, 2, 100_000_000, 0, 1);
    let mut players = soroban_sdk::Vec::new(&env);
    players.push_back(p1);
    players.push_back(p2);
    let cohort = client.get_cohort_stats(&players);
    assert_eq!(cohort.avg_win_rate_bps, 5_000);
}

#[test]
fn test_cohort_avg_roi() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    // p1: net +10_000_000 / 100_000_000 = 1_000 bps
    // p2: net -10_000_000 / 100_000_000 = -1_000 bps → avg = 0
    inject_stats(&env, &contract_id, &p1, 10, 6, 100_000_000, 10_000_000, 2);
    inject_stats(&env, &contract_id, &p2, 10, 4, 100_000_000, -10_000_000, 1);
    let mut players = soroban_sdk::Vec::new(&env);
    players.push_back(p1);
    players.push_back(p2);
    let cohort = client.get_cohort_stats(&players);
    assert_eq!(cohort.avg_roi_bps, 0);
}

#[test]
fn test_cohort_retention() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    env.ledger().with_mut(|l| l.sequence_number = 200_000);

    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    inject_stats(&env, &contract_id, &p1, 1, 1, 10_000_000, 0, 1);
    inject_stats(&env, &contract_id, &p2, 1, 0, 10_000_000, 0, 0);

    // p1 has a recent entry; p2 has an old entry.
    inject_history_entry(&env, &contract_id, &p1, true, 1, 10_000_000, 19_000_000, 190_000);
    inject_history_entry(&env, &contract_id, &p2, false, 0, 10_000_000, 0, 1_000);

    let mut players = soroban_sdk::Vec::new(&env);
    players.push_back(p1);
    players.push_back(p2);
    let cohort = client.get_cohort_stats(&players);
    assert_eq!(cohort.retained_players, 1);
}

#[test]
fn test_cohort_capped_at_20_players() {
    let env = Env::default();
    let (_, client) = setup(&env);
    let mut players = soroban_sdk::Vec::new(&env);
    for _ in 0..25 {
        players.push_back(Address::generate(&env));
    }
    let cohort = client.get_cohort_stats(&players);
    assert_eq!(cohort.cohort_size, 20);
}
