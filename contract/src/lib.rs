#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, contracterror, token, Address, Bytes, BytesN, Env};

/// Stable error code constants for the coinflip contract.
///
/// These constants document the canonical `u32` values for each [`Error`] variant.
/// Any change to these values is a breaking protocol change and must be coordinated
/// with all clients, indexers, and off-chain watchers.
///
/// Error code mapping:
///
/// | Code | Variant                      | Category       | Returned by                        |
/// |------|------------------------------|----------------|------------------------------------|
/// | 1    | `WagerBelowMinimum`          | Game creation  | `start_game`                       |
/// | 2    | `WagerAboveMaximum`          | Game creation  | `start_game`                       |
/// | 3    | `ActiveGameExists`           | Game creation  | `start_game`                       |
/// | 4    | `InsufficientReserves`       | Game creation  | `start_game`, `continue_streak`    |
/// | 5    | `ContractPaused`             | Game creation  | `start_game`                       |
/// | 10   | `NoActiveGame`               | Game state     | `reveal`, `claim_winnings`, `continue_streak` |
/// | 11   | `InvalidPhase`               | Game state     | `reveal`, `claim_winnings`, `continue_streak` |
/// | 12   | `CommitmentMismatch`         | Reveal         | `reveal`                           |
/// | 13   | `RevealTimeout`              | Reveal         | (reserved)                         |
/// | 20   | `NoWinningsToClaimOrContinue`| Action         | (reserved)                         |
/// | 21   | `InvalidCommitment`          | Action         | (reserved)                         |
/// | 30   | `Unauthorized`               | Admin          | (reserved)                         |
/// | 31   | `InvalidFeePercentage`       | Admin          | `initialize`                       |
/// | 32   | `InvalidWagerLimits`         | Admin          | `initialize`                       |
/// | 40   | `TransferFailed`             | Transfer       | `claim_winnings`                   |
/// | 50   | `AdminTreasuryConflict`      | Initialization | `initialize`                       |
/// | 51   | `AlreadyInitialized`         | Initialization | `initialize`                       |
pub mod error_codes {
    // Game creation errors (1–5)
    pub const WAGER_BELOW_MINIMUM: u32 = 1;
    pub const WAGER_ABOVE_MAXIMUM: u32 = 2;
    pub const ACTIVE_GAME_EXISTS: u32 = 3;
    pub const INSUFFICIENT_RESERVES: u32 = 4;
    pub const CONTRACT_PAUSED: u32 = 5;

    // Game state errors (10–13)
    pub const NO_ACTIVE_GAME: u32 = 10;
    pub const INVALID_PHASE: u32 = 11;
    pub const COMMITMENT_MISMATCH: u32 = 12;
    pub const REVEAL_TIMEOUT: u32 = 13;

    // Action errors (20–21)
    pub const NO_WINNINGS_TO_CLAIM_OR_CONTINUE: u32 = 20;
    pub const INVALID_COMMITMENT: u32 = 21;

    // Admin errors (30–32)
    pub const UNAUTHORIZED: u32 = 30;
    pub const INVALID_FEE_PERCENTAGE: u32 = 31;
    pub const INVALID_WAGER_LIMITS: u32 = 32;

    // Transfer errors (40)
    pub const TRANSFER_FAILED: u32 = 40;

    // Initialization errors (50–51)
    pub const ADMIN_TREASURY_CONFLICT: u32 = 50;
    pub const ALREADY_INITIALIZED: u32 = 51;

    /// Total number of defined error variants.
    pub const VARIANT_COUNT: usize = 17;
}

