/// # Storage TTL Management Tests
///
/// ## TTL Strategy
///
/// All three persistent storage keys use the same threshold/extend pattern:
///
/// ```text
/// extend_ttl(key, TTL_THRESHOLD = 100_000, TTL_EXTEND_TO = 500_000)
/// ```
///
/// The call is made on **every read and write** so that active entries never
/// silently expire.  The threshold avoids a redundant host call when the TTL
/// is already healthy (> 100_000 ledgers remaining).
///
/// | Key              | Extended by              |
/// |------------------|--------------------------|
/// | `Config`         | `initialize`, `save_config`, `load_config` |
/// | `Stats`          | `initialize`, `save_stats`, `load_stats`   |
/// | `PlayerGame(p)`  | `save_player_game`, `load_player_game`     |
///
/// ## Coverage
///
/// | Test                                          | What is verified                        |
/// |-----------------------------------------------|-----------------------------------------|
/// | `initialize_extends_config_ttl`               | Config TTL ≥ TTL_EXTEND_TO after init   |
/// | `initialize_extends_stats_ttl`                | Stats TTL ≥ TTL_EXTEND_TO after init    |
/// | `start_game_extends_player_game_ttl`          | PlayerGame TTL ≥ TTL_EXTEND_TO          |
/// | `start_game_extends_stats_ttl`                | Stats TTL refreshed on start_game       |
/// | `load_config_extends_ttl`                     | get_config refreshes Config TTL         |
/// | `load_stats_extends_ttl`                      | get_stats refreshes Stats TTL           |
/// | `cash_out_extends_stats_ttl`                  | Stats TTL refreshed on settlement       |
/// | `continue_streak_extends_player_game_ttl`     | PlayerGame TTL refreshed on continue    |
/// | `admin_ops_extend_config_ttl`                 | set_fee / set_paused refresh Config TTL |
/// | `data_persists_across_ledger_advance`         | Data readable after ledger advance      |
/// | `ttl_refreshed_on_repeated_reads`             | TTL stays healthy across multiple reads |
/// | `player_game_ttl_independent_per_player`      | Each player's TTL is independent        |
use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::testutils::storage::Persistent as _;

// ── Constants (mirrors private contract constants) ────────────────────────────

const TTL_THRESHOLD: u32 = 100_000;
const TTL_EXTEND_TO: u32 = 500_000;

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

fn fund(env: &Env, contract_id: &Address) {
    env.as_contract(contract_id, || {
        let mut stats = CoinflipContract::load_stats(env);
        stats.reserve_balance = 1_000_000_000;
        CoinflipContract::save_stats(env, &stats);
    });
}

fn commitment(env: &Env) -> BytesN<32> {
    env.crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(env, &[7u8; 32]))
        .into()
}

fn get_ttl(env: &Env, contract_id: &Address, key: &StorageKey) -> u32 {
    env.as_contract(contract_id, || {
        env.storage().persistent().get_ttl(key)
    })
}

// ── initialize extends TTL ────────────────────────────────────────────────────

/// initialize sets Config TTL to TTL_EXTEND_TO.
#[test]
fn initialize_extends_config_ttl() {
    let (env, _, contract_id, _) = setup();
    let ttl = get_ttl(&env, &contract_id, &StorageKey::Config);
    assert_eq!(ttl, TTL_EXTEND_TO);
}

/// initialize sets Stats TTL to TTL_EXTEND_TO.
#[test]
fn initialize_extends_stats_ttl() {
    let (env, _, contract_id, _) = setup();
    let ttl = get_ttl(&env, &contract_id, &StorageKey::Stats);
    assert_eq!(ttl, TTL_EXTEND_TO);
}

// ── start_game extends TTL ────────────────────────────────────────────────────

/// start_game creates a PlayerGame entry with TTL = TTL_EXTEND_TO.
#[test]
fn start_game_extends_player_game_ttl() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id);
    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    let ttl = get_ttl(&env, &contract_id, &StorageKey::PlayerGame(player));
    assert_eq!(ttl, TTL_EXTEND_TO);
}

