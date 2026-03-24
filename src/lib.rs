#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, contracterror, Address, BytesN, Env};

/// Error codes for the coinflip contract
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    // Game creation errors
    WagerBelowMinimum = 1,
    WagerAboveMaximum = 2,
    ActiveGameExists = 3,
    InsufficientReserves = 4,
    ContractPaused = 5,
    
    // Reveal errors
    NoActiveGame = 10,
    InvalidPhase = 11,
    CommitmentMismatch = 12,
    RevealTimeout = 13,
    
    // Action errors
    NoWinningsToClaimOrContinue = 20,
    InvalidCommitment = 21,
    
    // Admin errors
    Unauthorized = 30,
    InvalidFeePercentage = 31,
    InvalidWagerLimits = 32,
    
    // Transfer errors
    TransferFailed = 40,

    // Initialization errors
    /// admin and treasury must be distinct addresses
    AdminTreasuryConflict = 50,
    /// contract has already been initialized
    AlreadyInitialized = 51,
}

/// Side choice for the coinflip
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Side {
    Heads = 0,
    Tails = 1,
}

/// Game phase tracking
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum GamePhase {
    Committed,    // Waiting for reveal
    Revealed,     // Outcome determined, awaiting decision
    Completed,    // Game ended
}

/// Per-player game state
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameState {
    pub wager: i128,              // Original wager amount in stroops
    pub side: Side,               // Heads (0) or Tails (1)
    pub streak: u32,              // Current win streak (0-4+)
    pub commitment: BytesN<32>,   // Hash commitment for randomness
    pub contract_random: BytesN<32>, // Contract's random contribution
    pub phase: GamePhase,         // Current phase
}

/// Contract configuration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractConfig {
    pub admin: Address,           // Administrator address
    pub treasury: Address,        // Fee collection address
    pub fee_bps: u32,            // Fee in basis points (200-500 = 2-5%)
    pub min_wager: i128,         // Minimum wager in stroops
    pub max_wager: i128,         // Maximum wager in stroops
    pub paused: bool,            // Emergency pause flag
}

/// Contract statistics
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractStats {
    pub total_games: u64,        // Total games played
    pub total_volume: i128,      // Total XLM wagered
    pub total_fees: i128,        // Total fees collected
    pub reserve_balance: i128,   // Current contract reserves
}

/// Storage keys for contract data
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Config,                    // Global configuration
    Stats,                     // Global statistics
    PlayerGame(Address),       // Per-player game state
}

/// Multiplier values in basis points (1 bps = 0.0001x).
/// Applied to the wager to compute gross payout before fees.
///
/// | Streak | Multiplier | Rationale                          |
/// |--------|------------|------------------------------------|
/// | 1      | 1.9x       | ~5% house edge on a fair 2x payout |
/// | 2      | 3.5x       | ~6.25% edge compounded over 2 wins |
/// | 3      | 6.0x       | ~6.25% edge compounded over 3 wins |
/// | 4+     | 10.0x      | ~6.25% edge compounded over 4 wins |
const MULTIPLIER_STREAK_1: u32 = 19_000; // 1.9x
const MULTIPLIER_STREAK_2: u32 = 35_000; // 3.5x
const MULTIPLIER_STREAK_3: u32 = 60_000; // 6.0x
const MULTIPLIER_STREAK_4_PLUS: u32 = 100_000; // 10.0x

/// Returns the gross payout multiplier (in basis points, 10_000 = 1x)
/// for the given win `streak` level.
///
/// - streak 1  → 19_000 (1.9x)
/// - streak 2  → 35_000 (3.5x)
/// - streak 3  → 60_000 (6.0x)
/// - streak 4+ → 100_000 (10.0x)
pub fn get_multiplier(streak: u32) -> u32 {
    match streak {
        1 => MULTIPLIER_STREAK_1,
        2 => MULTIPLIER_STREAK_2,
        3 => MULTIPLIER_STREAK_3,
        _ => MULTIPLIER_STREAK_4_PLUS,
    }
}

/// Calculates the net payout for a winning streak.
///
/// Formulas (all in stroops):
/// - gross = wager × multiplier_bps / 10_000
/// - fee   = gross × fee_bps / 10_000
/// - net   = gross − fee
///
/// Returns `None` if any intermediate multiplication overflows `i128`.
///
/// # Arguments
/// - `wager`   – original wager in stroops (must be > 0)
/// - `streak`  – current win streak (passed to `get_multiplier`)
/// - `fee_bps` – protocol fee in basis points (200–500)
pub fn calculate_payout(wager: i128, streak: u32, fee_bps: u32) -> Option<i128> {
    let multiplier = get_multiplier(streak) as i128;
    let gross = wager.checked_mul(multiplier)?.checked_div(10_000)?;
    let fee   = gross.checked_mul(fee_bps as i128)?.checked_div(10_000)?;
    gross.checked_sub(fee)
}

#[contract]
pub struct CoinflipContract;

