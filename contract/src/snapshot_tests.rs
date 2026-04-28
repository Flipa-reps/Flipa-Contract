//! Snapshot tests for contract state serialization/deserialization.
//!
//! Strategy: write a value into `env.storage()`, read it back, and compare
//! the Debug representation as an `insta` snapshot.  This catches any
//! unintended change to field order, field names, or enum discriminants
//! across refactors and upgrades.
//!
//! Run:   `cargo test --features testutils snapshot`
//! Update snapshots: `INSTA_UPDATE=always cargo test --features testutils snapshot`

#[cfg(test)]
mod snapshot_tests {
    use crate::*;
    use soroban_sdk::{testutils::Address as _, Address, Bytes, BytesN, Env};
    use insta::assert_debug_snapshot;

    // ── helpers ──────────────────────────────────────────────────────────────

    fn env() -> Env {
        Env::default()
    }

    /// Round-trip a value through persistent storage and return the read-back copy.
    /// Panics if the value cannot be stored or retrieved.
    fn storage_roundtrip<K, V>(env: &Env, key: K, value: &V) -> V
    where
        K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>
            + soroban_sdk::TryFromVal<Env, soroban_sdk::Val>
            + Clone,
        V: soroban_sdk::IntoVal<Env, soroban_sdk::Val>
            + soroban_sdk::TryFromVal<Env, soroban_sdk::Val>,
    {
        env.storage().persistent().set(&key, value);
        env.storage().persistent().get(&key).unwrap()
    }

    fn zero_bytes32(env: &Env) -> BytesN<32> {
        BytesN::from_array(env, &[0u8; 32])
    }

    fn zero_bytes64(env: &Env) -> BytesN<64> {
        BytesN::from_array(env, &[0u8; 64])
    }

    fn deterministic_hash(env: &Env, seed: u8) -> BytesN<32> {
        env.crypto()
            .sha256(&Bytes::from_slice(env, &[seed; 32]))
            .into()
    }

    fn default_multipliers(env: &Env) -> MultiplierConfig {
        MultiplierConfig {
            streak1: 19_000,
            streak2: 35_000,
            streak3: 60_000,
            streak4_plus: 100_000,
        }
    }

    fn minimal_config(env: &Env) -> ContractConfig {
        let admin = Address::generate(env);
        let treasury = Address::generate(env);
        let token = Address::generate(env);
        ContractConfig {
            admin,
            treasury,
            token,
            fee_bps: 300,
            min_wager: 1_000_000,
            max_wager: 100_000_000,
            paused: false,
            shutdown_mode: false,
            multipliers: default_multipliers(env),
            min_reserve_threshold: 0,
            oracle_vrf_pk: zero_bytes32(env),
        }
    }

    fn minimal_game(env: &Env) -> GameState {
        GameState {
            wager: 10_000_000,
            side: Side::Heads,
            streak: 0,
            commitment: deterministic_hash(env, 1),
            contract_random: deterministic_hash(env, 2),
            fee_bps: 300,
            phase: GamePhase::Committed,
            start_ledger: 1000,
            side_bet: SideBet::None,
            side_bet_amount: 0,
            multipliers: default_multipliers(env),
            oracle_commitment: zero_bytes32(env),
            vrf_input: deterministic_hash(env, 3),
            token: Address::generate(env),
        }
    }

    // ── ContractConfig ────────────────────────────────────────────────────────

    #[test]
    fn snapshot_contract_config_default() {
        let env = env();
        let config = minimal_config(&env);
        assert_debug_snapshot!("contract_config_default", config);
    }

    #[test]
    fn snapshot_contract_config_paused() {
        let env = env();
        let mut config = minimal_config(&env);
        config.paused = true;
        config.fee_bps = 500;
        assert_debug_snapshot!("contract_config_paused", config);
    }

    #[test]
    fn snapshot_contract_config_shutdown() {
        let env = env();
        let mut config = minimal_config(&env);
        config.shutdown_mode = true;
        assert_debug_snapshot!("contract_config_shutdown", config);
    }

    #[test]
    fn roundtrip_contract_config() {
        let env = env();
        let original = minimal_config(&env);
        let restored: ContractConfig =
            storage_roundtrip(&env, StorageKey::Config, &original);
        assert_eq!(original, restored);
    }

    #[test]
    fn roundtrip_contract_config_fields_preserved() {
        let env = env();
        let original = minimal_config(&env);
        let restored: ContractConfig =
            storage_roundtrip(&env, StorageKey::Config, &original);
        assert_eq!(restored.fee_bps, 300);
        assert_eq!(restored.min_wager, 1_000_000);
        assert_eq!(restored.max_wager, 100_000_000);
        assert!(!restored.paused);
        assert!(!restored.shutdown_mode);
        assert_eq!(restored.min_reserve_threshold, 0);
    }

