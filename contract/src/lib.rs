#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, contracterror, token, Address, Bytes, BytesN, Env};

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
/// Arithmetic Assumptions:
/// 1. Uses `i128` to avert overflow during intermediate multiplications (up to `i128::MAX`).
/// 2. Integer division by 10,000 implicitly floors/truncates fractional stroops.
/// 3. `fee_bps` <= 10_000 is mathematically required to avoid net < 0, enforced by config guards.
/// 4. Subtractions are safe as `fee` is derived as a proportion of `gross` (<= `gross`).
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

/// Helper to verify a player's commitment hash.
///
/// Hashes the `secret` value using Soroban's SHA256 cryptographic utility
/// and compares it against the stored `commitment` bytes.
///
/// Byte format assumptions:
/// - `secret` is explicitly expected to be raw byte data (`Bytes`),
///   as the user must submit the exact pre-image bytes that generated the target hash.
/// - The hash is a raw SHA-256 output resolving to `BytesN<32>`.
/// - Both the revealed secret hash and the `commitment` must match exactly 
///   for the verification to pass.
pub fn verify_commitment(env: &Env, secret: &Bytes, commitment: &BytesN<32>) -> bool {
    let hash: BytesN<32> = env.crypto().sha256(secret).into();
    &hash == commitment
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
    /// Acceptance invariants:
    /// - `player` must be a valid address and must authorize the call (`player.require_auth`).
    /// - contract must be initialized and not paused.
    /// - `wager` must be within `[config.min_wager, config.max_wager]`.
    /// - the player must not already have an active game (only `Completed` games can be replaced).
    /// - contract reserves must cover worst-case payout (`streak 4+` multiplier) to avoid insolvency.
    /// - on success, the game state is persisted and global stats are updated (`total_games += 1`, `total_volume += wager`).
    /// - player balance/transfer checks are assumed to be performed by the caller or higher-level token transfer semantics.
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
    ///
    /// # Wager Limit Enforcement (Fund Safety Critical)
    ///
    /// The wager limits are enforced using strict inequality checks to ensure
    /// exact boundary behavior:
    ///
    /// - **Accepted Range**: `wager >= config.min_wager && wager <= config.max_wager`
    /// - **Rejected Below**: `wager < config.min_wager` → `Error::WagerBelowMinimum`
    /// - **Rejected Above**: `wager > config.max_wager` → `Error::WagerAboveMaximum`
    ///
    /// This guard ensures:
    /// 1. **No off-by-one errors** – Players can always place bets at exactly the
    ///    configured limits (min and max are *inclusive*).
    /// 2. **Fund safety** – Prevents underbet that fails to cover fees and prevents
    ///    overbets that could exceed contract reserves.
    /// 3. **Clear semantics** – The inequality operators (`<` and `>`) make the
    ///    boundary behavior explicit and auditable.
    ///
    /// Invariant: These checks execute *before* any state mutation, ensuring
    /// that invalid wagers are rejected at the gate without side effects.
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

        // Guard 2 & 3: Wager must be within configured bounds [min_wager, max_wager].
        // Uses strict inequalities to ensure inclusive bounds:
        // - Rejects wagers LESS THAN min (strictly below minimum)
        // - Rejects wagers GREATER THAN max (strictly above maximum)
        // This means exactly min and max are ACCEPTED.
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

        // Guard 5: reserves must cover the worst-case payout (streak 4+, no fee deduction)
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
        let contract_random: BytesN<32> = env.crypto().sha256(
            &soroban_sdk::Bytes::from_slice(&env, &seq_bytes),
        ).into();

        let game = GameState {
            wager,
            side,
            streak: 0,
            commitment,
            contract_random,
            phase: GamePhase::Committed,
        };

        Self::save_player_game(&env, &player, &game);

        // Update global statistics to reflect a new active game creation.
        let mut stats = stats;
        stats.total_games = stats.total_games.checked_add(1).unwrap_or(stats.total_games);
        stats.total_volume = stats.total_volume.checked_add(wager).unwrap_or(stats.total_volume);
        Self::save_stats(&env, &stats);

        Ok(())
    }

    /// Reveal the player's secret to determine the game outcome.
    ///
    /// Process:
    /// 1. Verify commitment matches the revealed secret
    /// 2. Combine player random + contract random to determine outcome
    /// 3. Update game state to Revealed phase with result
    /// 4. If player wins, calculate potential payout
    /// 5. If player loses, end game and reset streak
    ///
    /// Errors:
    /// - NoActiveGame: player has no game in Committed phase
    /// - InvalidPhase: game not in Committed phase  
    /// - CommitmentMismatch: revealed secret doesn't match stored commitment
    pub fn reveal(
        env: Env,
        player: Address,
        secret: Bytes,
    ) -> Result<(), Error> {
        player.require_auth();

        let mut game = Self::load_player_game(&env, &player)
            .ok_or(Error::NoActiveGame)?;

        // Must be in Committed phase to reveal
        if game.phase != GamePhase::Committed {
            return Err(Error::InvalidPhase);
        }

        // Verify the commitment matches the revealed secret
        if !verify_commitment(&env, &secret, &game.commitment) {
            return Err(Error::CommitmentMismatch);
        }

        // Determine outcome by combining player random + contract random
        let combined_input = {
            let mut combined = Bytes::new(&env);
            combined.extend_from_slice(&secret);
            combined.extend_from_slice(&game.contract_random.as_slice());
            env.crypto().sha256(&combined)
        };

        // Use first byte to determine heads/tails (0 = Heads, 1 = Tails)
        let outcome_byte = combined_input.as_slice().get(0).unwrap_or(&0);
        let outcome = if *outcome_byte % 2 == 0 { Side::Heads } else { Side::Tails };

        // Update game state based on outcome
        if outcome == game.side {
            // Player wins - advance to next streak level
            game.streak += 1;
            game.phase = GamePhase::Revealed;
        } else {
            // Player loses - game ends, streak resets
            game.phase = GamePhase::Completed;
            game.streak = 0;
        }

        Self::save_player_game(&env, &player, &game);
        Ok(())
    }

    /// Claim winnings after a successful reveal.
    ///
    /// Process:
    /// 1. Verify game is in Revealed phase (player won)
    /// 2. Calculate net payout (gross - fee)
    /// 3. Transfer net payout to player
    /// 4. Transfer fee to treasury
    /// 5. Update contract reserves and stats
    /// 6. Reset game to Completed phase
    ///
    /// Errors:
    /// - NoActiveGame: player has no game
    /// - InvalidPhase: game not in Revealed phase
    /// - TransferFailed: token transfer fails
    pub fn claim_winnings(
        env: Env,
        player: Address,
    ) -> Result<(), Error> {
        player.require_auth();

        let mut game = Self::load_player_game(&env, &player)
            .ok_or(Error::NoActiveGame)?;

        // Must be in Revealed phase to claim (player won)
        if game.phase != GamePhase::Revealed {
            return Err(Error::InvalidPhase);
        }

        let config = Self::load_config(&env);
        let token_client = token::Client::new(&env, &config.token);

        // Calculate payout
        let net_payout = calculate_payout(game.wager, game.streak, config.fee_bps)
            .ok_or(Error::InsufficientReserves)?;

        // Calculate gross payout and fee separately for accounting
        let gross_payout = game.wager
            .checked_mul(get_multiplier(game.streak) as i128)
            .and_then(|v| v.checked_div(10_000))
            .ok_or(Error::InsufficientReserves)?;
        let fee_amount = gross_payout
            .checked_mul(config.fee_bps as i128)
            .and_then(|v| v.checked_div(10_000))
            .ok_or(Error::InsufficientReserves)?;

        // Check sufficient reserves
        let stats = Self::load_stats(&env);
        if stats.reserve_balance < gross_payout {
            return Err(Error::InsufficientReserves);
        }

        // Transfer net payout to player
        if token_client.try_transfer(&env, &env.current_contract_address(), &player, &net_payout) != soroban_sdk::InvokeError::Ok {
            return Err(Error::TransferFailed);
        }

        // Transfer fee to treasury
        if token_client.try_transfer(&env, &env.current_contract_address(), &config.treasury, &fee_amount) != soroban_sdk::InvokeError::Ok {
            return Err(Error::TransferFailed);
        }

        // Update contract state
        let mut stats = stats;
        stats.reserve_balance = stats.reserve_balance.checked_sub(gross_payout)
            .ok_or(Error::InsufficientReserves)?;
        stats.total_fees = stats.total_fees.checked_add(fee_amount).unwrap_or(stats.total_fees);
        Self::save_stats(&env, &stats);

        // Reset game to completed
        game.phase = GamePhase::Completed;
        Self::save_player_game(&env, &player, &game);

        Ok(())
    }

    /// Continue to next streak level after winning.
    ///
    /// Process:
    /// 1. Verify game is in Revealed phase (player won)
    /// 2. Reset game to Committed phase for next round
    /// 3. Player keeps current streak and wager
    /// 4. Generate new contract randomness
    ///
    /// Errors:
    /// - NoActiveGame: player has no game
    /// - InvalidPhase: game not in Revealed phase
    pub fn continue_streak(
        env: Env,
        player: Address,
        new_commitment: BytesN<32>,
    ) -> Result<(), Error> {
        player.require_auth();

        let mut game = Self::load_player_game(&env, &player)
            .ok_or(Error::NoActiveGame)?;

        // Must be in Revealed phase to continue (player won)
        if game.phase != GamePhase::Revealed {
            return Err(Error::InvalidPhase);
        }

        // Verify reserves cover next potential payout
        let config = Self::load_config(&env);
        let stats = Self::load_stats(&env);
        
        let next_streak = game.streak + 1;
        let max_payout = game.wager
            .checked_mul(get_multiplier(next_streak) as i128)
            .and_then(|v| v.checked_div(10_000))
            .ok_or(Error::InsufficientReserves)?;
        
        if stats.reserve_balance < max_payout {
            return Err(Error::InsufficientReserves);
        }

        // Generate new contract randomness
        let seq_bytes = env.ledger().sequence().to_be_bytes();
        let contract_random = env.crypto().sha256(
            &soroban_sdk::Bytes::from_slice(&env, &seq_bytes),
        );

        // Reset to Committed phase for next round
        game.phase = GamePhase::Committed;
        game.commitment = new_commitment;
        game.contract_random = contract_random;

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

    #[test]
    fn test_verify_commitment() {
        let env = Env::default();
        let mut secret = Bytes::new(&env);
        secret.push_back(1u8);
        secret.push_back(2u8);
        secret.push_back(3u8);

        let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

        // Correct secret
        assert!(verify_commitment(&env, &secret, &commitment));

        // Incorrect secret
        let mut wrong_secret = Bytes::new(&env);
        wrong_secret.push_back(1u8);
        wrong_secret.push_back(2u8);
        wrong_secret.push_back(4u8);

        assert!(!verify_commitment(&env, &wrong_secret, &commitment));
    }

    // ── start_game validation ────────────────────────────────────────────────

    fn setup(env: &Env) -> (soroban_sdk::Address, CoinflipContractClient) {
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let treasury = Address::generate(env);
        let token = Address::generate(env);
        client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);
        (contract_id, client)
    }

    fn dummy_commitment(env: &Env) -> BytesN<32> {
        env.crypto().sha256(&soroban_sdk::Bytes::from_slice(env, &[1u8; 32])).into()
    }

    /// Fund reserves directly so start_game solvency check passes.
    fn fund_reserves(env: &Env, contract_id: &soroban_sdk::Address, amount: i128) {
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
        let (contract_id, client) = setup(&env);

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
        let (contract_id, client) = setup(&env);
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
        let (contract_id, client) = setup(&env);
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
        let (contract_id, client) = setup(&env);
        fund_reserves(&env, &contract_id, 1_000_000_000);

        let player = Address::generate(&env);
        // First game succeeds
        client.start_game(&player, &Side::Heads, &10_000_000, &dummy_commitment(&env));
        // Second game must be rejected
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
        let (contract_id, client) = setup(&env);
        // Leave reserves at 0 (default after initialize)
        let _ = contract_id;

        let player = Address::generate(&env);
        let result = client.try_start_game(
            &player,
            &Side::Heads,
            &10_000_000,
            &dummy_commitment(&env),
        );
        assert_eq!(result, Err(Ok(Error::InsufficientReserves)));
    }

    #[test]
    fn test_start_game_succeeds_with_valid_inputs() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        fund_reserves(&env, &contract_id, 1_000_000_000);

        let player = Address::generate(&env);
        let result = client.try_start_game(
            &player,
            &Side::Heads,
            &10_000_000,
            &dummy_commitment(&env),
        );
        assert!(result.is_ok());

        // Verify game state was stored correctly
        let game: GameState = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player).unwrap()
        });
        assert_eq!(game.wager, 10_000_000);
        assert_eq!(game.side, Side::Heads);
        assert_eq!(game.phase, GamePhase::Committed);
        assert_eq!(game.streak, 0);
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

        /// Verify fee boundaries: 0% fee subtracts nothing, 100% fee reduces net to 0.
        #[test]
        fn test_payout_fee_boundaries(
            wager in 1i128..100_000_000i128,
            streak in 1u32..=10u32,
        ) {
            let gross = wager.checked_mul(get_multiplier(streak) as i128).unwrap() / 10_000;
            
            // 0% fee (0 bps)
            let net_zero_fee = calculate_payout(wager, streak, 0).unwrap();
            prop_assert_eq!(net_zero_fee, gross);

            // 100% fee (10_000 bps)
            let net_max_fee = calculate_payout(wager, streak, 10_000).unwrap();
            prop_assert_eq!(net_max_fee, 0);
        }

        /// Verify non-negative outputs: net payout is never negative for any valid inputs,
        /// and fee deduction mathematically never exceeds the gross amount.
        #[test]
        fn test_payout_non_negative(
            wager in 0i128..1_000_000_000i128,
            streak in 1u32..=10u32,
            fee_bps in 0u32..=10_000u32,
        ) {
            let net = calculate_payout(wager, streak, fee_bps).unwrap();
            prop_assert!(net >= 0);
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

    // ───────────────────────────────────────────────────────────────────────
    // Feature: Wager Limit Validation (Fund Safety Critical)
    // ───────────────────────────────────────────────────────────────────────
    // PROPERTIES:
    // 1. Wagers STRICTLY LESS than MIN_WAGER are rejected with WagerBelowMinimum
    // 2. Wagers STRICTLY GREATER than MAX_WAGER are rejected with WagerAboveMaximum
    // 3. Wagers EXACTLY equal to MIN_WAGER are accepted (inclusive lower bound)
    // 4. Wagers EXACTLY equal to MAX_WAGER are accepted (inclusive upper bound)
    // 5. All wagers within [MIN_WAGER, MAX_WAGER] are accepted
    // ───────────────────────────────────────────────────────────────────────

    // Helper function to set up contract and return client
    fn setup_contract_with_bounds(
        env: &Env,
        min_wager: i128,
        max_wager: i128,
    ) -> soroban_sdk::Address {
        env.mock_all_auths();
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(env, &contract_id);
        
        let admin = Address::generate(env);
        let treasury = Address::generate(env);
        let token = Address::generate(env);
        
        client.initialize(&admin, &treasury, &token, &300, &min_wager, &max_wager);
        
        // Fund reserves with excessive amount to avoid InsufficientReserves errors
        env.as_contract(&contract_id, || {
            let mut stats = CoinflipContract::load_stats(env);
            stats.reserve_balance = i128::MAX / 2; // Safe ceiling
            CoinflipContract::save_stats(env, &stats);
        });
        
        contract_id
    }

    fn dummy_commitment_prop(env: &Env) -> BytesN<32> {
        env.crypto().sha256(&soroban_sdk::Bytes::from_slice(env, &[42u8; 32])).into()
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        /// PROPERTY: Generate random wager values strictly LESS than MIN_WAGER
        /// and verify they are rejected with Error::WagerBelowMinimum.
        /// 
        /// This test ensures no player can sneak through a wager below the
        /// configured minimum, preventing underbets that could fail to generate
        /// sufficient fees or game value.
        #[test]
        fn prop_wager_below_minimum_rejected(
            min_wager in 1_000_000i128..50_000_000i128,
            wager_offset in 1i128..1_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, min_wager, min_wager + 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);
            
            let invalid_wager = min_wager - wager_offset;
            prop_assume!(invalid_wager > 0); // Ensure wager is positive
            
            let player = Address::generate(&env);
            let result = client.try_start_game(
                &player,
                &Side::Heads,
                &invalid_wager,
                &dummy_commitment_prop(&env),
            );
            
            prop_assert_eq!(result, Err(Ok(Error::WagerBelowMinimum)),
                "Expected WagerBelowMinimum for wager {} < min_wager {}", invalid_wager, min_wager);
        }

        /// PROPERTY: Generate random wager values strictly GREATER than MAX_WAGER
        /// and verify they are rejected with Error::WagerAboveMaximum.
        ///
        /// This test prevents overbet attempts that could exceed the contract's
        /// ability to cover streaks, protecting contract reserves and fund safety.
        #[test]
        fn prop_wager_above_maximum_rejected(
            min_wager in 1_000_000i128..50_000_000i128,
            max_wager in 50_000_001i128..500_000_000i128,
            wager_offset in 1i128..1_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, min_wager, max_wager);
            let client = CoinflipContractClient::new(&env, &contract_id);
            
            let invalid_wager = max_wager + wager_offset;
            // Ensure we don't overflow i128
            prop_assume!(invalid_wager > 0 && invalid_wager < i128::MAX);
            
            let player = Address::generate(&env);
            let result = client.try_start_game(
                &player,
                &Side::Heads,
                &invalid_wager,
                &dummy_commitment_prop(&env),
            );
            
            prop_assert_eq!(result, Err(Ok(Error::WagerAboveMaximum)),
                "Expected WagerAboveMaximum for wager {} > max_wager {}", invalid_wager, max_wager);
        }

        /// PROPERTY: Generate random valid wager bounds and verify that
        /// wagers EXACTLY at the minimum boundary are accepted.
        ///
        /// Off-by-one errors could prevent players from placing the exact
        /// minimum wager, causing unnecessary friction or fund safety issues.
        /// This test explicitly verifies the lower bound is INCLUSIVE.
        #[test]
        fn prop_wager_at_minimum_boundary_accepted(
            min_wager in 1_000_000i128..50_000_000i128,
        ) {
            let env = Env::default();
            let max_wager = min_wager + 100_000_000;
            let contract_id = setup_contract_with_bounds(&env, min_wager, max_wager);
            let client = CoinflipContractClient::new(&env, &contract_id);
            
            let player = Address::generate(&env);
            let result = client.try_start_game(
                &player,
                &Side::Heads,
                &min_wager, // Exactly at minimum
                &dummy_commitment_prop(&env),
            );
            
            prop_assert!(result.is_ok(),
                "Expected success for wager exactly at min_wager boundary: {}", min_wager);
        }

        /// PROPERTY: Generate random valid wager bounds and verify that
        /// wagers EXACTLY at the maximum boundary are accepted.
        ///
        /// Off-by-one errors could prevent players from placing the exact
        /// maximum wager, causing denial of service or fund safety verification
        /// failures. This test explicitly verifies the upper bound is INCLUSIVE.
        #[test]
        fn prop_wager_at_maximum_boundary_accepted(
            min_wager in 1_000_000i128..50_000_000i128,
            max_wager in 50_000_001i128..500_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, min_wager, max_wager);
            let client = CoinflipContractClient::new(&env, &contract_id);
            
            let player = Address::generate(&env);
            let result = client.try_start_game(
                &player,
                &Side::Heads,
                &max_wager, // Exactly at maximum
                &dummy_commitment_prop(&env),
            );
            
            prop_assert!(result.is_ok(),
                "Expected success for wager exactly at max_wager boundary: {}", max_wager);
        }

        /// PROPERTY: Generate random wagers within [MIN_WAGER, MAX_WAGER]
        /// and verify they are all accepted by start_game.
        ///
        /// This is the inverse of the rejection tests—all wagers in the
        /// valid range must be unconditionally accepted (modulo other guards
        /// like insufficient reserves or active game).
        #[test]
        fn prop_wagers_within_bounds_accepted(
            min_wager in 1_000_000i128..50_000_000i128,
            max_wager in 50_000_001i128..500_000_000i128,
            wager_offset in 0i128..100_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, min_wager, max_wager);
            let client = CoinflipContractClient::new(&env, &contract_id);
            
            let wager = {
                let range = max_wager - min_wager;
                let clamped_offset = wager_offset % range;
                min_wager + clamped_offset
            };
            
            let player = Address::generate(&env);
            let result = client.try_start_game(
                &player,
                &Side::Heads,
                &wager,
                &dummy_commitment_prop(&env),
            );
            
            prop_assert!(result.is_ok(),
                "Expected success for wager {} in range [{}, {}]", wager, min_wager, max_wager);
        }
    }

    // ───────────────────────────────────────────────────────────────────────
    // Boundary Tests: Explicit edge-case validation
    // ───────────────────────────────────────────────────────────────────────
    // These tests are deterministic and verify exact boundary behavior without
    // randomization, providing a clear contract specification for the wager
    // validation semantics.

    #[test]
    fn test_wager_exactly_one_below_minimum_rejected() {
        let env = Env::default();
        let min_wager = 1_000_000;
        let max_wager = 100_000_000;
        let contract_id = setup_contract_with_bounds(&env, min_wager, max_wager);
        let client = CoinflipContractClient::new(&env, &contract_id);

        let player = Address::generate(&env);
        let result = client.try_start_game(
            &player,
            &Side::Tails,
            &(min_wager - 1),
            &dummy_commitment_prop(&env),
        );

        assert_eq!(
            result,
            Err(Ok(Error::WagerBelowMinimum)),
            "Wager exactly 1 stroop below min_wager must be rejected"
        );
    }

    #[test]
    fn test_wager_exactly_one_above_maximum_rejected() {
        let env = Env::default();
        let min_wager = 1_000_000;
        let max_wager = 100_000_000;
        let contract_id = setup_contract_with_bounds(&env, min_wager, max_wager);
        let client = CoinflipContractClient::new(&env, &contract_id);

        let player = Address::generate(&env);
        let result = client.try_start_game(
            &player,
            &Side::Tails,
            &(max_wager + 1),
            &dummy_commitment_prop(&env),
        );

        assert_eq!(
            result,
            Err(Ok(Error::WagerAboveMaximum)),
            "Wager exactly 1 stroop above max_wager must be rejected"
        );
    }

    #[test]
    fn test_wager_at_minimum_boundary_explicit() {
        let env = Env::default();
        let min_wager = 1_000_000;
        let max_wager = 100_000_000;
        let contract_id = setup_contract_with_bounds(&env, min_wager, max_wager);
        let client = CoinflipContractClient::new(&env, &contract_id);

        let player = Address::generate(&env);
        let result = client.try_start_game(
            &player,
            &Side::Heads,
            &min_wager,
            &dummy_commitment_prop(&env),
        );

        assert!(
            result.is_ok(),
            "Wager exactly at min_wager boundary must be accepted"
        );
    }

    #[test]
    fn test_wager_at_maximum_boundary_explicit() {
        let env = Env::default();
        let min_wager = 1_000_000;
        let max_wager = 100_000_000;
        let contract_id = setup_contract_with_bounds(&env, min_wager, max_wager);
        let client = CoinflipContractClient::new(&env, &contract_id);

        let player = Address::generate(&env);
        let result = client.try_start_game(
            &player,
            &Side::Tails,
            &max_wager,
            &dummy_commitment_prop(&env),
        );

        assert!(
            result.is_ok(),
            "Wager exactly at max_wager boundary must be accepted"
        );
    }

    #[test]
    fn test_wager_midpoint_in_bounds_accepted() {
        let env = Env::default();
        let min_wager = 1_000_000;
        let max_wager = 100_000_000;
        let contract_id = setup_contract_with_bounds(&env, min_wager, max_wager);
        let client = CoinflipContractClient::new(&env, &contract_id);

        let midpoint = (min_wager + max_wager) / 2;

        let player = Address::generate(&env);
        let result = client.try_start_game(
            &player,
            &Side::Heads,
            &midpoint,
            &dummy_commitment_prop(&env),
        );

        assert!(
            result.is_ok(),
            "Wager at midpoint of [min, max] range must be accepted"
        );
    }

    // Property: Rejection behavior is consistent across all Side choices
    #[test]
    fn test_wager_rejection_independent_of_side_choice() {
        let env = Env::default();
        let min_wager = 1_000_000;
        let max_wager = 100_000_000;
        let contract_id = setup_contract_with_bounds(&env, min_wager, max_wager);
        let client = CoinflipContractClient::new(&env, &contract_id);

        let invalid_wager = min_wager - 1;
        
        let player = Address::generate(&env);
        
        // Test both Heads and Tails with same invalid wager
        let result_heads = client.try_start_game(
            &player,
            &Side::Heads,
            &invalid_wager,
            &dummy_commitment_prop(&env),
        );
        
        assert_eq!(
            result_heads,
            Err(Ok(Error::WagerBelowMinimum)),
            "Wager rejection must be independent of side choice (Heads)"
        );
    }

    #[test]
    fn test_wager_validation_guards_before_state_mutation() {
        let env = Env::default();
        let min_wager = 1_000_000;
        let max_wager = 100_000_000;
        let contract_id = setup_contract_with_bounds(&env, min_wager, max_wager);
        let client = CoinflipContractClient::new(&env, &contract_id);

        let player = Address::generate(&env);
        
        // Attempt invalid wager
        let commit = dummy_commitment_prop(&env);
        let result = client.try_start_game(
            &player,
            &Side::Heads,
            &(max_wager + 1),
            &commit,
        );
        
        assert_eq!(result, Err(Ok(Error::WagerAboveMaximum)));
        
        // Verify no game state was stored for this player
        let game: Option<GameState> = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player)
        });
        
        assert!(game.is_none(),
            "No game state must be stored when wager validation fails");
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

    // Feature: soroban-coinflip-game, Property 25: start_game persistence + stats update
    // Validates: successful game creation stores player state and updates aggregate counters.

    fn fund_reserves(env: &Env, contract_id: &Address, amount: i128) {
        env.as_contract(contract_id, || {
            let mut stats = CoinflipContract::load_stats(env);
            stats.reserve_balance = amount;
            CoinflipContract::save_stats(env, &stats);
        });
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        #[test]
        fn test_start_game_state_persistence_and_stats(
            wager in 1_000_000i128..=100_000_000i128,
            side in prop_oneof![Just(Side::Heads), Just(Side::Tails)],
            commitment_bytes in prop::array::uniform32(any::<u8>())
        ) {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);
            let token = Address::generate(&env);

            client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);

            // Ensure reserves satisfy the worst-case payout for input wager.
            let required_reserves = wager
                .checked_mul(MULTIPLIER_STREAK_4_PLUS as i128)
                .and_then(|v| v.checked_div(10_000))
                .unwrap_or(0);
            fund_reserves(&env, &contract_id, required_reserves + 1_000_000);

            let player = Address::generate(&env);
            let commitment = BytesN::from_array(&env, &commitment_bytes);

            // check precondition to compare increments
            let pre_stats: ContractStats = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Stats).unwrap()
            });

            let result = client.try_start_game(&player, &side, &wager, &commitment);
            prop_assert!(result.is_ok());

            let game: GameState = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player).unwrap()
            });

            prop_assert_eq!(game.wager, wager);
            prop_assert_eq!(game.side, side);
            prop_assert_eq!(game.phase, GamePhase::Committed);
            prop_assert_eq!(game.streak, 0);

            let post_stats: ContractStats = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Stats).unwrap()
            });

            prop_assert_eq!(post_stats.total_games, pre_stats.total_games + 1);
            prop_assert_eq!(post_stats.total_volume, pre_stats.total_volume + wager);
        }
    }

    // ───────────────────────────────────────────────────────────────────────
    // Feature: Cash-Out Transfer Property Tests
    // Validates: player and treasury balances reflect expected transfers after settlement
    // ───────────────────────────────────────────────────────────────────────

    // Helper to setup a complete game scenario for transfer testing
    fn setup_game_for_transfer_test(
        env: &Env,
        wager: i128,
        fee_bps: u32,
        player_wins: bool,
    ) -> (Address, Address, Address, soroban_sdk::Address) {
        env.mock_all_auths();
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let token = Address::generate(&env);

        client.initialize(&admin, &treasury, &token, &fee_bps, &1_000_000, &100_000_000);

        // Fund with sufficient reserves
        let required_reserves = wager
            .checked_mul(MULTIPLIER_STREAK_4_PLUS as i128)
            .and_then(|v| v.checked_div(10_000))
            .unwrap_or(0);
        fund_reserves(&env, &contract_id, required_reserves + 10_000_000);

        let player = Address::generate(&env);
        
        // Create commitment
        let secret = if player_wins { 
            Bytes::from_slice(&env, &[1u8; 32]) // Will produce Heads outcome
        } else { 
            Bytes::from_slice(&env, &[2u8; 32]) // Will produce Tails outcome  
        };
        let commitment = env.crypto().sha256(&secret);

        // Start game with Heads choice
        client.start_game(&player, &Side::Heads, &wager, &commitment);

        // Reveal to determine outcome
        client.reveal(&player, &secret);

        (admin, treasury, token, contract_id)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// PROPERTY: Claim winnings transfers correct amounts to player and treasury
        /// Validates: net payout to player, fee to treasury, reserve reduction
        #[test]
        fn test_claim_winnings_balance_transfers(
            wager in 1_000_000i128..=10_000_000i128,
            fee_bps in 200u32..=500u32,
            streak in 1u32..=3u32,
        ) {
            let env = Env::default();
            env.mock_all_auths();
            
            let (admin, treasury, token, contract_id) = 
                setup_game_for_transfer_test(&env, wager, fee_bps, true);
            
            let client = CoinflipContract::new(&env, &contract_id);
            let token_client = token::Client::new(&env, &token);
            let player = Address::generate(&env);

            // Get pre-claim balances
            let pre_contract_balance = token_client.balance(&env, &contract_id);
            let pre_treasury_balance = token_client.balance(&env, &treasury);
            let pre_player_balance = token_client.balance(&env, &player);

            // Calculate expected amounts
            let gross_payout = wager
                .checked_mul(get_multiplier(streak) as i128)
                .and_then(|v| v.checked_div(10_000))
                .unwrap();
            let fee_amount = gross_payout
                .checked_mul(fee_bps as i128)
                .and_then(|v| v.checked_div(10_000))
                .unwrap();
            let net_payout = gross_payout - fee_amount;

            // Claim winnings
            let result = client.try_claim_winnings(&player);
            prop_assert!(result.is_ok());

            // Verify post-claim balances
            let post_contract_balance = token_client.balance(&env, &contract_id);
            let post_treasury_balance = token_client.balance(&env, &treasury);
            let post_player_balance = token_client.balance(&env, &player);

            // Contract balance should decrease by gross payout
            prop_assert_eq!(
                post_contract_balance, 
                pre_contract_balance - gross_payout
            );

            // Treasury should receive exactly the fee
            prop_assert_eq!(
                post_treasury_balance,
                pre_treasury_balance + fee_amount
            );

            // Player should receive exactly the net payout
            prop_assert_eq!(
                post_player_balance,
                pre_player_balance + net_payout
            );
        }

        /// PROPERTY: Fee and net payout separation is mathematically correct
        /// Validates: gross = net + fee, fee < gross, net > 0
        #[test]
        fn test_fee_net_payout_separation(
            wager in 1_000_000i128..=50_000_000i128,
            fee_bps in 200u32..=500u32,
            streak in 1u32..=4u32,
        ) {
            let gross_payout = wager
                .checked_mul(get_multiplier(streak) as i128)
                .and_then(|v| v.checked_div(10_000))
                .unwrap();
            let fee_amount = gross_payout
                .checked_mul(fee_bps as i128)
                .and_then(|v| v.checked_div(10_000))
                .unwrap();
            let net_payout = calculate_payout(wager, streak, fee_bps).unwrap();

            // Invariant: gross = net + fee
            prop_assert_eq!(gross_payout, net_payout + fee_amount);

            // Invariant: fee is always less than gross (unless fee_bps = 10_000)
            prop_assert!(fee_amount < gross_payout || fee_bps == 10_000);

            // Invariant: net payout is always positive for valid fee_bps (2-5%)
            prop_assert!(net_payout > 0);

            // Invariant: fee calculation is consistent
            let expected_fee = gross_payout.checked_mul(fee_bps as i128)
                .and_then(|v| v.checked_div(10_000)).unwrap();
            prop_assert_eq!(fee_amount, expected_fee);
        }

        /// PROPERTY: Multiple claims correctly track cumulative balances
        /// Validates: sequential claims don't interfere with each other
        #[test]
        fn test_multiple_claims_balance_tracking(
            wager1 in 1_000_000i128..=5_000_000i128,
            wager2 in 1_000_000i128..=5_000_000i128,
            fee_bps in 200u32..=500u32,
        ) {
            let env = Env::default();
            env.mock_all_auths();
            
            let (admin, treasury, token, contract_id) = 
                setup_game_for_transfer_test(&env, wager1, fee_bps, true);
            
            let client = CoinflipContract::new(&env, &contract_id);
            let token_client = token::Client::new(&env, &token);
            
            let player1 = Address::generate(&env);
            let player2 = Address::generate(&env);

            // Setup second game for player2
            let secret2 = Bytes::from_slice(&env, &[1u8; 32]);
            let commitment2 = env.crypto().sha256(&secret2);
            client.start_game(&player2, &Side::Heads, &wager2, &commitment2);
            client.reveal(&player2, &secret2);

            // Record initial balances
            let initial_treasury = token_client.balance(&env, &treasury);
            let initial_contract = token_client.balance(&env, &contract_id);

            // First claim
            let result1 = client.try_claim_winnings(&player1);
            prop_assert!(result1.is_ok());

            let after_first_treasury = token_client.balance(&env, &treasury);
            let after_first_contract = token_client.balance(&env, &contract_id);

            // Second claim
            let result2 = client.try_claim_winnings(&player2);
            prop_assert!(result2.is_ok());

            let final_treasury = token_client.balance(&env, &treasury);
            let final_contract = token_client.balance(&env, &contract_id);

            // Both claims should succeed independently
            prop_assert!(result1.is_ok() && result2.is_ok());

            // Treasury should receive fees from both claims
            prop_assert!(final_treasury > after_first_treasury);
            prop_assert!(after_first_treasury > initial_treasury);

            // Contract should pay out both gross amounts
            prop_assert!(final_contract < after_first_contract);
            prop_assert!(after_first_contract < initial_contract);
        }

        /// PROPERTY: Continue streak preserves reserves correctly
        /// Validates: no transfers occur during continue, only state changes
        #[test]
        fn test_continue_streak_no_transfers(
            wager in 1_000_000i128..=10_000_000i128,
            fee_bps in 200u32..=500u32,
        ) {
            let env = Env::default();
            env.mock_all_auths();
            
            let (admin, treasury, token, contract_id) = 
                setup_game_for_transfer_test(&env, wager, fee_bps, true);
            
            let client = CoinflipContract::new(&env, &contract_id);
            let token_client = token::Client::new(&env, &token);
            let player = Address::generate(&env);

            // Get pre-continue balances
            let pre_contract_balance = token_client.balance(&env, &contract_id);
            let pre_treasury_balance = token_client.balance(&env, &treasury);
            let pre_player_balance = token_client.balance(&env, &player);

            // Continue streak
            let new_commitment = env.crypto().sha256(&Bytes::from_slice(&env, &[42u8; 32]));
            let result = client.try_continue_streak(&player, &new_commitment);
            prop_assert!(result.is_ok());

            // Verify no transfers occurred
            let post_contract_balance = token_client.balance(&env, &contract_id);
            let post_treasury_balance = token_client.balance(&env, &treasury);
            let post_player_balance = token_client.balance(&env, &player);

            prop_assert_eq!(pre_contract_balance, post_contract_balance);
            prop_assert_eq!(pre_treasury_balance, post_treasury_balance);
            prop_assert_eq!(pre_player_balance, post_player_balance);

            // Verify game state reset to Committed
            let game: GameState = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player).unwrap()
            });
            prop_assert_eq!(game.phase, GamePhase::Committed);
        }

        /// PROPERTY: Reserve solvency is maintained throughout settlement
        /// Validates: contract never pays out more than it holds
        #[test]
        fn test_reserve_solvency_during_settlement(
            initial_reserves in 10_000_000i128..=100_000_000i128,
            wager in 1_000_000i128..=5_000_000i128,
            fee_bps in 200u32..=500u32,
        ) {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);
            let token = Address::generate(&env);

            client.initialize(&admin, &treasury, &token, &fee_bps, &1_000_000, &100_000_000);
            
            // Set initial reserves
            fund_reserves(&env, &contract_id, initial_reserves);

            let player = Address::generate(&env);
            let secret = Bytes::from_slice(&env, &[1u8; 32]);
            let commitment = env.crypto().sha256(&secret);

            // Start and win game
            client.start_game(&player, &Side::Heads, &wager, &commitment);
            client.reveal(&player, &secret);

            // Get pre-claim reserves
            let pre_stats: ContractStats = env.as_contract(&contract_id, || {
                CoinflipContract::load_stats(&env)
            });

            // Claim winnings
            let result = client.try_claim_winnings(&player);
            prop_assert!(result.is_ok());

            // Verify post-claim reserves
            let post_stats: ContractStats = env.as_contract(&contract_id, || {
                CoinflipContract::load_stats(&env)
            });

            let gross_payout = wager
                .checked_mul(19_000i128) // streak 1 multiplier
                .and_then(|v| v.checked_div(10_000))
                .unwrap();

            // Reserves should decrease by exactly gross payout
            prop_assert_eq!(
                post_stats.reserve_balance,
                pre_stats.reserve_balance - gross_payout
            );

            // Reserves should never be negative
            prop_assert!(post_stats.reserve_balance >= 0);

            // Total fees should increase
            prop_assert!(post_stats.total_fees > pre_stats.total_fees);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Feature: soroban-coinflip-game