#[contractimpl]
impl CoinflipContract {
    /// Initialize the contract with configuration.
    ///
    /// # Initialization invariant
    ///
    /// This function may only be called **once**. On the first call it writes
    /// `StorageKey::Config` to persistent storage; every subsequent call checks
    /// for that key and returns `Error::AlreadyInitialized` immediately,
    /// leaving all existing state untouched.
    ///
    /// This is the sole re-initialization guard. There is no admin override or
    /// migration path — a deployed contract's configuration is immutable after
    /// the first successful `initialize` call (fields may be updated through
    /// separate admin functions, but `initialize` itself cannot be re-run).
    ///
    /// Accepted inputs:
    /// - `admin`    – any valid Stellar address; must differ from `treasury`
    /// - `treasury` – any valid Stellar address; must differ from `admin`
    /// - `fee_bps`  – 200–500 (2–5%)
    /// - `min_wager` / `max_wager` – stroops, min < max
    ///
    /// Errors if the contract is already initialized, if admin == treasury,
    /// or if numeric parameters are out of range.
    pub fn initialize(
        env: Env,
        admin: Address,
        treasury: Address,
        fee_bps: u32,
        min_wager: i128,
        max_wager: i128,
    ) -> Result<(), Error> {
        // Guard: prevent re-initialization
        if env.storage().persistent().has(&StorageKey::Config) {
            return Err(Error::AlreadyInitialized);
        }

        // Guard: admin and treasury must be distinct roles
        if admin == treasury {
            return Err(Error::AdminTreasuryConflict);
        }

        // Validate fee percentage (2-5%)
        if fee_bps < 200 || fee_bps > 500 {
            return Err(Error::InvalidFeePercentage);
        }

        // Validate wager limits
        if min_wager >= max_wager {
            return Err(Error::InvalidWagerLimits);
        }
        
        let config = ContractConfig {
            admin,
            treasury,
            fee_bps,
            min_wager,
            max_wager,
            paused: false,
        };
        
        let stats = ContractStats {
            total_games: 0,
            total_volume: 0,
            total_fees: 0,
            reserve_balance: 0,
        };
        
        env.storage().persistent().set(&StorageKey::Config, &config);
        env.storage().persistent().set(&StorageKey::Stats, &stats);
        
        Ok(())
    }
    
    // Storage helper functions (internal use)
    fn save_config(env: &Env, config: &ContractConfig) {
        env.storage().persistent().set(&StorageKey::Config, config);
    }

    fn load_config(env: &Env) -> ContractConfig {
        env.storage()
            .persistent()
            .get(&StorageKey::Config)
            .unwrap()
    }

    fn save_stats(env: &Env, stats: &ContractStats) {
        env.storage().persistent().set(&StorageKey::Stats, stats);
    }

    fn load_stats(env: &Env) -> ContractStats {
        env.storage()
            .persistent()
            .get(&StorageKey::Stats)
            .unwrap()
    }

    fn save_player_game(env: &Env, player: &Address, game: &GameState) {
        env.storage()
            .persistent()
            .set(&StorageKey::PlayerGame(player.clone()), game);
    }

    fn load_player_game(env: &Env, player: &Address) -> Option<GameState> {
        env.storage()
            .persistent()
            .get(&StorageKey::PlayerGame(player.clone()))
    }