    // ── ContractStats ─────────────────────────────────────────────────────────

    #[test]
    fn snapshot_contract_stats_zero() {
        let env = env();
        let stats = ContractStats {
            total_games: 0,
            total_volume: 0,
            total_fees: 0,
            reserve_balance: 0,
            pool_size: 0,
            mix_count: 0,
        };
        assert_debug_snapshot!("contract_stats_zero", stats);
    }

    #[test]
    fn snapshot_contract_stats_populated() {
        let env = env();
        let stats = ContractStats {
            total_games: 1_000_000,
            total_volume: 1_000_000_000_000,
            total_fees: 30_000_000_000,
            reserve_balance: 500_000_000_000,
            pool_size: 1_000_000,
            mix_count: 999_999,
        };
        assert_debug_snapshot!("contract_stats_populated", stats);
    }

    #[test]
    fn roundtrip_contract_stats() {
        let env = env();
        let original = ContractStats {
            total_games: 1_234,
            total_volume: 123_456_789,
            total_fees: 12_345_678,
            reserve_balance: 1_000_000_000,
            pool_size: 42,
            mix_count: 21,
        };
        let restored: ContractStats =
            storage_roundtrip(&env, StorageKey::Stats, &original);
        assert_eq!(original, restored);
    }

    #[test]
    fn roundtrip_contract_stats_fields_preserved() {
        let env = env();
        let original = ContractStats {
            total_games: 500,
            total_volume: 600_000_000,
            total_fees: 18_000_000,
            reserve_balance: 50_000_000,
            pool_size: 500,
            mix_count: 500,
        };
        let restored: ContractStats =
            storage_roundtrip(&env, StorageKey::Stats, &original);
        assert_eq!(restored.total_games, 500);
        assert_eq!(restored.total_volume, 600_000_000);
        assert_eq!(restored.total_fees, 18_000_000);
        assert_eq!(restored.reserve_balance, 50_000_000);
        assert_eq!(restored.pool_size, 500);
        assert_eq!(restored.mix_count, 500);
    }

    // ── GameState ─────────────────────────────────────────────────────────────

    #[test]
    fn snapshot_game_state_committed() {
        let env = env();
        let game = minimal_game(&env);
        assert_debug_snapshot!("game_state_committed", game);
    }

    #[test]
    fn snapshot_game_state_revealed() {
        let env = env();
        let mut game = minimal_game(&env);
        game.phase = GamePhase::Revealed;
        game.streak = 1;
        assert_debug_snapshot!("game_state_revealed", game);
    }

    #[test]
    fn snapshot_game_state_completed() {
        let env = env();
        let mut game = minimal_game(&env);
        game.phase = GamePhase::Completed;
        game.streak = 3;
        assert_debug_snapshot!("game_state_completed", game);
    }

    #[test]
    fn snapshot_game_state_with_side_bet_exact() {
        let env = env();
        let mut game = minimal_game(&env);
        game.side_bet = SideBet::ExactStreak(5);
        game.side_bet_amount = 5_000_000;
        assert_debug_snapshot!("game_state_side_bet_exact", game);
    }

    #[test]
    fn snapshot_game_state_with_side_bet_sequence() {
        let env = env();
        let mut game = minimal_game(&env);
        game.side_bet = SideBet::Sequence(3);
        game.side_bet_amount = 3_000_000;
        assert_debug_snapshot!("game_state_side_bet_sequence", game);
    }

    #[test]
    fn roundtrip_game_state() {
        let env = env();
        let player = Address::generate(&env);
        let original = minimal_game(&env);
        let restored: GameState =
            storage_roundtrip(&env, StorageKey::PlayerGame(player), &original);
        assert_eq!(original, restored);
    }

    #[test]
    fn roundtrip_game_state_fields_preserved() {
        let env = env();
        let player = Address::generate(&env);
        let original = minimal_game(&env);
        let restored: GameState =
            storage_roundtrip(&env, StorageKey::PlayerGame(player), &original);
        assert_eq!(restored.wager, 10_000_000);
        assert_eq!(restored.fee_bps, 300);
        assert_eq!(restored.streak, 0);
        assert_eq!(restored.phase, GamePhase::Committed);
        assert_eq!(restored.start_ledger, 1000);
        assert_eq!(restored.side_bet_amount, 0);
    }

    // ── Enum variants ─────────────────────────────────────────────────────────

    #[test]
    fn snapshot_side_enum_all_variants() {
        assert_debug_snapshot!("side_heads", Side::Heads);
        assert_debug_snapshot!("side_tails", Side::Tails);
    }