/// start_game touches Stats (total_games/volume update), refreshing its TTL.
#[test]
fn start_game_extends_stats_ttl() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id);
    // Decay the Stats TTL below TTL_EXTEND_TO by advancing the ledger
    env.ledger().with_mut(|l| l.sequence_number += TTL_THRESHOLD + 1);
    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    let ttl = get_ttl(&env, &contract_id, &StorageKey::Stats);
    assert_eq!(ttl, TTL_EXTEND_TO);
}

// ── read operations extend TTL ────────────────────────────────────────────────

/// get_config (load_config) refreshes Config TTL to TTL_EXTEND_TO.
#[test]
fn load_config_extends_ttl() {
    let (env, client, contract_id, _) = setup();
    // Advance ledger to decay TTL below TTL_EXTEND_TO
    env.ledger().with_mut(|l| l.sequence_number += TTL_THRESHOLD + 1);
    client.get_config(); // triggers load_config → extend_ttl
    let ttl = get_ttl(&env, &contract_id, &StorageKey::Config);
    assert_eq!(ttl, TTL_EXTEND_TO);
}

/// get_stats (load_stats) refreshes Stats TTL to TTL_EXTEND_TO.
#[test]
fn load_stats_extends_ttl() {
    let (env, client, contract_id, _) = setup();
    env.ledger().with_mut(|l| l.sequence_number += TTL_THRESHOLD + 1);
    client.get_stats(); // triggers load_stats → extend_ttl
    let ttl = get_ttl(&env, &contract_id, &StorageKey::Stats);
    assert_eq!(ttl, TTL_EXTEND_TO);
}

// ── settlement operations extend TTL ─────────────────────────────────────────

/// cash_out updates Stats (reserve/fees), refreshing Stats TTL.
#[test]
fn cash_out_extends_stats_ttl() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id);
    let player = Address::generate(&env);
    // Inject a Revealed game directly
    let c: BytesN<32> = env.crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]))
        .into();
    let cr: BytesN<32> = env.crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(&env, &[2u8; 32]))
        .into();
    let game = GameState {
        wager: 10_000_000, side: Side::Heads, streak: 1,
        commitment: c, contract_random: cr, fee_bps: 300,
        phase: GamePhase::Revealed, start_ledger: env.ledger().sequence(),
    
        vrf_input: env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into(),
    };
    env.as_contract(&contract_id, || {
        CoinflipContract::save_player_game(&env, &player, &game);
    });
    env.ledger().with_mut(|l| l.sequence_number += TTL_THRESHOLD + 1);
    client.cash_out(&player);
    let ttl = get_ttl(&env, &contract_id, &StorageKey::Stats);
    assert_eq!(ttl, TTL_EXTEND_TO);
}

/// continue_streak re-saves PlayerGame, refreshing its TTL.
#[test]
fn continue_streak_extends_player_game_ttl() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id);
    let player = Address::generate(&env);
    let c: BytesN<32> = env.crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]))
        .into();
    let cr: BytesN<32> = env.crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(&env, &[2u8; 32]))
        .into();
    let game = GameState {
        wager: 10_000_000, side: Side::Heads, streak: 1,
        commitment: c, contract_random: cr, fee_bps: 300,
        phase: GamePhase::Revealed, start_ledger: env.ledger().sequence(),
    
        vrf_input: env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into(),
    };
    env.as_contract(&contract_id, || {
        CoinflipContract::save_player_game(&env, &player, &game);
    });
    env.ledger().with_mut(|l| l.sequence_number += TTL_THRESHOLD + 1);
    let nc: BytesN<32> = env.crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32]))
        .into();
    client.continue_streak(&player, &nc);
    let ttl = get_ttl(&env, &contract_id, &StorageKey::PlayerGame(player));
    assert_eq!(ttl, TTL_EXTEND_TO);
}

// ── admin operations extend TTL ───────────────────────────────────────────────

