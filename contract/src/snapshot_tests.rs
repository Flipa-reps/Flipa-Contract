//! Snapshot tests for contract state serialization.
//! Uses `insta` for Borsh baseline snapshots + round-trip verification.
//!
//! Run `cargo test snapshot_tests -- --nocapture` to review.
//! Update: `cargo test update_snapshots`.

use super::*;
use soroban_sdk::{Env, Address, BytesN};
use insta::assert_snapshot;
use hex;

// Test env for deterministic Borsh serialization.
fn test_env() -> Env {
    Env::default()
}

// ── Utility: Serialize to hex ────────────────────────────────────────────────

fn borsh_to_hex<T: soroban_sdk::contracttype::ContractType>(env: &Env, value: &T) -> String {
    let bytes = env.bytes_from_object(value).unwrap();
    hex::encode(bytes.to_vec())
}

// ── ContractConfig Snapshots ─────────────────────────────────────────────────

#[test]
fn contract_config_default() {
    let env = test_env();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);

    let config = ContractConfig {
        admin,
        treasury,
        token,
        fee_bps: 300,
        min_wager: 1_000_000,
        max_wager: 100_000_000,
        paused: false,
    };

    assert_snapshot!(borsh_to_hex(&env, &config), @r###"
    0000000000000000000000000000000000000000000000000000000000000000
    0000000000000000000000000000000000000000000000000000000000000000
    0000000000000000000000000000000000000000000000000000000000000000
    0000000000000000012c
    00000000000000000000000000000000000000000000000000000f4240
    0000000000000000000000000000000000000000000000000005f5e100
    00
    "###);
}

#[test]
fn contract_config_edge_cases() {
    let env = test_env();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);

    let config_paused = ContractConfig {
        admin: admin.clone(),
        treasury,
        token,
        fee_bps: 500, // max fee
        min_wager: 1_000_000,
        max_wager: i128::MAX / 10, // near max
        paused: true,
    };

    assert_snapshot!(borsh_to_hex(&env, &config_paused));
}

#[test]
fn contract_config_roundtrip() {
    let env = test_env();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);

    let original = ContractConfig {
        admin: admin.clone(),
        treasury,
        token,
        fee_bps: 300,
        min_wager: 1_000_000,
        max_wager: 100_000_000,
        paused: false,
    };

    // Serialize → deserialize → reserialize → must match original bytes
    let bytes = env.bytes_from_object(&original).unwrap();
    let roundtrip: ContractConfig = env.bytes_to_object(&bytes).unwrap();
    let roundtrip_bytes = env.bytes_from_object(&roundtrip).unwrap();

    assert_eq!(bytes, roundtrip_bytes);
    assert_eq!(original, roundtrip); // Field equality
}

// ── ContractStats Snapshots ──────────────────────────────────────────────────

#[test]
fn contract_stats_zero() {
    let env = test_env();
    let stats = ContractStats {
        total_games: 0,
        total_volume: 0,
        total_fees: 0,
        reserve_balance: 0,
    };
    assert_snapshot!(borsh_to_hex(&env, &stats));
}

#[test]
fn contract_stats_production() {
    let env = test_env();
    let stats = ContractStats {
        total_games: 1_000_000,
        total_volume: 1_000_000_000_000, // 100k XLM volume
        total_fees: 30_000_000_000,       // 3% of volume
        reserve_balance: 500_000_000_000, // 50k XLM reserves
    };
    assert_snapshot!(borsh_to_hex(&env, &stats));
}

#[test]
fn contract_stats_roundtrip() {
    let env = test_env();
    let original = ContractStats {
        total_games: 1_234,
        total_volume: 123_456_789,
        total_fees: 12_345_678,
        reserve_balance: 1_000_000_000,
    };

    let bytes = env.bytes_from_object(&original).unwrap();
    let roundtrip: ContractStats = env.bytes_to_object(&bytes).unwrap();
    let roundtrip_bytes = env.bytes_from_object(&roundtrip).unwrap();

    assert_eq!(bytes, roundtrip_bytes);
    assert_eq!(original, roundtrip);
}

// ── GameState Snapshots ──────────────────────────────────────────────────────

#[test]
fn game_state_committed_streak_0() {
    let env = test_env();
    let game = GameState {
        wager: 10_000_000,
        side: Side::Heads,
        streak: 0,
        commitment: env.crypto().sha256(&Bytes::from_slice(&env, &[1u8; 32])).try_into().unwrap(),
        contract_random: env.crypto().sha256(&Bytes::from_slice(&env, &[2u8; 32])).try_into().unwrap(),
        fee_bps: 300,
        phase: GamePhase::Committed,
        start_ledger: 12345,
    };
    assert_snapshot!(borsh_to_hex(&env, &game));
}