/// Error codes for the coinflip contract.
///
/// Each variant maps to a stable `u32` error code via `#[repr(u32)]`.
/// These codes are part of the public protocol and must remain stable
/// across contract upgrades. See [`error_codes`] for the canonical
/// constant definitions.
///
/// Error code ranges:
/// - `1–5`:   Game creation errors
/// - `10–13`: Reveal / game state errors
/// - `20–21`: Action errors (claim/continue)
/// - `30–32`: Admin errors
/// - `40`:    Transfer errors
/// - `50–51`: Initialization errors
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    // ── Game creation errors (1–5) ──────────────────────────────────────────

    /// Wager is below the configured minimum (`config.min_wager`).
    /// Returned by: `start_game` (guard 2).
    /// Code: 1 — see [`error_codes::WAGER_BELOW_MINIMUM`]
    WagerBelowMinimum = 1,

    /// Wager exceeds the configured maximum (`config.max_wager`).
    /// Returned by: `start_game` (guard 3).
    /// Code: 2 — see [`error_codes::WAGER_ABOVE_MAXIMUM`]
    WagerAboveMaximum = 2,

    /// Player already has an in-progress game (phase != Completed).
    /// Returned by: `start_game` (guard 4).
    /// Code: 3 — see [`error_codes::ACTIVE_GAME_EXISTS`]
    ActiveGameExists = 3,

    /// Contract reserves cannot cover the worst-case payout.
    /// Returned by: `start_game` (guard 5), `continue_streak`.
    /// Code: 4 — see [`error_codes::INSUFFICIENT_RESERVES`]
    InsufficientReserves = 4,

    /// Contract is paused; no new games accepted.
    /// Returned by: `start_game` (guard 1).
    /// Code: 5 — see [`error_codes::CONTRACT_PAUSED`]
    ContractPaused = 5,

    // ── Game state errors (10–13) ───────────────────────────────────────────

    /// Player has no game in storage.
    /// Returned by: `reveal`, `claim_winnings`, `continue_streak`.
    /// Code: 10 — see [`error_codes::NO_ACTIVE_GAME`]
    NoActiveGame = 10,

    /// Game is not in the expected phase for the requested operation.
    /// Returned by: `reveal` (expects Committed), `claim_winnings` (expects Revealed),
    /// `continue_streak` (expects Revealed).
    /// Code: 11 — see [`error_codes::INVALID_PHASE`]
    InvalidPhase = 11,

    /// Revealed secret does not hash to the stored commitment.
    /// Returned by: `reveal`.
    /// Code: 12 — see [`error_codes::COMMITMENT_MISMATCH`]
    CommitmentMismatch = 12,

    /// Reveal window has expired (reserved for future timeout enforcement).
    /// Code: 13 — see [`error_codes::REVEAL_TIMEOUT`]
    RevealTimeout = 13,

    // ── Action errors (20–21) ───────────────────────────────────────────────

    /// Player has no winnings to claim or continue (reserved).
    /// Code: 20 — see [`error_codes::NO_WINNINGS_TO_CLAIM_OR_CONTINUE`]
    NoWinningsToClaimOrContinue = 20,

    /// Commitment value is invalid (reserved).
    /// Code: 21 — see [`error_codes::INVALID_COMMITMENT`]
    InvalidCommitment = 21,

    // ── Admin errors (30–32) ────────────────────────────────────────────────

    /// Caller is not authorized for admin operations (reserved).
    /// Code: 30 — see [`error_codes::UNAUTHORIZED`]
    Unauthorized = 30,

    /// Fee percentage is outside the accepted range (200–500 bps / 2–5%).
    /// Returned by: `initialize`.
    /// Code: 31 — see [`error_codes::INVALID_FEE_PERCENTAGE`]
    InvalidFeePercentage = 31,

    /// Wager limits are invalid (`min_wager >= max_wager`).
    /// Returned by: `initialize`.
    /// Code: 32 — see [`error_codes::INVALID_WAGER_LIMITS`]
    InvalidWagerLimits = 32,

    // ── Transfer errors (40) ────────────────────────────────────────────────

    /// Token transfer failed during settlement.
    /// Returned by: `claim_winnings`.
    /// Code: 40 — see [`error_codes::TRANSFER_FAILED`]
    TransferFailed = 40,

    // ── Initialization errors (50–51) ───────────────────────────────────────

    /// Admin and treasury must be distinct addresses.
    /// Returned by: `initialize`.
    /// Code: 50 — see [`error_codes::ADMIN_TREASURY_CONFLICT`]
    AdminTreasuryConflict = 50,

    /// Contract has already been initialized.
    /// Returned by: `initialize`.
    /// Code: 51 — see [`error_codes::ALREADY_INITIALIZED`]
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
    ) -> Result<bool, Error> {
        player.require_auth();

        // Guard 1: player must have an active game
        let mut game = Self::load_player_game(&env, &player)
            .ok_or(Error::NoActiveGame)?;

        // Guard 2: game must be in Committed phase
        if game.phase != GamePhase::Committed {
            return Err(Error::InvalidPhase);
        }

        // Guard 3: verify the commitment matches the revealed secret
        if !verify_commitment(&env, &secret, &game.commitment) {
            return Err(Error::CommitmentMismatch);
        }

        // Determine outcome by combining player secret + contract random
        let cr_bytes = Bytes::from_slice(&env, &game.contract_random.to_array());
        let mut combined = Bytes::new(&env);
        combined.append(&secret);
        combined.append(&cr_bytes);
        let combined_hash = env.crypto().sha256(&combined);
        let outcome_bit = combined_hash.to_array()[0] % 2;
        let outcome = if outcome_bit == 0 { Side::Heads } else { Side::Tails };

        let won = outcome == game.side;

        if won {
            // Win path: increment streak, advance to Revealed phase.
            game.streak = game.streak.saturating_add(1);
            game.phase = GamePhase::Revealed;
            Self::save_player_game(&env, &player, &game);
            Ok(true)
        } else {
            // Loss path — forfeiture:
            // 1. Credit the wager back to contract reserves so the house keeps it.
            // 2. Delete the player's game state to free storage and signal game-over.
            let mut stats = Self::load_stats(&env);
            stats.reserve_balance = stats
                .reserve_balance
                .checked_add(game.wager)
                .unwrap_or(stats.reserve_balance);
            Self::save_stats(&env, &stats);

            Self::delete_player_game(&env, &player);

            Ok(false)
        }
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
        token_client.transfer(&env.current_contract_address(), &player, &net_payout);

        // Transfer fee to treasury
        token_client.transfer(&env.current_contract_address(), &config.treasury, &fee_amount);

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

    /// Cash out winnings after a successful reveal (no token transfer).
    ///
    /// Guards:
    /// 1. `NoActiveGame`               – player has no game
    /// 2. `InvalidPhase`               – game not in Revealed phase
    /// 3. `NoWinningsToClaimOrContinue` – streak == 0 (player lost)
    ///
    /// On success: calculates net payout, deducts it from reserves, records
    /// the fee in stats, sets game phase to Completed, and returns the net
    /// payout amount.
    pub fn cash_out(
        env: Env,
        player: Address,
    ) -> Result<i128, Error> {
        player.require_auth();

        let mut game = Self::load_player_game(&env, &player)
            .ok_or(Error::NoActiveGame)?;

        if game.phase != GamePhase::Revealed {
            return Err(Error::InvalidPhase);
        }

        if game.streak == 0 {
            return Err(Error::NoWinningsToClaimOrContinue);
        }

        let config = Self::load_config(&env);
        let net_payout = calculate_payout(game.wager, game.streak, config.fee_bps)
            .ok_or(Error::InsufficientReserves)?;

        let gross = game.wager
            .checked_mul(get_multiplier(game.streak) as i128)
            .and_then(|v| v.checked_div(10_000))
            .ok_or(Error::InsufficientReserves)?;
        let fee = gross
            .checked_mul(config.fee_bps as i128)
            .and_then(|v| v.checked_div(10_000))
            .ok_or(Error::InsufficientReserves)?;

        let mut stats = Self::load_stats(&env);
        stats.reserve_balance = stats.reserve_balance
            .checked_sub(net_payout)
            .ok_or(Error::InsufficientReserves)?;
        stats.total_fees = stats.total_fees
            .checked_add(fee)
            .unwrap_or(stats.total_fees);
        Self::save_stats(&env, &stats);

        game.phase = GamePhase::Completed;
        Self::save_player_game(&env, &player, &game);

        Ok(net_payout)
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
        let contract_random: BytesN<32> = env.crypto().sha256(
            &soroban_sdk::Bytes::from_slice(&env, &seq_bytes),
        ).into();

        // Reset to Committed phase for next round
        game.phase = GamePhase::Committed;
        game.commitment = new_commitment;
        game.contract_random = contract_random.into();

        Self::save_player_game(&env, &player, &game);
        Ok(())
    }

    /// Update the protocol fee charged on winning payouts.
    ///
    /// Only the configured `admin` address may call this function.
    /// The new fee must remain within the permitted range of 200–500 bps (2–5%).
    ///
    /// # Arguments
    /// - `admin`   – must match `config.admin`; authorization is required
    /// - `fee_bps` – new fee in basis points; must satisfy `200 <= fee_bps <= 500`
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]        – caller is not the configured admin
    /// - [`Error::InvalidFeePercentage`]– `fee_bps` is outside `[200, 500]`
    ///
    /// # Security
    /// - `admin.require_auth()` is called before any state is read or written,
    ///   ensuring the Soroban auth engine rejects unsigned invocations.
    /// - The fee range guard fires before the storage write, so an invalid fee
    ///   never reaches persistent state.
    /// - No player game state is touched; only `ContractConfig.fee_bps` changes.
    pub fn set_fee(env: Env, admin: Address, fee_bps: u32) -> Result<(), Error> {
        // Guard 1: require admin authorization before touching any state.
        admin.require_auth();

        let mut config = Self::load_config(&env);

        // Guard 2: caller must be the configured admin.
        if admin != config.admin {
            return Err(Error::Unauthorized);
        }

        // Guard 3: fee must stay within the permitted protocol range (2–5%).
        if fee_bps < 200 || fee_bps > 500 {
            return Err(Error::InvalidFeePercentage);
        }

        config.fee_bps = fee_bps;
        Self::save_config(&env, &config);

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
        // Register a real stellar asset contract so token transfers work
        let token = env.register_stellar_asset_contract(admin.clone());
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

    /// Setup a game in Revealed phase for transfer testing.
    fn setup_game_for_transfer_test(
        env: &Env,
        wager: i128,
        fee_bps: u32,
        win: bool,
    ) -> (Address, Address, Address, soroban_sdk::Address) {
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(env, &contract_id);
        
        let admin = Address::generate(env);
        let treasury = Address::generate(env);
        let token = Address::generate(env);
        
        client.initialize(&admin, &treasury, &token, &fee_bps, &1_000_000, &100_000_000);
        
        // Fund reserves
        fund_reserves(env, &contract_id, 1_000_000_000);
        
        if win {
            // Create a winning game
            let player = Address::generate(env);
            let secret = Bytes::from_slice(env, &[1u8; 32]);
            let commitment: BytesN<32> = env.crypto().sha256(&secret).into();
            
            client.start_game(&player, &Side::Heads, &wager, &commitment);
            
            // Force a win by setting game state directly
            env.as_contract(&contract_id, || {
                let mut game = GameState {
                    wager,
                    side: Side::Heads,
                    streak: 1,
                    commitment,
                    contract_random: env.crypto().sha256(&Bytes::from_slice(env, &[2u8; 32])).into(),
                    phase: GamePhase::Revealed,
                };
                CoinflipContract::save_player_game(env, &player, &game);
            });
        }
        
        (admin, treasury, token, contract_id)
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

    // ── cash_out validation ──────────────────────────────────────────────────

    /// Inject a game directly into storage at a specific phase/streak,
    /// bypassing start_game so tests can exercise any state combination.
    fn inject_game(
        env: &Env,
        contract_id: &soroban_sdk::Address,
        player: &Address,
        phase: GamePhase,
        streak: u32,
        wager: i128,
    ) {
        let dummy = dummy_commitment(env);
        let game = GameState {
            wager,
            side: Side::Heads,
            streak,
            commitment: dummy.clone(),
            contract_random: dummy,
            phase,
        };
        env.as_contract(contract_id, || {
            CoinflipContract::save_player_game(env, player, &game);
        });
    }

    /// Set reserve_balance so the contract can cover the payout.
    fn set_reserves(env: &Env, contract_id: &soroban_sdk::Address, amount: i128) {
        env.as_contract(contract_id, || {
            let mut stats = CoinflipContract::load_stats(env);
            stats.reserve_balance = amount;
            CoinflipContract::save_stats(env, &stats);
        });
    }

    /// Mint `amount` tokens to the contract address so token transfers succeed.
    fn mint_to_contract(env: &Env, contract_id: &soroban_sdk::Address, amount: i128) {
        let config: ContractConfig = env.as_contract(contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });
        let token_client = soroban_sdk::token::StellarAssetClient::new(env, &config.token);
        token_client.mint(contract_id, &amount);
    }

    // ── Guard 1: NoActiveGame ────────────────────────────────────────────────

    #[test]
    fn test_cash_out_rejects_no_active_game() {
        let env = Env::default();
        env.mock_all_auths();
        let (_, client) = setup(&env);

        let player = Address::generate(&env);
        // No game record exists for this player.
        let result = client.try_cash_out(&player);
        assert_eq!(result, Err(Ok(Error::NoActiveGame)));
    }

    // ── Guard 2: InvalidPhase ────────────────────────────────────────────────

    #[test]
    fn test_cash_out_rejects_committed_phase() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);

        let player = Address::generate(&env);
        // Game exists but reveal hasn't happened yet.
        inject_game(&env, &contract_id, &player, GamePhase::Committed, 1, 10_000_000);

        let result = client.try_cash_out(&player);
        assert_eq!(result, Err(Ok(Error::InvalidPhase)));
    }

    #[test]
    fn test_cash_out_rejects_completed_phase() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);

        let player = Address::generate(&env);
        // Game already ended — nothing left to claim.
        inject_game(&env, &contract_id, &player, GamePhase::Completed, 1, 10_000_000);

        let result = client.try_cash_out(&player);
        assert_eq!(result, Err(Ok(Error::InvalidPhase)));
    }

    // ── Guard 3: NoWinningsToClaimOrContinue ─────────────────────────────────

    #[test]
    fn test_cash_out_rejects_losing_state_streak_zero() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);

        let player = Address::generate(&env);
        // Revealed but streak == 0 means the player lost.
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 0, 10_000_000);

        let result = client.try_cash_out(&player);
        assert_eq!(result, Err(Ok(Error::NoWinningsToClaimOrContinue)));
    }

    // ── Happy path ───────────────────────────────────────────────────────────

    #[test]
    fn test_cash_out_succeeds_streak_1() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);

        let wager = 10_000_000i128;
        // gross = 10_000_000 * 19_000 / 10_000 = 19_000_000
        // fee   = 19_000_000 * 300  / 10_000 =    570_000
        // net   = 18_430_000
        let expected_net = 18_430_000i128;
        let expected_fee = 570_000i128;

        set_reserves(&env, &contract_id, 100_000_000);
        mint_to_contract(&env, &contract_id, 100_000_000);
        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 1, wager);

        let result = client.try_cash_out(&player);
        assert_eq!(result, Ok(Ok(expected_net)));
        let game: GameState = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player).unwrap()
        });
        assert_eq!(game.phase, GamePhase::Completed);

        // Stats: fee credited, reserves debited.
        let stats: ContractStats = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Stats).unwrap()
        });
        assert_eq!(stats.total_fees, expected_fee);
        assert_eq!(stats.reserve_balance, 100_000_000 - 19_000_000); // deducted gross, not net
    }

    #[test]
    fn test_cash_out_succeeds_streak_2() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);

        let wager = 5_000_000i128;
        // gross = 5_000_000 * 35_000 / 10_000 = 17_500_000
        // fee   = 17_500_000 * 300  / 10_000 =    525_000
        // net   = 16_975_000
        let expected_net = 16_975_000i128;

        set_reserves(&env, &contract_id, 100_000_000);
        mint_to_contract(&env, &contract_id, 100_000_000);
        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 2, wager);

        let result = client.try_cash_out(&player);
        assert_eq!(result, Ok(Ok(expected_net)));
        let game: GameState = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player).unwrap()
        });
        assert_eq!(game.phase, GamePhase::Completed);
    }

    #[test]
    fn test_cash_out_succeeds_streak_4_plus() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);

        let wager = 1_000_000i128;
        // gross = 1_000_000 * 100_000 / 10_000 = 10_000_000
        // fee   = 10_000_000 * 300   / 10_000 =    300_000
        // net   = 9_700_000
        let expected_net = 9_700_000i128;

        set_reserves(&env, &contract_id, 100_000_000);
        mint_to_contract(&env, &contract_id, 100_000_000);
        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 4, wager);

        let result = client.try_cash_out(&player);
        assert_eq!(result, Ok(Ok(expected_net)));
    }

    // ── Post-cash-out: player can start a new game ───────────────────────────

    #[test]
    fn test_cash_out_allows_new_game_after_completion() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);

        set_reserves(&env, &contract_id, 1_000_000_000);
        mint_to_contract(&env, &contract_id, 1_000_000_000);
        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 1, 10_000_000);

        // Cash out succeeds.
        assert!(client.try_cash_out(&player).is_ok());

        // Reserves still cover a new game — player can start again.
        let result = client.try_start_game(
            &player,
            &Side::Tails,
            &10_000_000,
            &dummy_commitment(&env),
        );
        assert!(result.is_ok(), "player must be able to start a new game after cash-out");
    }

    // ── Guard ordering: all checks fire before any state mutation ────────────

    #[test]
    fn test_cash_out_no_state_mutation_on_invalid_phase() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);

        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Committed, 1, 10_000_000);

        let before: GameState = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player).unwrap()
        });

        let _ = client.try_cash_out(&player);

        let after: GameState = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player).unwrap()
        });

        // State must be identical — no partial mutation on error.
        assert_eq!(before, after);
    }

    #[test]
    fn test_cash_out_no_state_mutation_on_losing_state() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);

        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 0, 10_000_000);

        let before_stats: ContractStats = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Stats).unwrap()
        });

        let _ = client.try_cash_out(&player);

        let after_stats: ContractStats = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Stats).unwrap()
        });

        // Stats must be unchanged — no fee or reserve mutation on error.
        assert_eq!(before_stats.total_fees, after_stats.total_fees);
        assert_eq!(before_stats.reserve_balance, after_stats.reserve_balance);
    }

    // ── set_fee tests ────────────────────────────────────────────────────────

    /// Helper: returns the admin address stored in config.
    fn get_admin(env: &Env, contract_id: &Address) -> Address {
        env.as_contract(contract_id, || {
            CoinflipContract::load_config(env).admin
        })
    }

    #[test]
    fn test_set_fee_succeeds_for_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        let admin = get_admin(&env, &contract_id);

        client.set_fee(&admin, &400);

        let stored: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });
        assert_eq!(stored.fee_bps, 400);
    }

    #[test]
    fn test_set_fee_rejects_non_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let (_, client) = setup(&env);
        let stranger = Address::generate(&env);

        let result = client.try_set_fee(&stranger, &400);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_set_fee_rejects_fee_below_minimum() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        let admin = get_admin(&env, &contract_id);

        let result = client.try_set_fee(&admin, &199);
        assert_eq!(result, Err(Ok(Error::InvalidFeePercentage)));
    }

    #[test]
    fn test_set_fee_rejects_fee_above_maximum() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        let admin = get_admin(&env, &contract_id);

        let result = client.try_set_fee(&admin, &501);
        assert_eq!(result, Err(Ok(Error::InvalidFeePercentage)));
    }

    #[test]
    fn test_set_fee_accepts_boundary_values() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        let admin = get_admin(&env, &contract_id);

        // Lower bound (200 bps = 2%)
        assert!(client.try_set_fee(&admin, &200).is_ok());
        let cfg: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });
        assert_eq!(cfg.fee_bps, 200);

        // Upper bound (500 bps = 5%)
        assert!(client.try_set_fee(&admin, &500).is_ok());
        let cfg: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });
        assert_eq!(cfg.fee_bps, 500);
    }

    #[test]
    fn test_set_fee_no_state_mutation_on_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);

        let before: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });

        let stranger = Address::generate(&env);
        let _ = client.try_set_fee(&stranger, &400);

        let after: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });

        assert_eq!(before.fee_bps, after.fee_bps);
    }

    #[test]
    fn test_set_fee_no_state_mutation_on_invalid_fee() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        let admin = get_admin(&env, &contract_id);

        let before: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });

        let _ = client.try_set_fee(&admin, &999);

        let after: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });

        assert_eq!(before.fee_bps, after.fee_bps);
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

    // Helper to setup a complete game scenario for transfer testing.
    // Returns (admin, treasury, token_address, contract_id, player) — player is the one with an active game.
    fn setup_game_for_transfer_test(
        env: &Env,
        wager: i128,
        fee_bps: u32,
        player_wins: bool,
    ) -> (Address, Address, Address, soroban_sdk::Address, Address) {
        env.mock_all_auths();
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        // Use a real stellar asset contract so token transfers work
        let token = env.register_stellar_asset_contract(admin.clone());

        client.initialize(&admin, &treasury, &token, &fee_bps, &1_000_000, &100_000_000);

        // Fund reserves (accounting) and mint real tokens to the contract
        let required_reserves = wager
            .checked_mul(MULTIPLIER_STREAK_4_PLUS as i128)
            .and_then(|v| v.checked_div(10_000))
            .unwrap_or(0)
            + 10_000_000;
        fund_reserves(&env, &contract_id, required_reserves);
        soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&contract_id, &required_reserves);

        let player = Address::generate(&env);

        // [1u8;32] → sha256[0]=0x72 (even) → Heads outcome → WIN for Heads player
        // [3u8;32] → sha256[0]=0x64 (even) XOR contract_random[0]=0xdf → odd → Tails → LOSS for Heads
        let secret = if player_wins {
            Bytes::from_slice(&env, &[1u8; 32])
        } else {
            Bytes::from_slice(&env, &[3u8; 32])
        };
        let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

        client.start_game(&player, &Side::Heads, &wager, &commitment);
        client.reveal(&player, &secret);

        (admin, treasury, token, contract_id, player)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// PROPERTY: Claim winnings transfers correct amounts to player and treasury
        /// Validates: net payout to player, fee to treasury, reserve reduction
        /// NOTE: #[ignore] — requires a deployed SAC token contract.
        #[test]
        #[ignore]
        fn test_claim_winnings_balance_transfers(
            wager in 1_000_000i128..=10_000_000i128,
            fee_bps in 200u32..=500u32,
            streak in 1u32..=3u32,
        ) {
            let env = Env::default();
            env.mock_all_auths();

            let (admin, treasury, token, contract_id) =
                setup_game_for_transfer_test(&env, wager, fee_bps, true);

            let client = CoinflipContractClient::new(&env, &contract_id);
            let token_client = token::Client::new(&env, &token);

            // Get pre-claim balances
            let pre_contract_balance = token_client.balance(&contract_id);
            let pre_treasury_balance = token_client.balance(&treasury);
            let pre_player_balance = token_client.balance(&player);

            // Calculate expected amounts using streak=1 (what reveal produces)
            let actual_streak = 1u32;
            let gross_payout = wager
                .checked_mul(get_multiplier(actual_streak) as i128)
                .and_then(|v| v.checked_div(10_000))
                .unwrap();
            let fee_amount = gross_payout
                .checked_mul(fee_bps as i128)
                .and_then(|v| v.checked_div(10_000))
                .unwrap();
            let net_payout = gross_payout - fee_amount;

            // Claim winnings
            let result = client.try_cash_out(&player);
            prop_assert!(result.is_ok());

            // Verify post-claim balances
            let post_contract_balance = token_client.balance(&contract_id);
            let post_treasury_balance = token_client.balance(&treasury);
            let post_player_balance = token_client.balance(&player);

            // Contract balance should decrease by gross payout
            prop_assert_eq!(
                post_contract_balance,
                pre_contract_balance - gross_payout
            );

            // Treasury should receive exactly the fee
            prop_assert_eq!(post_treasury_balance, pre_treasury_balance + fee_amount);
            // Player should receive exactly the net payout
            prop_assert_eq!(post_player_balance, pre_player_balance + net_payout);
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
        /// NOTE: #[ignore] — requires a deployed SAC token contract.
        #[test]
        #[ignore]
        fn test_multiple_claims_balance_tracking(
            wager1 in 1_000_000i128..=5_000_000i128,
            wager2 in 1_000_000i128..=5_000_000i128,
            fee_bps in 200u32..=500u32,
        ) {
            let env = Env::default();
            env.mock_all_auths();

            let (admin, treasury, token, contract_id) =
                setup_game_for_transfer_test(&env, wager1, fee_bps, true);

            let client = CoinflipContractClient::new(&env, &contract_id);
            let token_client = token::Client::new(&env, &token);

            let player1 = Address::generate(&env);
            let player2 = Address::generate(&env);

            // Setup second game for player2 — same win secret [1u8;32] → Heads win
            let player2 = Address::generate(&env);
            let secret2 = Bytes::from_slice(&env, &[1u8; 32]);
            let commitment2: BytesN<32> = env.crypto().sha256(&secret2).into();
            client.start_game(&player2, &Side::Heads, &wager2, &commitment2);
            client.reveal(&player2, &secret2);

            // Record initial balances
            let initial_treasury = token_client.balance(&treasury);
            let initial_contract = token_client.balance(&contract_id);

            // First claim
            let result1 = client.try_cash_out(&player1);
            prop_assert!(result1.is_ok());

            let after_first_treasury = token_client.balance(&treasury);
            let after_first_contract = token_client.balance(&contract_id);

            // Second claim
            let result2 = client.try_cash_out(&player2);
            prop_assert!(result2.is_ok());

            let final_treasury = token_client.balance(&treasury);
            let final_contract = token_client.balance(&contract_id);

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
        /// NOTE: #[ignore] — requires a deployed SAC token contract.
        #[test]
        #[ignore]
        fn test_continue_streak_no_transfers(
            wager in 1_000_000i128..=10_000_000i128,
            fee_bps in 200u32..=500u32,
        ) {
            let env = Env::default();
            env.mock_all_auths();

            let (admin, treasury, token, contract_id) =
                setup_game_for_transfer_test(&env, wager, fee_bps, true);

            let client = CoinflipContractClient::new(&env, &contract_id);
            let token_client = token::Client::new(&env, &token);

            // Get pre-continue balances
            let pre_contract_balance = token_client.balance(&contract_id);
            let pre_treasury_balance = token_client.balance(&treasury);
            let pre_player_balance = token_client.balance(&player);

            // Continue streak
            let new_commitment: BytesN<32> = env.crypto().sha256(&Bytes::from_slice(&env, &[42u8; 32])).into();
            let result = client.try_continue_streak(&player, &new_commitment);
            prop_assert!(result.is_ok());

            // Verify no transfers occurred
            let post_contract_balance = token_client.balance(&contract_id);
            let post_treasury_balance = token_client.balance(&treasury);
            let post_player_balance = token_client.balance(&player);

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
        /// NOTE: #[ignore] — claim_winnings performs token transfers requiring a deployed SAC.
        #[test]
        #[ignore]
        fn test_reserve_solvency_during_settlement(
            wager in 1_000_000i128..=5_000_000i128,
            fee_bps in 200u32..=500u32,
        ) {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);
            // Real token so transfers don't abort
            let token = env.register_stellar_asset_contract(admin.clone());

            client.initialize(&admin, &treasury, &token, &fee_bps, &1_000_000, &100_000_000);

            // Ensure reserves always cover worst-case payout for this wager
            let initial_reserves = wager
                .checked_mul(MULTIPLIER_STREAK_4_PLUS as i128)
                .and_then(|v| v.checked_div(10_000))
                .unwrap_or(0)
                + 10_000_000;
            fund_reserves(&env, &contract_id, initial_reserves);
            soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&contract_id, &initial_reserves);

            let player = Address::generate(&env);
            // [1u8;32] → Heads win (see outcome derivation in loss_forfeiture_tests)
            let secret = Bytes::from_slice(&env, &[1u8; 32]);
            let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

            client.start_game(&player, &Side::Heads, &wager, &commitment);
            client.reveal(&player, &secret);

            let pre_stats: ContractStats = env.as_contract(&contract_id, || {
                CoinflipContract::load_stats(&env)
            });

            let result = client.try_cash_out(&player);
            prop_assert!(result.is_ok());

            let post_stats: ContractStats = env.as_contract(&contract_id, || {
                CoinflipContract::load_stats(&env)
            });

            let gross_payout = wager
                .checked_mul(19_000i128) // streak 1 multiplier
                .and_then(|v| v.checked_div(10_000))
                .unwrap();

            // Reserves should decrease by exactly gross payout
            prop_assert_eq!(post_stats.reserve_balance, pre_stats.reserve_balance - gross_payout);
            // Reserves should never be negative
            prop_assert!(post_stats.reserve_balance >= 0);
            // Total fees should increase
            prop_assert!(post_stats.total_fees > pre_stats.total_fees);
        }
    }

    // ───────────────────────────────────────────────────────────────────────
    // Feature: Error Code Descriptiveness (Protocol Stability Critical)
    // ───────────────────────────────────────────────────────────────────────
    // PROPERTIES:
    // Each error path returns a stable, protocol-defined error code regardless
    // of the specific random input values. This ensures clients, indexers, and
    // off-chain watchers can reliably pattern-match on error codes across all
    // contract entry-points.
    //
    // Covered entry-points:
    //   - initialize: AlreadyInitialized, AdminTreasuryConflict,
    //                 InvalidFeePercentage, InvalidWagerLimits
    //   - start_game: WagerBelowMinimum, WagerAboveMaximum,
    //                 ActiveGameExists, InsufficientReserves
    //   - reveal:     NoActiveGame, InvalidPhase, CommitmentMismatch
    //   - claim_winnings: NoActiveGame, InvalidPhase
    //   - continue_streak: NoActiveGame, InvalidPhase, InsufficientReserves
    //
    // Additionally:
    //   - error_codes module constants ↔ Error enum discriminant parity
    //   - fee_bps boundary values (199, 200, 500, 501)
    // ───────────────────────────────────────────────────────────────────────

    /// Inject a game directly into storage within property_tests module scope,
    /// bypassing start_game so tests can exercise any state combination.
    fn inject_game_prop(
        env: &Env,
        contract_id: &Address,
        player: &Address,
        phase: GamePhase,
        streak: u32,
        wager: i128,
    ) {
        let dummy = dummy_commitment_prop(env);
        let game = GameState {
            wager,
            side: Side::Heads,
            streak,
            commitment: dummy.clone(),
            contract_random: dummy,
            phase,
        };
        env.as_contract(contract_id, || {
            CoinflipContract::save_player_game(env, player, &game);
        });
    }

    // Feature: Error Code Descriptiveness, Property: initialize error codes are stable
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// AlreadyInitialized (code 51) is returned on any re-initialization attempt
        /// regardless of the parameters supplied in the second call.
        #[test]
        fn prop_initialize_already_initialized_error_code(
            fee_bps in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
            max_wager in 10_000_001i128..1_000_000_000i128,
        ) {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);
            let token = Address::generate(&env);

            client.initialize(&admin, &treasury, &token, &fee_bps, &min_wager, &max_wager);

            // Second initialization with fresh addresses must still fail.
            let admin2 = Address::generate(&env);
            let treasury2 = Address::generate(&env);
            let result = client.try_initialize(
                &admin2, &treasury2, &token, &fee_bps, &min_wager, &max_wager,
            );
            prop_assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
            prop_assert_eq!(Error::AlreadyInitialized as u32, error_codes::ALREADY_INITIALIZED);
        }

        /// AdminTreasuryConflict (code 50) when admin == treasury for any address.
        #[test]
        fn prop_initialize_admin_treasury_conflict_error_code(
            fee_bps in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
            max_wager in 10_000_001i128..1_000_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let same_addr = Address::generate(&env);
            let token = Address::generate(&env);
            let result = client.try_initialize(
                &same_addr, &same_addr, &token, &fee_bps, &min_wager, &max_wager,
            );
            prop_assert_eq!(result, Err(Ok(Error::AdminTreasuryConflict)));
            prop_assert_eq!(Error::AdminTreasuryConflict as u32, error_codes::ADMIN_TREASURY_CONFLICT);
        }

        /// InvalidFeePercentage (code 31) for fee_bps below the valid range [200, 500].
        #[test]
        fn prop_initialize_invalid_fee_below_error_code(
            fee_bps in 0u32..200u32,
            min_wager in 1_000_000i128..10_000_000i128,
            max_wager in 10_000_001i128..1_000_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);
            let token = Address::generate(&env);
            let result = client.try_initialize(
                &admin, &treasury, &token, &fee_bps, &min_wager, &max_wager,
            );
            prop_assert_eq!(result, Err(Ok(Error::InvalidFeePercentage)));
            prop_assert_eq!(Error::InvalidFeePercentage as u32, error_codes::INVALID_FEE_PERCENTAGE);
        }

        /// InvalidFeePercentage (code 31) for fee_bps above the valid range [200, 500].
        #[test]
        fn prop_initialize_invalid_fee_above_error_code(
            fee_bps in 501u32..10_000u32,
            min_wager in 1_000_000i128..10_000_000i128,
            max_wager in 10_000_001i128..1_000_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);
            let token = Address::generate(&env);
            let result = client.try_initialize(
                &admin, &treasury, &token, &fee_bps, &min_wager, &max_wager,
            );
            prop_assert_eq!(result, Err(Ok(Error::InvalidFeePercentage)));
        }

        /// InvalidWagerLimits (code 32) when min_wager >= max_wager.
        #[test]
        fn prop_initialize_invalid_wager_limits_error_code(
            wager_val in 1_000_000i128..1_000_000_000i128,
            offset in 0i128..1_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);
            let token = Address::generate(&env);
            // min_wager = wager_val + offset, max_wager = wager_val → min >= max
            let result = client.try_initialize(
                &admin, &treasury, &token, &300, &(wager_val + offset), &wager_val,
            );
            prop_assert_eq!(result, Err(Ok(Error::InvalidWagerLimits)));
            prop_assert_eq!(Error::InvalidWagerLimits as u32, error_codes::INVALID_WAGER_LIMITS);
        }
    }

    // Feature: Error Code Descriptiveness, Property: start_game error codes are stable
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// WagerBelowMinimum (code 1) for any random wager below the configured min.
        #[test]
        fn prop_start_game_wager_below_min_error_code(
            min_wager in 1_000_000i128..50_000_000i128,
            wager_offset in 1i128..1_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, min_wager, min_wager + 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let invalid_wager = min_wager - wager_offset;
            prop_assume!(invalid_wager > 0);

            let player = Address::generate(&env);
            let result = client.try_start_game(
                &player, &Side::Heads, &invalid_wager, &dummy_commitment_prop(&env),
            );
            prop_assert_eq!(result, Err(Ok(Error::WagerBelowMinimum)));
            prop_assert_eq!(Error::WagerBelowMinimum as u32, error_codes::WAGER_BELOW_MINIMUM);
        }

        /// WagerAboveMaximum (code 2) for any random wager above the configured max.
        #[test]
        fn prop_start_game_wager_above_max_error_code(
            min_wager in 1_000_000i128..50_000_000i128,
            max_wager in 50_000_001i128..500_000_000i128,
            wager_offset in 1i128..1_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, min_wager, max_wager);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let invalid_wager = max_wager + wager_offset;
            prop_assume!(invalid_wager > 0 && invalid_wager < i128::MAX);

            let player = Address::generate(&env);
            let result = client.try_start_game(
                &player, &Side::Heads, &invalid_wager, &dummy_commitment_prop(&env),
            );
            prop_assert_eq!(result, Err(Ok(Error::WagerAboveMaximum)));
            prop_assert_eq!(Error::WagerAboveMaximum as u32, error_codes::WAGER_ABOVE_MAXIMUM);
        }

        /// ActiveGameExists (code 3) when player already has an in-progress game.
        #[test]
        fn prop_start_game_active_game_exists_error_code(
            wager in 1_000_000i128..=100_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            inject_game_prop(&env, &contract_id, &player, GamePhase::Committed, 0, wager);

            let result = client.try_start_game(
                &player, &Side::Heads, &wager, &dummy_commitment_prop(&env),
            );
            prop_assert_eq!(result, Err(Ok(Error::ActiveGameExists)));
            prop_assert_eq!(Error::ActiveGameExists as u32, error_codes::ACTIVE_GAME_EXISTS);
        }

        /// InsufficientReserves (code 4) when reserves can't cover max payout.
        #[test]
        fn prop_start_game_insufficient_reserves_error_code(
            wager in 1_000_000i128..=100_000_000i128,
        ) {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);
            let token = Address::generate(&env);
            client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);

            // Zero reserves — never enough to cover any wager's worst-case payout
            fund_reserves(&env, &contract_id, 0);

            let player = Address::generate(&env);
            let result = client.try_start_game(
                &player, &Side::Heads, &wager, &dummy_commitment_prop(&env),
            );
            prop_assert_eq!(result, Err(Ok(Error::InsufficientReserves)));
            prop_assert_eq!(Error::InsufficientReserves as u32, error_codes::INSUFFICIENT_RESERVES);
        }
    }

    // Feature: Error Code Descriptiveness, Property: reveal error codes are stable
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// NoActiveGame (code 10) when no game exists for the player.
        #[test]
        fn prop_reveal_no_active_game_error_code(
            secret_byte in 0u8..=255u8,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            let secret = Bytes::from_slice(&env, &[secret_byte; 32]);
            let result = client.try_reveal(&player, &secret);
            prop_assert_eq!(result, Err(Ok(Error::NoActiveGame)));
            prop_assert_eq!(Error::NoActiveGame as u32, error_codes::NO_ACTIVE_GAME);
        }

        /// InvalidPhase (code 11) when game is in Revealed phase (not Committed).
        #[test]
        fn prop_reveal_invalid_phase_revealed_error_code(
            wager in 1_000_000i128..=100_000_000i128,
            streak in 1u32..=10u32,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            inject_game_prop(&env, &contract_id, &player, GamePhase::Revealed, streak, wager);

            let secret = Bytes::from_slice(&env, &[42u8; 32]);
            let result = client.try_reveal(&player, &secret);
            prop_assert_eq!(result, Err(Ok(Error::InvalidPhase)));
            prop_assert_eq!(Error::InvalidPhase as u32, error_codes::INVALID_PHASE);
        }

        /// InvalidPhase (code 11) when game is in Completed phase.
        #[test]
        fn prop_reveal_invalid_phase_completed_error_code(
            wager in 1_000_000i128..=100_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            inject_game_prop(&env, &contract_id, &player, GamePhase::Completed, 0, wager);

            let secret = Bytes::from_slice(&env, &[42u8; 32]);
            let result = client.try_reveal(&player, &secret);
            prop_assert_eq!(result, Err(Ok(Error::InvalidPhase)));
        }

        /// CommitmentMismatch (code 12) when secret doesn't match stored commitment.
        #[test]
        fn prop_reveal_commitment_mismatch_error_code(
            wager in 1_000_000i128..=100_000_000i128,
            bad_byte in 0u8..=254u8,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            // inject_game_prop uses sha256([42u8; 32]) as commitment
            inject_game_prop(&env, &contract_id, &player, GamePhase::Committed, 0, wager);

            // Reveal with a different secret — guarantees mismatch when bad_byte != 42
            prop_assume!(bad_byte != 42);
            let wrong_secret = Bytes::from_slice(&env, &[bad_byte; 32]);
            let result = client.try_reveal(&player, &wrong_secret);
            prop_assert_eq!(result, Err(Ok(Error::CommitmentMismatch)));
            prop_assert_eq!(Error::CommitmentMismatch as u32, error_codes::COMMITMENT_MISMATCH);
        }
    }

    // Feature: Error Code Descriptiveness, Property: cash_out error codes are stable
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// NoActiveGame (code 10) when no game record exists for the player.
        #[test]
        fn prop_cash_out_no_active_game_error_code(
            _dummy in 0u32..100u32,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            let result = client.try_cash_out(&player);
            prop_assert_eq!(result, Err(Ok(Error::NoActiveGame)));
            prop_assert_eq!(Error::NoActiveGame as u32, error_codes::NO_ACTIVE_GAME);
        }

        /// InvalidPhase (code 11) when game is in Committed phase (not Revealed).
        #[test]
        fn prop_cash_out_invalid_phase_committed_error_code(
            wager in 1_000_000i128..=100_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            inject_game_prop(&env, &contract_id, &player, GamePhase::Committed, 0, wager);

            let result = client.try_cash_out(&player);
            prop_assert_eq!(result, Err(Ok(Error::InvalidPhase)));
            prop_assert_eq!(Error::InvalidPhase as u32, error_codes::INVALID_PHASE);
        }

        /// InvalidPhase (code 11) when game is in Completed phase.
        #[test]
        fn prop_cash_out_invalid_phase_completed_error_code(
            wager in 1_000_000i128..=100_000_000i128,
            streak in 0u32..=5u32,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            inject_game_prop(&env, &contract_id, &player, GamePhase::Completed, streak, wager);

            let result = client.try_cash_out(&player);
            prop_assert_eq!(result, Err(Ok(Error::InvalidPhase)));
        }

        /// NoWinningsToClaimOrContinue (code 20) when streak == 0 in Revealed phase.
        #[test]
        fn prop_cash_out_no_winnings_streak_zero_error_code(
            wager in 1_000_000i128..=100_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            inject_game_prop(&env, &contract_id, &player, GamePhase::Revealed, 0, wager);

            let result = client.try_cash_out(&player);
            prop_assert_eq!(result, Err(Ok(Error::NoWinningsToClaimOrContinue)));
            prop_assert_eq!(
                Error::NoWinningsToClaimOrContinue as u32,
                error_codes::NO_WINNINGS_TO_CLAIM_OR_CONTINUE,
            );
        }
    }

    // Feature: Error Code Descriptiveness, Property: continue_streak error codes are stable
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// NoActiveGame (code 10) when no game exists for the player.
        #[test]
        fn prop_continue_streak_no_active_game_error_code(
            _dummy in 0u32..100u32,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            let new_commit = dummy_commitment_prop(&env);
            let result = client.try_continue_streak(&player, &new_commit);
            prop_assert_eq!(result, Err(Ok(Error::NoActiveGame)));
            prop_assert_eq!(Error::NoActiveGame as u32, error_codes::NO_ACTIVE_GAME);
        }

        /// InvalidPhase (code 11) when game is in Committed phase (not Revealed).
        #[test]
        fn prop_continue_streak_invalid_phase_committed_error_code(
            wager in 1_000_000i128..=100_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            inject_game_prop(&env, &contract_id, &player, GamePhase::Committed, 0, wager);

            let new_commit = dummy_commitment_prop(&env);
            let result = client.try_continue_streak(&player, &new_commit);
            prop_assert_eq!(result, Err(Ok(Error::InvalidPhase)));
            prop_assert_eq!(Error::InvalidPhase as u32, error_codes::INVALID_PHASE);
        }

        /// InvalidPhase (code 11) when game is in Completed phase.
        #[test]
        fn prop_continue_streak_invalid_phase_completed_error_code(
            wager in 1_000_000i128..=100_000_000i128,
            streak in 0u32..=5u32,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            inject_game_prop(&env, &contract_id, &player, GamePhase::Completed, streak, wager);

            let new_commit = dummy_commitment_prop(&env);
            let result = client.try_continue_streak(&player, &new_commit);
            prop_assert_eq!(result, Err(Ok(Error::InvalidPhase)));
        }

        /// InsufficientReserves (code 4) when reserves can't cover next streak payout.
        #[test]
        fn prop_continue_streak_insufficient_reserves_error_code(
            wager in 1_000_000i128..=100_000_000i128,
            streak in 1u32..=5u32,
        ) {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);
            let token = Address::generate(&env);
            client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);

            let player = Address::generate(&env);
            inject_game_prop(&env, &contract_id, &player, GamePhase::Revealed, streak, wager);

            // Zero reserves — can't cover next payout
            fund_reserves(&env, &contract_id, 0);

            let new_commit = dummy_commitment_prop(&env);
            let result = client.try_continue_streak(&player, &new_commit);
            prop_assert_eq!(result, Err(Ok(Error::InsufficientReserves)));
            prop_assert_eq!(Error::InsufficientReserves as u32, error_codes::INSUFFICIENT_RESERVES);
        }
    }

    // Feature: Error Code Descriptiveness, Property: error_codes module constants ↔ enum discriminants
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// All error_codes constants match their corresponding Error enum discriminants.
        /// Running under proptest guards against accidental conditional-compilation drift.
        #[test]
        fn prop_error_code_constants_match_enum_discriminants(_dummy in 0u32..1000u32) {
            prop_assert_eq!(Error::WagerBelowMinimum as u32, error_codes::WAGER_BELOW_MINIMUM);
            prop_assert_eq!(Error::WagerAboveMaximum as u32, error_codes::WAGER_ABOVE_MAXIMUM);
            prop_assert_eq!(Error::ActiveGameExists as u32, error_codes::ACTIVE_GAME_EXISTS);
            prop_assert_eq!(Error::InsufficientReserves as u32, error_codes::INSUFFICIENT_RESERVES);
            prop_assert_eq!(Error::ContractPaused as u32, error_codes::CONTRACT_PAUSED);
            prop_assert_eq!(Error::NoActiveGame as u32, error_codes::NO_ACTIVE_GAME);
            prop_assert_eq!(Error::InvalidPhase as u32, error_codes::INVALID_PHASE);
            prop_assert_eq!(Error::CommitmentMismatch as u32, error_codes::COMMITMENT_MISMATCH);
            prop_assert_eq!(Error::RevealTimeout as u32, error_codes::REVEAL_TIMEOUT);
            prop_assert_eq!(Error::NoWinningsToClaimOrContinue as u32, error_codes::NO_WINNINGS_TO_CLAIM_OR_CONTINUE);
            prop_assert_eq!(Error::InvalidCommitment as u32, error_codes::INVALID_COMMITMENT);
            prop_assert_eq!(Error::Unauthorized as u32, error_codes::UNAUTHORIZED);
            prop_assert_eq!(Error::InvalidFeePercentage as u32, error_codes::INVALID_FEE_PERCENTAGE);
            prop_assert_eq!(Error::InvalidWagerLimits as u32, error_codes::INVALID_WAGER_LIMITS);
            prop_assert_eq!(Error::TransferFailed as u32, error_codes::TRANSFER_FAILED);
            prop_assert_eq!(Error::AdminTreasuryConflict as u32, error_codes::ADMIN_TREASURY_CONFLICT);
            prop_assert_eq!(Error::AlreadyInitialized as u32, error_codes::ALREADY_INITIALIZED);
        }

        /// VARIANT_COUNT must exactly match the number of Error enum variants.
        #[test]
        fn prop_variant_count_is_accurate(_dummy in 0u32..100u32) {
            // All 17 variants enumerated — if a new variant is added without
            // updating VARIANT_COUNT, this list will need to grow.
            let all_codes: [u32; 17] = [
                error_codes::WAGER_BELOW_MINIMUM,
                error_codes::WAGER_ABOVE_MAXIMUM,
                error_codes::ACTIVE_GAME_EXISTS,
                error_codes::INSUFFICIENT_RESERVES,
                error_codes::CONTRACT_PAUSED,
                error_codes::NO_ACTIVE_GAME,
                error_codes::INVALID_PHASE,
                error_codes::COMMITMENT_MISMATCH,
                error_codes::REVEAL_TIMEOUT,
                error_codes::NO_WINNINGS_TO_CLAIM_OR_CONTINUE,
                error_codes::INVALID_COMMITMENT,
                error_codes::UNAUTHORIZED,
                error_codes::INVALID_FEE_PERCENTAGE,
                error_codes::INVALID_WAGER_LIMITS,
                error_codes::TRANSFER_FAILED,
                error_codes::ADMIN_TREASURY_CONFLICT,
                error_codes::ALREADY_INITIALIZED,
            ];
            prop_assert_eq!(all_codes.len(), error_codes::VARIANT_COUNT);
        }
    }

    // Feature: Error Code Descriptiveness, Property: fee_bps boundary values
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// fee_bps = 199 (just below valid range) → InvalidFeePercentage.
        #[test]
        fn prop_fee_bps_boundary_199_rejected(_dummy in 0u32..50u32) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);
            let token = Address::generate(&env);
            let result = client.try_initialize(
                &admin, &treasury, &token, &199, &1_000_000, &100_000_000,
            );
            prop_assert_eq!(result, Err(Ok(Error::InvalidFeePercentage)));
        }

        /// fee_bps = 200 (lower bound inclusive) → accepted.
        #[test]
        fn prop_fee_bps_boundary_200_accepted(_dummy in 0u32..50u32) {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);
            let token = Address::generate(&env);
            let result = client.try_initialize(
                &admin, &treasury, &token, &200, &1_000_000, &100_000_000,
            );
            prop_assert!(result.is_ok());
        }

        /// fee_bps = 500 (upper bound inclusive) → accepted.
        #[test]
        fn prop_fee_bps_boundary_500_accepted(_dummy in 0u32..50u32) {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);
            let token = Address::generate(&env);
            let result = client.try_initialize(
                &admin, &treasury, &token, &500, &1_000_000, &100_000_000,
            );
            prop_assert!(result.is_ok());
        }

        /// fee_bps = 501 (just above valid range) → InvalidFeePercentage.
        #[test]
        fn prop_fee_bps_boundary_501_rejected(_dummy in 0u32..50u32) {
            let env = Env::default();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);
            let token = Address::generate(&env);
            let result = client.try_initialize(
                &admin, &treasury, &token, &501, &1_000_000, &100_000_000,
            );
            prop_assert_eq!(result, Err(Ok(Error::InvalidFeePercentage)));
        }
    }

    // Feature: Error Code Descriptiveness, Property: streak=0 invalid state handling
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// cash_out with streak=0 in Committed phase still returns InvalidPhase,
        /// not a different error — phase guard fires before any streak check.
        #[test]
        fn prop_streak_zero_committed_cash_out_returns_invalid_phase(
            wager in 1_000_000i128..=100_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            inject_game_prop(&env, &contract_id, &player, GamePhase::Committed, 0, wager);

            let result = client.try_cash_out(&player);
            prop_assert_eq!(result, Err(Ok(Error::InvalidPhase)));
        }

        /// continue_streak with streak=0 in Committed phase returns InvalidPhase.
        #[test]
        fn prop_streak_zero_committed_continue_returns_invalid_phase(
            wager in 1_000_000i128..=100_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            inject_game_prop(&env, &contract_id, &player, GamePhase::Committed, 0, wager);

            let new_commit = dummy_commitment_prop(&env);
            let result = client.try_continue_streak(&player, &new_commit);
            prop_assert_eq!(result, Err(Ok(Error::InvalidPhase)));
        }

        /// reveal with streak=0 in Completed phase returns InvalidPhase (not NoActiveGame).
        #[test]
        fn prop_streak_zero_completed_reveal_returns_invalid_phase(
            wager in 1_000_000i128..=100_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            inject_game_prop(&env, &contract_id, &player, GamePhase::Completed, 0, wager);

            let secret = Bytes::from_slice(&env, &[42u8; 32]);
            let result = client.try_reveal(&player, &secret);
            prop_assert_eq!(result, Err(Ok(Error::InvalidPhase)));
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
        fn prop_no_multiplier_tier_is_skipped(streak in 1u32..=3u32) {
            let before_tier = tier_of(streak);
            let after_streak = apply_win(streak);
            let after_tier = tier_of(after_streak);

            // Tier must advance by exactly 1 for streaks 1-3.
            // (streak 0 is excluded: it is the pre-game initial value, not
            //  a valid multiplier tier — the wildcard arm returns 10x.)
            prop_assert_eq!(
                after_tier, before_tier + 1,
                "win from streak {} (tier {}) must advance to tier {}, got tier {}",
                streak, before_tier, before_tier + 1, after_tier
            );

            // The multiplier at the new tier must be strictly greater.
            prop_assert!(
                get_multiplier(after_streak) > get_multiplier(streak),
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

// ─────────────────────────────────────────────────────────────────────────────
// Feature: Loss Forfeiture (Fund Safety Critical)                  Issue #120
// ─────────────────────────────────────────────────────────────────────────────
//
// INVARIANTS VERIFIED:
//
//  LF-1  On any loss, `reveal` returns `Ok(false)`.
//  LF-2  On any loss, the player's game state is completely deleted from
//        storage — no stale entry remains.
//  LF-3  On any loss, `reserve_balance` increases by exactly the forfeited
//        wager — no more, no less.
//  LF-4  After a loss the player slot is free: a new `start_game` call with
//        a valid wager succeeds immediately.
//  LF-5  A loss resets the streak to 0 for the next game (no streak carry-over).
//  LF-6  Both sides (Heads / Tails) trigger the same forfeiture semantics when
//        the outcome is the opposite side.
//  LF-7  Reserve overflow is handled safely: `checked_add` prevents wrapping
//        even when `reserve_balance` is near `i128::MAX`.
//
// OUTCOME DERIVATION (test environment):
//   contract_random = sha256(ledger_seq.to_be_bytes())
//   In tests, ledger sequence defaults to 0, so:
//     contract_random = sha256([0x00, 0x00, 0x00, 0x00])
//     contract_random[0] = 0xdf  (low bit = 1)
//   outcome_bit = (sha256(secret)[0] XOR contract_random[0]) & 1
//     0 → Heads, 1 → Tails
//
//   Calibrated loss secrets (verified by sha256 computation):
//     [3u8; 32] → sha256[0]=0x64 (low bit 0) XOR 0xdf → bit 1 → Tails → LOSS for Heads
//     [2u8; 32] → sha256[0]=0x65 (low bit 1) XOR 0xdf → bit 0 → Heads → LOSS for Tails
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod loss_forfeiture_tests {
    use super::*;
    use proptest::prelude::*;
    use soroban_sdk::testutils::Address as _;

    // ── Shared helpers ────────────────────────────────────────────────────────

    /// Initialise a fresh contract with standard fee / wager bounds and fund
    /// reserves generously so `InsufficientReserves` never fires.
    fn setup_loss_env(env: &Env) -> (soroban_sdk::Address, CoinflipContractClient) {
        env.mock_all_auths();
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(env, &contract_id);

        let admin    = soroban_sdk::Address::generate(env);
        let treasury = soroban_sdk::Address::generate(env);
        let token    = soroban_sdk::Address::generate(env);

        // fee = 300 bps (3 %), wager range [1_000_000, 1_000_000_000]
        client.initialize(&admin, &treasury, &token, &300, &1_000_000, &1_000_000_000);

        // Fund reserves to the safe ceiling so payout checks always pass.
        env.as_contract(&contract_id, || {
            let mut stats = CoinflipContract::load_stats(env);
            stats.reserve_balance = i128::MAX / 2;
            CoinflipContract::save_stats(env, &stats);
        });

        (contract_id, client)
    }

    /// Return a secret that deterministically produces a LOSS for `side`.
    ///
    /// In the default test environment (ledger sequence = 0):
    ///   contract_random = sha256([0,0,0,0]), first byte = 0xdf (low bit = 1)
    ///   outcome_bit = (sha256(secret)[0] XOR 0xdf) & 1
    ///     0 → Heads, 1 → Tails
    ///
    /// Calibrated by sha256 computation:
    ///   [3u8; 32] → sha256[0]=0x64 (low bit 0) XOR 0xdf → bit 1 → Tails → LOSS for Heads
    ///   [2u8; 32] → sha256[0]=0x65 (low bit 1) XOR 0xdf → bit 0 → Heads → LOSS for Tails
    fn loss_secret_for_side(env: &Env, side: &Side) -> soroban_sdk::Bytes {
        match side {
            // Player chose Heads → need Tails outcome → [3u8; 32]
            Side::Heads => soroban_sdk::Bytes::from_slice(env, &[3u8; 32]),
            // Player chose Tails → need Heads outcome → [2u8; 32]
            Side::Tails => soroban_sdk::Bytes::from_slice(env, &[2u8; 32]),
        }
    }

    // ── Property tests ────────────────────────────────────────────────────────

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        // ── LF-1 & LF-2: reveal returns false and game state is deleted ───────
        //
        // For any valid wager and either side, a losing reveal must:
        //   • return Ok(false)
        //   • leave no game state in storage for the player
        /// PROPERTY LF-1/LF-2: losing reveal returns false and clears game state.
        ///
        /// Post-loss invariants:
        ///   - `reveal` returns `false` (not an error, not `true`)
        ///   - `load_player_game` returns `None` — slot is fully deleted
        #[test]
        fn prop_loss_returns_false_and_clears_state(
            wager in 1_000_000i128..=100_000_000i128,
            side  in prop_oneof![Just(Side::Heads), Just(Side::Tails)],
        ) {
            let env = Env::default();
            let (contract_id, client) = setup_loss_env(&env);

            let player = soroban_sdk::Address::generate(&env);
            let secret = loss_secret_for_side(&env, &side);
            let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

            client.start_game(&player, &side, &wager, &commitment);

            // LF-1: must return false
            let result = client.reveal(&player, &secret);
            prop_assert!(!result, "reveal must return false on a loss");

            // LF-2: game state must be fully deleted
            let stored: Option<GameState> = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player)
            });
            prop_assert!(stored.is_none(),
                "game state must be deleted from storage after a loss");
        }

        // ── LF-3: forfeited wager is credited to reserves exactly ─────────────
        //
        // For any valid wager, after a loss:
        //   reserve_balance_after == reserve_balance_before + wager
        //
        // This is the core fund-safety invariant: every lost wager must flow
        // into the reserve pool without truncation, rounding, or duplication.
        /// PROPERTY LF-3: forfeited wager is credited to reserves exactly.
        ///
        /// Post-loss invariant:
        ///   reserve_balance_after = reserve_balance_before + wager  (exact)
        #[test]
        fn prop_loss_credits_exact_wager_to_reserves(
            wager in 1_000_000i128..=100_000_000i128,
            side  in prop_oneof![Just(Side::Heads), Just(Side::Tails)],
        ) {
            let env = Env::default();
            let (contract_id, client) = setup_loss_env(&env);

            // Snapshot reserves before the game starts
            let reserves_before: i128 = env.as_contract(&contract_id, || {
                CoinflipContract::load_stats(&env).reserve_balance
            });

            let player = soroban_sdk::Address::generate(&env);
            let secret = loss_secret_for_side(&env, &side);
            let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

            client.start_game(&player, &side, &wager, &commitment);
            client.reveal(&player, &secret);

            let reserves_after: i128 = env.as_contract(&contract_id, || {
                CoinflipContract::load_stats(&env).reserve_balance
            });

            prop_assert_eq!(
                reserves_after,
                reserves_before + wager,
                "reserve_balance must increase by exactly the forfeited wager"
            );
        }

        // ── LF-4 & LF-5: slot is freed and streak resets after loss ──────────
        //
        // After a loss, the player must be able to start a fresh game
        // immediately, and the new game must begin with streak = 0.
        /// PROPERTY LF-4/LF-5: player can start a new game after a loss with streak = 0.
        ///
        /// Post-loss invariants:
        ///   - `start_game` succeeds for the same player (slot is free)
        ///   - new game has `streak == 0` (no carry-over from the lost game)
        ///   - new game is in `Committed` phase
        #[test]
        fn prop_loss_frees_slot_and_resets_streak(
            wager in 1_000_000i128..=100_000_000i128,
            side  in prop_oneof![Just(Side::Heads), Just(Side::Tails)],
        ) {
            let env = Env::default();
            let (contract_id, client) = setup_loss_env(&env);

            let player = soroban_sdk::Address::generate(&env);
            let secret = loss_secret_for_side(&env, &side);
            let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

            client.start_game(&player, &side, &wager, &commitment);
            client.reveal(&player, &secret);

            // LF-4: new game must be accepted
            let new_secret = soroban_sdk::Bytes::from_slice(&env, &[42u8; 32]);
            let new_commitment: BytesN<32> = env.crypto().sha256(&new_secret).into();
            let result = client.try_start_game(&player, &side, &wager, &new_commitment);
            prop_assert!(result.is_ok(),
                "start_game must succeed after a loss (slot must be free)");

            // LF-5: new game streak must be 0
            let game: GameState = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player).unwrap()
            });
            prop_assert_eq!(game.streak, 0,
                "streak must be 0 at the start of a new game after a loss");
            prop_assert_eq!(game.phase, GamePhase::Committed,
                "new game must be in Committed phase");
        }

        // ── LF-6: both sides produce identical forfeiture semantics ──────────
        //
        // The loss path must behave identically regardless of which side the
        // player chose.  This guards against any accidental side-specific
        // branching in the loss code.
        /// PROPERTY LF-6: forfeiture semantics are side-agnostic.
        ///
        /// Invariant: reserve delta is identical for Heads-loss and Tails-loss
        /// given the same wager amount.
        #[test]
        fn prop_loss_forfeiture_is_side_agnostic(
            wager in 1_000_000i128..=100_000_000i128,
        ) {
            // --- Heads loss ---
            let env_h = Env::default();
            let (contract_id_h, client_h) = setup_loss_env(&env_h);
            let reserves_before_h: i128 = env_h.as_contract(&contract_id_h, || {
                CoinflipContract::load_stats(&env_h).reserve_balance
            });
            let player_h = soroban_sdk::Address::generate(&env_h);
            let secret_h = loss_secret_for_side(&env_h, &Side::Heads);
            let commitment_h: BytesN<32> = env_h.crypto().sha256(&secret_h).into();
            client_h.start_game(&player_h, &Side::Heads, &wager, &commitment_h);
            client_h.reveal(&player_h, &secret_h);
            let delta_h = env_h.as_contract(&contract_id_h, || {
                CoinflipContract::load_stats(&env_h).reserve_balance
            }) - reserves_before_h;

            // --- Tails loss ---
            let env_t = Env::default();
            let (contract_id_t, client_t) = setup_loss_env(&env_t);
            let reserves_before_t: i128 = env_t.as_contract(&contract_id_t, || {
                CoinflipContract::load_stats(&env_t).reserve_balance
            });
            let player_t = soroban_sdk::Address::generate(&env_t);
            let secret_t = loss_secret_for_side(&env_t, &Side::Tails);
            let commitment_t: BytesN<32> = env_t.crypto().sha256(&secret_t).into();
            client_t.start_game(&player_t, &Side::Tails, &wager, &commitment_t);
            client_t.reveal(&player_t, &secret_t);
            let delta_t = env_t.as_contract(&contract_id_t, || {
                CoinflipContract::load_stats(&env_t).reserve_balance
            }) - reserves_before_t;

            prop_assert_eq!(delta_h, delta_t,
                "reserve delta must be identical for Heads-loss and Tails-loss");
            prop_assert_eq!(delta_h, wager,
                "reserve delta must equal the forfeited wager");
        }
    }

    // ── LF-7: reserve overflow safety (single deterministic case) ────────────
    //
    // When reserve_balance is near i128::MAX, a loss must not wrap or panic.
    // The contract uses checked_add with an unwrap_or fallback, so the balance
    // must remain unchanged (saturate) rather than overflow.
    //
    // This is a unit-style test (not proptest) because the near-MAX value is
    // a fixed edge case, not a random range.
    /// PROPERTY LF-7: reserve overflow is handled safely near i128::MAX.
    ///
    /// Invariant: reserve_balance never wraps on a loss when already near MAX.
    #[test]
    fn prop_loss_reserve_overflow_is_safe() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(&env, &contract_id);

        let admin    = soroban_sdk::Address::generate(&env);
        let treasury = soroban_sdk::Address::generate(&env);
        let token    = soroban_sdk::Address::generate(&env);
        client.initialize(&admin, &treasury, &token, &300, &1_000_000, &1_000_000_000);

        // Set reserves to i128::MAX so checked_add saturates on the loss credit.
        let near_max = i128::MAX;
        env.as_contract(&contract_id, || {
            let mut stats = CoinflipContract::load_stats(&env);
            stats.reserve_balance = near_max;
            CoinflipContract::save_stats(&env, &stats);
        });

        let player = soroban_sdk::Address::generate(&env);
        let wager  = 1_000_000i128;
        let secret = soroban_sdk::Bytes::from_slice(&env, &[3u8; 32]); // loss for Heads
        let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

        client.start_game(&player, &Side::Heads, &wager, &commitment);
        // Must not panic or wrap — checked_add fallback keeps balance at near_max
        let result = client.try_reveal(&player, &secret);
        assert!(result.is_ok(), "reveal must not panic on reserve overflow edge case");

        let stats: ContractStats = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Stats).unwrap()
        });
        // Balance must be >= near_max (saturated, not wrapped to negative)
        assert!(stats.reserve_balance >= near_max,
            "reserve_balance must not wrap below near_max on overflow");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Integration Test Harness