    #[test]
    fn snapshot_game_phase_all_variants() {
        assert_debug_snapshot!("phase_committed", GamePhase::Committed);
        assert_debug_snapshot!("phase_revealed", GamePhase::Revealed);
        assert_debug_snapshot!("phase_completed", GamePhase::Completed);
    }

    #[test]
    fn snapshot_side_bet_all_variants() {
        assert_debug_snapshot!("side_bet_none", SideBet::None);
        assert_debug_snapshot!("side_bet_exact_streak", SideBet::ExactStreak(5));
        assert_debug_snapshot!("side_bet_sequence", SideBet::Sequence(3));
    }

    // ── Error code stability ──────────────────────────────────────────────────

    /// These discriminants are part of the public protocol and must never change.
    #[test]
    fn error_discriminants_stable() {
        assert_eq!(Error::WagerBelowMinimum as u32, 1);
        assert_eq!(Error::WagerAboveMaximum as u32, 2);
        assert_eq!(Error::ActiveGameExists as u32, 3);
        assert_eq!(Error::InsufficientReserves as u32, 4);
        assert_eq!(Error::ContractPaused as u32, 5);
        assert_eq!(Error::NoActiveGame as u32, 10);
        assert_eq!(Error::InvalidPhase as u32, 11);
        assert_eq!(Error::CommitmentMismatch as u32, 12);
        assert_eq!(Error::RevealTimeout as u32, 13);
        assert_eq!(Error::NoWinningsToClaimOrContinue as u32, 20);
        assert_eq!(Error::InvalidCommitment as u32, 21);
        assert_eq!(Error::Unauthorized as u32, 30);
        assert_eq!(Error::InvalidFeePercentage as u32, 31);
        assert_eq!(Error::InvalidWagerLimits as u32, 32);
        assert_eq!(Error::TransferFailed as u32, 40);
        assert_eq!(Error::AdminTreasuryConflict as u32, 50);
        assert_eq!(Error::AlreadyInitialized as u32, 51);
    }

    // ── StorageKey uniqueness ─────────────────────────────────────────────────

    #[test]
    fn storage_key_player_game_unique_per_address() {
        let env = env();
        let a1 = Address::generate(&env);
        let a2 = Address::generate(&env);
        // Different addresses must produce different keys (no collision).
        assert_ne!(
            StorageKey::PlayerGame(a1),
            StorageKey::PlayerGame(a2)
        );
    }

    #[test]
    fn storage_key_player_game_deterministic() {
        let env = env();
        let addr = Address::generate(&env);
        assert_eq!(
            StorageKey::PlayerGame(addr.clone()),
            StorageKey::PlayerGame(addr)
        );
    }

    #[test]
    fn storage_key_global_variants_distinct() {
        assert_ne!(StorageKey::Config, StorageKey::Stats);
        assert_ne!(StorageKey::Config, StorageKey::EntropyPool);
        assert_ne!(StorageKey::Stats, StorageKey::EntropyPool);
    }

    // ── MultiplierConfig ──────────────────────────────────────────────────────

    #[test]
    fn snapshot_multiplier_config_default() {
        let env = env();
        let m = default_multipliers(&env);
        assert_debug_snapshot!("multiplier_config_default", m);
    }

    #[test]
    fn roundtrip_multiplier_config() {
        let env = env();
        let original = MultiplierConfig {
            streak1: 19_000,
            streak2: 35_000,
            streak3: 60_000,
            streak4_plus: 100_000,
        };
        // Store inside a ContractConfig to exercise the nested path.
        let mut config = minimal_config(&env);
        config.multipliers = original.clone();
        let restored: ContractConfig =
            storage_roundtrip(&env, StorageKey::Config, &config);
        assert_eq!(restored.multipliers, original);
    }

    #[test]
    fn multiplier_config_validity_invariant() {
        let valid = MultiplierConfig {
            streak1: 19_000,
            streak2: 35_000,
            streak3: 60_000,
            streak4_plus: 100_000,
        };
        assert!(valid.is_valid());

        let invalid_not_monotone = MultiplierConfig {
            streak1: 35_000,
            streak2: 19_000,
            streak3: 60_000,
            streak4_plus: 100_000,
        };
        assert!(!invalid_not_monotone.is_valid());

        let invalid_below_1x = MultiplierConfig {
            streak1: 9_999,
            streak2: 35_000,
            streak3: 60_000,
            streak4_plus: 100_000,
        };
        assert!(!invalid_below_1x.is_valid());
    }

    // ── EntropyPool ───────────────────────────────────────────────────────────

    #[test]
    fn snapshot_entropy_pool_zero() {
        let env = env();
        let pool = EntropyPool {
            pool: zero_bytes32(&env),
            pool_size: 0,
            mix_count: 0,
        };
        assert_debug_snapshot!("entropy_pool_zero", pool);
    }