/// set_fee (save_config) refreshes Config TTL.
#[test]
fn admin_set_fee_extends_config_ttl() {
    let (env, client, contract_id, admin) = setup();
    env.ledger().with_mut(|l| l.sequence_number += TTL_THRESHOLD + 1);
    client.set_fee(&admin, &400);
    let ttl = get_ttl(&env, &contract_id, &StorageKey::Config);
    assert_eq!(ttl, TTL_EXTEND_TO);
}

/// set_paused (save_config) refreshes Config TTL.
#[test]
fn admin_set_paused_extends_config_ttl() {
    let (env, client, contract_id, admin) = setup();
    env.ledger().with_mut(|l| l.sequence_number += TTL_THRESHOLD + 1);
    client.set_paused(&admin, &true);
    let ttl = get_ttl(&env, &contract_id, &StorageKey::Config);
    assert_eq!(ttl, TTL_EXTEND_TO);
}

// ── data persistence across ledger advance ────────────────────────────────────

/// Config and Stats data are readable after a large ledger advance (< TTL_EXTEND_TO).
#[test]
fn data_persists_across_ledger_advance() {
    let (env, client, _, _) = setup();
    // Advance well within the TTL window
    env.ledger().with_mut(|l| l.sequence_number += TTL_EXTEND_TO / 2);
    // Data must still be readable and correct
    let config = client.get_config();
    assert_eq!(config.fee_bps, 300);
    assert_eq!(config.min_wager, 1_000_000);
    let stats = client.get_stats();
    assert_eq!(stats.total_games, 0);
}

/// PlayerGame data is readable after a ledger advance within TTL window.
#[test]
fn player_game_persists_across_ledger_advance() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id);
    let player = Address::generate(&env);
    client.start_game(&player, &Side::Tails, &10_000_000, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    env.ledger().with_mut(|l| l.sequence_number += TTL_EXTEND_TO / 2);
    // get_game_state triggers load_player_game → extend_ttl
    let game = client.get_game_state(&player);
    assert!(game.is_some());
    assert_eq!(game.unwrap().phase, GamePhase::Committed);
}

// ── TTL refreshed on repeated reads ──────────────────────────────────────────

/// Each get_config call resets Config TTL to TTL_EXTEND_TO regardless of how
/// many times it has been called.
#[test]
fn ttl_refreshed_on_repeated_reads() {
    let (env, client, contract_id, _) = setup();
    for _ in 0..3 {
        env.ledger().with_mut(|l| l.sequence_number += TTL_THRESHOLD + 1);
        client.get_config();
        assert_eq!(get_ttl(&env, &contract_id, &StorageKey::Config), TTL_EXTEND_TO);
    }
}

// ── Per-player TTL independence ───────────────────────────────────────────────

/// Each player's PlayerGame entry has its own independent TTL.
/// Accessing one player's game does not affect another's TTL.
#[test]
fn player_game_ttl_independent_per_player() {
    let (env, client, contract_id, _) = setup();
    fund(&env, &contract_id);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    client.start_game(&p1, &Side::Heads, &10_000_000, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));
    client.start_game(&p2, &Side::Tails, &10_000_000, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));

    // Advance ledger to decay both TTLs below TTL_EXTEND_TO
    env.ledger().with_mut(|l| l.sequence_number += TTL_THRESHOLD + 1);

    // Only read p1's game — should refresh p1's TTL but not p2's
    client.get_game_state(&p1);

    let ttl_p1 = get_ttl(&env, &contract_id, &StorageKey::PlayerGame(p1));
    let ttl_p2 = get_ttl(&env, &contract_id, &StorageKey::PlayerGame(p2.clone()));

    assert_eq!(ttl_p1, TTL_EXTEND_TO, "p1 TTL must be refreshed after read");
    // p2's TTL was not refreshed — it decayed by TTL_THRESHOLD + 1
    assert_eq!(
        ttl_p2,
        TTL_EXTEND_TO - (TTL_THRESHOLD + 1),
        "p2 TTL must not be affected by p1 read"
    );
}