    fn delete_player_game(env: &Env, player: &Address) {
        env.storage()
            .persistent()
            .remove(&StorageKey::PlayerGame(player.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_get_multiplier_streak_levels() {
        assert_eq!(get_multiplier(1), 19_000);
        assert_eq!(get_multiplier(2), 35_000);
        assert_eq!(get_multiplier(3), 60_000);
    }

    #[test]
    fn test_get_multiplier_streak_4_plus() {
        // streak 4 and beyond all return the max multiplier
        assert_eq!(get_multiplier(4), 100_000);
        assert_eq!(get_multiplier(10), 100_000);
        assert_eq!(get_multiplier(u32::MAX), 100_000);
    }

    #[test]
    fn test_get_multiplier_streak_0_returns_max() {
        // streak 0 is not a valid game state, but the function must not panic;
        // it falls through to the wildcard arm and returns the 4+ multiplier.
        assert_eq!(get_multiplier(0), 100_000);
    }

    #[test]
    fn test_initialize_rejects_same_admin_and_treasury() {
        let env = Env::default();
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(&env, &contract_id);

        let addr = Address::generate(&env);
        let result = client.try_initialize(&addr, &addr, &300, &1_000_000, &100_000_000);
        assert_eq!(result, Err(Ok(Error::AdminTreasuryConflict)));
    }

    #[test]
    fn test_initialize_rejects_reinitialization() {
        let env = Env::default();
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.initialize(&admin, &treasury, &300, &1_000_000, &100_000_000);

        // Second call must fail
        let result = client.try_initialize(&admin, &treasury, &300, &1_000_000, &100_000_000);
        assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
    }

    #[test]
    fn test_calculate_payout_basic() {
        // wager=10_000_000, streak=1 (1.9x), fee=300bps (3%)
        // gross = 10_000_000 * 19_000 / 10_000 = 19_000_000
        // fee   = 19_000_000 * 300  / 10_000 =    570_000
        // net   = 18_430_000
        assert_eq!(calculate_payout(10_000_000, 1, 300), Some(18_430_000));
    }

    #[test]
    fn test_calculate_payout_streak_4_plus() {
        // wager=1_000_000, streak=4 (10x), fee=500bps (5%)
        // gross = 10_000_000, fee = 500_000, net = 9_500_000
        assert_eq!(calculate_payout(1_000_000, 4, 500), Some(9_500_000));
    }

    #[test]
    fn test_calculate_payout_overflow_returns_none() {
        assert_eq!(calculate_payout(i128::MAX, 1, 300), None);
    }

    #[test]
    fn test_calculate_payout_zero_wager() {
        assert_eq!(calculate_payout(0, 1, 300), Some(0));
    }

    #[test]
    fn test_error_codes_defined() {
        // Verify all error codes are unique and properly defined
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

    #[test]
    fn test_side_enum_values() {
        assert_eq!(Side::Heads as u32, 0);
        assert_eq!(Side::Tails as u32, 1);
    }

    #[test]
    fn test_game_phase_variants() {
        let committed = GamePhase::Committed;
        let revealed = GamePhase::Revealed;
        let completed = GamePhase::Completed;
        
        assert_ne!(committed, revealed);
        assert_ne!(revealed, completed);
        assert_ne!(committed, completed);
    }

    #[test]
    fn test_initialize_contract() {
        let env = Env::default();
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(&env, &contract_id);
        
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        
        client.initialize(&admin, &treasury, &300, &1_000_000, &100_000_000);
        
        // Verify config was stored
        let stored_config: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });
        
        assert_eq!(stored_config.fee_bps, 300);
        assert_eq!(stored_config.min_wager, 1_000_000);
        assert_eq!(stored_config.max_wager, 100_000_000);
    }

    #[test]
    fn test_initialize_invalid_fee() {
        let env = Env::default();
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(&env, &contract_id);
        
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        
        // Fee too low
        let result = client.try_initialize(&admin, &treasury, &100, &1_000_000, &100_000_000);
        assert_eq!(result, Err(Ok(Error::InvalidFeePercentage)));
        
        // Fee too high
        let result = client.try_initialize(&admin, &treasury, &600, &1_000_000, &100_000_000);
        assert_eq!(result, Err(Ok(Error::InvalidFeePercentage)));
    }

    #[test]
    fn test_initialize_invalid_wager_limits() {
        let env = Env::default();
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(&env, &contract_id);
        
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        
        // Min >= Max
        let result = client.try_initialize(&admin, &treasury, &300, &100_000_000, &1_000_000);
        assert_eq!(result, Err(Ok(Error::InvalidWagerLimits)));
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;
    use soroban_sdk::testutils::Address as _;

    // Feature: soroban-coinflip-game, Property: payout correctness
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Net payout is always strictly less than gross (fee is always deducted).
        #[test]
        fn test_payout_net_less_than_gross(
            wager   in 1i128..100_000_000i128,
            streak  in 1u32..=10u32,
            fee_bps in 200u32..=500u32,
        ) {
            let net   = calculate_payout(wager, streak, fee_bps).unwrap();
            let gross = wager.checked_mul(get_multiplier(streak) as i128).unwrap() / 10_000;
            prop_assert!(net < gross);
        }

        /// Net payout is always positive for any valid wager.
        #[test]
        fn test_payout_always_positive(
            wager   in 1i128..100_000_000i128,
            streak  in 1u32..=10u32,
            fee_bps in 200u32..=500u32,
        ) {
            prop_assert!(calculate_payout(wager, streak, fee_bps).unwrap() > 0);
        }

        /// Higher streak → higher net payout for the same wager and fee.
        #[test]
        fn test_payout_increases_with_streak(
            wager   in 1i128..100_000_000i128,
            streak  in 1u32..=3u32,
            fee_bps in 200u32..=500u32,
        ) {
            let lower  = calculate_payout(wager, streak,     fee_bps).unwrap();
            let higher = calculate_payout(wager, streak + 1, fee_bps).unwrap();
            prop_assert!(higher > lower);
        }

        /// Payout scales linearly with wager within integer-division rounding (≤ 1 stroop diff).
        #[test]
        fn test_payout_linear_in_wager(
            wager   in 1i128..50_000_000i128,
            streak  in 1u32..=10u32,
            fee_bps in 200u32..=500u32,
        ) {
            let single = calculate_payout(wager,     streak, fee_bps).unwrap();
            let double = calculate_payout(wager * 2, streak, fee_bps).unwrap();
            // Integer division can cause a ±1 stroop rounding difference
            prop_assert!((double - single * 2).abs() <= 1);
        }
    }

    // Feature: soroban-coinflip-game, Property: multiplier monotonicity
    // Validates: streak multipliers are strictly increasing from streak 1 → 2 → 3 → 4+
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn test_multiplier_monotonically_increasing(streak in 1u32..=3u32) {
            prop_assert!(get_multiplier(streak) < get_multiplier(streak + 1));
        }

        #[test]
        fn test_multiplier_streak_4_plus_is_constant(streak in 4u32..=100u32) {
            prop_assert_eq!(get_multiplier(streak), 100_000u32);
        }

        #[test]
        fn test_multiplier_always_greater_than_1x(streak in 1u32..=100u32) {
            // Every valid streak must yield a multiplier above 1x (10_000 bps)
            prop_assert!(get_multiplier(streak) > 10_000);
        }

        /// Invariant: multiplier never exceeds the 10x cap (100_000 bps) for any input.
        #[test]
        fn test_multiplier_never_exceeds_cap(streak in 0u32..=u32::MAX) {
            prop_assert!(get_multiplier(streak) <= 100_000);
        }

        /// Invariant: streaks 1–3 each map to their exact documented constant.
        /// Catches any accidental reordering or off-by-one in the match arms.
        #[test]
        fn test_multiplier_exact_values_streaks_1_to_3(streak in 1u32..=3u32) {
            let expected = match streak {
                1 => 19_000,
                2 => 35_000,
                3 => 60_000,
                _ => unreachable!(),
            };
            prop_assert_eq!(get_multiplier(streak), expected);
        }

        /// Invariant: the cap boundary is exactly at streak 4 — streak 3 must be
        /// strictly below the cap and streak 4 must equal it.
        #[test]
        fn test_multiplier_cap_boundary(streak in 4u32..=1_000u32) {
            prop_assert!(get_multiplier(3) < get_multiplier(streak));
            prop_assert_eq!(get_multiplier(streak), get_multiplier(4));
        }
    }

    // Feature: soroban-coinflip-game, Property: distinct addresses always accepted
    // Validates: admin != treasury is the only address constraint
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        #[test]
        fn test_distinct_addresses_always_accepted(
            fee_bps in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
            max_wager in 10_000_001i128..1_000_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            // Two independently generated addresses are always distinct
            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);

            let result = client.try_initialize(&admin, &treasury, &fee_bps, &min_wager, &max_wager);
            prop_assert!(result.is_ok());
        }
    }

    // Feature: soroban-coinflip-game, Property 24: State retrieval accuracy
    // Validates: Requirements 8.1, 8.2, 11.4
    //
    // Storage defaults assumed by these tests:
    //   - ContractConfig.paused  → false  (contract starts unpaused)
    //   - ContractStats.*        → 0      (all counters start at zero)
    //   - StorageKey::PlayerGame → None   (no game exists until one is started)
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Round-trip: every field written to Config storage during initialize()
        /// is read back unchanged, including admin and treasury addresses.
        #[test]
        fn test_config_full_round_trip(
            fee_bps   in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
            max_wager in 10_000_001i128..1_000_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let admin    = Address::generate(&env);
            let treasury = Address::generate(&env);

            client.initialize(&admin, &treasury, &fee_bps, &min_wager, &max_wager);

            let stored: ContractConfig = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Config).unwrap()
            });

            prop_assert_eq!(stored.admin,     admin);
            prop_assert_eq!(stored.treasury,  treasury);
            prop_assert_eq!(stored.fee_bps,   fee_bps);
            prop_assert_eq!(stored.min_wager, min_wager);
            prop_assert_eq!(stored.max_wager, max_wager);
            // Storage default: contract starts unpaused
            prop_assert_eq!(stored.paused, false);
        }

        /// Round-trip: a mutated ContractConfig written directly to storage is
        /// read back with every field intact (covers save_config / load_config).
        #[test]
        fn test_config_mutation_round_trip(
            fee_bps   in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
            max_wager in 10_000_001i128..1_000_000_000i128,
            paused    in proptest::bool::ANY,
        ) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let admin    = Address::generate(&env);
            let treasury = Address::generate(&env);

            // Initialise with valid params, then overwrite config with arbitrary paused flag
            client.initialize(&admin, &treasury, &fee_bps, &min_wager, &max_wager);

            let mutated = ContractConfig {
                admin:    admin.clone(),
                treasury: treasury.clone(),
                fee_bps,
                min_wager,
                max_wager,
                paused,
            };

            env.as_contract(&contract_id, || {
                env.storage().persistent().set(&StorageKey::Config, &mutated);
            });

            let stored: ContractConfig = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Config).unwrap()
            });

            prop_assert_eq!(stored, mutated);
        }

        /// Round-trip: every field written to Stats storage during initialize()
        /// is read back as zero (storage default for all counters).
        #[test]
        fn test_stats_zero_default_round_trip(
            fee_bps   in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
            max_wager in 10_000_001i128..1_000_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let admin    = Address::generate(&env);
            let treasury = Address::generate(&env);

            client.initialize(&admin, &treasury, &fee_bps, &min_wager, &max_wager);

            let stored: ContractStats = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Stats).unwrap()
            });

            // Storage defaults: all counters start at zero
            prop_assert_eq!(stored.total_games,    0u64);
            prop_assert_eq!(stored.total_volume,   0i128);
            prop_assert_eq!(stored.total_fees,     0i128);
            prop_assert_eq!(stored.reserve_balance, 0i128);
        }

        /// Round-trip: arbitrary ContractStats written to storage are read back
        /// with every field intact (covers save_stats / load_stats).
        #[test]
        fn test_stats_mutation_round_trip(
            total_games     in 0u64..u64::MAX,
            total_volume    in 0i128..i128::MAX,
            total_fees      in 0i128..i128::MAX,
            reserve_balance in 0i128..i128::MAX,
        ) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let admin    = Address::generate(&env);
            let treasury = Address::generate(&env);

            client.initialize(&admin, &treasury, &300, &1_000_000, &100_000_000);

            let mutated = ContractStats { total_games, total_volume, total_fees, reserve_balance };

            env.as_contract(&contract_id, || {
                env.storage().persistent().set(&StorageKey::Stats, &mutated);
            });

            let stored: ContractStats = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Stats).unwrap()
            });

            prop_assert_eq!(stored, mutated);
        }
    }
}