    #[test]
    fn roundtrip_entropy_pool() {
        let env = env();
        let original = EntropyPool {
            pool: deterministic_hash(&env, 42),
            pool_size: 100,
            mix_count: 50,
        };
        let restored: EntropyPool =
            storage_roundtrip(&env, StorageKey::EntropyPool, &original);
        assert_eq!(original, restored);
    }

    // ── Backward-compatibility guard ──────────────────────────────────────────

    /// Write a GameState, then read it back and verify every field survives.
    /// This is the primary guard against accidental struct reordering.
    #[test]
    fn backward_compat_game_state_all_fields() {
        let env = env();
        let player = Address::generate(&env);
        let token = Address::generate(&env);

        let original = GameState {
            wager: 99_000_000,
            side: Side::Tails,
            streak: 4,
            commitment: deterministic_hash(&env, 10),
            contract_random: deterministic_hash(&env, 11),
            fee_bps: 250,
            phase: GamePhase::Revealed,
            start_ledger: 55_000,
            side_bet: SideBet::Sequence(2),
            side_bet_amount: 1_000_000,
            multipliers: MultiplierConfig {
                streak1: 19_000,
                streak2: 35_000,
                streak3: 60_000,
                streak4_plus: 100_000,
            },
            oracle_commitment: deterministic_hash(&env, 12),
            vrf_input: deterministic_hash(&env, 13),
            token: token.clone(),
        };

        let restored: GameState =
            storage_roundtrip(&env, StorageKey::PlayerGame(player), &original);

        assert_eq!(restored.wager, 99_000_000);
        assert_eq!(restored.side, Side::Tails);
        assert_eq!(restored.streak, 4);
        assert_eq!(restored.commitment, original.commitment);
        assert_eq!(restored.contract_random, original.contract_random);
        assert_eq!(restored.fee_bps, 250);
        assert_eq!(restored.phase, GamePhase::Revealed);
        assert_eq!(restored.start_ledger, 55_000);
        assert_eq!(restored.side_bet, SideBet::Sequence(2));
        assert_eq!(restored.side_bet_amount, 1_000_000);
        assert_eq!(restored.multipliers, original.multipliers);
        assert_eq!(restored.oracle_commitment, original.oracle_commitment);
        assert_eq!(restored.vrf_input, original.vrf_input);
        assert_eq!(restored.token, token);
    }

    /// Write a ContractConfig, then read it back and verify every field survives.
    #[test]
    fn backward_compat_contract_config_all_fields() {
        let env = env();
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let token = Address::generate(&env);
        let vrf_pk = deterministic_hash(&env, 99);

        let original = ContractConfig {
            admin: admin.clone(),
            treasury: treasury.clone(),
            token: token.clone(),
            fee_bps: 400,
            min_wager: 500_000,
            max_wager: 200_000_000,
            paused: true,
            shutdown_mode: false,
            multipliers: MultiplierConfig {
                streak1: 20_000,
                streak2: 40_000,
                streak3: 70_000,
                streak4_plus: 110_000,
            },
            min_reserve_threshold: 10_000_000,
            oracle_vrf_pk: vrf_pk.clone(),
        };

        let restored: ContractConfig =
            storage_roundtrip(&env, StorageKey::Config, &original);

        assert_eq!(restored.admin, admin);
        assert_eq!(restored.treasury, treasury);
        assert_eq!(restored.token, token);
        assert_eq!(restored.fee_bps, 400);
        assert_eq!(restored.min_wager, 500_000);
        assert_eq!(restored.max_wager, 200_000_000);
        assert!(restored.paused);
        assert!(!restored.shutdown_mode);
        assert_eq!(restored.multipliers.streak1, 20_000);
        assert_eq!(restored.multipliers.streak4_plus, 110_000);
        assert_eq!(restored.min_reserve_threshold, 10_000_000);
        assert_eq!(restored.oracle_vrf_pk, vrf_pk);
    }

    /// Write ContractStats, then read it back and verify every field survives.
    #[test]
    fn backward_compat_contract_stats_all_fields() {
        let env = env();
        let original = ContractStats {
            total_games: 9_999,
            total_volume: 999_999_999,
            total_fees: 29_999_999,
            reserve_balance: 100_000_000,
            pool_size: 9_999,
            mix_count: 4_999,
        };

        let restored: ContractStats =
            storage_roundtrip(&env, StorageKey::Stats, &original);

        assert_eq!(restored.total_games, 9_999);
        assert_eq!(restored.total_volume, 999_999_999);
        assert_eq!(restored.total_fees, 29_999_999);
        assert_eq!(restored.reserve_balance, 100_000_000);
        assert_eq!(restored.pool_size, 9_999);
        assert_eq!(restored.mix_count, 4_999);
    }
}