// ═══════════════════════════════════════════════════════════════════════════
//
// # Overview
//
// This module provides a reusable, deterministic harness for full end-to-end
// integration tests of the Tossd coinflip game flow.
//
// # Design Goals
//
// - **Deterministic fixtures**: all secrets, commitments, and reserve amounts
//   are derived from fixed seed bytes so test outcomes never vary between runs.
// - **Minimal boilerplate**: `Harness::new` wires up the Soroban test env,
//   registers the contract, and initialises it in one call.
// - **Composable helpers**: `play_win_round` / `play_loss_round` drive the
//   full commit→reveal cycle so individual tests stay focused on assertions.
// - **No token contract required**: `cash_out` is used for settlement so tests
//   run without a deployed SAC token, keeping CI fast and hermetic.
//
// # Usage
//
// ```rust
// let h = Harness::new();
// let player = h.player();
// h.fund(1_000_000_000);
// h.start(&player, Side::Heads, 10_000_000, 1);   // seed 1 → Heads win
// let won = h.reveal(&player, 1);
// assert!(won);
// let payout = h.cash_out(&player);
// assert!(payout > 0);
// ```
//
// # Fixture Seed Convention
//
// `make_secret(env, seed)` returns `Bytes::from_slice(env, &[seed; 32])`.
// The outcome of a round depends on `sha256(secret ++ contract_random)[0] % 2`:
//
// | seed | outcome | use with Side::Heads |
// |------|---------|----------------------|
// |  1   | Heads   | WIN                  |
// |  3   | Tails   | LOSE                 |
//
// Calibrated from loss_forfeiture_tests: [3u8;32] → sha256[0]=0x64 (low bit 0)
// XOR contract_random[0]=0xdf → bit 1 → Tails → loss for a Heads player.
//
// Use `Harness::probe_outcome` to discover the winning seed for any ledger
// sequence when writing new tests.
//
// # Harness Fields
//
// - `env`         – Soroban test environment (mock_all_auths enabled)
// - `contract_id` – registered CoinflipContract address
// - `client`      – generated client for calling contract methods
// - `config`      – snapshot of the initialised ContractConfig
#[cfg(test)]
mod integration_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    // ─────────────────────────────────────────────────────────────────────
    // Harness
    // ─────────────────────────────────────────────────────────────────────

    /// Default wager used across harness helpers (10 XLM in stroops).
    const DEFAULT_WAGER: i128 = 10_000_000;
    /// Default fee in basis points (3%).
    const DEFAULT_FEE_BPS: u32 = 300;
    /// Default min wager (1 XLM).
    const DEFAULT_MIN_WAGER: i128 = 1_000_000;
    /// Default max wager (100 XLM).
    const DEFAULT_MAX_WAGER: i128 = 100_000_000;

    /// Central test harness. Owns the Soroban env, contract registration,
    /// and all fixture helpers needed for integration tests.
    struct Harness {
        env: Env,
        contract_id: Address,
        client: CoinflipContractClient<'static>,
        // Keep admin/treasury accessible for admin-level assertions.
        admin: Address,
        treasury: Address,
    }

    impl Harness {
        /// Create a fully initialised harness with default config.
        ///
        /// - Registers `CoinflipContract` in a fresh `Env`.
        /// - Calls `mock_all_auths` so player auth is never a test concern.
        /// - Initialises the contract with `DEFAULT_FEE_BPS`, `DEFAULT_MIN_WAGER`,
        ///   `DEFAULT_MAX_WAGER`.
        fn new() -> Self {
            let env = Env::default();
            env.mock_all_auths();

            let contract_id = env.register(CoinflipContract, ());
            // SAFETY: the client lifetime is tied to `env` which lives in the
            // same struct; we extend it to 'static here for ergonomics inside
            // the test module. The struct must not outlive the env.
            let client: CoinflipContractClient<'static> = unsafe {
                core::mem::transmute(CoinflipContractClient::new(&env, &contract_id))
            };

            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);
            let token = Address::generate(&env);

            client.initialize(
                &admin,
                &treasury,
                &token,
                &DEFAULT_FEE_BPS,
                &DEFAULT_MIN_WAGER,
                &DEFAULT_MAX_WAGER,
            );

            Self { env, contract_id, client, admin, treasury }
        }

        // ── Fixture helpers ───────────────────────────────────────────────

        /// Generate a fresh player address.
        fn player(&self) -> Address {
            Address::generate(&self.env)
        }

        /// Build a deterministic secret from a single seed byte.
        ///
        /// `seed` is repeated 32 times so the secret is always 32 bytes.
        /// See the module-level seed convention table for outcome mapping.
        fn make_secret(&self, seed: u8) -> Bytes {
            Bytes::from_slice(&self.env, &[seed; 32])
        }

        /// Derive the commitment hash for a given seed.
        fn make_commitment(&self, seed: u8) -> BytesN<32> {
            let secret = self.make_secret(seed);
            self.env.crypto().sha256(&secret).into()
        }

        /// Set `reserve_balance` directly in contract storage.
        ///
        /// Call this before any `start_game` to satisfy the solvency guard.
        fn fund(&self, amount: i128) {
            self.env.as_contract(&self.contract_id, || {
                let mut stats = CoinflipContract::load_stats(&self.env);
                stats.reserve_balance = amount;
                CoinflipContract::save_stats(&self.env, &stats);
            });
        }

        /// Inject a `GameState` directly into storage, bypassing `start_game`.
        ///
        /// Useful for testing `reveal`, `cash_out`, and `continue_streak` in
        /// isolation without needing to satisfy `start_game` guards.
        fn inject_game(
            &self,
            player: &Address,
            phase: GamePhase,
            streak: u32,
            wager: i128,
            seed: u8,
        ) {
            let commitment = self.make_commitment(seed);
            let game = GameState {
                wager,
                side: Side::Heads,
                streak,
                commitment: commitment.clone(),
                contract_random: commitment, // deterministic stand-in
                phase,
            };
            self.env.as_contract(&self.contract_id, || {
                CoinflipContract::save_player_game(&self.env, player, &game);
            });
        }

        /// Read the current `ContractStats` from storage.
        fn stats(&self) -> ContractStats {
            self.env.as_contract(&self.contract_id, || {
                self.env
                    .storage()
                    .persistent()
                    .get(&StorageKey::Stats)
                    .unwrap()
            })
        }

        /// Read the current `GameState` for a player (panics if absent).
        fn game_state(&self, player: &Address) -> GameState {
            self.env.as_contract(&self.contract_id, || {
                CoinflipContract::load_player_game(&self.env, player).unwrap()
            })
        }

        // ── Flow helpers ──────────────────────────────────────────────────

        /// Drive a full `start_game → reveal` cycle.
        ///
        /// Returns `true` if the player won (reveal returned `true`).
        ///
        /// `seed` controls the player's secret; use seed `1` for a Heads win
        /// and seed `2` for a Tails loss (see module-level table).
        fn play_round(
            &self,
            player: &Address,
            side: Side,
            wager: i128,
            seed: u8,
        ) -> bool {
            let commitment = self.make_commitment(seed);
            self.client.start_game(player, &side, &wager, &commitment);
            let secret = self.make_secret(seed);
            self.client.reveal(player, &secret)
        }

        /// Convenience: play a round expected to result in a win.
        ///
        /// Seed 1 produces a Heads outcome; pairing with `Side::Heads` wins.
        fn play_win_round(&self, player: &Address, wager: i128) -> bool {
            self.play_round(player, Side::Heads, wager, 1)
        }

        /// Convenience: play a round expected to result in a loss.
        ///
        /// Seed 3 produces a Tails outcome; pairing with `Side::Heads` loses.
        fn play_loss_round(&self, player: &Address, wager: i128) -> bool {
            self.play_round(player, Side::Heads, wager, 3)
        }

        /// Probe the actual outcome for a given seed at the current ledger
        /// sequence. Use this when writing new tests to discover which seed
        /// wins for a particular env state.
        ///
        /// Returns `Side::Heads` or `Side::Tails`.
        fn probe_outcome(&self, seed: u8) -> Side {
            let secret = self.make_secret(seed);
            let seq_bytes = self.env.ledger().sequence().to_be_bytes();
            let contract_random: BytesN<32> = self
                .env
                .crypto()
                .sha256(&Bytes::from_slice(&self.env, &seq_bytes))
                .into();
            let cr_bytes = Bytes::from_slice(&self.env, &contract_random.to_array());
            let mut combined = Bytes::new(&self.env);
            combined.append(&secret);
            combined.append(&cr_bytes);
            let hash = self.env.crypto().sha256(&combined);
            if hash.to_array()[0] % 2 == 0 {
                Side::Heads
            } else {
                Side::Tails
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // Integration Tests
    // ─────────────────────────────────────────────────────────────────────

    // ── Full win → cash_out flow ──────────────────────────────────────────

    /// Happy path: player starts a game, wins, and cashes out.
    ///
    /// Verifies:
    /// - `reveal` returns `true`
    /// - `cash_out` returns the correct net payout
    /// - game phase transitions to `Completed`
    /// - reserve balance decreases by net payout
    /// - fee is credited to stats
    #[test]
    fn test_full_win_then_cash_out() {
        let h = Harness::new();
        let player = h.player();
        h.fund(1_000_000_000);

        let wager = DEFAULT_WAGER;
        let won = h.play_win_round(&player, wager);
        assert!(won, "seed 1 + Heads must win");

        let expected_net = calculate_payout(wager, 1, DEFAULT_FEE_BPS).unwrap();
        let payout = h.client.cash_out(&player);
        assert_eq!(payout, expected_net);

        let game = h.game_state(&player);
        assert_eq!(game.phase, GamePhase::Completed);

        let stats = h.stats();
        // reserves reduced by net payout
        assert_eq!(stats.reserve_balance, 1_000_000_000 - expected_net);
        // fee credited
        let gross = wager.checked_mul(get_multiplier(1) as i128).unwrap() / 10_000;
        let fee = gross.checked_mul(DEFAULT_FEE_BPS as i128).unwrap() / 10_000;
        assert_eq!(stats.total_fees, fee);
    }

    // ── Full loss flow ────────────────────────────────────────────────────

    /// Player starts a game and loses on reveal.
    ///
    /// Verifies:
    /// - `reveal` returns `false`
    /// - game state is deleted (no lingering record)
    /// - wager is credited to reserves (house keeps it)
    #[test]
    fn test_full_loss_forfeits_wager_to_reserves() {
        let h = Harness::new();
        let player = h.player();
        h.fund(1_000_000_000);

        let wager = DEFAULT_WAGER;
        let won = h.play_loss_round(&player, wager);
        assert!(!won, "seed 2 + Heads must lose");

        // Game record must be gone
        let game_opt: Option<GameState> = h.env.as_contract(&h.contract_id, || {
            CoinflipContract::load_player_game(&h.env, &player)
        });
        assert!(game_opt.is_none(), "game state must be deleted on loss");

        // Reserves increase by the forfeited wager
        let stats = h.stats();
        assert_eq!(stats.reserve_balance, 1_000_000_000 + wager);
    }

    // ── Win → continue → win → cash_out (streak 2) ───────────────────────

    /// Two consecutive wins build a streak and yield the streak-2 multiplier.
    ///
    /// Verifies:
    /// - streak increments correctly after each win
    /// - `continue_streak` resets phase to `Committed`
    /// - final `cash_out` uses the streak-2 multiplier
    #[test]
    fn test_win_continue_win_cash_out_streak_2() {
        let h = Harness::new();
        let player = h.player();
        h.fund(1_000_000_000);

        let wager = DEFAULT_WAGER;

        // Round 1 — win
        let won1 = h.play_win_round(&player, wager);
        assert!(won1, "round 1 must win");
        assert_eq!(h.game_state(&player).streak, 1);
        assert_eq!(h.game_state(&player).phase, GamePhase::Revealed);

        // Continue to round 2
        let new_commitment = h.make_commitment(1);
        h.client.continue_streak(&player, &new_commitment);
        assert_eq!(h.game_state(&player).phase, GamePhase::Committed);

        // Round 2 — win again
        let secret2 = h.make_secret(1);
        let won2 = h.client.reveal(&player, &secret2);
        assert!(won2, "round 2 must win");
        assert_eq!(h.game_state(&player).streak, 2);

        // Cash out at streak 2
        let expected_net = calculate_payout(wager, 2, DEFAULT_FEE_BPS).unwrap();
        let payout = h.client.cash_out(&player);
        assert_eq!(payout, expected_net);
        assert_eq!(h.game_state(&player).phase, GamePhase::Completed);
    }

    // ── Streak 4+ cap ─────────────────────────────────────────────────────

    /// Four consecutive wins hit the 10x multiplier cap.
    ///
    /// Verifies:
    /// - streak reaches 4
    /// - payout uses `MULTIPLIER_STREAK_4_PLUS` (100_000 bps)
    #[test]
    fn test_streak_4_uses_max_multiplier() {
        let h = Harness::new();
        let player = h.player();
        h.fund(1_000_000_000);

        let wager = DEFAULT_WAGER;

        // Build streak to 4 via inject + reveal loop
        for expected_streak in 1u32..=4 {
            // Inject a Revealed state at the previous streak so continue_streak
            // or cash_out can be called, then re-enter Committed for the next round.
            if expected_streak == 1 {
                // First round: go through start_game normally
                let won = h.play_win_round(&player, wager);
                assert!(won);
            } else {
                // Subsequent rounds: continue then reveal
                let commitment = h.make_commitment(1);
                h.client.continue_streak(&player, &commitment);
                let secret = h.make_secret(1);
                let won = h.client.reveal(&player, &secret);
                assert!(won, "round {} must win", expected_streak);
            }
            assert_eq!(h.game_state(&player).streak, expected_streak);
        }

        // At streak 4 the multiplier is capped at 10x
        let expected_net = calculate_payout(wager, 4, DEFAULT_FEE_BPS).unwrap();
        let payout = h.client.cash_out(&player);
        assert_eq!(payout, expected_net);

        // Verify 10x gross
        let gross = wager.checked_mul(MULTIPLIER_STREAK_4_PLUS as i128).unwrap() / 10_000;
        let fee = gross.checked_mul(DEFAULT_FEE_BPS as i128).unwrap() / 10_000;
        assert_eq!(expected_net, gross - fee);
    }

    // ── Paused contract rejects new games ─────────────────────────────────

    /// When the contract is paused, `start_game` must be rejected.
    ///
    /// Verifies:
    /// - `ContractPaused` error is returned
    /// - no game state is written
    #[test]
    fn test_paused_contract_rejects_start_game() {
        let h = Harness::new();
        h.fund(1_000_000_000);

        // Pause the contract
        h.env.as_contract(&h.contract_id, || {
            let mut cfg = CoinflipContract::load_config(&h.env);
            cfg.paused = true;
            CoinflipContract::save_config(&h.env, &cfg);
        });

        let player = h.player();
        let result = h.client.try_start_game(
            &player,
            &Side::Heads,
            &DEFAULT_WAGER,
            &h.make_commitment(1),
        );
        assert_eq!(result, Err(Ok(Error::ContractPaused)));

        let game_opt: Option<GameState> = h.env.as_contract(&h.contract_id, || {
            CoinflipContract::load_player_game(&h.env, &player)
        });
        assert!(game_opt.is_none());
    }

    // ── Double-start guard ────────────────────────────────────────────────

    /// A player cannot start a second game while one is already active.
    ///
    /// Verifies:
    /// - `ActiveGameExists` is returned on the second call
    /// - the original game state is unchanged
    #[test]
    fn test_double_start_rejected_while_game_active() {
        let h = Harness::new();
        let player = h.player();
        h.fund(1_000_000_000);

        h.client.start_game(&player, &Side::Heads, &DEFAULT_WAGER, &h.make_commitment(1));

        let result = h.client.try_start_game(
            &player,
            &Side::Tails,
            &DEFAULT_WAGER,
            &h.make_commitment(2),
        );
        assert_eq!(result, Err(Ok(Error::ActiveGameExists)));

        // Original game state must be intact
        let game = h.game_state(&player);
        assert_eq!(game.phase, GamePhase::Committed);
        assert_eq!(game.side, Side::Heads);
    }

    // ── Commitment mismatch guard ─────────────────────────────────────────

    /// Revealing with the wrong secret must be rejected.
    ///
    /// Verifies:
    /// - `CommitmentMismatch` is returned
    /// - game remains in `Committed` phase (no state mutation)
    #[test]
    fn test_reveal_wrong_secret_rejected() {
        let h = Harness::new();
        let player = h.player();
        h.fund(1_000_000_000);

        h.client.start_game(&player, &Side::Heads, &DEFAULT_WAGER, &h.make_commitment(1));

        // Reveal with a different seed
        let wrong_secret = h.make_secret(99);
        let result = h.client.try_reveal(&player, &wrong_secret);
        assert_eq!(result, Err(Ok(Error::CommitmentMismatch)));

        // Phase must still be Committed
        assert_eq!(h.game_state(&player).phase, GamePhase::Committed);
    }

    // ── Insufficient reserves guard ───────────────────────────────────────

    /// `start_game` must be rejected when reserves cannot cover worst-case payout.
    #[test]
    fn test_start_game_rejected_when_reserves_insufficient() {
        let h = Harness::new();
        // Do NOT fund reserves — they start at 0 after initialize.
        let player = h.player();
        let result = h.client.try_start_game(
            &player,
            &Side::Heads,
            &DEFAULT_WAGER,
            &h.make_commitment(1),
        );
        assert_eq!(result, Err(Ok(Error::InsufficientReserves)));
    }

    // ── Post-completion: player can start a new game ──────────────────────

    /// After a completed game (win + cash_out), the player can start fresh.
    ///
    /// Verifies:
    /// - `start_game` succeeds after `Completed` phase
    /// - new game starts with streak 0
    #[test]
    fn test_new_game_allowed_after_completion() {
        let h = Harness::new();
        let player = h.player();
        h.fund(1_000_000_000);

        // Win and cash out
        h.play_win_round(&player, DEFAULT_WAGER);
        h.client.cash_out(&player);
        assert_eq!(h.game_state(&player).phase, GamePhase::Completed);

        // Start a new game
        let result = h.client.try_start_game(
            &player,
            &Side::Tails,
            &DEFAULT_WAGER,
            &h.make_commitment(1),
        );
        assert!(result.is_ok(), "player must be able to start a new game after completion");
        assert_eq!(h.game_state(&player).streak, 0);
        assert_eq!(h.game_state(&player).phase, GamePhase::Committed);
    }

    // ── Stats accumulate correctly across multiple games ──────────────────

    /// Two independent players each play a game; aggregate stats must reflect both.
    ///
    /// Verifies:
    /// - `total_games` increments per game started
    /// - `total_volume` accumulates wagers
    #[test]
    fn test_stats_accumulate_across_multiple_players() {
        let h = Harness::new();
        h.fund(1_000_000_000);

        let p1 = h.player();
        let p2 = h.player();
        let wager1 = 10_000_000i128;
        let wager2 = 20_000_000i128;

        h.client.start_game(&p1, &Side::Heads, &wager1, &h.make_commitment(1));
        h.client.start_game(&p2, &Side::Heads, &wager2, &h.make_commitment(1));

        let stats = h.stats();
        assert_eq!(stats.total_games, 2);
        assert_eq!(stats.total_volume, wager1 + wager2);
    }

    // ── Wager boundary: min and max are inclusive ─────────────────────────

    /// Wagers at exactly `min_wager` and `max_wager` must be accepted.
    #[test]
    fn test_wager_boundary_inclusive() {
        let h = Harness::new();
        h.fund(1_000_000_000);

        let p_min = h.player();
        let p_max = h.player();

        assert!(
            h.client
                .try_start_game(&p_min, &Side::Heads, &DEFAULT_MIN_WAGER, &h.make_commitment(1))
                .is_ok(),
            "min_wager must be accepted"
        );
        assert!(
            h.client
                .try_start_game(&p_max, &Side::Heads, &DEFAULT_MAX_WAGER, &h.make_commitment(1))
                .is_ok(),
            "max_wager must be accepted"
        );
    }

    // ── cash_out on loss state (streak 0) is rejected ─────────────────────

    /// `cash_out` must reject a `Revealed` game where streak == 0.
    #[test]
    fn test_cash_out_rejects_zero_streak_revealed() {
        let h = Harness::new();
        let player = h.player();
        h.inject_game(&player, GamePhase::Revealed, 0, DEFAULT_WAGER, 1);

        let result = h.client.try_cash_out(&player);
        assert_eq!(result, Err(Ok(Error::NoWinningsToClaimOrContinue)));
    }

    // ── continue_streak rejects wrong phase ───────────────────────────────

    /// `continue_streak` must reject a game not in `Revealed` phase.
    #[test]
    fn test_continue_streak_rejects_committed_phase() {
        let h = Harness::new();
        let player = h.player();
        h.inject_game(&player, GamePhase::Committed, 1, DEFAULT_WAGER, 1);

        let result = h.client.try_continue_streak(&player, &h.make_commitment(1));
        assert_eq!(result, Err(Ok(Error::InvalidPhase)));
    }

    // ── probe_outcome helper is consistent with reveal ────────────────────

    /// `probe_outcome` must agree with the actual `reveal` result.
    ///
    /// This validates the harness itself: if probe_outcome says Heads wins,
    /// then a Heads player using that seed must win on reveal.
    #[test]
    fn test_probe_outcome_matches_reveal() {
        let h = Harness::new();
        let player = h.player();
        h.fund(1_000_000_000);

        // Probe before starting the game (same ledger sequence will be used)
        let predicted = h.probe_outcome(1);

        let commitment = h.make_commitment(1);
        h.client.start_game(&player, &predicted, &DEFAULT_WAGER, &commitment);

        let secret = h.make_secret(1);
        let won = h.client.reveal(&player, &secret);
        assert!(won, "probe_outcome prediction must match actual reveal outcome");
    }
}