#[cfg(test)]
mod player_game_storage_tests {
    use super::*;
    use proptest::prelude::*;
    use soroban_sdk::{testutils::Address as _, Bytes};

    /// Build a deterministic 32-byte value from a seed byte for use in tests.
    fn bytes32(env: &Env, seed: u8) -> BytesN<32> {
        let mut b = [0u8; 32];
        b[0] = seed;
        BytesN::from_array(env, &b)
    }

    /// Construct a minimal GameState with the given phase and streak.
    fn make_game(env: &Env, wager: i128, side: Side, streak: u32, phase: GamePhase) -> GameState {
        GameState {
            wager,
            side,
            streak,
            commitment: bytes32(env, 0xAA),
            contract_random: bytes32(env, 0xBB),
            phase,
        }
    }

    // ── Unit tests ────────────────────────────────────────────────────────────

    /// Storage default: no game exists for a fresh player address.
    #[test]
    fn test_player_game_absent_by_default() {
        let env = Env::default();
        let contract_id = env.register(CoinflipContract, ());
        let player = Address::generate(&env);

        let result: Option<GameState> = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::PlayerGame(player.clone()))
        });
        assert!(result.is_none());
    }

    /// Save then load returns the identical GameState.
    #[test]
    fn test_player_game_save_and_load() {
        let env = Env::default();
        let contract_id = env.register(CoinflipContract, ());
        let player = Address::generate(&env);
        let game = make_game(&env, 5_000_000, Side::Heads, 1, GamePhase::Committed);

        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .set(&StorageKey::PlayerGame(player.clone()), &game);
        });

        let loaded: GameState = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&StorageKey::PlayerGame(player.clone()))
                .unwrap()
        });
        assert_eq!(loaded, game);
    }

    /// Overwriting a game replaces all fields.
    #[test]
    fn test_player_game_overwrite() {
        let env = Env::default();
        let contract_id = env.register(CoinflipContract, ());
        let player = Address::generate(&env);

        let first  = make_game(&env, 1_000_000, Side::Heads, 1, GamePhase::Committed);
        let second = make_game(&env, 9_000_000, Side::Tails, 3, GamePhase::Revealed);

        env.as_contract(&contract_id, || {
            env.storage().persistent().set(&StorageKey::PlayerGame(player.clone()), &first);
            env.storage().persistent().set(&StorageKey::PlayerGame(player.clone()), &second);
        });

        let loaded: GameState = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&StorageKey::PlayerGame(player.clone()))
                .unwrap()
        });
        assert_eq!(loaded, second);
    }

    /// Delete removes the entry; subsequent load returns None.
    #[test]
    fn test_player_game_delete() {
        let env = Env::default();
        let contract_id = env.register(CoinflipContract, ());
        let player = Address::generate(&env);
        let game = make_game(&env, 2_000_000, Side::Tails, 2, GamePhase::Completed);

        env.as_contract(&contract_id, || {
            env.storage().persistent().set(&StorageKey::PlayerGame(player.clone()), &game);
            env.storage().persistent().remove(&StorageKey::PlayerGame(player.clone()));
        });

        let result: Option<GameState> = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::PlayerGame(player.clone()))
        });
        assert!(result.is_none());
    }

    /// Two distinct players have independent storage slots.
    #[test]
    fn test_player_game_isolation() {
        let env = Env::default();
        let contract_id = env.register(CoinflipContract, ());
        let player_a = Address::generate(&env);
        let player_b = Address::generate(&env);

        let game_a = make_game(&env, 1_000_000, Side::Heads, 1, GamePhase::Committed);
        let game_b = make_game(&env, 8_000_000, Side::Tails, 4, GamePhase::Revealed);

        env.as_contract(&contract_id, || {
            env.storage().persistent().set(&StorageKey::PlayerGame(player_a.clone()), &game_a);
            env.storage().persistent().set(&StorageKey::PlayerGame(player_b.clone()), &game_b);
        });

        let loaded_a: GameState = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::PlayerGame(player_a.clone())).unwrap()
        });
        let loaded_b: GameState = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::PlayerGame(player_b.clone())).unwrap()
        });

        assert_eq!(loaded_a, game_a);
        assert_eq!(loaded_b, game_b);
    }

    // ── Property tests ────────────────────────────────────────────────────────
    //
    // Storage assumptions for PlayerGame:
    //   - Default: None  (key absent until explicitly written)
    //   - Keyed per player address; writes are isolated across players
    //   - All GameState fields (wager, side, streak, commitment,
    //     contract_random, phase) survive a save → load round-trip unchanged

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Every GameState field survives a save → load round-trip unchanged,
        /// across all valid combinations of side, streak, and phase.
        #[test]
        fn test_player_game_full_round_trip(
            wager        in 1_000_000i128..100_000_000i128,
            streak       in 0u32..=10u32,
            side_val     in 0u32..=1u32,
            phase_val    in 0u32..=2u32,
            commit_seed  in 0u8..=255u8,
            random_seed  in 0u8..=255u8,
        ) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());
            let player = Address::generate(&env);

            let side  = if side_val == 0 { Side::Heads } else { Side::Tails };
            let phase = match phase_val {
                0 => GamePhase::Committed,
                1 => GamePhase::Revealed,
                _ => GamePhase::Completed,
            };

            let game = GameState {
                wager,
                side,
                streak,
                commitment:      bytes32(&env, commit_seed),
                contract_random: bytes32(&env, random_seed),
                phase,
            };

            env.as_contract(&contract_id, || {
                env.storage().persistent().set(&StorageKey::PlayerGame(player.clone()), &game);
            });

            let loaded: GameState = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::PlayerGame(player.clone())).unwrap()
            });

            prop_assert_eq!(loaded.wager,           game.wager);
            prop_assert_eq!(loaded.side,            game.side);
            prop_assert_eq!(loaded.streak,          game.streak);
            prop_assert_eq!(loaded.commitment,      game.commitment);
            prop_assert_eq!(loaded.contract_random, game.contract_random);
            prop_assert_eq!(loaded.phase,           game.phase);
        }

        /// Deleting a player's game always leaves None in storage,
        /// regardless of what was stored.
        #[test]
        fn test_player_game_delete_always_clears(
            wager  in 1_000_000i128..100_000_000i128,
            streak in 0u32..=10u32,
        ) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());
            let player = Address::generate(&env);
            let game = make_game(&env, wager, Side::Heads, streak, GamePhase::Committed);

            env.as_contract(&contract_id, || {
                env.storage().persistent().set(&StorageKey::PlayerGame(player.clone()), &game);
                env.storage().persistent().remove(&StorageKey::PlayerGame(player.clone()));
            });

            let result: Option<GameState> = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::PlayerGame(player.clone()))
            });
            prop_assert!(result.is_none());
        }

        /// Two players with different wagers stored independently never bleed
        /// into each other's slot.
        #[test]
        fn test_player_game_slots_are_independent(
            wager_a in 1_000_000i128..50_000_000i128,
            wager_b in 50_000_001i128..100_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());
            let player_a = Address::generate(&env);
            let player_b = Address::generate(&env);

            let game_a = make_game(&env, wager_a, Side::Heads, 1, GamePhase::Committed);
            let game_b = make_game(&env, wager_b, Side::Tails, 2, GamePhase::Revealed);

            env.as_contract(&contract_id, || {
                env.storage().persistent().set(&StorageKey::PlayerGame(player_a.clone()), &game_a);
                env.storage().persistent().set(&StorageKey::PlayerGame(player_b.clone()), &game_b);
            });

            let loaded_a: GameState = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::PlayerGame(player_a.clone())).unwrap()
            });
            let loaded_b: GameState = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::PlayerGame(player_b.clone())).unwrap()
            });

            prop_assert_eq!(loaded_a.wager, wager_a);
            prop_assert_eq!(loaded_b.wager, wager_b);
        }
    }
}