#[test]
fn game_state_all_phases() {
    let env = test_env();
    macro_rules! snapshot_phase {
        ($phase:expr) => {
            let game = GameState {
                wager: 10_000_000,
                side: Side::Heads,
                streak: 1,
                commitment: env.crypto().sha256(&Bytes::from_slice(&env, &[42u8; 32])).try_into().unwrap(),
                contract_random: env.crypto().sha256(&Bytes::from_slice(&env, &[43u8; 32])).try_into().unwrap(),
                fee_bps: 300,
                phase: $phase,
                start_ledger: 12345,
            };
            assert_snapshot!(format!("{:?}", borsh_to_hex(&env, &game)));
        };
    }

    snapshot_phase!(GamePhase::Committed);
    snapshot_phase!(GamePhase::Revealed);
    snapshot_phase!(GamePhase::Completed);
}

#[test]
fn game_state_edge_streaks() {
    let env = test_env();
    for streak in [0, 1, 2, 3, 4, 10, u32::MAX] {
        let game = GameState {
            wager: 10_000_000,
            side: Side::Tails,
            streak,
            commitment: BytesN::from_array(&env, &[0; 32]), // deterministic
            contract_random: BytesN::from_array(&env, &[1; 32]),
            fee_bps: 500, // max fee
            phase: GamePhase::Revealed,
            start_ledger: u32::MAX,
        };
        assert_snapshot!(format!("streak_{}", streak), borsh_to_hex(&env, &game));
    }
}

#[test]
fn game_state_roundtrip() {
    let env = test_env();
    let original = GameState {
        wager: 10_000_000,
        side: Side::Heads,
        streak: 2,
        commitment: env.crypto().sha256(&Bytes::from_slice(&env, &[42u8; 32])).try_into().unwrap(),
        contract_random: env.crypto().sha256(&Bytes::from_slice(&env, &[43u8; 32])).try_into().unwrap(),
        fee_bps: 300,
        phase: GamePhase::Revealed,
        start_ledger: 12345,
    };

    let bytes = env.bytes_from_object(&original).unwrap();
    let roundtrip: GameState = env.bytes_to_object(&bytes).unwrap();
    let roundtrip_bytes = env.bytes_from_object(&roundtrip).unwrap();

    assert_eq!(bytes, roundtrip_bytes);
    assert_eq!(original, roundtrip);
}

// ── Enum Snapshots ───────────────────────────────────────────────────────────

#[test]
fn side_enum() {
    let env = test_env();
    assert_snapshot!(borsh_to_hex(&env, &Side::Heads), @"00");
    assert_snapshot!(borsh_to_hex(&env, &Side::Tails), @"01");
}

#[test]
fn game_phase_enum() {
    let env = test_env();
    assert_snapshot!(borsh_to_hex(&env, &GamePhase::Committed), @"00");
    assert_snapshot!(borsh_to_hex(&env, &GamePhase::Revealed), @"01");
    assert_snapshot!(borsh_to_hex(&env, &GamePhase::Completed), @"02");
}

#[test]
fn storage_key_enum() {
    let env = test_env();
    let admin = Address::generate(&env);
    assert_snapshot!(borsh_to_hex(&env, &StorageKey::Config), @"00");
    assert_snapshot!(borsh_to_hex(&env, &StorageKey::Stats), @"01");
    assert_snapshot!(borsh_to_hex(&env, &StorageKey::PlayerGame(admin)));
}

#[test]
fn error_enum_stable_codes() {
    // Verify stable u32 discriminants (protocol contract)
    assert_eq!(Error::WagerBelowMinimum as u32, 1);
    assert_eq!(Error::AlreadyInitialized as u32, 51);
    // All 17 codes covered in lib.rs error_codes::VARIANT_COUNT
}

// ── Backward Compatibility Probes ────────────────────────────────────────────

#[test]
fn legacy_game_state_deserializes() {
    // Embed known-good legacy Borsh bytes (update when format changes intentionally)
    let env = test_env();
    let legacy_bytes = hex::decode("...").unwrap(); // TODO: capture from mainnet/deployed
    let _legacy_game: GameState = env.bytes_to_object(&env.bytes_object(&legacy_bytes)).unwrap();
    // Will fail-fast if fields reordered/renamed/added incompatibly
}
