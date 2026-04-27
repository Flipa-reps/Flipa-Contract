//! # Analytics Tests
//!
//! Tests for `get_analytics_report` and `export_player_data`.

use super::*;
use soroban_sdk::testutils::Address as _;

// ── helpers ───────────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (Address, Address, CoinflipContractClient) {
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let treasury = Address::generate(env);
    let token = env.register_stellar_asset_contract(admin.clone());
    client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);
    (admin, contract_id, client)
}

// ── get_analytics_report ──────────────────────────────────────────────────────

#[test]
fn test_analytics_report_zero_state() {
    let env = Env::default();
    let (_, _, client) = setup(&env);
    let report = client.get_analytics_report();
    assert_eq!(report.total_games, 0);
    assert_eq!(report.total_volume, 0);
    assert_eq!(report.total_fees, 0);
    assert_eq!(report.win_rate_bps, 0);
    assert_eq!(report.avg_wager, 0);
    assert_eq!(report.avg_fee, 0);
    assert_eq!(report.leaderboard_size, 0);
    assert_eq!(report.top_streak, 0);
    assert_eq!(report.top_player_winnings, 0);
}

#[test]
fn test_analytics_report_reflects_stats() {
    let env = Env::default();
    let (_, contract_id, client) = setup(&env);

    // Inject stats directly.
    env.as_contract(&contract_id, || {
        let mut stats = CoinflipContract::load_stats(&env);
        stats.total_games = 10;
        stats.total_volume = 100_000_000;
        stats.total_fees = 3_000_000;
        stats.reserve_balance = 500_000_000;
        CoinflipContract::save_stats(&env, &stats);
    });

    let report = client.get_analytics_report();
    assert_eq!(report.total_games, 10);
    assert_eq!(report.total_volume, 100_000_000);
    assert_eq!(report.total_fees, 3_000_000);
    assert_eq!(report.reserve_balance, 500_000_000);
    assert_eq!(report.avg_wager, 10_000_000); // 100_000_000 / 10
    assert_eq!(report.avg_fee, 300_000);       // 3_000_000 / 10
}

#[test]
fn test_analytics_report_leaderboard_dimensions() {
    let env = Env::default();
    let (_, contract_id, client) = setup(&env);

    let player_a = Address::generate(&env);
    let player_b = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut lb = CoinflipContract::load_leaderboard(&env);
        lb.entries.push_back(LeaderboardEntry {
            player: player_a.clone(),
            total_winnings: 50_000_000,
            longest_streak: 4,
            total_games: 20,
        });
        lb.entries.push_back(LeaderboardEntry {
            player: player_b.clone(),
            total_winnings: 80_000_000,
            longest_streak: 7,
            total_games: 30,
        });
        CoinflipContract::save_leaderboard(&env, &lb);
    });

    let report = client.get_analytics_report();
    assert_eq!(report.leaderboard_size, 2);
    assert_eq!(report.top_streak, 7);
    assert_eq!(report.top_player_winnings, 80_000_000);
}

#[test]
fn test_analytics_report_win_rate_nonzero_with_fees() {
    let env = Env::default();
    let (_, contract_id, client) = setup(&env);

    env.as_contract(&contract_id, || {
        let mut stats = CoinflipContract::load_stats(&env);
        stats.total_games = 100;
        stats.total_volume = 1_000_000_000;
        stats.total_fees = 30_000_000; // 3% fee rate
        CoinflipContract::save_stats(&env, &stats);
    });

    let report = client.get_analytics_report();
    // win_rate_bps = 5000 + (fee_ratio_bps / 2)
    // fee_ratio_bps = (30_000_000 * 10_000) / 1_000_000_000 = 300
    // win_rate_bps = 5000 + 150 = 5150
    assert_eq!(report.win_rate_bps, 5150);
}

// ── export_player_data ────────────────────────────────────────────────────────

#[test]
fn test_export_player_data_zero_state() {
    let env = Env::default();
    let (_, _, client) = setup(&env);
    let player = Address::generate(&env);
    let export = client.export_player_data(&player);
    assert_eq!(export.player, player);
    assert_eq!(export.games_played, 0);
    assert_eq!(export.wins, 0);
    assert_eq!(export.losses, 0);
    assert_eq!(export.win_rate_bps, 0);
    assert_eq!(export.max_streak, 0);
    assert_eq!(export.total_wagered, 0);
    assert_eq!(export.net_winnings, 0);
    assert_eq!(export.leaderboard_winnings, 0);
    assert_eq!(export.leaderboard_streak, 0);
}

#[test]
fn test_export_player_data_reflects_player_stats() {
    let env = Env::default();
    let (_, contract_id, client) = setup(&env);
    let player = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let stats = PlayerStats {
            games_played: 20,
            wins: 12,
            losses: 8,
            max_streak: 3,
            current_streak: 1,
            total_wagered: 200_000_000,
            net_winnings: 15_000_000,
        };
        CoinflipContract::save_player_stats(&env, &player, &stats);
    });

    let export = client.export_player_data(&player);
    assert_eq!(export.games_played, 20);
    assert_eq!(export.wins, 12);
    assert_eq!(export.losses, 8);
    assert_eq!(export.win_rate_bps, 6000); // 12/20 * 10_000
    assert_eq!(export.max_streak, 3);
    assert_eq!(export.total_wagered, 200_000_000);
    assert_eq!(export.net_winnings, 15_000_000);
}

#[test]
fn test_export_player_data_includes_leaderboard_stats() {
    let env = Env::default();
    let (_, contract_id, client) = setup(&env);
    let player = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let lb_stats = PlayerLeaderboardStats {
            total_winnings: 75_000_000,
            longest_streak: 5,
            total_games: 15,
        };
        CoinflipContract::save_player_leaderboard_stats(&env, &player, &lb_stats);
    });

    let export = client.export_player_data(&player);
    assert_eq!(export.leaderboard_winnings, 75_000_000);
    assert_eq!(export.leaderboard_streak, 5);
}

#[test]
fn test_export_player_data_win_rate_100_percent() {
    let env = Env::default();
    let (_, contract_id, client) = setup(&env);
    let player = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let stats = PlayerStats {
            games_played: 5,
            wins: 5,
            losses: 0,
            max_streak: 5,
            current_streak: 5,
            total_wagered: 50_000_000,
            net_winnings: 40_000_000,
        };
        CoinflipContract::save_player_stats(&env, &player, &stats);
    });

    let export = client.export_player_data(&player);
    assert_eq!(export.win_rate_bps, 10_000); // 100%
}

#[test]
fn test_export_player_data_win_rate_zero_percent() {
    let env = Env::default();
    let (_, contract_id, client) = setup(&env);
    let player = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let stats = PlayerStats {
            games_played: 5,
            wins: 0,
            losses: 5,
            max_streak: 0,
            current_streak: 0,
            total_wagered: 50_000_000,
            net_winnings: -50_000_000,
        };
        CoinflipContract::save_player_stats(&env, &player, &stats);
    });

    let export = client.export_player_data(&player);
    assert_eq!(export.win_rate_bps, 0);
}