/// # Player Game Delete and Cleanup Tests
///
/// Absence semantics assumed by these tests:
///   - After `remove(&StorageKey::PlayerGame(addr))`, `get(...)` returns `None`.
///   - Deleting a key that was never written is a no-op and does not panic.
///   - Deleting one player's key never affects any other player's key.
///   - A key can be re-written after deletion and will be readable again.
#[cfg(test)]
mod player_game_delete_tests {
    use super::*;
    use proptest::prelude::*;
    use soroban_sdk::testutils::Address as _;

    fn bytes32(env: &Env, seed: u8) -> BytesN<32> {
        let mut b = [0u8; 32];
        b[0] = seed;
        BytesN::from_array(env, &b)
    }

    fn make_game(env: &Env, wager: i128) -> GameState {
        GameState {
            wager,
            side: Side::Heads,
            streak: 1,
            commitment:      bytes32(env, 0xAA),
            contract_random: bytes32(env, 0xBB),
            phase: GamePhase::Committed,
        }
    }

    // ── Unit tests ────────────────────────────────────────────────────────────

    /// Deleting an existing game leaves None in storage.
    #[test]
    fn test_delete_existing_game_returns_none() {
        let env = Env::default();
        let contract_id = env.register(CoinflipContract, ());
        let player = Address::generate(&env);

        env.as_contract(&contract_id, || {
            env.storage().persistent().set(&StorageKey::PlayerGame(player.clone()), &make_game(&env, 5_000_000));
            env.storage().persistent().remove(&StorageKey::PlayerGame(player.clone()));
        });

        let result: Option<GameState> = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::PlayerGame(player.clone()))
        });
        assert!(result.is_none());
    }

    /// Deleting a key that was never written does not panic.
    #[test]
    fn test_delete_nonexistent_game_is_noop() {
        let env = Env::default();
        let contract_id = env.register(CoinflipContract, ());
        let player = Address::generate(&env);

        // Should not panic
        env.as_contract(&contract_id, || {
            env.storage().persistent().remove(&StorageKey::PlayerGame(player.clone()));
        });

        let result: Option<GameState> = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::PlayerGame(player.clone()))
        });
        assert!(result.is_none());
    }

    /// A game can be re-written after deletion and is readable again.
    #[test]
    fn test_rewrite_after_delete() {
        let env = Env::default();
        let contract_id = env.register(CoinflipContract, ());
        let player = Address::generate(&env);

        env.as_contract(&contract_id, || {
            env.storage().persistent().set(&StorageKey::PlayerGame(player.clone()), &make_game(&env, 1_000_000));
            env.storage().persistent().remove(&StorageKey::PlayerGame(player.clone()));
            env.storage().persistent().set(&StorageKey::PlayerGame(player.clone()), &make_game(&env, 9_000_000));
        });

        let loaded: GameState = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::PlayerGame(player.clone())).unwrap()
        });
        assert_eq!(loaded.wager, 9_000_000);
    }

    /// Deleting all players in sequence leaves all slots as None.
    #[test]
    fn test_cleanup_multiple_players() {
        let env = Env::default();
        let contract_id = env.register(CoinflipContract, ());
        let p1 = Address::generate(&env);
        let p2 = Address::generate(&env);
        let p3 = Address::generate(&env);

        env.as_contract(&contract_id, || {
            env.storage().persistent().set(&StorageKey::PlayerGame(p1.clone()), &make_game(&env, 1_000_000));
            env.storage().persistent().set(&StorageKey::PlayerGame(p2.clone()), &make_game(&env, 2_000_000));
            env.storage().persistent().set(&StorageKey::PlayerGame(p3.clone()), &make_game(&env, 3_000_000));
            env.storage().persistent().remove(&StorageKey::PlayerGame(p1.clone()));
            env.storage().persistent().remove(&StorageKey::PlayerGame(p2.clone()));
            env.storage().persistent().remove(&StorageKey::PlayerGame(p3.clone()));
        });

        for p in [&p1, &p2, &p3] {
            let result: Option<GameState> = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::PlayerGame(p.clone()))
            });
            assert!(result.is_none());
        }
    }

    /// Deleting one player does not affect a sibling player's slot.
    #[test]
    fn test_delete_one_does_not_affect_sibling() {
        let env = Env::default();
        let contract_id = env.register(CoinflipContract, ());
        let player_a = Address::generate(&env);
        let player_b = Address::generate(&env);

        env.as_contract(&contract_id, || {
            env.storage().persistent().set(&StorageKey::PlayerGame(player_a.clone()), &make_game(&env, 3_000_000));
            env.storage().persistent().set(&StorageKey::PlayerGame(player_b.clone()), &make_game(&env, 7_000_000));
            env.storage().persistent().remove(&StorageKey::PlayerGame(player_a.clone()));
        });

        let result_a: Option<GameState> = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::PlayerGame(player_a.clone()))
        });
        let result_b: GameState = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::PlayerGame(player_b.clone())).unwrap()
        });

        assert!(result_a.is_none());
        assert_eq!(result_b.wager, 7_000_000);
    }

    // ── Property tests ────────────────────────────────────────────────────────

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// For any wager, deleting after writing always yields None.
        #[test]
        fn prop_delete_always_yields_none(wager in 1_000_000i128..100_000_000i128) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());
            let player = Address::generate(&env);

            env.as_contract(&contract_id, || {
                env.storage().persistent().set(&StorageKey::PlayerGame(player.clone()), &make_game(&env, wager));
                env.storage().persistent().remove(&StorageKey::PlayerGame(player.clone()));
            });

            let result: Option<GameState> = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::PlayerGame(player.clone()))
            });
            prop_assert!(result.is_none());
        }

        /// Deleting player A never removes player B's game.
        #[test]
        fn prop_delete_does_not_affect_sibling(
            wager_a in 1_000_000i128..50_000_000i128,
            wager_b in 50_000_001i128..100_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());
            let player_a = Address::generate(&env);
            let player_b = Address::generate(&env);

            env.as_contract(&contract_id, || {
                env.storage().persistent().set(&StorageKey::PlayerGame(player_a.clone()), &make_game(&env, wager_a));
                env.storage().persistent().set(&StorageKey::PlayerGame(player_b.clone()), &make_game(&env, wager_b));
                env.storage().persistent().remove(&StorageKey::PlayerGame(player_a.clone()));
            });

            let result_b: Option<GameState> = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::PlayerGame(player_b.clone()))
            });
            prop_assert!(result_b.is_some());
            prop_assert_eq!(result_b.unwrap().wager, wager_b);
        }

        /// A game written after deletion is always readable with the new wager.
        #[test]
        fn prop_rewrite_after_delete_is_readable(
            wager_first  in 1_000_000i128..50_000_000i128,
            wager_second in 50_000_001i128..100_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());
            let player = Address::generate(&env);

            env.as_contract(&contract_id, || {
                env.storage().persistent().set(&StorageKey::PlayerGame(player.clone()), &make_game(&env, wager_first));
                env.storage().persistent().remove(&StorageKey::PlayerGame(player.clone()));
                env.storage().persistent().set(&StorageKey::PlayerGame(player.clone()), &make_game(&env, wager_second));
            });

            let loaded: GameState = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::PlayerGame(player.clone())).unwrap()
            });
            prop_assert_eq!(loaded.wager, wager_second);
        }
    }
}

