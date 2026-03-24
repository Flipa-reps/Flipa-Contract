#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, contracterror, token, Address, BytesN, Env};

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

/// Per-player game state persisted in `Committed` phase at game start.
///
/// Field meanings:
/// - `wager`          – original bet in stroops; locked for the duration of the game
/// - `side`           – player's chosen outcome (`Heads` or `Tails`)
/// - `streak`         – consecutive wins so far; starts at 0, incremented on each win
///                      (determines the multiplier tier on reveal)
/// - `commitment`     – SHA-256 hash of the player's secret random value;
///                      submitted up-front so the player cannot change their
///                      random input after seeing the contract's contribution
/// - `contract_random`– SHA-256 of the ledger sequence at game-start time;
///                      combined with the player's revealed secret to produce
///                      the final, unpredictable outcome
/// - `phase`          – lifecycle position: `Committed` → `Revealed` → `Completed`
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameState {
    pub wager: i128,
    pub side: Side,
    pub streak: u32,
    pub commitment: BytesN<32>,
    pub contract_random: BytesN<32>,
    pub phase: GamePhase,
}

/// Contract configuration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractConfig {
    pub admin: Address,           // Administrator address
    pub treasury: Address,        // Fee collection address
    pub token: Address,           // SAC token address for wager custody (XLM or any SEP-41 token)
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
        token: Address,
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
            token,
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

    /// Begin a new coinflip game for `player`.
    ///
    /// Validation guards (in order):
    /// 1. `ContractPaused`        – rejected when the contract is paused
    /// 2. `WagerBelowMinimum`     – rejected when `wager < config.min_wager`
    /// 3. `WagerAboveMaximum`     – rejected when `wager > config.max_wager`
    /// 4. `ActiveGameExists`      – rejected when the player already has an
    ///                              in-progress game (phase != Completed)
    /// 5. `InsufficientReserves`  – rejected when the contract cannot cover the
    ///                              maximum possible payout at the highest streak
    ///
    /// On success the game is stored in `Committed` phase and the player's
    /// commitment hash is recorded for the subsequent reveal step.
    pub fn start_game(
        env: Env,
        player: Address,
        side: Side,
        wager: i128,
        commitment: BytesN<32>,
    ) -> Result<(), Error> {
        player.require_auth();

        let config = Self::load_config(&env);

        // Guard 1: contract must not be paused
        if config.paused {
            return Err(Error::ContractPaused);
        }

        // Guard 2 & 3: wager must be within configured bounds
        if wager < config.min_wager {
            return Err(Error::WagerBelowMinimum);
        }
        if wager > config.max_wager {
            return Err(Error::WagerAboveMaximum);
        }

        // Guard 4: player must not have an active game
        if let Some(existing) = Self::load_player_game(&env, &player) {
            if existing.phase != GamePhase::Completed {
                return Err(Error::ActiveGameExists);
            }
        }

        // Guard 5: reserves must cover the worst-case payout.
        //
        // Formula:
        //   max_payout = wager × MULTIPLIER_STREAK_4_PLUS / 10_000
        //              = wager × 100_000 / 10_000
        //              = wager × 10
        //
        // We use the gross (pre-fee) 10x figure so the check is conservative —
        // the contract always holds enough to pay out even before the fee is
        // deducted from the winner's share.  Overflow in the multiplication
        // is treated as insolvent (wager is unreasonably large).
        let stats = Self::load_stats(&env);
        let max_payout = wager
            .checked_mul(MULTIPLIER_STREAK_4_PLUS as i128)
            .and_then(|v| v.checked_div(10_000))
            .ok_or(Error::InsufficientReserves)?;
        if stats.reserve_balance < max_payout {
            return Err(Error::InsufficientReserves);
        }

        // Generate contract-side randomness contribution from ledger sequence
        let seq_bytes = env.ledger().sequence().to_be_bytes();
        let contract_random = env.crypto().sha256(
            &soroban_sdk::Bytes::from_slice(&env, &seq_bytes),
        );

        // Transfer wager into contract custody.
        //
        // Custody assumptions:
        // - The wager moves from `player` to this contract via the SEP-41 token
        //   interface (SAC for native XLM, or any compatible token).
        // - `player.require_auth()` at the top of this function authorises the
        //   transfer; no separate approval step is needed on Soroban.
        // - Funds remain locked in the contract until the game resolves:
        //   a win pays out (wager × multiplier − fee), a loss forfeits the wager,
        //   and a timeout allows the player to reclaim via a future recover call.
        // - `reserve_balance` is updated here so the solvency check on the *next*
        //   game start reflects the newly locked funds.
        token::Client::new(&env, &config.token)
            .transfer(&player, &env.current_contract_address(), &wager);

        let mut stats = Self::load_stats(&env);
        stats.reserve_balance = stats.reserve_balance.checked_add(wager)
            .ok_or(Error::TransferFailed)?;
        Self::save_stats(&env, &stats);

        let game = GameState {
            wager,
            side,
            streak: 0,
            commitment,
            contract_random,
            phase: GamePhase::Committed,
        };

        Self::save_player_game(&env, &player, &game);

        Ok(())
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
        let token = Address::generate(&env);
        let result = client.try_initialize(&addr, &addr, &token, &300, &1_000_000, &100_000_000);
        assert_eq!(result, Err(Ok(Error::AdminTreasuryConflict)));
    }

    #[test]
    fn test_initialize_rejects_reinitialization() {
        let env = Env::default();
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let token = Address::generate(&env);

        client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);

        // Second call must fail
        let result = client.try_initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);
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
        let token = Address::generate(&env);
        
        client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);
        
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
        let token = Address::generate(&env);
        
        // Fee too low
        let result = client.try_initialize(&admin, &treasury, &token, &100, &1_000_000, &100_000_000);
        assert_eq!(result, Err(Ok(Error::InvalidFeePercentage)));
        
        // Fee too high
        let result = client.try_initialize(&admin, &treasury, &token, &600, &1_000_000, &100_000_000);
        assert_eq!(result, Err(Ok(Error::InvalidFeePercentage)));
    }

    #[test]
    fn test_initialize_invalid_wager_limits() {
        let env = Env::default();
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(&env, &contract_id);
        
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let token = Address::generate(&env);
        
        // Min >= Max
        let result = client.try_initialize(&admin, &treasury, &token, &300, &100_000_000, &1_000_000);
        assert_eq!(result, Err(Ok(Error::InvalidWagerLimits)));
    }

    // ── start_game validation ────────────────────────────────────────────────

    fn setup(env: &Env) -> (Address, Address, CoinflipContractClient) {
        let token_admin = Address::generate(env);
        let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_id = sac.address();

        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let treasury = Address::generate(env);
        client.initialize(&admin, &treasury, &token_id, &300, &1_000_000, &100_000_000);
        (contract_id, token_id, client)
    }

    /// Mint `amount` tokens to `to`.
    fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
        soroban_sdk::token::StellarAssetClient::new(env, token_id)
            .mock_all_auths()
            .mint(to, &amount);
    }

    fn dummy_commitment(env: &Env) -> BytesN<32> {
        env.crypto().sha256(&soroban_sdk::Bytes::from_slice(env, &[1u8; 32]))
    }

    /// Fund reserves directly so validation-only tests (no real transfer) pass the solvency check.
    fn fund_reserves(env: &Env, contract_id: &Address, amount: i128) {
        env.as_contract(contract_id, || {
            let mut stats = CoinflipContract::load_stats(env);
            stats.reserve_balance = amount;
            CoinflipContract::save_stats(env, &stats);
        });
    }

    #[test]
    fn test_start_game_rejects_when_paused() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _token_id, client) = setup(&env);

        // Pause the contract
        env.as_contract(&contract_id, || {
            let mut cfg = CoinflipContract::load_config(&env);
            cfg.paused = true;
            CoinflipContract::save_config(&env, &cfg);
        });

        let player = Address::generate(&env);
        let result = client.try_start_game(
            &player,
            &Side::Heads,
            &10_000_000,
            &dummy_commitment(&env),
        );
        assert_eq!(result, Err(Ok(Error::ContractPaused)));
    }

    #[test]
    fn test_start_game_rejects_wager_below_minimum() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _token_id, client) = setup(&env);
        fund_reserves(&env, &contract_id, 1_000_000_000);

        let player = Address::generate(&env);
        let result = client.try_start_game(
            &player,
            &Side::Heads,
            &500_000, // below min_wager of 1_000_000
            &dummy_commitment(&env),
        );
        assert_eq!(result, Err(Ok(Error::WagerBelowMinimum)));
    }

    #[test]
    fn test_start_game_rejects_wager_above_maximum() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _token_id, client) = setup(&env);
        fund_reserves(&env, &contract_id, 1_000_000_000);

        let player = Address::generate(&env);
        let result = client.try_start_game(
            &player,
            &Side::Heads,
            &200_000_000, // above max_wager of 100_000_000
            &dummy_commitment(&env),
        );
        assert_eq!(result, Err(Ok(Error::WagerAboveMaximum)));
    }

    #[test]
    fn test_start_game_rejects_active_game() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, client) = setup(&env);
        fund_reserves(&env, &contract_id, 1_000_000_000);

        let player = Address::generate(&env);
        mint(&env, &token_id, &player, 100_000_000);
        // First game succeeds
        client.start_game(&player, &Side::Heads, &10_000_000, &dummy_commitment(&env));
        // Second game must be rejected (transfer never reached, so no extra mint needed)
        let result = client.try_start_game(
            &player,
            &Side::Tails,
            &10_000_000,
            &dummy_commitment(&env),
        );
        assert_eq!(result, Err(Ok(Error::ActiveGameExists)));
    }

    #[test]
    fn test_start_game_rejects_insufficient_reserves() {
        let env = Env::default();
        env.mock_all_auths();
        let (_contract_id, _token_id, client) = setup(&env);
        // Leave reserves at 0 (default after initialize)

        let player = Address::generate(&env);
        let result = client.try_start_game(
            &player,
            &Side::Heads,
            &10_000_000,
            &dummy_commitment(&env),
        );
        assert_eq!(result, Err(Ok(Error::InsufficientReserves)));
    }

    /// Reserves equal to exactly max_payout (wager × 10) must be accepted.
    #[test]
    fn test_reserve_solvency_exact_boundary_accepted() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, client) = setup(&env);
        let wager = 10_000_000i128;
        fund_reserves(&env, &contract_id, wager * 10);
        let player = Address::generate(&env);
        mint(&env, &token_id, &player, wager);
        assert!(client.try_start_game(&player, &Side::Heads, &wager, &dummy_commitment(&env)).is_ok());
    }

    /// Reserves one stroop below max_payout must be rejected.
    #[test]
    fn test_reserve_solvency_one_below_boundary_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _token_id, client) = setup(&env);
        let wager = 10_000_000i128;
        fund_reserves(&env, &contract_id, wager * 10 - 1);

        let player = Address::generate(&env);
        assert_eq!(
            client.try_start_game(&player, &Side::Heads, &wager, &dummy_commitment(&env)),
            Err(Ok(Error::InsufficientReserves))
        );
    }

    /// Reserves above max_payout must be accepted.
    #[test]
    fn test_reserve_solvency_above_boundary_accepted() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, client) = setup(&env);
        let wager = 10_000_000i128;
        fund_reserves(&env, &contract_id, wager * 10 + 1);
        let player = Address::generate(&env);
        mint(&env, &token_id, &player, wager);
        assert!(client.try_start_game(&player, &Side::Heads, &wager, &dummy_commitment(&env)).is_ok());
    }

    #[test]
    fn test_start_game_succeeds_with_valid_inputs() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, client) = setup(&env);
        fund_reserves(&env, &contract_id, 1_000_000_000);
        let player = Address::generate(&env);
        mint(&env, &token_id, &player, 100_000_000);

        let result = client.try_start_game(
            &player,
            &Side::Heads,
            &10_000_000,
            &dummy_commitment(&env),
        );
        assert!(result.is_ok());

        let game: GameState = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player).unwrap()
        });
        assert_eq!(game.wager, 10_000_000);
        assert_eq!(game.side, Side::Heads);
        assert_eq!(game.phase, GamePhase::Committed);
        assert_eq!(game.streak, 0);
    }

    /// Verifies every field of the persisted GameState after a successful start_game call.
    #[test]
    fn test_start_game_state_all_fields_persisted() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, client) = setup(&env);
        fund_reserves(&env, &contract_id, 1_000_000_000);
        let player = Address::generate(&env);
        mint(&env, &token_id, &player, 100_000_000);
        let commitment = dummy_commitment(&env);

        client.start_game(&player, &Side::Tails, &5_000_000, &commitment);

        let game: GameState = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player).unwrap()
        });

        assert_eq!(game.wager, 5_000_000);
        assert_eq!(game.side, Side::Tails);
        assert_eq!(game.streak, 0);
        assert_eq!(game.commitment, commitment);
        assert_ne!(game.contract_random, BytesN::from_array(&env, &[0u8; 32]));
        assert_eq!(game.phase, GamePhase::Committed);
    }

    /// Two players starting games independently get isolated state.
    #[test]
    fn test_start_game_state_isolated_per_player() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, client) = setup(&env);
        fund_reserves(&env, &contract_id, 1_000_000_000);

        let p1 = Address::generate(&env);
        let p2 = Address::generate(&env);
        mint(&env, &token_id, &p1, 10_000_000);
        mint(&env, &token_id, &p2, 20_000_000);

        client.start_game(&p1, &Side::Heads, &1_000_000, &dummy_commitment(&env));
        client.start_game(&p2, &Side::Tails, &2_000_000, &dummy_commitment(&env));

        let (g1, g2) = env.as_contract(&contract_id, || {(
            CoinflipContract::load_player_game(&env, &p1).unwrap(),
            CoinflipContract::load_player_game(&env, &p2).unwrap(),
        )});

        assert_eq!(g1.wager, 1_000_000);
        assert_eq!(g1.side, Side::Heads);
        assert_eq!(g2.wager, 2_000_000);
        assert_eq!(g2.side, Side::Tails);
    }

    /// Wager is transferred from player to contract on game start.
    #[test]
    fn test_wager_transferred_to_contract_on_start() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, client) = setup(&env);
        fund_reserves(&env, &contract_id, 1_000_000_000);

        let player = Address::generate(&env);
        let wager = 10_000_000i128;
        mint(&env, &token_id, &player, wager);

        let token_client = soroban_sdk::token::Client::new(&env, &token_id);
        assert_eq!(token_client.balance(&player), wager);
        assert_eq!(token_client.balance(&contract_id), 0);

        client.start_game(&player, &Side::Heads, &wager, &dummy_commitment(&env));

        // Wager moved from player to contract
        assert_eq!(token_client.balance(&player), 0);
        assert_eq!(token_client.balance(&contract_id), wager);
    }

    /// reserve_balance is incremented by the wager amount after transfer.
    #[test]
    fn test_reserve_balance_updated_after_transfer() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, client) = setup(&env);
        fund_reserves(&env, &contract_id, 1_000_000_000);

        let player = Address::generate(&env);
        let wager = 10_000_000i128;
        mint(&env, &token_id, &player, wager);

        let before: ContractStats = env.as_contract(&contract_id, || {
            CoinflipContract::load_stats(&env)
        });

        client.start_game(&player, &Side::Heads, &wager, &dummy_commitment(&env));

        let after: ContractStats = env.as_contract(&contract_id, || {
            CoinflipContract::load_stats(&env)
        });

        assert_eq!(after.reserve_balance, before.reserve_balance + wager);
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

            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);
            let token = Address::generate(&env);

            let result = client.try_initialize(&admin, &treasury, &token, &fee_bps, &min_wager, &max_wager);
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
            let token = Address::generate(&env);
            
            client.initialize(&admin, &treasury, &token, &fee_bps, &min_wager, &max_wager);
            
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
            let token = Address::generate(&env);
            
            client.initialize(&admin, &treasury, &token, &fee_bps, &min_wager, &max_wager);
            
            let stored_stats: ContractStats = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Stats).unwrap()
            });
            
            prop_assert_eq!(stored_stats.total_games, 0);
            prop_assert_eq!(stored_stats.total_volume, 0);
            prop_assert_eq!(stored_stats.total_fees, 0);
            prop_assert_eq!(stored_stats.reserve_balance, 0);
        }
    }
}