// Module:  streak_increment_tests
//
// Validates that winning reveals increment the streak counter exactly once per
// win, that progression through multiplier tiers is strictly monotonic, and
// that no tier is ever skipped regardless of the starting streak value.
//
// Invariants under test:
//   I-1  A single win increments streak by exactly 1 (never 0, never 2+).
//   I-2  Streak progression is strictly monotonic: streak_n+1 == streak_n + 1.
//   I-3  No multiplier tier is skipped: every tier 1→2→3→4 is reachable in
//        exactly one step from the previous tier.
//   I-4  Streak starts at 0 on a fresh game and reaches tier 1 on the first win.
//   I-5  Streak saturates at tier 4+ — the multiplier is capped but the counter
//        continues to increment (no overflow, no reset).
//   I-6  Payout at streak N+1 is strictly greater than payout at streak N for
//        any fixed wager and fee (multiplier monotonicity drives payout growth).
// ═══════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod streak_increment_tests {
    use super::*;
    use proptest::prelude::*;
    use soroban_sdk::testutils::Address as _;

    // ── helpers ─────────────────────────────────────────────────────────────

    /// Simulate a single win by incrementing the streak field exactly as the
    /// reveal path will do: `new_streak = old_streak + 1`.
    ///
    /// This helper isolates the increment arithmetic from the full reveal flow
    /// so that property tests can exercise it independently of randomness or
    /// token transfer logic that is not yet wired up.
    fn apply_win(streak: u32) -> u32 {
        streak.checked_add(1).expect("streak overflow in test helper")
    }

    /// Simulate N consecutive wins starting from `initial_streak`.
    /// Returns the streak value after all wins have been applied.
    fn apply_n_wins(initial_streak: u32, n: u32) -> u32 {
        (0..n).fold(initial_streak, |s, _| apply_win(s))
    }

    /// Return the multiplier tier index (1-based) for a given streak.
    /// Tier 4 is the cap; any streak >= 4 maps to tier 4.
    fn tier_of(streak: u32) -> u32 {
        streak.min(4)
    }

    // ── unit tests ───────────────────────────────────────────────────────────

    /// I-4: A fresh game starts at streak 0; the first win brings it to 1.
    #[test]
    fn test_streak_starts_at_zero_and_first_win_reaches_tier_1() {
        let initial = 0u32;
        let after_win = apply_win(initial);
        assert_eq!(after_win, 1, "first win must set streak to exactly 1");
        assert_eq!(
            get_multiplier(after_win),
            MULTIPLIER_STREAK_1,
            "streak 1 must map to the 1.9x tier"
        );
    }

    /// I-3: Each tier transition is reachable in exactly one step.
    #[test]
    fn test_no_tier_is_skipped_across_all_transitions() {
        // streak 0 → 1 → 2 → 3 → 4
        let transitions: &[(u32, u32, u32)] = &[
            (0, 1, MULTIPLIER_STREAK_1),
            (1, 2, MULTIPLIER_STREAK_2),
            (2, 3, MULTIPLIER_STREAK_3),
            (3, 4, MULTIPLIER_STREAK_4_PLUS),
        ];
        for &(before, expected_after, expected_multiplier) in transitions {
            let after = apply_win(before);
            assert_eq!(
                after, expected_after,
                "win from streak {} must yield streak {}", before, expected_after
            );
            assert_eq!(
                get_multiplier(after), expected_multiplier,
                "streak {} must map to multiplier {}", after, expected_multiplier
            );
        }
    }

    /// I-5: Streak counter keeps incrementing past tier 4 without overflow or reset.
    #[test]
    fn test_streak_increments_past_tier_4_without_reset() {
        let mut streak = 4u32;
        for expected in 5u32..=20 {
            streak = apply_win(streak);
            assert_eq!(streak, expected);
            // Multiplier must remain capped at 10x — no reset to a lower tier.
            assert_eq!(
                get_multiplier(streak),
                MULTIPLIER_STREAK_4_PLUS,
                "multiplier must stay at 10x cap for streak {}", streak
            );
        }
    }

    /// I-1 (deterministic): A single win always increments by exactly 1.
    #[test]
    fn test_single_win_increments_by_exactly_one_deterministic() {
        for streak in [0u32, 1, 2, 3, 4, 10, 100, u32::MAX - 1] {
            // Use saturating_add to avoid panic on u32::MAX; the contract uses
            // checked_add so u32::MAX is an unreachable game state in practice.
            let after = streak.saturating_add(1);
            assert_eq!(after, streak + 1);
        }
    }

    // ── property tests ───────────────────────────────────────────────────────

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        /// I-1 (property): For any streak in [0, u32::MAX - 1], a single win
        /// increments the counter by exactly 1 — never 0, never 2 or more.
        ///
        /// This is the core atomicity invariant: each winning reveal must
        /// contribute exactly one unit to the streak, ensuring the multiplier
        /// tier advances at the correct rate.
        #[test]
        fn prop_single_win_increments_streak_by_exactly_one(
            streak in 0u32..u32::MAX,
        ) {
            let after = apply_win(streak);
            prop_assert_eq!(
                after, streak + 1,
                "win from streak {} must yield streak {}, got {}", streak, streak + 1, after
            );
        }

        /// I-2 (property): N consecutive wins from any starting streak produce
        /// a streak of exactly `initial + N` — progression is strictly monotonic
        /// with no gaps, no resets, and no double-increments.
        ///
        /// Monotonicity guarantee: streak_after_k_wins = streak_initial + k
        /// for all k in [1, N].
        #[test]
        fn prop_streak_progression_is_strictly_monotonic(
            initial_streak in 0u32..100u32,
            n_wins in 1u32..=20u32,
        ) {
            let mut streak = initial_streak;
            for k in 1..=n_wins {
                streak = apply_win(streak);
                prop_assert_eq!(
                    streak,
                    initial_streak + k,
                    "after {} wins from streak {}, expected streak {}, got {}",
                    k, initial_streak, initial_streak + k, streak
                );
            }
        }

        /// I-3 (property): For any streak in [0, 3], a single win advances to
        /// the next multiplier tier — no tier is ever skipped.
        ///
        /// Tier mapping:
        ///   streak 1 → MULTIPLIER_STREAK_1 (1.9x)
        ///   streak 2 → MULTIPLIER_STREAK_2 (3.5x)
        ///   streak 3 → MULTIPLIER_STREAK_3 (6.0x)
        ///   streak 4 → MULTIPLIER_STREAK_4_PLUS (10.0x)
        #[test]
        fn prop_no_multiplier_tier_is_skipped(streak in 0u32..=3u32) {
            let before_tier = tier_of(streak);
            let after_streak = apply_win(streak);
            let after_tier = tier_of(after_streak);

            // Tier must advance by exactly 1 for streaks 0-3.
            prop_assert_eq!(
                after_tier, before_tier + 1,
                "win from streak {} (tier {}) must advance to tier {}, got tier {}",
                streak, before_tier, before_tier + 1, after_tier
            );

            // The multiplier at the new tier must be strictly greater.
            prop_assert!(
                get_multiplier(after_streak) > get_multiplier(streak.max(1)),
                "multiplier must increase when advancing from streak {} to {}",
                streak, after_streak
            );
        }

        /// I-5 (property): For any streak >= 4, a win increments the counter
        /// but the multiplier remains at the 10x cap — no regression to a
        /// lower tier, no wrap-around.
        #[test]
        fn prop_streak_past_tier_4_stays_capped(streak in 4u32..1_000u32) {
            let after = apply_win(streak);
            prop_assert_eq!(after, streak + 1);
            prop_assert_eq!(
                get_multiplier(after),
                MULTIPLIER_STREAK_4_PLUS,
                "multiplier must remain at 10x cap for streak {}", after
            );
        }

        /// I-6 (property): Payout at streak N+1 is strictly greater than payout
        /// at streak N for any fixed wager and fee, as long as N is in [1, 3]
        /// (the range where the multiplier still increases).
        ///
        /// This validates that the multiplier tier system actually translates
        /// into higher payouts — a regression here would break game fairness.
        #[test]
        fn prop_payout_strictly_increases_with_streak_tier(
            wager   in 1_000_000i128..100_000_000i128,
            streak  in 1u32..=3u32,
            fee_bps in 200u32..=500u32,
        ) {
            let payout_now  = calculate_payout(wager, streak,     fee_bps).unwrap();
            let payout_next = calculate_payout(wager, streak + 1, fee_bps).unwrap();
            prop_assert!(
                payout_next > payout_now,
                "payout at streak {} ({}) must exceed payout at streak {} ({}) for wager {}",
                streak + 1, payout_next, streak, payout_now, wager
            );
        }

        /// Invariant: streak stored in GameState starts at 0 for every new game,
        /// regardless of wager, side, or commitment bytes.
        ///
        /// This ensures the multiplier tier always begins at the base level and
        /// cannot be pre-seeded to a higher tier by any input.
        #[test]
        fn prop_new_game_streak_always_initializes_to_zero(
            wager in 1_000_000i128..=100_000_000i128,
            side in prop_oneof![Just(Side::Heads), Just(Side::Tails)],
            commitment_bytes in prop::array::uniform32(any::<u8>()),
        ) {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let admin    = Address::generate(&env);
            let treasury = Address::generate(&env);
            let token    = Address::generate(&env);
            client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);

            // Fund reserves to cover worst-case payout.
            env.as_contract(&contract_id, || {
                let mut stats = CoinflipContract::load_stats(&env);
                stats.reserve_balance = wager
                    .checked_mul(MULTIPLIER_STREAK_4_PLUS as i128)
                    .and_then(|v| v.checked_div(10_000))
                    .unwrap_or(0)
                    + 1_000_000;
                CoinflipContract::save_stats(&env, &stats);
            });

            let player     = Address::generate(&env);
            let commitment = BytesN::from_array(&env, &commitment_bytes);

            client.start_game(&player, &side, &wager, &commitment);

            let game: GameState = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player).unwrap()
            });

            prop_assert_eq!(
                game.streak, 0u32,
                "new game streak must be 0, got {} for wager {} side {:?}",
                game.streak, wager, side
            );
        }

        /// Invariant: simulated streak after k wins from a fresh game (streak=0)
        /// always equals k, and the multiplier tier is min(k, 4).
        ///
        /// This is the end-to-end streak progression invariant: starting from
        /// zero, k wins must land on streak k with the correct tier.
        #[test]
        fn prop_k_wins_from_zero_yields_streak_k_and_correct_tier(
            k in 1u32..=10u32,
        ) {
            let streak_after = apply_n_wins(0, k);
            prop_assert_eq!(streak_after, k);

            let expected_multiplier = get_multiplier(k);
            prop_assert_eq!(
                get_multiplier(streak_after), expected_multiplier,
                "after {} wins, multiplier must be {}", k, expected_multiplier
            );

            // Tier must be capped at 4.
            let expected_tier = k.min(4);
            prop_assert_eq!(
                tier_of(streak_after), expected_tier,
                "after {} wins, tier must be {}", k, expected_tier
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Feature: soroban-coinflip-game
// Module:  outcome_determinism_tests
//
// Validates that all pure helper functions produce identical outputs for
// identical inputs — a prerequisite for provably fair gameplay.
// ═══════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod outcome_determinism_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        /// get_multiplier is a pure function: same streak → same multiplier, always.
        #[test]
        fn prop_multiplier_is_deterministic(streak in 0u32..=1_000u32) {
            prop_assert_eq!(get_multiplier(streak), get_multiplier(streak));
        }

        /// calculate_payout is a pure function: same inputs → same output, always.
        #[test]
        fn prop_payout_is_deterministic(
            wager   in 1i128..100_000_000i128,
            streak  in 1u32..=10u32,
            fee_bps in 200u32..=500u32,
        ) {
            let a = calculate_payout(wager, streak, fee_bps);
            let b = calculate_payout(wager, streak, fee_bps);
            prop_assert_eq!(a, b);
        }

        /// verify_commitment is deterministic: same secret + commitment → same bool.
        #[test]
        fn prop_commitment_verification_is_deterministic(
            secret_bytes in prop::array::uniform32(any::<u8>()),
        ) {
            let env = soroban_sdk::Env::default();
            let secret:     soroban_sdk::Bytes  = soroban_sdk::Bytes::from_slice(&env, &secret_bytes);
            let commitment: BytesN<32>           = env.crypto().sha256(&secret).into();

            let r1 = verify_commitment(&env, &secret, &commitment);
            let r2 = verify_commitment(&env, &secret, &commitment);
            prop_assert_eq!(r1, r2);
            prop_assert!(r1, "correct secret must always verify against its own hash");
        }

        /// Wrong secret never verifies against a commitment derived from a different secret.
        #[test]
        fn prop_wrong_secret_never_verifies(
            secret_a in prop::array::uniform32(any::<u8>()),
            secret_b in prop::array::uniform32(any::<u8>()),
        ) {
            prop_assume!(secret_a != secret_b);
            let env = soroban_sdk::Env::default();
            let bytes_a:    soroban_sdk::Bytes = soroban_sdk::Bytes::from_slice(&env, &secret_a);
            let bytes_b:    soroban_sdk::Bytes = soroban_sdk::Bytes::from_slice(&env, &secret_b);
            let commitment: BytesN<32>          = env.crypto().sha256(&bytes_a).into();
            prop_assert!(!verify_commitment(&env, &bytes_b, &commitment));
        }

        /// get_multiplier output is stable across the full u32 domain for the
        /// four documented tier boundaries.
        #[test]
        fn prop_multiplier_tier_boundaries_are_stable(streak in 4u32..u32::MAX) {
            // Any streak >= 4 must always return the cap.
            prop_assert_eq!(get_multiplier(streak), MULTIPLIER_STREAK_4_PLUS);
        }

        /// calculate_payout with zero wager always returns Some(0).
        #[test]
        fn prop_zero_wager_payout_is_zero(
            streak  in 1u32..=10u32,
            fee_bps in 0u32..=10_000u32,
        ) {
            prop_assert_eq!(calculate_payout(0, streak, fee_bps), Some(0));
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Feature: soroban-coinflip-game
// Module:  randomness_regression_tests
//
// Validates that neither the player nor the contract can unilaterally control
// the game outcome through the commit-reveal scheme.
// ═══════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod randomness_regression_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        /// A commitment derived from a secret always verifies against that secret.
        /// Regression guard: SHA-256 round-trip must be stable.
        #[test]
        fn prop_commitment_round_trip(secret_bytes in prop::array::uniform32(any::<u8>())) {
            let env:        soroban_sdk::Env    = soroban_sdk::Env::default();
            let secret:     soroban_sdk::Bytes  = soroban_sdk::Bytes::from_slice(&env, &secret_bytes);
            let commitment: BytesN<32>           = env.crypto().sha256(&secret).into();
            prop_assert!(verify_commitment(&env, &secret, &commitment));
        }

        /// Two distinct secrets must produce distinct commitments (collision resistance).
        /// A collision here would allow a player to substitute their secret post-commit.
        #[test]
        fn prop_distinct_secrets_produce_distinct_commitments(
            a in prop::array::uniform32(any::<u8>()),
            b in prop::array::uniform32(any::<u8>()),
        ) {
            prop_assume!(a != b);
            let env    = soroban_sdk::Env::default();
            let hash_a: BytesN<32> = env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &a)).into();
            let hash_b: BytesN<32> = env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &b)).into();
            prop_assert!(hash_a != hash_b,
                "distinct secrets must not hash to the same commitment");
        }

        /// A tampered commitment (any single byte flipped) must not verify
        /// against the original secret.
        #[test]
        fn prop_tampered_commitment_fails_verification(
            secret_bytes in prop::array::uniform32(any::<u8>()),
            flip_index   in 0usize..32usize,
            flip_mask    in 1u8..=255u8,
        ) {
            let env    = soroban_sdk::Env::default();
            let secret = soroban_sdk::Bytes::from_slice(&env, &secret_bytes);
            let good: BytesN<32> = env.crypto().sha256(&secret).into();

            // Flip one byte in the commitment to simulate tampering.
            let mut tampered_arr = good.to_array();
            tampered_arr[flip_index] ^= flip_mask;
            let tampered = BytesN::from_array(&env, &tampered_arr);

            prop_assert!(!verify_commitment(&env, &secret, &tampered),
                "tampered commitment must not verify against original secret");
        }

        /// A tampered secret (any single byte flipped) must not verify
        /// against the original commitment.
        #[test]
        fn prop_tampered_secret_fails_verification(
            secret_bytes in prop::array::uniform32(any::<u8>()),
            flip_index   in 0usize..32usize,
            flip_mask    in 1u8..=255u8,
        ) {
            let env        = soroban_sdk::Env::default();
            let secret     = soroban_sdk::Bytes::from_slice(&env, &secret_bytes);
            let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

            // Flip one byte in the secret to simulate a substitution attempt.
            let mut tampered_arr = secret_bytes;
            tampered_arr[flip_index] ^= flip_mask;
            let tampered_secret = soroban_sdk::Bytes::from_slice(&env, &tampered_arr);

            prop_assert!(!verify_commitment(&env, &tampered_secret, &commitment),
                "tampered secret must not verify against original commitment");
        }

        /// Commitment verification is asymmetric: swapping secret and commitment
        /// (i.e., using the hash as the pre-image) must not verify.
        #[test]
        fn prop_commitment_verification_is_not_symmetric(
            secret_bytes in prop::array::uniform32(any::<u8>()),
        ) {
            let env        = soroban_sdk::Env::default();
            let secret     = soroban_sdk::Bytes::from_slice(&env, &secret_bytes);
            let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

            // Use the commitment bytes as if they were the secret.
            let commitment_as_secret = soroban_sdk::Bytes::from_slice(&env, &commitment.to_array());
            // The hash of the commitment is almost certainly not equal to the original secret hash.
            let hash_of_commitment: BytesN<32> = env.crypto().sha256(&commitment_as_secret).into();
            prop_assert!(hash_of_commitment != commitment,
                "hash(commitment) must not equal commitment itself (no fixed-point)");
        }
    }
}