/// # Initialization Guard Tests
///
/// Security invariant: `initialize()` must succeed exactly once per contract
/// instance. Every subsequent call must return `Error::AlreadyInitialized`
/// and must not mutate any stored state.
///
/// Assumptions:
///   - The guard key is `StorageKey::Config`; its presence signals init is done.
///   - No caller — including the original admin — can bypass the guard.
///   - Invalid params on a retry still return `AlreadyInitialized`, not a
///     param-validation error, because the guard fires first.
#[cfg(test)]
mod init_guard_tests {
    use super::*;
    use proptest::prelude::*;
    use soroban_sdk::testutils::Address as _;

    fn setup(env: &Env) -> (soroban_sdk::Address, CoinflipContractClient) {
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(env, &contract_id);
        (contract_id, client)
    }

    // ── Unit tests ────────────────────────────────────────────────────────────

    /// Second call with identical valid params returns AlreadyInitialized.
    #[test]
    fn test_second_call_same_params_rejected() {
        let env = Env::default();
        let (_, client) = setup(&env);
        let admin    = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.initialize(&admin, &treasury, &300, &1_000_000, &100_000_000);
        let result = client.try_initialize(&admin, &treasury, &300, &1_000_000, &100_000_000);
        assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
    }

    /// Second call with different valid params still returns AlreadyInitialized.
    #[test]
    fn test_second_call_different_params_rejected() {
        let env = Env::default();
        let (_, client) = setup(&env);
        let admin    = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.initialize(&admin, &treasury, &300, &1_000_000, &100_000_000);

        let admin2    = Address::generate(&env);
        let treasury2 = Address::generate(&env);
        let result = client.try_initialize(&admin2, &treasury2, &400, &2_000_000, &50_000_000);
        assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
    }

