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

/// Verifies that a player's revealed preimage matches the stored commitment.
///
/// # Commitment Verification Invariants
///
/// 1. **Match succeeds**: `sha256(preimage) == commitment` → returns `Ok(())`
/// 2. **Mismatch fails**: any other preimage → returns `Err(Error::CommitmentMismatch)`
/// 3. **State is never mutated** by this function; callers are responsible for
///    acting on the result before writing any state changes.
/// 4. **Determinism**: the same `(preimage, commitment)` pair always produces
///    the same result across invocations.
pub fn verify_commitment(
    env: &Env,
    preimage: &BytesN<32>,
    commitment: &BytesN<32>,
) -> Result<(), Error> {
    let hash = env.crypto().sha256(&preimage.clone().into());
    let hash_bytes: BytesN<32> = hash.into();
    if hash_bytes == *commitment {
        Ok(())
    } else {
        Err(Error::CommitmentMismatch)
    }
}

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
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        
        #[test]
        fn test_config_storage_accuracy(
            fee_bps in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
            max_wager in 10_000_001i128..1_000_000_000i128
        ) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);
            
            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);
            
            client.initialize(&admin, &treasury, &fee_bps, &min_wager, &max_wager);
            
            // Verify storage by reading back through contract storage
            let stored_config: ContractConfig = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Config).unwrap()
            });
            
            prop_assert_eq!(stored_config.fee_bps, fee_bps);
            prop_assert_eq!(stored_config.min_wager, min_wager);
            prop_assert_eq!(stored_config.max_wager, max_wager);
            prop_assert_eq!(stored_config.paused, false);
        }

        #[test]
        fn test_stats_initialization(
            fee_bps in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
            max_wager in 10_000_001i128..1_000_000_000i128
        ) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);
            
            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);
            
            client.initialize(&admin, &treasury, &fee_bps, &min_wager, &max_wager);
            
            // Verify stats are initialized to zero
            let stored_stats: ContractStats = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Stats).unwrap()
            });
            
            prop_assert_eq!(stored_stats.total_games, 0);
            prop_assert_eq!(stored_stats.total_volume, 0);
            prop_assert_eq!(stored_stats.total_fees, 0);
            prop_assert_eq!(stored_stats.reserve_balance, 0);
        }
    }

    // Feature: soroban-coinflip-game, Property: commitment verification
    //
    // Invariants validated:
    //   A. A preimage whose sha256 equals the stored commitment always succeeds.
    //   B. A preimage that differs from the original always returns CommitmentMismatch.
    //   C. A mismatch never mutates GameState (state-stability invariant).
    //   D. Verification is deterministic: same inputs always produce the same result.
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Invariant A: matching reveal always succeeds.
        /// The commitment is built as sha256(preimage), so verify_commitment must
        /// return Ok(()) for the original preimage.
        #[test]
        fn test_commitment_match_succeeds(preimage in prop::array::uniform32(0u8..)) {
            let env = Env::default();
            let preimage_bytes: BytesN<32> = BytesN::from_array(&env, &preimage);
            let hash = env.crypto().sha256(&preimage_bytes.clone().into());
            let commitment: BytesN<32> = hash.into();

            prop_assert!(verify_commitment(&env, &preimage_bytes, &commitment).is_ok());
        }

        /// Invariant B: any differing preimage returns CommitmentMismatch.
        /// We flip the first byte to guarantee the preimage differs from the original.
        #[test]
        fn test_commitment_mismatch_fails(preimage in prop::array::uniform32(0u8..)) {
            let env = Env::default();
            let preimage_bytes: BytesN<32> = BytesN::from_array(&env, &preimage);
            let hash = env.crypto().sha256(&preimage_bytes.clone().into());
            let commitment: BytesN<32> = hash.into();

            // Construct a wrong preimage by flipping the first byte
            let mut wrong = preimage;
            wrong[0] = wrong[0].wrapping_add(1);
            let wrong_bytes: BytesN<32> = BytesN::from_array(&env, &wrong);

            prop_assert_eq!(
                verify_commitment(&env, &wrong_bytes, &commitment),
                Err(Error::CommitmentMismatch)
            );
        }

        /// Invariant C: a mismatch does not mutate GameState.
        /// We snapshot the GameState before calling verify_commitment with a wrong
        /// preimage and assert the snapshot is identical afterwards.
        #[test]
        fn test_commitment_mismatch_does_not_mutate_state(
            preimage in prop::array::uniform32(0u8..),
            wager    in 1_000_000i128..100_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());

            let preimage_bytes: BytesN<32> = BytesN::from_array(&env, &preimage);
            let hash = env.crypto().sha256(&preimage_bytes.clone().into());
            let commitment: BytesN<32> = hash.into();

            // Build a representative GameState and store it
            let player = Address::generate(&env);
            let contract_random = BytesN::from_array(&env, &[0u8; 32]);
            let game = GameState {
                wager,
                side: Side::Heads,
                streak: 0,
                commitment: commitment.clone(),
                contract_random: contract_random.clone(),
                phase: GamePhase::Committed,
            };
            env.as_contract(&contract_id, || {
                env.storage()
                    .persistent()
                    .set(&StorageKey::PlayerGame(player.clone()), &game);
            });

            // Attempt a mismatched reveal — must not change stored state
            let mut wrong = preimage;
            wrong[0] = wrong[0].wrapping_add(1);
            let wrong_bytes: BytesN<32> = BytesN::from_array(&env, &wrong);
            let _ = verify_commitment(&env, &wrong_bytes, &commitment);

            let stored: GameState = env.as_contract(&contract_id, || {
                env.storage()
                    .persistent()
                    .get(&StorageKey::PlayerGame(player.clone()))
                    .unwrap()
            });
            prop_assert_eq!(stored, game);
        }

        /// Invariant D: verification is deterministic — same inputs always agree.
        #[test]
        fn test_commitment_verification_is_deterministic(
            preimage in prop::array::uniform32(0u8..),
        ) {
            let env = Env::default();
            let preimage_bytes: BytesN<32> = BytesN::from_array(&env, &preimage);
            let hash = env.crypto().sha256(&preimage_bytes.clone().into());
            let commitment: BytesN<32> = hash.into();

            let r1 = verify_commitment(&env, &preimage_bytes, &commitment);
            let r2 = verify_commitment(&env, &preimage_bytes, &commitment);
            prop_assert_eq!(r1, r2);
        }
    }
}