    /// Second call with invalid fee still returns AlreadyInitialized (guard fires first).
    #[test]
    fn test_second_call_invalid_fee_still_already_initialized() {
        let env = Env::default();
        let (_, client) = setup(&env);
        let admin    = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.initialize(&admin, &treasury, &300, &1_000_000, &100_000_000);
        let result = client.try_initialize(&admin, &treasury, &999, &1_000_000, &100_000_000);
        assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
    }

    /// Second call with admin == treasury still returns AlreadyInitialized.
    #[test]
    fn test_second_call_same_admin_treasury_still_already_initialized() {
        let env = Env::default();
        let (_, client) = setup(&env);
        let admin    = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.initialize(&admin, &treasury, &300, &1_000_000, &100_000_000);
        let result = client.try_initialize(&admin, &admin, &300, &1_000_000, &100_000_000);
        assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
    }

    /// Config stored after first init is unchanged after a rejected second call.
    #[test]
    fn test_config_unchanged_after_rejected_reinit() {
        let env = Env::default();
        let (contract_id, client) = setup(&env);
        let admin    = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.initialize(&admin, &treasury, &300, &1_000_000, &100_000_000);
        let _ = client.try_initialize(&Address::generate(&env), &Address::generate(&env), &400, &2_000_000, &50_000_000);

        let cfg: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });
        assert_eq!(cfg.fee_bps,   300);
        assert_eq!(cfg.min_wager, 1_000_000);
        assert_eq!(cfg.max_wager, 100_000_000);
        assert_eq!(cfg.admin,     admin);
        assert_eq!(cfg.treasury,  treasury);
    }

    // ── Property tests ────────────────────────────────────────────────────────

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Any number of retries after a successful init always return AlreadyInitialized.
        #[test]
        fn prop_reinit_always_rejected(
            fee_bps   in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
            max_wager in 10_000_001i128..100_000_000i128,
        ) {
            let env = Env::default();
            let (_, client) = setup(&env);
            let admin    = Address::generate(&env);
            let treasury = Address::generate(&env);

            client.initialize(&admin, &treasury, &fee_bps, &min_wager, &max_wager);

            // Retry with fresh addresses and different params
            let result = client.try_initialize(
                &Address::generate(&env),
                &Address::generate(&env),
                &fee_bps,
                &min_wager,
                &max_wager,
            );
            prop_assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
        }

        /// Config written on first init is never overwritten by a rejected retry.
        #[test]
        fn prop_config_immutable_after_init(
            fee_bps   in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
            max_wager in 10_000_001i128..100_000_000i128,
        ) {
            let env = Env::default();
            let (contract_id, client) = setup(&env);
            let admin    = Address::generate(&env);
            let treasury = Address::generate(&env);

            client.initialize(&admin, &treasury, &fee_bps, &min_wager, &max_wager);
            let _ = client.try_initialize(
                &Address::generate(&env),
                &Address::generate(&env),
                &(if fee_bps == 500 { 200 } else { fee_bps + 1 }),
                &min_wager,
                &max_wager,
            );

            let cfg: ContractConfig = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Config).unwrap()
            });
            prop_assert_eq!(cfg.fee_bps,   fee_bps);
            prop_assert_eq!(cfg.min_wager, min_wager);
            prop_assert_eq!(cfg.max_wager, max_wager);
            prop_assert_eq!(cfg.admin,     admin);
            prop_assert_eq!(cfg.treasury,  treasury);
        }
    }
}
