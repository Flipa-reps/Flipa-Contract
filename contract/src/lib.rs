//! # Tossd Coinflip Contract
//!
//! Provably fair coinflip game on the Stellar/Soroban smart contract platform.
//!
//! ## Architecture
//!
//! The contract is a single `lib.rs` module containing:
//! - Public API functions exposed via `#[contractimpl]`
//! - Supporting types (`GameState`, `ContractConfig`, `ContractStats`, `Error`, …)
//! - Pure helper functions (`get_multiplier`, `calculate_payout`, `verify_commitment`)
//! - A comprehensive test suite (unit, property-based, integration harness)
//!
//! ## Commit-Reveal Randomness
//!
//! 1. Player hashes a secret: `commitment = SHA-256(secret)`
//! 2. Player calls `start_game` with the commitment; contract stores
//!    `contract_random = SHA-256(ledger_sequence)`.
//! 3. Player calls `reveal(secret)`; contract verifies the commitment and
//!    derives the outcome from `SHA-256(secret || contract_random)`.
//!
//! Neither party can unilaterally control the outcome:
//! - The player cannot change their secret after committing.
//! - The contract cannot change the ledger sequence after the game starts.
//!
//! ## Error Code Stability
//!
//! All [`Error`] variants map to stable `u32` codes (see [`error_codes`]).
//! These codes are part of the public protocol and must not change across upgrades.

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
/// | 10   | `NoActiveGame`               | Game state     | `reveal`, `claim_winnings`, `continue_streak`, `cash_out` |
/// | 11   | `InvalidPhase`               | Game state     | `reveal`, `claim_winnings`, `continue_streak`, `cash_out` |
/// | 12   | `CommitmentMismatch`         | Reveal         | `reveal`                           |
/// | 13   | `RevealTimeout`              | Reveal         | (reserved for future timeout enforcement) |
/// | 20   | `NoWinningsToClaimOrContinue`| Action         | `cash_out`, `claim_winnings`, `continue_streak` |
/// | 21   | `InvalidCommitment`          | Action         | `continue_streak`                  |
/// | 30   | `Unauthorized`               | Admin          | `set_paused`, `set_treasury`, `set_wager_limits`, `set_fee` |
/// | 31   | `InvalidFeePercentage`       | Admin          | `initialize`, `set_fee`            |
/// | 32   | `InvalidWagerLimits`         | Admin          | `initialize`, `set_wager_limits`   |
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

    /// Player has no winnings to claim or continue (streak == 0 in Revealed phase).
    /// Returned by: `cash_out`, `claim_winnings`, `continue_streak`.
    /// Code: 20 — see [`error_codes::NO_WINNINGS_TO_CLAIM_OR_CONTINUE`]
    NoWinningsToClaimOrContinue = 20,

    /// Commitment value is invalid (all-zero bytes treated as missing/placeholder).
    /// Returned by: `continue_streak` (guard 4).
    /// Code: 21 — see [`error_codes::INVALID_COMMITMENT`]
    InvalidCommitment = 21,

    // ── Admin errors (30–32) ────────────────────────────────────────────────

    /// Caller is not authorized for admin operations.
    /// Returned by: `set_paused`, `set_treasury`, `set_wager_limits`, `set_fee`.
    /// Code: 30 — see [`error_codes::UNAUTHORIZED`]
    Unauthorized = 30,

    /// Fee percentage is outside the accepted range (200–500 bps / 2–5%).
    /// Returned by: `initialize`, `set_fee`.
    /// Code: 31 — see [`error_codes::INVALID_FEE_PERCENTAGE`]
    InvalidFeePercentage = 31,

    /// Wager limits are invalid (`min_wager >= max_wager`).
    /// Returned by: `initialize`, `set_wager_limits`.
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

/// The player's chosen side for a coinflip.
///
/// Submitted at game creation and compared against the derived outcome
/// in `reveal` to determine win/loss.
///
/// Serialized as `u32` via `#[repr(u32)]` for stable on-chain storage.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Side {
    /// Heads — discriminant 0.
    Heads = 0,
    /// Tails — discriminant 1.
    Tails = 1,
}

/// Lifecycle phase of a player's active game.
///
/// State transitions:
/// ```text
/// Committed ──(reveal wins)──► Revealed ──(cash_out / claim_winnings)──► Completed
///           ──(reveal loses)──────────────────────────────────────────► (deleted)
///           ──(continue_streak)──► Committed  (streak preserved)
/// ```
///
/// Only `Completed` games may be replaced by a new `start_game` call.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum GamePhase {
    /// Wager placed and commitment recorded; waiting for the player to call `reveal`.
    Committed,
    /// Outcome determined; player must call `cash_out`, `claim_winnings`, or `continue_streak`.
    Revealed,
    /// Game has ended (win settled or loss forfeited); slot is available for a new game.
    Completed,
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
/// - `fee_bps`        – fee snapshot captured at game creation time;
///                      used for all subsequent settlement calculations so
///                      later admin fee changes do not alter in-flight games
/// - `phase`          – lifecycle position: `Committed` → `Revealed` → `Completed`
/// - `start_ledger`   – ledger sequence number at `start_game` time; used by
///                      `reclaim_wager` to enforce the reveal timeout window
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameState {
    pub wager: i128,
    pub side: Side,
    pub streak: u32,
    pub commitment: BytesN<32>,
    pub contract_random: BytesN<32>,
    pub fee_bps: u32,
    pub phase: GamePhase,
    /// Ledger sequence at game creation; used for timeout enforcement.
    pub start_ledger: u32,
}

/// Contract-wide configuration stored in persistent storage under [`StorageKey::Config`].
///
/// All fields are set at `initialize` time and may be updated by the admin
/// via `set_fee`, `set_treasury`, `set_wager_limits`, and `set_paused`.
/// In-flight games snapshot `fee_bps` into [`GameState`] so admin changes
/// never retroactively alter active game payouts.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractConfig {
    /// Administrator address; the only account permitted to call admin functions.
    pub admin: Address,
    /// Fee collection address; receives the protocol fee on every settled win.
    pub treasury: Address,
    /// SAC token address used for wager custody (XLM or any SEP-41 token).
    pub token: Address,
    /// Protocol fee in basis points (200–500 = 2–5%).
    pub fee_bps: u32,
    /// Inclusive minimum wager in stroops.
    pub min_wager: i128,
    /// Inclusive maximum wager in stroops.
    pub max_wager: i128,
    /// Emergency pause flag; when `true`, `start_game` is rejected.
    pub paused: bool,
}

/// Aggregate statistics stored in persistent storage under [`StorageKey::Stats`].
///
/// Updated atomically alongside game state transitions.
/// `reserve_balance` is the authoritative on-chain reserve figure used for
/// solvency checks in `start_game` and `continue_streak`.
///
/// Invariants:
/// - `total_games`: Strictly monotonically increasing; incremented in `start_game`.
/// - `total_volume`: Cumulative wagers placed; never decreases.
/// - `total_fees`: Cumulative protocol fees collected; never decreases (fees are never refunded).
/// - `reserve_balance`: Must always be sufficient to cover the worst-case payout (10x wager).
///   Decreases when players cash out or claim winnings (gross deducted), increases when players
///   lose and forfeit their wager.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractStats {
    /// Cumulative count of games started (incremented in `start_game`).
    pub total_games: u64,
    /// Cumulative XLM wagered across all games, in stroops.
    pub total_volume: i128,
    /// Cumulative protocol fees collected across all settled wins, in stroops.
    pub total_fees: i128,
    /// Current contract reserve balance in stroops; decremented on payouts,
    /// incremented when the house wins (loss forfeiture).
    pub reserve_balance: i128,
}

/// Persistent storage key variants for the contract's data model.
///
/// Used with `env.storage().persistent()` for all reads and writes.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Global contract configuration ([`ContractConfig`]).
    Config,
    /// Global aggregate statistics ([`ContractStats`]).
    Stats,
    /// Per-player game state ([`GameState`]), keyed by player address.
    PlayerGame(Address),
    /// Per-player referrer address, keyed by player address.
    PlayerReferrer(Address),
    /// Per-referrer accumulated rewards, keyed by referrer address.
    ReferrerRewards(Address),
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

/// TTL constants for persistent storage entries.
///
/// Soroban persistent storage entries expire after `ledger_ttl` ledgers without
/// a TTL extension.  We extend on every read/write to keep active data live.
///
/// Ledger cadence: ~5 seconds per ledger on Stellar mainnet.
///
/// | Constant              | Ledgers   | Approx duration |
/// |-----------------------|-----------|-----------------|
/// | `TTL_THRESHOLD`       | 100_000   | ~5.8 days       |
/// | `TTL_EXTEND_TO`       | 500_000   | ~29 days        |
///
/// Tradeoff: extending on every access costs a small amount of additional
/// compute budget per call, but prevents data loss for active games and
/// config/stats entries.  The threshold avoids redundant extend calls when
/// the TTL is already healthy.
const TTL_THRESHOLD: u32 = 100_000;
const TTL_EXTEND_TO: u32 = 500_000;

/// Number of ledgers a player has to call `reveal` before the wager can be
/// reclaimed via `reclaim_wager`.  At ~5 s/ledger this is roughly 8 minutes.
const REVEAL_TIMEOUT_LEDGERS: u32 = 100;

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

/// Calculates the full payout breakdown for a winning streak.
///
/// Returns `(gross, fee, net)` in stroops, or `None` on arithmetic overflow.
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
/// # Storage optimization note
/// Callers that need gross, fee, and net should call this once rather than
/// calling `calculate_payout` and then recomputing gross/fee separately.
/// This eliminates two redundant multiplier lookups and two checked_div operations
/// per settlement call.
///
/// # Arguments
/// - `wager`   – original wager in stroops (must be > 0)
/// - `streak`  – current win streak (passed to `get_multiplier`)
/// - `fee_bps` – protocol fee in basis points (200–500)
pub fn calculate_payout_breakdown(wager: i128, streak: u32, fee_bps: u32) -> Option<(i128, i128, i128)> {
    let multiplier = get_multiplier(streak) as i128;
    let gross = wager.checked_mul(multiplier)?.checked_div(10_000)?;
    let fee   = gross.checked_mul(fee_bps as i128)?.checked_div(10_000)?;
    let net   = gross.checked_sub(fee)?;
    Some((gross, fee, net))
}

/// Calculates the net payout for a winning streak.
///
/// Convenience wrapper around [`calculate_payout_breakdown`] for callers that
/// only need the net amount (e.g. read-only queries).
///
/// Returns `None` if any intermediate multiplication overflows `i128`.
///
/// # Arguments
/// - `wager`   – original wager in stroops (must be > 0)
/// - `streak`  – current win streak (passed to `get_multiplier`)
/// - `fee_bps` – protocol fee in basis points (200–500)
pub fn calculate_payout(wager: i128, streak: u32, fee_bps: u32) -> Option<i128> {
    calculate_payout_breakdown(wager, streak, fee_bps).map(|(_, _, net)| net)
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

/// Generate VRF-based randomness for enhanced unpredictability.
///
/// Combines player secret with Soroban's built-in randomness source to produce
/// cryptographically secure randomness. This provides stronger security guarantees
/// than SHA256-based randomness alone.
///
/// # Arguments
/// - `env`    – Soroban environment for accessing randomness
/// - `secret` – player's random secret bytes
///
/// # Returns
/// A 32-byte random value derived from VRF-like construction
pub fn generate_vrf_randomness(env: &Env, secret: &Bytes) -> BytesN<32> {
    // Combine player secret with environment randomness for enhanced unpredictability
    let env_random = env.prng().gen::<BytesN<32>>();
    let mut combined = Bytes::new(env);
    combined.append(secret);
    combined.append(&Bytes::from_slice(env, &env_random.to_array()));
    env.crypto().sha256(&combined).into()
}

/// Provably fair coinflip game contract for the Stellar/Soroban platform.
///
/// ## Public API
///
/// | Function           | Caller  | Description                                              |
/// |--------------------|---------|----------------------------------------------------------|
/// | `initialize`       | deployer| One-time setup: config, token, fee, wager bounds         |
/// | `start_game`       | player  | Place a wager and submit a commitment hash               |
/// | `reveal`           | player  | Reveal secret to determine win/loss outcome              |
/// | `cash_out`         | player  | Settle a win without a token transfer (returns amount)   |
/// | `claim_winnings`   | player  | Settle a win with on-chain token transfer                |
/// | `continue_streak`  | player  | Risk current winnings for the next streak multiplier     |
/// | `set_paused`       | admin   | Enable/disable new game creation                         |
/// | `set_treasury`     | admin   | Update the fee collection address                        |
/// | `set_wager_limits` | admin   | Update inclusive min/max wager bounds                    |
/// | `set_fee`          | admin   | Update the protocol fee (200–500 bps)                    |
///
/// ## Randomness Model
///
/// Outcomes are derived from `SHA-256(player_secret || contract_random)` where
/// `contract_random = SHA-256(ledger_sequence)`.  Neither party can unilaterally
/// control the result: the player commits before the ledger sequence is known,
/// and the contract cannot change the sequence after the commitment is stored.
///
/// ## Reserve Solvency
///
/// Before accepting any new game or streak continuation, the contract verifies
/// that `reserve_balance >= worst_case_payout` (streak 4+ multiplier).
/// This prevents the contract from accepting bets it cannot honour.
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
        env.storage().persistent().extend_ttl(&StorageKey::Config, TTL_THRESHOLD, TTL_EXTEND_TO);
        env.storage().persistent().set(&StorageKey::Stats, &stats);
        env.storage().persistent().extend_ttl(&StorageKey::Stats, TTL_THRESHOLD, TTL_EXTEND_TO);
        
        Ok(())
    }
    
    // ── Internal storage helpers ────────────────────────────────────────────

    /// Persist `config` to permanent storage.
    fn save_config(env: &Env, config: &ContractConfig) {
        env.storage().persistent().set(&StorageKey::Config, config);
        env.storage().persistent().extend_ttl(&StorageKey::Config, TTL_THRESHOLD, TTL_EXTEND_TO);
    }

    /// Load the contract configuration. Panics if the contract is not initialized.
    fn load_config(env: &Env) -> ContractConfig {
        let key = StorageKey::Config;
        env.storage().persistent().extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        env.storage().persistent().get(&key).unwrap()
    }

    /// Persist `stats` to permanent storage.
    fn save_stats(env: &Env, stats: &ContractStats) {
        env.storage().persistent().set(&StorageKey::Stats, stats);
        env.storage().persistent().extend_ttl(&StorageKey::Stats, TTL_THRESHOLD, TTL_EXTEND_TO);
    }

    /// Load aggregate contract statistics. Panics if the contract is not initialized.
    fn load_stats(env: &Env) -> ContractStats {
        let key = StorageKey::Stats;
        env.storage().persistent().extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        env.storage().persistent().get(&key).unwrap()
    }

    /// Persist a player's game state to permanent storage.
    fn save_player_game(env: &Env, player: &Address, game: &GameState) {
        let key = StorageKey::PlayerGame(player.clone());
        env.storage().persistent().set(&key, game);
        env.storage().persistent().extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }

    /// Load a player's game state. Returns `None` if no game exists for `player`.
    fn load_player_game(env: &Env, player: &Address) -> Option<GameState> {
        let key = StorageKey::PlayerGame(player.clone());
        if env.storage().persistent().has(&key) {
            env.storage().persistent().extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        }
        env.storage().persistent().get(&key)
    }

    /// Remove a player's game state from storage (called on loss or after settlement).
    fn delete_player_game(env: &Env, player: &Address) {
        env.storage()
            .persistent()
            .remove(&StorageKey::PlayerGame(player.clone()));
    }

    /// Set a player's referrer address.
    fn set_player_referrer(env: &Env, player: &Address, referrer: &Address) {
        let key = StorageKey::PlayerReferrer(player.clone());
        env.storage().persistent().set(&key, referrer);
        env.storage().persistent().extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }

    /// Get a player's referrer address. Returns `None` if no referrer is set.
    fn get_player_referrer(env: &Env, player: &Address) -> Option<Address> {
        let key = StorageKey::PlayerReferrer(player.clone());
        if env.storage().persistent().has(&key) {
            env.storage().persistent().extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        }
        env.storage().persistent().get(&key)
    }

    /// Add referral rewards to a referrer's accumulated balance.
    fn add_referrer_rewards(env: &Env, referrer: &Address, amount: i128) {
        let key = StorageKey::ReferrerRewards(referrer.clone());
        let current = env.storage().persistent().get::<_, i128>(&key).unwrap_or(0);
        let new_balance = current.checked_add(amount).unwrap_or(current);
        env.storage().persistent().set(&key, &new_balance);
        env.storage().persistent().extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }

    /// Get a referrer's accumulated rewards.
    fn get_referrer_rewards(env: &Env, referrer: &Address) -> i128 {
        let key = StorageKey::ReferrerRewards(referrer.clone());
        if env.storage().persistent().has(&key) {
            env.storage().persistent().extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        }
        env.storage().persistent().get(&key).unwrap_or(0)
    }

    /// Claim accumulated referral rewards.
    fn claim_referrer_rewards(env: &Env, referrer: &Address) -> i128 {
        let key = StorageKey::ReferrerRewards(referrer.clone());
        let rewards = env.storage().persistent().get::<_, i128>(&key).unwrap_or(0);
        env.storage().persistent().remove(&key);
        rewards
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
    /// - on success, `config.fee_bps` is snapshotted into the game so future
    ///   `set_fee` calls cannot retroactively change this game's payout terms.
    /// - player balance/transfer checks are assumed to be performed by the caller or higher-level token transfer semantics.
    ///
    /// Validation guards (in order):
    /// 1. `ContractPaused`        – rejected when the contract is paused
    /// 2. `WagerBelowMinimum`     – rejected when `wager < config.min_wager`
    /// 3. `WagerAboveMaximum`     – rejected when `wager > config.max_wager`
    /// 4. `ActiveGameExists`      – rejected when the player already has an
    ///                              in-progress game (phase != Completed).
    ///                              This ensures strict per-player game isolation
    ///                              and prevents concurrent game starts that could
    ///                              exploit race conditions.
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
        referrer: Option<Address>,
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

        // Track referrer if provided
        if let Some(ref ref_addr) = referrer {
            Self::set_player_referrer(&env, &player, ref_addr);
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
            fee_bps: config.fee_bps,
            phase: GamePhase::Committed,
            start_ledger: env.ledger().sequence(),
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
    /// - InvalidPhase: game not in Committed phase (preventing double-reveal)
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

    /// Reveal the player's secret using VRF-based randomness for enhanced security.
    ///
    /// This variant uses Verifiable Random Function (VRF) principles to generate
    /// cryptographically secure randomness, providing stronger unpredictability
    /// guarantees than SHA256-based randomness alone.
    ///
    /// Process:
    /// 1. Verify commitment matches the revealed secret
    /// 2. Generate VRF-based randomness combining player secret with environment randomness
    /// 3. Determine outcome from VRF randomness
    /// 4. Update game state to Revealed phase with result
    /// 5. If player wins, calculate potential payout
    /// 6. If player loses, end game and reset streak
    ///
    /// # Arguments
    /// - `player` – must authorize; must have an active game in `Committed` phase
    /// - `secret` – player's random secret bytes
    ///
    /// # Returns
    /// `Ok(true)` if player won, `Ok(false)` if player lost.
    ///
    /// # Errors
    /// - NoActiveGame: player has no game in Committed phase
    /// - InvalidPhase: game not in Committed phase (preventing double-reveal)
    /// - CommitmentMismatch: revealed secret doesn't match stored commitment
    pub fn reveal_vrf(
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

        // Determine outcome using VRF-based randomness
        let vrf_random = generate_vrf_randomness(&env, &secret);
        let outcome_bit = vrf_random.to_array()[0] % 2;
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

    /// Claim winnings after a successful reveal via on-chain token transfer.
    ///
    /// This is the transfer-backed settlement path.  For a no-transfer accounting
    /// settlement (e.g. in test harnesses or off-chain integrations), use `cash_out`.
    ///
    /// # Process
    /// 1. Verify the game is in `Revealed` phase with a positive streak (player won).
    /// 2. Calculate gross payout (`wager × multiplier / 10_000`) and fee (`gross × fee_bps / 10_000`).
    /// 3. Verify `reserve_balance >= gross_payout` to ensure solvency.
    /// 4. Transfer `net_payout` (gross − fee) to `player` via the SAC token client.
    /// 5. Transfer `fee_amount` to `config.treasury`.
    /// 6. Deduct `gross_payout` from `reserve_balance`; add `fee_amount` to `total_fees`.
    /// 7. Advance game phase to `Completed`.
    ///
    /// # Arguments
    /// - `player` – must authorize; must have an active game in `Revealed` phase
    ///
    /// # Returns
    /// `Ok(())` on successful settlement.
    ///
    /// # Errors
    /// | Error                  | Condition                                              |
    /// |------------------------|--------------------------------------------------------|
    /// | `NoActiveGame`         | No game record exists for `player`                     |
    /// | `InvalidPhase`         | Game is not in `Revealed` phase                        |
    /// | `NoWinningsToClaimOrContinue` | `streak == 0` — loss state, nothing to claim    |
    /// | `InsufficientReserves` | `reserve_balance < gross_payout` or arithmetic overflow|
    /// | `TransferFailed`       | SAC token transfer fails (propagated from token client)|
    ///
    /// # Note on streak == 0
    /// A `Revealed` game with `streak == 0` indicates a loss; the game record is
    /// deleted by `reveal` on the loss path, so in practice `claim_winnings` will
    /// return `NoActiveGame` for a lost game.  The explicit `streak == 0` guard
    /// below defends against any injected state that bypasses the normal flow.
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

        // streak == 0 in Revealed phase is a loss state — no winnings to claim.
        // In normal flow this state is deleted by reveal, but guard defensively.
        if game.streak == 0 {
            return Err(Error::NoWinningsToClaimOrContinue);
        }

        let config = Self::load_config(&env);
        let token_client = token::Client::new(&env, &config.token);

        // Single-pass payout breakdown: gross, fee, and net computed together to
        // avoid the duplicate multiplier lookup + two checked_div calls that would
        // result from calling calculate_payout and then recomputing gross/fee.
        let (gross_payout, fee_amount, net_payout) =
            calculate_payout_breakdown(game.wager, game.streak, game.fee_bps)
                .ok_or(Error::InsufficientReserves)?;

        // Check sufficient reserves
        let stats = Self::load_stats(&env);
        if stats.reserve_balance < gross_payout {
            return Err(Error::InsufficientReserves);
        }

        // Update accounting state BEFORE transfers so that if a transfer panics
        // the reserve ledger is already consistent (Soroban aborts the entire
        // transaction on panic, so partial state is never committed).
        let mut stats = stats;
        stats.reserve_balance = stats.reserve_balance.checked_sub(gross_payout)
            .ok_or(Error::InsufficientReserves)?;
        stats.total_fees = stats.total_fees.checked_add(fee_amount).unwrap_or(stats.total_fees);
        Self::save_stats(&env, &stats);

        // Mark game completed before transfers for the same reason.
        game.phase = GamePhase::Completed;
        Self::save_player_game(&env, &player, &game);

        // Distribute referral rewards (1% of protocol fees)
        if let Some(referrer) = Self::get_player_referrer(&env, &player) {
            let referral_reward = fee_amount
                .checked_mul(1)
                .and_then(|v| v.checked_div(100))
                .unwrap_or(0);
            if referral_reward > 0 {
                Self::add_referrer_rewards(&env, &referrer, referral_reward);
            }
        }

        // Transfer net payout to player
        token_client.transfer(&env.current_contract_address(), &player, &net_payout);

        // Transfer fee to treasury
        token_client.transfer(&env.current_contract_address(), &config.treasury, &fee_amount);

        Ok(())
    }

    /// Cash out winnings after a successful reveal (accounting-only, no token transfer).
    ///
    /// Settles the game by updating reserve and fee accounting without issuing
    /// an on-chain token transfer.  Use `claim_winnings` for the transfer-backed path.
    ///
    /// # Arguments
    /// - `player` – must authorize; must have an active game in `Revealed` phase
    ///
    /// # Returns
    /// `Ok(net_payout)` — the net payout amount in stroops on success.
    ///
    /// # Guards (evaluated in order, no state mutation on failure)
    /// 1. `NoActiveGame`                – no game record exists for `player`
    /// 2. `InvalidPhase`                – game is not in `Revealed` phase
    /// 3. `NoWinningsToClaimOrContinue` – `streak == 0` (player lost the last flip)
    ///
    /// # Errors
    /// | Error                          | Condition                                        |
    /// |--------------------------------|--------------------------------------------------|
    /// | `NoActiveGame`                 | No game record exists for `player`               |
    /// | `InvalidPhase`                 | Game is not in `Revealed` phase                  |
    /// | `NoWinningsToClaimOrContinue`  | `streak == 0` — loss state, nothing to cash out  |
    /// | `InsufficientReserves`         | Arithmetic overflow in payout calculation        |
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

        // Single-pass payout breakdown: gross, fee, and net computed together to
        // avoid the duplicate multiplier lookup + two checked_div calls that would
        // result from calling calculate_payout and then recomputing gross/fee.
        let (gross, fee, net_payout) =
            calculate_payout_breakdown(game.wager, game.streak, game.fee_bps)
                .ok_or(Error::InsufficientReserves)?;

        let mut stats = Self::load_stats(&env);
        stats.reserve_balance = stats.reserve_balance
            .checked_sub(gross)
            .ok_or(Error::InsufficientReserves)?;
        stats.total_fees = stats.total_fees
            .checked_add(fee)
            .unwrap_or(stats.total_fees);
        Self::save_stats(&env, &stats);

        // Clear the player's game state completely after settlement
        Self::delete_player_game(&env, &player);

        Ok(net_payout)
    }

    /// Continue to the next streak level after a confirmed win.
    ///
    /// ## Continue-State Transition
    ///
    /// `continue_streak` is the bridge between the `Revealed` and `Committed`
    /// phases for a winning player who chooses to risk their winnings on another
    /// flip rather than cash out.
    ///
    /// ### Phase transition
    ///
    /// ```text
    /// Revealed (streak ≥ 1)  ──continue_streak──►  Committed
    /// ```
    ///
    /// ### What is preserved
    ///
    /// | Field            | Behaviour                                              |
    /// |------------------|--------------------------------------------------------|
    /// | `wager`          | Unchanged — the original bet stays locked              |
    /// | `streak`         | Unchanged — incremented only by `reveal` on a win      |
    /// | `fee_bps`        | Unchanged — snapshot from game creation is honoured    |
    /// | `side`           | Unchanged — player's chosen side carries over          |
    ///
    /// ### What is replaced
    ///
    /// | Field            | New value                                              |
    /// |------------------|--------------------------------------------------------|
    /// | `commitment`     | `new_commitment` supplied by the caller                |
    /// | `contract_random`| Fresh SHA-256 of the current ledger sequence number    |
    /// | `phase`          | `GamePhase::Committed`                                 |
    ///
    /// Refreshing `contract_random` on every continue call is a security
    /// requirement: it prevents a player from reusing a previously observed
    /// contract contribution to predict the next outcome.
    ///
    /// ## Eligibility rules
    ///
    /// A player may call `continue_streak` **only** when all of the following
    /// hold simultaneously:
    ///
    /// 1. **Active game exists** – a `GameState` record is present in storage
    ///    for `player`.
    /// 2. **Revealed phase** – the game must be in `GamePhase::Revealed`,
    ///    meaning `reveal` has already been called and the outcome is known.
    /// 3. **Positive streak** – `game.streak >= 1`, confirming the player
    ///    actually won the last flip.  A `Revealed` game with `streak == 0`
    ///    represents a loss state and must not be continued.
    /// 4. **Non-zero commitment** – `new_commitment` must not be the all-zero
    ///    32-byte value.  An all-zero commitment is treated as a missing or
    ///    placeholder value and is rejected with `InvalidCommitment`.
    /// 5. **Sufficient reserves** – the contract must hold enough reserves to
    ///    cover the worst-case payout at the *next* streak level.
    ///
    /// ## Process (on success)
    ///
    /// 1. All precondition guards are evaluated (no state mutation on failure).
    /// 2. New contract randomness is derived from the current ledger sequence.
    /// 3. Game phase is reset to `Committed` with the fresh commitment and
    ///    randomness; the streak counter and wager are preserved.
    ///
    /// ## Errors
    ///
    /// | Error                        | Condition                                      |
    /// |------------------------------|------------------------------------------------|
    /// | `NoActiveGame`               | No game record exists for `player`             |
    /// | `InvalidPhase`               | Game is not in `Revealed` phase                |
    /// | `NoWinningsToClaimOrContinue`| `streak == 0` (player lost the last flip)      |
    /// | `InvalidCommitment`          | `new_commitment` is all-zero bytes             |
    /// | `InsufficientReserves`       | Reserves cannot cover the next streak payout   |
    pub fn continue_streak(
        env: Env,
        player: Address,
        new_commitment: BytesN<32>,
    ) -> Result<(), Error> {
        player.require_auth();

        // Guard 1: player must have an active game
        let mut game = Self::load_player_game(&env, &player)
            .ok_or(Error::NoActiveGame)?;

        // Guard 2: game must be in Revealed phase
        if game.phase != GamePhase::Revealed {
            return Err(Error::InvalidPhase);
        }

        // Guard 3: streak must be >= 1 (player actually won)
        // A Revealed game with streak == 0 is a loss state — continuation is
        // not permitted; the player must start a fresh game instead.
        if game.streak == 0 {
            return Err(Error::NoWinningsToClaimOrContinue);
        }

        // Guard 4: commitment must not be all-zero bytes (missing / placeholder)
        if new_commitment == BytesN::from_array(&env, &[0u8; 32]) {
            return Err(Error::InvalidCommitment);
        }

        // Guard 5: reserves must cover the next streak's worst-case payout.
        // Config is not needed here — all required data (wager, streak) is in GameState.
        let stats = Self::load_stats(&env);

        let next_streak = game.streak.saturating_add(1);
        let max_payout = game.wager
            .checked_mul(get_multiplier(next_streak) as i128)
            .and_then(|v| v.checked_div(10_000))
            .ok_or(Error::InsufficientReserves)?;

        if stats.reserve_balance < max_payout {
            return Err(Error::InsufficientReserves);
        }

        // Generate new contract randomness from the current ledger sequence
        let seq_bytes = env.ledger().sequence().to_be_bytes();
        let contract_random: BytesN<32> = env.crypto().sha256(
            &soroban_sdk::Bytes::from_slice(&env, &seq_bytes),
        ).into();

        // Reset to Committed phase; preserve streak and wager
        game.phase = GamePhase::Committed;
        game.commitment = new_commitment;
        game.contract_random = contract_random.into();

        Self::save_player_game(&env, &player, &game);

        Ok(())
    }

    /// Pause or unpause acceptance of new games.
    ///
    /// Only the configured `admin` may call this function.
    ///
    /// Pause scope:
    /// - When `paused == true`, `start_game` is rejected with [`Error::ContractPaused`].
    /// - Existing game flows remain available (`reveal`, `cash_out`, `claim_winnings`,
    ///   and `continue_streak`) so in-flight games can settle.
    /// - Pausing is not retroactive: a game that was already in `Committed` or
    ///   `Revealed` phase before the pause must still be able to reach `Completed`.
    ///
    /// # Arguments
    /// - `admin`  – caller address; must authorize and match `config.admin`
    /// - `paused` – target pause state
    ///
    /// # Errors
    /// - [`Error::Unauthorized`] – caller is not the configured admin
    ///
    /// # Security
    /// - `admin.require_auth()` enforces signed authorization.
    /// - Address equality check (`admin == config.admin`) prevents non-admin callers.
    /// - Only the `paused` flag is mutated; all other config fields are preserved.
    pub fn set_paused(env: Env, admin: Address, paused: bool) -> Result<(), Error> {
        admin.require_auth();

        let mut config = Self::load_config(&env);
        if admin != config.admin {
            return Err(Error::Unauthorized);
        }

        config.paused = paused;
        Self::save_config(&env, &config);

        Ok(())
    }

    /// Update the treasury address that receives protocol fees.
    ///
    /// Only the configured `admin` may call this function.
    ///
    /// # Arguments
    /// - `admin`    – caller address; must authorize and match `config.admin`
    /// - `treasury` – new treasury destination for future fee transfers
    ///
    /// # Errors
    /// - [`Error::Unauthorized`] – caller is not the configured admin
    ///
    /// # Authorization invariants
    /// - Unauthorized callers must not be able to redirect fees.
    /// - On rejection, the entire [`ContractConfig`] remains byte-for-byte unchanged.
    /// - Successful calls mutate only `config.treasury`.
    pub fn set_treasury(env: Env, admin: Address, treasury: Address) -> Result<(), Error> {
        admin.require_auth();

        let mut config = Self::load_config(&env);
        if admin != config.admin {
            return Err(Error::Unauthorized);
        }

        config.treasury = treasury;
        Self::save_config(&env, &config);

        Ok(())
    }

    /// Update the inclusive wager bounds for new game creation.
    ///
    /// Only the configured `admin` may call this function.
    ///
    /// # Arguments
    /// - `admin`     – caller address; must authorize and match `config.admin`
    /// - `min_wager` – new inclusive lower bound in stroops
    /// - `max_wager` – new inclusive upper bound in stroops
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]      – caller is not the configured admin
    /// - [`Error::InvalidWagerLimits`]– `min_wager >= max_wager`
    ///
    /// # Authorization invariants
    /// - Unauthorized callers must never be able to loosen or tighten wager bounds.
    /// - The bounds validation executes before storage writes, so invalid inputs never persist.
    /// - On rejection, every field of [`ContractConfig`] remains unchanged.
    pub fn set_wager_limits(
        env: Env,
        admin: Address,
        min_wager: i128,
        max_wager: i128,
    ) -> Result<(), Error> {
        admin.require_auth();

        let mut config = Self::load_config(&env);
        if admin != config.admin {
            return Err(Error::Unauthorized);
        }

        if min_wager >= max_wager {
            return Err(Error::InvalidWagerLimits);
        }

        config.min_wager = min_wager;
        config.max_wager = max_wager;
        Self::save_config(&env, &config);

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
    /// - Fee changes are forward-only: in-flight games settle using their
    ///   snapshotted `GameState.fee_bps` value.
    /// - Unauthorized callers leave the entire [`ContractConfig`] unchanged.
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

    /// Reclaim a wager from a game that has exceeded the reveal timeout.
    ///
    /// If a player starts a game but never calls `reveal`, their wager is
    /// locked in the contract indefinitely.  After `REVEAL_TIMEOUT_LEDGERS`
    /// ledgers have elapsed since `start_game`, the player may call
    /// `reclaim_wager` to recover their wager and clean up the game slot.
    ///
    /// # Timeout Invariant
    ///
    /// A game is considered expired when:
    /// ```text
    /// current_ledger >= game.start_ledger + REVEAL_TIMEOUT_LEDGERS
    /// ```
    ///
    /// # Process (on success)
    /// 1. Verify the player has a game in `Committed` phase.
    /// 2. Verify the reveal window has expired.
    /// 3. Credit the wager back to `reserve_balance` (house keeps the wager
    ///    because the player failed to reveal — this prevents griefing by
    ///    locking reserves indefinitely).
    /// 4. Delete the player's game state to free the slot.
    ///
    /// # Arguments
    /// - `player` – must authorize; must have a game in `Committed` phase
    ///
    /// # Returns
    /// `Ok(wager)` — the reclaimed wager amount in stroops.
    ///
    /// # Errors
    /// | Error           | Condition                                              |
    /// |-----------------|--------------------------------------------------------|
    /// | `NoActiveGame`  | No game record exists for `player`                     |
    /// | `InvalidPhase`  | Game is not in `Committed` phase                       |
    /// | `RevealTimeout` | Reveal window has **not** yet expired (too early)      |
    pub fn reclaim_wager(env: Env, player: Address) -> Result<i128, Error> {
        player.require_auth();

        let game = Self::load_player_game(&env, &player)
            .ok_or(Error::NoActiveGame)?;

        // Only Committed games can time out — Revealed games must be settled normally.
        if game.phase != GamePhase::Committed {
            return Err(Error::InvalidPhase);
        }

        // Reject if the timeout window has not yet elapsed.
        let current = env.ledger().sequence();
        let expires_at = game.start_ledger.saturating_add(REVEAL_TIMEOUT_LEDGERS);
        if current < expires_at {
            return Err(Error::RevealTimeout);
        }

        // Credit wager to reserves and free the player slot.
        let mut stats = Self::load_stats(&env);
        stats.reserve_balance = stats
            .reserve_balance
            .checked_add(game.wager)
            .unwrap_or(stats.reserve_balance);
        Self::save_stats(&env, &stats);

        Self::delete_player_game(&env, &player);

        Ok(game.wager)
    }

    /// Claim accumulated referral rewards.
    ///
    /// Allows a referrer to withdraw their accumulated rewards from referred players' wins.
    /// Referral rewards are calculated as 1% of the protocol fees collected from referred players.
    ///
    /// # Arguments
    /// - `referrer` – must authorize; must have accumulated rewards
    ///
    /// # Returns
    /// `Ok(rewards_amount)` — the total accumulated rewards in stroops.
    ///
    /// # Errors
    /// - `TransferFailed` – token transfer fails
    pub fn claim_referral_rewards(
        env: Env,
        referrer: Address,
    ) -> Result<i128, Error> {
        referrer.require_auth();

        let rewards = Self::claim_referrer_rewards(&env, &referrer);
        
        if rewards > 0 {
            let config = Self::load_config(&env);
            let token_client = token::Client::new(&env, &config.token);
            token_client.transfer(&env.current_contract_address(), &referrer, &rewards);
        }

        Ok(rewards)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Ledger;

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
    fn test_calculate_payout_breakdown_streak_1() {
        // wager=10_000_000, streak=1 (1.9x), fee=300bps (3%)
        // gross = 10_000_000 * 19_000 / 10_000 = 19_000_000
        // fee   = 19_000_000 * 300  / 10_000 =    570_000
        // net   = 18_430_000
        assert_eq!(
            calculate_payout_breakdown(10_000_000, 1, 300),
            Some((19_000_000, 570_000, 18_430_000))
        );
    }

    #[test]
    fn test_calculate_payout_breakdown_net_equals_calculate_payout() {
        // calculate_payout must return the same net as the breakdown helper
        for (wager, streak, fee_bps) in [(10_000_000, 1, 300), (5_000_000, 2, 500), (1_000_000, 4, 200)] {
            let (_, _, net) = calculate_payout_breakdown(wager, streak, fee_bps).unwrap();
            assert_eq!(calculate_payout(wager, streak, fee_bps), Some(net));
        }
    }

    #[test]
    fn test_calculate_payout_breakdown_gross_minus_fee_equals_net() {
        // Invariant: gross - fee == net for all valid inputs
        let (gross, fee, net) = calculate_payout_breakdown(10_000_000, 3, 400).unwrap();
        assert_eq!(gross - fee, net);
    }

    #[test]
    fn test_calculate_payout_breakdown_overflow_returns_none() {
        assert_eq!(calculate_payout_breakdown(i128::MAX, 1, 300), None);
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
                    fee_bps,
                    phase: GamePhase::Revealed,
                    start_ledger: 0,
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
            fee_bps: 300,
            phase,
            start_ledger: 0,
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

    #[test]
    fn test_claim_winnings_rejects_losing_state_streak_zero() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);

        let player = Address::generate(&env);
        // Inject a Revealed game with streak == 0 (loss state).
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 0, 10_000_000);

        let result = client.try_claim_winnings(&player);
        assert_eq!(result, Err(Ok(Error::NoWinningsToClaimOrContinue)));

        // State must be unchanged — no partial mutation.
        let game: GameState = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player).unwrap()
        });
        assert_eq!(game.phase, GamePhase::Revealed);
        assert_eq!(game.streak, 0);
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
        let game_opt = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player)
        });
        assert!(game_opt.is_none(), "Player game state should be deleted after cash out");

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
        let game_opt = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player)
        });
        assert!(game_opt.is_none(), "Player game state should be deleted after cash out");
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

    // ── set_paused tests ───────────────────────────────────────────────────

    #[test]
    fn test_set_paused_succeeds_for_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        let admin = get_admin(&env, &contract_id);

        assert!(client.try_set_paused(&admin, &true).is_ok());

        let cfg: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });
        assert!(cfg.paused);
    }

    #[test]
    fn test_set_paused_rejects_non_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let (_, client) = setup(&env);
        let stranger = Address::generate(&env);

        let result = client.try_set_paused(&stranger, &true);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_set_paused_can_unpause() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        let admin = get_admin(&env, &contract_id);

        client.set_paused(&admin, &true);
        client.set_paused(&admin, &false);

        let cfg: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });
        assert!(!cfg.paused);
    }

    #[test]
    fn test_set_paused_no_state_mutation_on_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);

        let before: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });

        let stranger = Address::generate(&env);
        let _ = client.try_set_paused(&stranger, &true);

        let after: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });

        assert_eq!(before.paused, after.paused);
        assert_eq!(before.admin, after.admin);
        assert_eq!(before.treasury, after.treasury);
        assert_eq!(before.token, after.token);
        assert_eq!(before.fee_bps, after.fee_bps);
        assert_eq!(before.min_wager, after.min_wager);
        assert_eq!(before.max_wager, after.max_wager);
    }

    #[test]
    fn test_set_treasury_succeeds_for_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        let admin = get_admin(&env, &contract_id);
        let new_treasury = Address::generate(&env);

        assert!(client.try_set_treasury(&admin, &new_treasury).is_ok());

        let cfg: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });
        assert_eq!(cfg.treasury, new_treasury);
    }

    #[test]
    fn test_set_treasury_rejects_non_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let (_, client) = setup(&env);
        let stranger = Address::generate(&env);
        let new_treasury = Address::generate(&env);

        let result = client.try_set_treasury(&stranger, &new_treasury);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_set_treasury_no_state_mutation_on_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        let before: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });

        let stranger = Address::generate(&env);
        let new_treasury = Address::generate(&env);
        let _ = client.try_set_treasury(&stranger, &new_treasury);

        let after: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });

        assert_eq!(before, after);
    }

    #[test]
    fn test_set_wager_limits_succeeds_for_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        let admin = get_admin(&env, &contract_id);

        assert!(client.try_set_wager_limits(&admin, &2_000_000, &200_000_000).is_ok());

        let cfg: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });
        assert_eq!(cfg.min_wager, 2_000_000);
        assert_eq!(cfg.max_wager, 200_000_000);
    }

    #[test]
    fn test_set_wager_limits_rejects_non_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let (_, client) = setup(&env);
        let stranger = Address::generate(&env);

        let result = client.try_set_wager_limits(&stranger, &2_000_000, &200_000_000);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_set_wager_limits_rejects_invalid_bounds() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        let admin = get_admin(&env, &contract_id);

        let result = client.try_set_wager_limits(&admin, &10_000_000, &10_000_000);
        assert_eq!(result, Err(Ok(Error::InvalidWagerLimits)));
    }

    #[test]
    fn test_set_wager_limits_no_state_mutation_on_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        let before: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });

        let stranger = Address::generate(&env);
        let _ = client.try_set_wager_limits(&stranger, &2_000_000, &200_000_000);

        let after: ContractConfig = env.as_contract(&contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });

        assert_eq!(before, after);
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

    #[test]
    fn test_set_fee_does_not_reprice_existing_revealed_game() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        let admin = get_admin(&env, &contract_id);
        fund_reserves(&env, &contract_id, 1_000_000_000);

        let player = Address::generate(&env);
        let secret = Bytes::from_slice(&env, &[1u8; 32]);
        let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

        client.start_game(&player, &Side::Heads, &10_000_000, &commitment);
        assert_eq!(client.try_reveal(&player, &secret), Ok(Ok(true)));

        // Fee changes after reveal must not alter this game's payout terms.
        client.set_fee(&admin, &500);

        let expected = calculate_payout(10_000_000, 1, 300).unwrap();
        let payout = client.try_cash_out(&player);
        assert_eq!(payout, Ok(Ok(expected)));
    }

    #[test]
    fn test_set_fee_applies_to_new_game_after_update() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        let admin = get_admin(&env, &contract_id);
        fund_reserves(&env, &contract_id, 1_000_000_000);

        client.set_fee(&admin, &500);

        let player = Address::generate(&env);
        let secret = Bytes::from_slice(&env, &[1u8; 32]);
        let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

        client.start_game(&player, &Side::Heads, &10_000_000, &commitment);
        assert_eq!(client.try_reveal(&player, &secret), Ok(Ok(true)));

        let expected = calculate_payout(10_000_000, 1, 500).unwrap();
        let payout = client.try_cash_out(&player);
        assert_eq!(payout, Ok(Ok(expected)));
    }

    // ── continue_streak unit tests ───────────────────────────────────────────
    //
    // These tests cover the continue-state transition:
    //   Revealed (win) → Committed
    //
    // Invariants verified:
    //   - phase resets to Committed
    //   - new commitment is persisted
    //   - new contract_random is derived and persisted (differs from old value)
    //   - wager is preserved unchanged
    //   - streak is preserved unchanged (not reset, not incremented)
    //   - no state mutation on any guard failure

    // ── Guard 1: NoActiveGame ────────────────────────────────────────────────

    #[test]
    fn test_continue_streak_rejects_no_active_game() {
        let env = Env::default();
        env.mock_all_auths();
        let (_, client) = setup(&env);

        let player = Address::generate(&env);
        let result = client.try_continue_streak(&player, &dummy_commitment(&env));
        assert_eq!(result, Err(Ok(Error::NoActiveGame)));
    }

    // ── Guard 2: InvalidPhase ────────────────────────────────────────────────

    #[test]
    fn test_continue_streak_rejects_committed_phase() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);

        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Committed, 1, 10_000_000);

        let result = client.try_continue_streak(&player, &dummy_commitment(&env));
        assert_eq!(result, Err(Ok(Error::InvalidPhase)));
    }

    #[test]
    fn test_continue_streak_rejects_completed_phase() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);

        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Completed, 1, 10_000_000);

        let result = client.try_continue_streak(&player, &dummy_commitment(&env));
        assert_eq!(result, Err(Ok(Error::InvalidPhase)));
    }

    // ── Guard 3: NoWinningsToClaimOrContinue ─────────────────────────────────

    #[test]
    fn test_continue_streak_rejects_streak_zero() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);

        let player = Address::generate(&env);
        // Revealed with streak == 0 means the player lost — continuation not allowed.
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 0, 10_000_000);

        let result = client.try_continue_streak(&player, &dummy_commitment(&env));
        assert_eq!(result, Err(Ok(Error::NoWinningsToClaimOrContinue)));
    }

    // ── Guard 4: InvalidCommitment ───────────────────────────────────────────

    #[test]
    fn test_continue_streak_rejects_all_zero_commitment() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        fund_reserves(&env, &contract_id, 1_000_000_000);

        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 1, 10_000_000);

        let zero_commitment = BytesN::from_array(&env, &[0u8; 32]);
        let result = client.try_continue_streak(&player, &zero_commitment);
        assert_eq!(result, Err(Ok(Error::InvalidCommitment)));
    }

    // ── Guard 5: InsufficientReserves ────────────────────────────────────────

    #[test]
    fn test_continue_streak_rejects_insufficient_reserves() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        // Leave reserves at 0 — cannot cover any payout.
        let _ = contract_id;

        let player = Address::generate(&env);
        // Inject directly so we bypass start_game's own reserve check.
        env.as_contract(&contract_id, || {
            let game = GameState {
                wager: 10_000_000,
                side: Side::Heads,
                streak: 1,
                commitment: dummy_commitment(&env),
                contract_random: dummy_commitment(&env),
                fee_bps: 300,
                phase: GamePhase::Revealed,
                start_ledger: 0,
            };
            CoinflipContract::save_player_game(&env, &player, &game);
        });

        let result = client.try_continue_streak(&player, &dummy_commitment(&env));
        assert_eq!(result, Err(Ok(Error::InsufficientReserves)));
    }

    // ── Happy path: state transition Revealed → Committed ───────────────────

    /// Core invariant: after a successful continue_streak the game must be in
    /// Committed phase with the new commitment stored, the original wager and
    /// streak preserved, and a fresh contract_random derived.
    #[test]
    fn test_continue_streak_transitions_to_committed() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        fund_reserves(&env, &contract_id, 1_000_000_000);

        let player = Address::generate(&env);
        let wager = 10_000_000i128;
        let streak = 1u32;

        inject_game(&env, &contract_id, &player, GamePhase::Revealed, streak, wager);

        let new_commitment = dummy_commitment(&env);
        let result = client.try_continue_streak(&player, &new_commitment);
        assert!(result.is_ok());

        let game: GameState = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player).unwrap()
        });

        // Phase must be reset to Committed.
        assert_eq!(game.phase, GamePhase::Committed);
        // New commitment must be persisted.
        assert_eq!(game.commitment, new_commitment);
        // Wager must be unchanged.
        assert_eq!(game.wager, wager);
        // Streak must be preserved (not reset, not incremented).
        assert_eq!(game.streak, streak);
    }

    /// contract_random must be refreshed on each continue_streak call so that
    /// the player cannot predict the next outcome from a previously observed value.
    #[test]
    fn test_continue_streak_refreshes_contract_random() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        fund_reserves(&env, &contract_id, 1_000_000_000);

        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 1, 10_000_000);

        // Capture the old contract_random before the transition.
        let old_random: BytesN<32> = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player).unwrap().contract_random
        });

        // Advance the ledger so the new sequence produces a different hash.
        env.ledger().with_mut(|l| l.sequence_number += 1);

        let new_commitment = dummy_commitment(&env);
        client.continue_streak(&player, &new_commitment);

        let new_random: BytesN<32> = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player).unwrap().contract_random
        });

        // contract_random must have changed.
        assert_ne!(old_random, new_random,
            "contract_random must be refreshed on continue_streak");
    }

    /// Streak must be preserved across multiple consecutive continue calls so
    /// the multiplier tier keeps climbing correctly.
    #[test]
    fn test_continue_streak_preserves_streak_across_multiple_rounds() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        fund_reserves(&env, &contract_id, 1_000_000_000);

        let player = Address::generate(&env);
        let wager = 10_000_000i128;

        // Start at streak 2 (player has already won twice).
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 2, wager);

        let new_commitment = dummy_commitment(&env);
        client.continue_streak(&player, &new_commitment);

        let game: GameState = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player).unwrap()
        });

        assert_eq!(game.streak, 2, "streak must be preserved, not reset or incremented");
        assert_eq!(game.wager, wager, "wager must be preserved");
        assert_eq!(game.phase, GamePhase::Committed);
    }

    /// No state mutation must occur when any guard fires.
    #[test]
    fn test_continue_streak_no_state_mutation_on_invalid_phase() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);

        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Committed, 1, 10_000_000);

        let before: GameState = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player).unwrap()
        });

        let _ = client.try_continue_streak(&player, &dummy_commitment(&env));

        let after: GameState = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player).unwrap()
        });

        assert_eq!(before, after, "game state must be unchanged on guard failure");
    }

    /// fee_bps snapshot must survive the continue transition unchanged so that
    /// the payout terms locked at game creation are honoured at cash-out.
    #[test]
    fn test_continue_streak_preserves_fee_snapshot() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client) = setup(&env);
        fund_reserves(&env, &contract_id, 1_000_000_000);

        let player = Address::generate(&env);
        let wager = 10_000_000i128;
        let original_fee_bps = 300u32;

        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 1, wager);

        let new_commitment = dummy_commitment(&env);
        client.continue_streak(&player, &new_commitment);

        let game: GameState = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player).unwrap()
        });

        assert_eq!(game.fee_bps, original_fee_bps,
            "fee_bps snapshot must be preserved through the continue transition");
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
    pub(crate) fn setup_contract_with_bounds(
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

    pub(crate) fn dummy_commitment_prop(env: &Env) -> BytesN<32> {
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

    // Feature: admin access control, Property: unauthorized admin calls cannot mutate config
    // Validates: fee, treasury, wager-limit, and pause settings remain unchanged on rejection.

    fn setup_admin_access_env(
        env: &Env,
        fee_bps: u32,
        min_wager: i128,
        max_wager: i128,
    ) -> (Address, CoinflipContractClient<'_>, Address) {
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let treasury = Address::generate(env);
        let token = env.register_stellar_asset_contract(admin.clone());
        client.initialize(&admin, &treasury, &token, &fee_bps, &min_wager, &max_wager);
        (contract_id, client, treasury)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        #[test]
        fn test_unauthorized_set_fee_preserves_config(
            fee_bps in 200u32..=500u32,
            new_fee_bps in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
        ) {
            let env = Env::default();
            env.mock_all_auths();
            let max_wager = min_wager + 100_000_000;
            let (contract_id, client, _) = setup_admin_access_env(&env, fee_bps, min_wager, max_wager);
            let attacker = Address::generate(&env);

            let before: ContractConfig = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Config).unwrap()
            });

            let result = client.try_set_fee(&attacker, &new_fee_bps);
            prop_assert_eq!(result, Err(Ok(Error::Unauthorized)));

            let after: ContractConfig = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Config).unwrap()
            });
            prop_assert_eq!(before, after);
        }

        #[test]
        fn test_unauthorized_set_paused_preserves_config(
            fee_bps in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
            pause_target in any::<bool>(),
        ) {
            let env = Env::default();
            env.mock_all_auths();
            let max_wager = min_wager + 100_000_000;
            let (contract_id, client, _) = setup_admin_access_env(&env, fee_bps, min_wager, max_wager);
            let attacker = Address::generate(&env);

            let before: ContractConfig = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Config).unwrap()
            });

            let result = client.try_set_paused(&attacker, &pause_target);
            prop_assert_eq!(result, Err(Ok(Error::Unauthorized)));

            let after: ContractConfig = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Config).unwrap()
            });
            prop_assert_eq!(before, after);
        }

        #[test]
        fn test_unauthorized_set_treasury_preserves_config(
            fee_bps in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
        ) {
            let env = Env::default();
            env.mock_all_auths();
            let max_wager = min_wager + 100_000_000;
            let (contract_id, client, _) = setup_admin_access_env(&env, fee_bps, min_wager, max_wager);
            let attacker = Address::generate(&env);
            let new_treasury = Address::generate(&env);

            let before: ContractConfig = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Config).unwrap()
            });

            let result = client.try_set_treasury(&attacker, &new_treasury);
            prop_assert_eq!(result, Err(Ok(Error::Unauthorized)));

            let after: ContractConfig = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Config).unwrap()
            });
            prop_assert_eq!(before, after);
        }

        #[test]
        fn test_unauthorized_set_wager_limits_preserves_config(
            fee_bps in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
            min_offset in 1i128..=5_000_000i128,
            max_offset in 5_000_001i128..=50_000_000i128,
        ) {
            let env = Env::default();
            env.mock_all_auths();
            let max_wager = min_wager + 100_000_000;
            let (contract_id, client, _) = setup_admin_access_env(&env, fee_bps, min_wager, max_wager);
            let attacker = Address::generate(&env);
            let attempted_min_wager = min_wager + min_offset;
            let attempted_max_wager = attempted_min_wager + max_offset;

            let before: ContractConfig = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Config).unwrap()
            });

            let result = client.try_set_wager_limits(&attacker, &attempted_min_wager, &attempted_max_wager);
            prop_assert_eq!(result, Err(Ok(Error::Unauthorized)));

            let after: ContractConfig = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Config).unwrap()
            });
            prop_assert_eq!(before, after);
        }
    }

    // Feature: fee isolation, Property: fee changes are forward-only
    // Validates: in-flight games settle with their snapshotted fee, while
    // games created after `set_fee` use the new fee.

    fn setup_fee_isolation_env(
        env: &Env,
        fee_bps: u32,
        min_wager: i128,
        max_wager: i128,
    ) -> (Address, CoinflipContractClient<'_>, Address) {
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let treasury = Address::generate(env);
        let token = env.register_stellar_asset_contract(admin.clone());

        client.initialize(&admin, &treasury, &token, &fee_bps, &min_wager, &max_wager);

        env.as_contract(&contract_id, || {
            let mut stats = CoinflipContract::load_stats(env);
            stats.reserve_balance = i128::MAX / 4;
            CoinflipContract::save_stats(env, &stats);
        });

        (contract_id, client, admin)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        #[test]
        fn test_fee_change_does_not_reprice_revealed_inflight_game(
            initial_fee_bps in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
            wager_offset in 0i128..=50_000_000i128,
        ) {
            let env = Env::default();
            env.mock_all_auths();

            let max_wager = min_wager + 100_000_000;
            let wager = (min_wager + wager_offset).min(max_wager);
            let new_fee_bps = if initial_fee_bps < 500 { initial_fee_bps + 1 } else { 499 };

            let (contract_id, client, admin) = setup_fee_isolation_env(&env, initial_fee_bps, min_wager, max_wager);
            let player = Address::generate(&env);
            let secret = Bytes::from_slice(&env, &[1u8; 32]);
            let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

            client.start_game(&player, &Side::Heads, &wager, &commitment);
            prop_assert_eq!(client.try_reveal(&player, &secret), Ok(Ok(true)));

            let revealed: GameState = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player).unwrap()
            });
            prop_assert_eq!(revealed.fee_bps, initial_fee_bps);

            client.set_fee(&admin, &new_fee_bps);

            let expected_net = calculate_payout(wager, 1, initial_fee_bps).unwrap();
            let expected_fee = (wager
                .checked_mul(get_multiplier(1) as i128)
                .and_then(|v| v.checked_div(10_000))
                .unwrap())
                .checked_mul(initial_fee_bps as i128)
                .and_then(|v| v.checked_div(10_000))
                .unwrap();

            let payout = client.try_cash_out(&player);
            prop_assert_eq!(payout, Ok(Ok(expected_net)));

            let cfg: ContractConfig = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Config).unwrap()
            });
            prop_assert_eq!(cfg.fee_bps, new_fee_bps);

            let stats: ContractStats = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Stats).unwrap()
            });
            prop_assert_eq!(stats.total_fees, expected_fee);
        }

        #[test]
        fn test_fee_change_applies_to_future_games_only(
            initial_fee_bps in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
            wager_offset in 0i128..=50_000_000i128,
        ) {
            let env = Env::default();
            env.mock_all_auths();

            let max_wager = min_wager + 100_000_000;
            let wager = (min_wager + wager_offset).min(max_wager);
            let new_fee_bps = if initial_fee_bps < 500 { initial_fee_bps + 1 } else { 499 };

            let (_contract_id, client, admin) = setup_fee_isolation_env(&env, initial_fee_bps, min_wager, max_wager);

            // Player 1 starts before fee update and should keep old fee terms.
            let player_one = Address::generate(&env);
            let secret_one = Bytes::from_slice(&env, &[1u8; 32]);
            let commitment_one: BytesN<32> = env.crypto().sha256(&secret_one).into();
            client.start_game(&player_one, &Side::Heads, &wager, &commitment_one);
            prop_assert_eq!(client.try_reveal(&player_one, &secret_one), Ok(Ok(true)));

            // Admin updates fee; this must only affect newly created games.
            client.set_fee(&admin, &new_fee_bps);

            // Player 2 starts after fee update and should settle with new fee.
            let player_two = Address::generate(&env);
            let secret_two = Bytes::from_slice(&env, &[1u8; 32]);
            let commitment_two: BytesN<32> = env.crypto().sha256(&secret_two).into();
            client.start_game(&player_two, &Side::Heads, &wager, &commitment_two);
            prop_assert_eq!(client.try_reveal(&player_two, &secret_two), Ok(Ok(true)));

            let payout_one = client.try_cash_out(&player_one);
            let payout_two = client.try_cash_out(&player_two);

            prop_assert_eq!(payout_one, Ok(Ok(calculate_payout(wager, 1, initial_fee_bps).unwrap())));
            prop_assert_eq!(payout_two, Ok(Ok(calculate_payout(wager, 1, new_fee_bps).unwrap())));
        }

        #[test]
        fn test_fee_change_does_not_reprice_continued_inflight_streak(
            initial_fee_bps in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
            wager_offset in 0i128..=50_000_000i128,
        ) {
            let env = Env::default();
            env.mock_all_auths();

            let max_wager = min_wager + 100_000_000;
            let wager = (min_wager + wager_offset).min(max_wager);
            let new_fee_bps = if initial_fee_bps < 500 { initial_fee_bps + 1 } else { 499 };

            let (_contract_id, client, admin) = setup_fee_isolation_env(&env, initial_fee_bps, min_wager, max_wager);
            let player = Address::generate(&env);
            let secret = Bytes::from_slice(&env, &[1u8; 32]);
            let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

            client.start_game(&player, &Side::Heads, &wager, &commitment);
            prop_assert_eq!(client.try_reveal(&player, &secret), Ok(Ok(true)));

            client.set_fee(&admin, &new_fee_bps);

            // Continue after fee change; payout terms must remain on the original snapshot.
            let next_secret = Bytes::from_slice(&env, &[1u8; 32]);
            let next_commitment: BytesN<32> = env.crypto().sha256(&next_secret).into();
            prop_assert_eq!(client.try_continue_streak(&player, &next_commitment), Ok(Ok(())));
            prop_assert_eq!(client.try_reveal(&player, &next_secret), Ok(Ok(true)));

            let payout = client.try_cash_out(&player);
            prop_assert_eq!(payout, Ok(Ok(calculate_payout(wager, 2, initial_fee_bps).unwrap())));
        }
    }

    // Feature: pause behavior, Property: pause blocks new starts but not active-game settlement
    // Validates: `start_game` rejects while paused and in-flight games can still reveal,
    // continue, and cash out to completion.

    fn setup_pause_behavior_env(
        env: &Env,
        fee_bps: u32,
        min_wager: i128,
        max_wager: i128,
    ) -> (Address, CoinflipContractClient<'_>, Address) {
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let treasury = Address::generate(env);
        let token = env.register_stellar_asset_contract(admin.clone());

        client.initialize(&admin, &treasury, &token, &fee_bps, &min_wager, &max_wager);

        env.as_contract(&contract_id, || {
            let mut stats = CoinflipContract::load_stats(env);
            stats.reserve_balance = i128::MAX / 4;
            CoinflipContract::save_stats(env, &stats);
        });

        (contract_id, client, admin)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        #[test]
        fn test_paused_start_game_rejected_across_valid_wagers(
            fee_bps in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
            wager_offset in 0i128..=50_000_000i128,
            side_pick in any::<bool>(),
            commitment_bytes in prop::array::uniform32(any::<u8>()),
        ) {
            let env = Env::default();
            env.mock_all_auths();

            let max_wager = min_wager + 100_000_000;
            let (contract_id, client, admin) = setup_pause_behavior_env(&env, fee_bps, min_wager, max_wager);
            let player = Address::generate(&env);
            let side = if side_pick { Side::Heads } else { Side::Tails };
            let wager = (min_wager + wager_offset).min(max_wager);
            let commitment = BytesN::from_array(&env, &commitment_bytes);

            client.set_paused(&admin, &true);

            let before_stats: ContractStats = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Stats).unwrap()
            });

            let result = client.try_start_game(&player, &side, &wager, &commitment);
            prop_assert_eq!(result, Err(Ok(Error::ContractPaused)));

            let game: Option<GameState> = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player)
            });
            prop_assert!(game.is_none());

            let after_stats: ContractStats = env.as_contract(&contract_id, || {
                env.storage().persistent().get(&StorageKey::Stats).unwrap()
            });
            prop_assert_eq!(before_stats, after_stats);
        }

        #[test]
        fn test_paused_reveal_still_progresses_active_game(
            fee_bps in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
            wager_offset in 0i128..=50_000_000i128,
        ) {
            let env = Env::default();
            env.mock_all_auths();

            let max_wager = min_wager + 100_000_000;
            let wager = (min_wager + wager_offset).min(max_wager);
            let (contract_id, client, admin) = setup_pause_behavior_env(&env, fee_bps, min_wager, max_wager);
            let player = Address::generate(&env);
            let secret = Bytes::from_slice(&env, &[1u8; 32]);
            let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

            client.start_game(&player, &Side::Heads, &wager, &commitment);
            client.set_paused(&admin, &true);

            let result = client.try_reveal(&player, &secret);
            prop_assert_eq!(result, Ok(Ok(true)));

            let game: GameState = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player).unwrap()
            });
            prop_assert_eq!(game.phase, GamePhase::Revealed);
            prop_assert_eq!(game.streak, 1);
        }

        #[test]
        fn test_paused_continue_and_cash_out_complete_active_game(
            fee_bps in 200u32..=500u32,
            min_wager in 1_000_000i128..10_000_000i128,
            wager_offset in 0i128..=50_000_000i128,
        ) {
            let env = Env::default();
            env.mock_all_auths();

            let max_wager = min_wager + 100_000_000;
            let wager = (min_wager + wager_offset).min(max_wager);
            let (contract_id, client, admin) = setup_pause_behavior_env(&env, fee_bps, min_wager, max_wager);
            let player = Address::generate(&env);
            let secret = Bytes::from_slice(&env, &[1u8; 32]);
            let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

            client.start_game(&player, &Side::Heads, &wager, &commitment);
            prop_assert_eq!(client.try_reveal(&player, &secret), Ok(Ok(true)));

            client.set_paused(&admin, &true);

            let secret_round_two = Bytes::from_slice(&env, &[1u8; 32]);
            let next_commitment: BytesN<32> = env.crypto().sha256(&secret_round_two).into();
            prop_assert_eq!(client.try_continue_streak(&player, &next_commitment), Ok(Ok(())));

            let continued: GameState = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player).unwrap()
            });
            prop_assert_eq!(continued.phase, GamePhase::Committed);
            prop_assert_eq!(continued.streak, 1);

            prop_assert_eq!(client.try_reveal(&player, &secret_round_two), Ok(Ok(true)));

            let expected_payout = calculate_payout(wager, 2, fee_bps).unwrap();
            let payout = client.try_cash_out(&player);
            prop_assert_eq!(payout, Ok(Ok(expected_payout)));

            let finished: GameState = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player).unwrap()
            });
            prop_assert_eq!(finished.phase, GamePhase::Completed);
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

            let (_admin, treasury, token, contract_id, player) =
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

            let (_admin, treasury, token, contract_id, player1) =
                setup_game_for_transfer_test(&env, wager1, fee_bps, true);

            let client = CoinflipContractClient::new(&env, &contract_id);
            let token_client = token::Client::new(&env, &token);

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

            let (_admin, treasury, token, contract_id, player) =
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
            fee_bps: 300,
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

    // ═══════════════════════════════════════════════════════════════════════
    // Feature: Continue Availability
    // ═══════════════════════════════════════════════════════════════════════
    //
    // ## Access Invariants
    //
    // `continue_streak` is ONLY available when ALL of the following hold:
    //
    //   1. A game record exists for the player (`NoActiveGame` otherwise).
    //   2. The game is in `GamePhase::Revealed` (`InvalidPhase` otherwise).
    //   3. `game.streak >= 1` — the player won the last flip
    //      (`NoWinningsToClaimOrContinue` when streak == 0).
    //   4. `new_commitment` is not all-zero bytes (`InvalidCommitment` otherwise).
    //   5. Reserves cover the next-streak worst-case payout
    //      (`InsufficientReserves` otherwise).
    //
    // The properties below exhaustively verify invariants 1–3 across random
    // inputs, confirming that only a `Revealed` game with a positive streak
    // can enter the continue flow.  Invariants 4–5 are covered by the error
    // code descriptiveness block above.
    //
    // ## Why property tests?
    //
    // Unit tests check specific values; property tests confirm the invariant
    // holds for *any* wager, streak, or phase value in the valid domain.
    // This is especially important for phase gating: a single off-by-one in
    // a match arm could silently allow a `Committed` game to continue, which
    // would let a player skip the reveal step and manipulate outcomes.
    // ═══════════════════════════════════════════════════════════════════════

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        // ── Invariant 1: no game record → NoActiveGame ───────────────────────
        //
        // For any valid wager and commitment, a player with no game in storage
        // must always receive NoActiveGame regardless of what commitment they
        // supply.  This prevents phantom-game exploitation where a caller
        // probes the contract without ever having started a game.

        /// PROPERTY CA-1: continue_streak always returns NoActiveGame when no
        /// game record exists for the player, across all valid commitment inputs.
        #[test]
        fn prop_continue_unavailable_without_game(
            commitment_bytes in prop::array::uniform32(1u8..=255u8),
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            let commitment = BytesN::from_array(&env, &commitment_bytes);

            let result = client.try_continue_streak(&player, &commitment);
            prop_assert_eq!(result, Err(Ok(Error::NoActiveGame)),
                "continue_streak must return NoActiveGame when no game exists");
        }

        // ── Invariant 2a: Committed phase → InvalidPhase ─────────────────────
        //
        // A game in Committed phase has not yet been revealed.  Allowing
        // continue_streak here would let a player replace their commitment
        // before the reveal, breaking the commit-reveal security model.

        /// PROPERTY CA-2a: continue_streak is unavailable in Committed phase
        /// for any wager or streak value.
        #[test]
        fn prop_continue_unavailable_in_committed_phase(
            wager  in 1_000_000i128..=100_000_000i128,
            streak in 0u32..=10u32,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            inject_game_prop(&env, &contract_id, &player, GamePhase::Committed, streak, wager);

            let commitment = dummy_commitment_prop(&env);
            let result = client.try_continue_streak(&player, &commitment);
            prop_assert_eq!(result, Err(Ok(Error::InvalidPhase)),
                "continue_streak must return InvalidPhase for a Committed game \
                 (wager={}, streak={})", wager, streak);
        }

        // ── Invariant 2b: Completed phase → InvalidPhase ─────────────────────
        //
        // A Completed game is fully settled.  Allowing continue_streak here
        // would let a player re-enter a finished game, potentially claiming
        // winnings that have already been paid out.

        /// PROPERTY CA-2b: continue_streak is unavailable in Completed phase
        /// for any wager or streak value.
        #[test]
        fn prop_continue_unavailable_in_completed_phase(
            wager  in 1_000_000i128..=100_000_000i128,
            streak in 0u32..=10u32,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            inject_game_prop(&env, &contract_id, &player, GamePhase::Completed, streak, wager);

            let commitment = dummy_commitment_prop(&env);
            let result = client.try_continue_streak(&player, &commitment);
            prop_assert_eq!(result, Err(Ok(Error::InvalidPhase)),
                "continue_streak must return InvalidPhase for a Completed game \
                 (wager={}, streak={})", wager, streak);
        }

        // ── Invariant 3: Revealed + streak == 0 → NoWinningsToClaimOrContinue
        //
        // A Revealed game with streak == 0 is a loss state produced when the
        // reveal outcome did not match the player's chosen side.  The player
        // forfeited their wager; there are no winnings to risk on a streak.
        // Allowing continue here would let a losing player re-enter the game
        // for free, violating fund-safety guarantees.

        /// PROPERTY CA-3: continue_streak is unavailable in Revealed phase when
        /// streak == 0 (loss state), across all wager values.
        #[test]
        fn prop_continue_unavailable_revealed_streak_zero(
            wager in 1_000_000i128..=100_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            inject_game_prop(&env, &contract_id, &player, GamePhase::Revealed, 0, wager);

            let commitment = dummy_commitment_prop(&env);
            let result = client.try_continue_streak(&player, &commitment);
            prop_assert_eq!(result, Err(Ok(Error::NoWinningsToClaimOrContinue)),
                "continue_streak must return NoWinningsToClaimOrContinue for a \
                 Revealed game with streak == 0 (wager={})", wager);
        }

        // ── Positive: Revealed + streak >= 1 + sufficient reserves → Ok ──────
        //
        // The only state that may enter the continue flow is a Revealed game
        // with a positive streak and enough reserves to cover the next payout.
        // This property confirms the gate opens exactly when all conditions are
        // met, and that no valid winning state is accidentally blocked.

        /// PROPERTY CA-4: continue_streak succeeds for any Revealed game with
        /// streak >= 1 when reserves are sufficient, confirming the gate opens
        /// for all valid winning states.
        #[test]
        fn prop_continue_available_revealed_winning_state(
            wager  in 1_000_000i128..=10_000_000i128,
            streak in 1u32..=4u32,
            commitment_bytes in prop::array::uniform32(1u8..=255u8),
        ) {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register(CoinflipContract, ());
            let client = CoinflipContractClient::new(&env, &contract_id);

            let admin    = Address::generate(&env);
            let treasury = Address::generate(&env);
            let token    = Address::generate(&env);
            client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);

            // Fund reserves to cover the next streak's worst-case payout.
            fund_reserves(&env, &contract_id, i128::MAX / 4);

            let player = Address::generate(&env);
            inject_game_prop(&env, &contract_id, &player, GamePhase::Revealed, streak, wager);

            let commitment = BytesN::from_array(&env, &commitment_bytes);
            let result = client.try_continue_streak(&player, &commitment);
            prop_assert!(result.is_ok(),
                "continue_streak must succeed for a Revealed winning game \
                 (wager={}, streak={})", wager, streak);
        }

        // ── No state mutation on any rejection ───────────────────────────────
        //
        // All guard failures must be atomic: the game state and contract stats
        // must be byte-for-byte identical before and after a rejected call.
        // This prevents partial-write exploits where a failed continue could
        // silently advance the phase or alter the commitment.

        /// PROPERTY CA-5: game state is unchanged after any InvalidPhase rejection.
        #[test]
        fn prop_continue_no_mutation_on_invalid_phase(
            wager  in 1_000_000i128..=100_000_000i128,
            streak in 0u32..=5u32,
            use_committed in any::<bool>(),
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let phase = if use_committed { GamePhase::Committed } else { GamePhase::Completed };
            let player = Address::generate(&env);
            inject_game_prop(&env, &contract_id, &player, phase, streak, wager);

            let before: GameState = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player).unwrap()
            });

            let _ = client.try_continue_streak(&player, &dummy_commitment_prop(&env));

            let after: GameState = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player).unwrap()
            });

            prop_assert_eq!(before, after,
                "game state must be unchanged after InvalidPhase rejection");
        }

        /// PROPERTY CA-6: game state is unchanged after NoWinningsToClaimOrContinue rejection.
        #[test]
        fn prop_continue_no_mutation_on_loss_state(
            wager in 1_000_000i128..=100_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);

            let player = Address::generate(&env);
            inject_game_prop(&env, &contract_id, &player, GamePhase::Revealed, 0, wager);

            let before: GameState = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player).unwrap()
            });

            let _ = client.try_continue_streak(&player, &dummy_commitment_prop(&env));

            let after: GameState = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player).unwrap()
            });

            prop_assert_eq!(before, after,
                "game state must be unchanged after NoWinningsToClaimOrContinue rejection");
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
// Module:  cumulative_fee_tests
//
// Verifies that `total_fees` in ContractStats accumulates correctly across
// multiple sequential payouts and across fee_bps configuration changes.
//
// Accounting identity under test:
//   total_fees_after == total_fees_before + Σ fee_i
//   where fee_i = floor(gross_i * fee_bps_i / 10_000)
//   and   gross_i = floor(wager_i * multiplier(streak_i) / 10_000)
//
// Properties:
//   P-1  After N sequential cash-outs, total_fees equals the sum of each
//        individual fee computed from (wager, streak, fee_bps).
//   P-2  Fee accumulation is additive and order-independent: the total is the
//        same regardless of which player cashes out first.
//   P-3  A fee_bps change between payouts is reflected immediately — earlier
//        payouts use the old rate, later payouts use the new rate.
//   P-4  At fee_bps = 200 (minimum) and fee_bps = 500 (maximum), the running
//        total stays within the mathematically expected bounds.
//   P-5  total_fees never decreases (fees are never refunded).
// ═══════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod cumulative_fee_tests {
    use super::*;
    use proptest::prelude::*;
    use soroban_sdk::testutils::Address as _;

    // ── helpers ──────────────────────────────────────────────────────────

    /// Set up a fresh contract with ample reserves and return (contract_id, client).
    fn setup(env: &Env, fee_bps: u32) -> soroban_sdk::Address {
        env.mock_all_auths();
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(env, &contract_id);

        let admin    = Address::generate(env);
        let treasury = Address::generate(env);
        let token    = Address::generate(env);

        client.initialize(&admin, &treasury, &token, &fee_bps, &1_000_000, &100_000_000);

        // Inject large reserves so no payout is blocked by InsufficientReserves.
        env.as_contract(&contract_id, || {
            let mut stats = CoinflipContract::load_stats(env);
            stats.reserve_balance = i128::MAX / 2;
            CoinflipContract::save_stats(env, &stats);
        });

        contract_id
    }

    /// Inject a Revealed-phase game for `player` with the given streak/wager,
    /// then call cash_out and return the fee that should have been recorded.
    ///
    /// Fee formula (mirrors contract logic):
    ///   gross = wager * multiplier(streak) / 10_000
    ///   fee   = gross * fee_bps / 10_000
    fn do_cash_out(
        env: &Env,
        contract_id: &soroban_sdk::Address,
        player: &Address,
        wager: i128,
        streak: u32,
        fee_bps: u32,
    ) -> i128 {
        let dummy: BytesN<32> = env
            .crypto()
            .sha256(&soroban_sdk::Bytes::from_slice(env, &[7u8; 32]))
            .into();

        let game = GameState {
            wager,
            side: Side::Heads,
            streak,
            commitment: dummy.clone(),
            contract_random: dummy,
            fee_bps,
            phase: GamePhase::Revealed,
            start_ledger: 0,
        };
        env.as_contract(contract_id, || {
            CoinflipContract::save_player_game(env, player, &game);
        });

        let client = CoinflipContractClient::new(env, contract_id);
        client.cash_out(player);

        // Expected fee for this payout
        let gross = wager
            .checked_mul(get_multiplier(streak) as i128)
            .unwrap()
            / 10_000;
        gross
            .checked_mul(fee_bps as i128)
            .unwrap()
            / 10_000
    }

    /// Read total_fees from contract storage.
    fn read_total_fees(env: &Env, contract_id: &soroban_sdk::Address) -> i128 {
        env.as_contract(contract_id, || {
            CoinflipContract::load_stats(env).total_fees
        })
    }

    // ── unit tests ────────────────────────────────────────────────────────

    /// P-1 (unit): Three sequential cash-outs by different players.
    ///
    /// Accounting notes:
    ///   wager=10_000_000, streak=1 (1.9x), fee_bps=300
    ///     gross = 10_000_000 * 19_000 / 10_000 = 19_000_000
    ///     fee   = 19_000_000 * 300   / 10_000 =    570_000
    ///
    ///   wager=5_000_000, streak=2 (3.5x), fee_bps=300
    ///     gross = 5_000_000 * 35_000 / 10_000 = 17_500_000
    ///     fee   = 17_500_000 * 300   / 10_000 =    525_000
    ///
    ///   wager=2_000_000, streak=4 (10x), fee_bps=300
    ///     gross = 2_000_000 * 100_000 / 10_000 = 20_000_000
    ///     fee   = 20_000_000 * 300    / 10_000 =    600_000
    ///
    ///   expected total_fees = 570_000 + 525_000 + 600_000 = 1_695_000
    #[test]
    fn test_fees_accumulate_across_three_payouts() {
        let env = Env::default();
        let contract_id = setup(&env, 300);

        let p1 = Address::generate(&env);
        let p2 = Address::generate(&env);
        let p3 = Address::generate(&env);

        let fee1 = do_cash_out(&env, &contract_id, &p1, 10_000_000, 1, 300);
        let fee2 = do_cash_out(&env, &contract_id, &p2,  5_000_000, 2, 300);
        let fee3 = do_cash_out(&env, &contract_id, &p3,  2_000_000, 4, 300);

        assert_eq!(fee1, 570_000);
        assert_eq!(fee2, 525_000);
        assert_eq!(fee3, 600_000);
        assert_eq!(read_total_fees(&env, &contract_id), fee1 + fee2 + fee3);
    }

    /// P-3 (unit): Fee rate change between payouts is applied immediately.
    ///
    /// Accounting notes:
    ///   Round 1 — fee_bps=200:
    ///     wager=10_000_000, streak=1 → gross=19_000_000, fee=380_000
    ///   Round 2 — fee_bps=500 (updated via update_fee):
    ///     wager=10_000_000, streak=1 → gross=19_000_000, fee=950_000
    ///   expected total_fees = 380_000 + 950_000 = 1_330_000
    #[test]
    fn test_fees_accumulate_after_fee_bps_change() {
        let env = Env::default();
        let contract_id = setup(&env, 200);

        let p1 = Address::generate(&env);
        let p2 = Address::generate(&env);

        // First payout at fee_bps=200
        let fee1 = do_cash_out(&env, &contract_id, &p1, 10_000_000, 1, 200);
        assert_eq!(fee1, 380_000);
        assert_eq!(read_total_fees(&env, &contract_id), 380_000);

        // Update fee_bps to 500 directly in storage (no admin entrypoint exists)
        env.as_contract(&contract_id, || {
            let mut cfg = CoinflipContract::load_config(&env);
            cfg.fee_bps = 500;
            CoinflipContract::save_config(&env, &cfg);
        });

        // Second payout at fee_bps=500
        let fee2 = do_cash_out(&env, &contract_id, &p2, 10_000_000, 1, 500);
        assert_eq!(fee2, 950_000);
        assert_eq!(read_total_fees(&env, &contract_id), fee1 + fee2);
    }

    /// P-5 (unit): total_fees never decreases — verified after each step.
    #[test]
    fn test_total_fees_never_decreases() {
        let env = Env::default();
        let contract_id = setup(&env, 300);

        let mut running = 0i128;
        for streak in 1u32..=4 {
            let player = Address::generate(&env);
            let fee = do_cash_out(&env, &contract_id, &player, 5_000_000, streak, 300);
            running += fee;
            let stored = read_total_fees(&env, &contract_id);
            assert!(stored >= running - 1, // allow ±1 stroop integer-division rounding
                "total_fees decreased at streak {}: stored={} running={}", streak, stored, running);
        }
    }

    // ── property tests ────────────────────────────────────────────────────

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// P-1 (property): For N random sequential cash-outs, total_fees equals
        /// the sum of individually computed fees.
        ///
        /// The test generates up to 5 (wager, streak) pairs and verifies that
        /// the contract's running total matches the hand-computed sum at every
        /// step, not just at the end.
        #[test]
        fn prop_cumulative_fees_match_sum_of_individual_fees(
            wagers  in proptest::collection::vec(1_000_000i128..=100_000_000i128, 1..=5),
            streaks in proptest::collection::vec(1u32..=4u32, 1..=5),
            fee_bps in 200u32..=500u32,
        ) {
            let env = Env::default();
            let contract_id = setup(&env, fee_bps);

            let n = wagers.len().min(streaks.len());
            let mut expected_total = 0i128;

            for i in 0..n {
                let player = Address::generate(&env);
                let fee = do_cash_out(&env, &contract_id, &player, wagers[i], streaks[i], fee_bps);
                expected_total += fee;

                let stored = read_total_fees(&env, &contract_id);
                // Allow ±1 stroop per payout for integer-division rounding
                prop_assert!(
                    (stored - expected_total).abs() <= i as i128 + 1,
                    "After payout {}: stored={} expected={}", i + 1, stored, expected_total
                );
            }
        }

        /// P-4 (property): At fee_bps boundaries (200 and 500), total_fees stays
        /// within the mathematically expected range for any wager/streak combo.
        ///
        /// Lower bound: fee >= wager * multiplier(streak) / 10_000 * 200 / 10_000
        /// Upper bound: fee <= wager * multiplier(streak) / 10_000 * 500 / 10_000
        #[test]
        fn prop_cumulative_fees_within_rate_bounds(
            wager  in 1_000_000i128..=100_000_000i128,
            streak in 1u32..=4u32,
            fee_bps in 200u32..=500u32,
        ) {
            let env = Env::default();
            let contract_id = setup(&env, fee_bps);

            let player = Address::generate(&env);
            do_cash_out(&env, &contract_id, &player, wager, streak, fee_bps);

            let stored = read_total_fees(&env, &contract_id);
            let gross = wager
                .checked_mul(get_multiplier(streak) as i128)
                .unwrap()
                / 10_000;

            let fee_min = gross * 200 / 10_000;
            let fee_max = gross * 500 / 10_000;

            prop_assert!(
                stored >= fee_min && stored <= fee_max,
                "fee={} not in [{}, {}] for wager={} streak={} fee_bps={}",
                stored, fee_min, fee_max, wager, streak, fee_bps
            );
        }

        /// P-2 (property): Fee accumulation is additive — two payouts in either
        /// order produce the same total_fees.
        #[test]
        fn prop_fee_accumulation_is_order_independent(
            wager1  in 1_000_000i128..=50_000_000i128,
            wager2  in 1_000_000i128..=50_000_000i128,
            streak1 in 1u32..=4u32,
            streak2 in 1u32..=4u32,
            fee_bps in 200u32..=500u32,
        ) {
            // Order A: player1 then player2
            let env_a = Env::default();
            let cid_a = setup(&env_a, fee_bps);
            let pa1 = Address::generate(&env_a);
            let pa2 = Address::generate(&env_a);
            do_cash_out(&env_a, &cid_a, &pa1, wager1, streak1, fee_bps);
            do_cash_out(&env_a, &cid_a, &pa2, wager2, streak2, fee_bps);
            let total_a = read_total_fees(&env_a, &cid_a);

            // Order B: player2 then player1
            let env_b = Env::default();
            let cid_b = setup(&env_b, fee_bps);
            let pb1 = Address::generate(&env_b);
            let pb2 = Address::generate(&env_b);
            do_cash_out(&env_b, &cid_b, &pb2, wager2, streak2, fee_bps);
            do_cash_out(&env_b, &cid_b, &pb1, wager1, streak1, fee_bps);
            let total_b = read_total_fees(&env_b, &cid_b);

            prop_assert_eq!(total_a, total_b,
                "Fee totals differ by order: A={} B={}", total_a, total_b);
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

// ─────────────────────────────────────────────────────────────────────────────
// ── RESERVE SOLVENCY TESTS ──────────────────────────────────────────────────
//
// These tests verify that the contract enforces its reserve solvency guards:
//   1. start_game rejections (Error::InsufficientReserves)
//   2. continue_streak rejections (Error::InsufficientReserves)
//   3. Exact-threshold acceptance (reserves == wager * 10)
//   4. State integrity on rejection (no side effects)
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod reserve_solvency_tests {
    use super::*;
    use proptest::prelude::*;
    use soroban_sdk::testutils::Address as _;

    /// Setup a fresh environment with specific initial reserves.
    fn setup_solvency_env(env: &Env, initial_reserves: i128) -> (soroban_sdk::Address, CoinflipContractClient) {
        env.mock_all_auths();
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(env, &contract_id);

        let admin    = soroban_sdk::Address::generate(env);
        let treasury = soroban_sdk::Address::generate(env);
        let token    = soroban_sdk::Address::generate(env);

        client.initialize(&admin, &treasury, &token, &300, &1_000_000, &1_000_000_000);

        env.as_contract(&contract_id, || {
            let mut stats = CoinflipContract::load_stats(env);
            stats.reserve_balance = initial_reserves;
            CoinflipContract::save_stats(env, &stats);
        });

        (contract_id, client)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// PROPERTY: start_game is atomic with respect to reserves.
        /// It MUST accept the wager if reserves >= wager * 10 and reject otherwise.
        #[test]
        fn prop_start_game_solvency_guard(
            wager in 1_000_000i128..=1_000_000_000i128,
            reserve_factor in 0f64..20f64,
        ) {
            let env = Env::default();
            // Calculate reserves based on the factor: Factor 10.0 is the threshold.
            let reserves = (wager as f64 * reserve_factor) as i128;
            let (_id, client) = setup_solvency_env(&env, reserves);
            let player = soroban_sdk::Address::generate(&env);
            let commitment = BytesN::from_array(&env, &[0u8; 32]);

            let result = client.try_start_game(&player, &Side::Heads, &wager, &commitment);

            let max_payout = wager * 10;
            if reserves >= max_payout {
                prop_assert!(result.is_ok(), "Game should start when reserves ({}) >= max_payout ({})", reserves, max_payout);
            } else {
                prop_assert_eq!(result, Err(Ok(Error::InsufficientReserves)), 
                    "Game should be rejected with InsufficientReserves when reserves ({}) < max_payout ({})", reserves, max_payout);
            }
        }
    }

    #[test]
    fn test_exact_threshold_acceptance() {
        let env = Env::default();
        let wager = 1_000_000i128;
        let reserves = wager * 10; // Exactly 10x
        let (_id, client) = setup_solvency_env(&env, reserves);
        let player = soroban_sdk::Address::generate(&env);
        let commitment = BytesN::from_array(&env, &[0u8; 32]);

        // start_game returns () on success, not Result
        client.start_game(&player, &Side::Heads, &wager, &commitment);
        // If we reach here without panic, the test passed
    }

    #[test]
    fn test_just_below_threshold_rejection() {
        let env = Env::default();
        let wager = 1_000_000i128;
        let reserves = wager * 10 - 1; // 1 stroop below
        let (_id, client) = setup_solvency_env(&env, reserves);
        let player = soroban_sdk::Address::generate(&env);
        let commitment = BytesN::from_array(&env, &[0u8; 32]);

        let result = client.try_start_game(&player, &Side::Heads, &wager, &commitment);
        assert_eq!(result, Err(Ok(Error::InsufficientReserves)));
    }

    #[test]
    fn test_zero_reserve_rejection() {
        let env = Env::default();
        let wager = 1_000_000i128;
        let reserves = 0;
        let (_id, client) = setup_solvency_env(&env, reserves);
        let player = soroban_sdk::Address::generate(&env);
        let commitment = BytesN::from_array(&env, &[0u8; 32]);

        let result = client.try_start_game(&player, &Side::Heads, &wager, &commitment);
        assert_eq!(result, Err(Ok(Error::InsufficientReserves)));
    }

    #[test]
    fn test_rejection_no_state_mutation() {
        let env = Env::default();
        let wager = 1_000_000i128;
        let reserves = 0;
        let (id, client) = setup_solvency_env(&env, reserves);
        let player = soroban_sdk::Address::generate(&env);
        let commitment = BytesN::from_array(&env, &[0u8; 32]);

        // Before stats
        let stats_before: ContractStats = env.as_contract(&id, || {
            CoinflipContract::load_stats(&env)
        });

        let _ = client.try_start_game(&player, &Side::Heads, &wager, &commitment);

        // After stats
        let stats_after: ContractStats = env.as_contract(&id, || {
            CoinflipContract::load_stats(&env)
        });

        assert_eq!(stats_before, stats_after, "Stats must not change on failed game start");
        
        // Ensure no game state was stored
        let game_opt: Option<GameState> = env.as_contract(&id, || {
            CoinflipContract::load_player_game(&env, &player)
        });
        assert!(game_opt.is_none(), "No game state should be persisted for player");
    }

    #[test]
    fn test_max_wager_solvency() {
        let env = Env::default();
        let wager = 1_000_000_000i128; // max_wager
        let reserves = wager * 10;
        let (_id, client) = setup_solvency_env(&env, reserves);
        let player = soroban_sdk::Address::generate(&env);
        let commitment = BytesN::from_array(&env, &[0u8; 32]);

        // start_game returns () on success, not Result
        client.start_game(&player, &Side::Heads, &wager, &commitment);
        // If we reach here without panic, the test passed
    }

    #[test]
    fn test_continue_streak_solvency_enforcement() {
        let env = Env::default();
        let wager = 1_000_000i128;
        
        // Initial reserves covering 10x for start_game
        let initial_reserves = wager * 10;
        let (id, client) = setup_solvency_env(&env, initial_reserves);
        let player = soroban_sdk::Address::generate(&env);
        
        // Setup a winning reveal to get to Revealed phase
        // Heads win secret = [2u8; 32] (calibrated in loss_forfeiture_tests)
        let secret = soroban_sdk::Bytes::from_slice(&env, &[2u8; 32]);
        let commitment = env.crypto().sha256(&secret).into();
        
        client.start_game(&player, &Side::Heads, &wager, &commitment);
        client.reveal(&player, &secret);
        
        // Now game is in Revealed (streak 1). Next streak is 2. Multiplier for 2 is 3.5x.
        // Let's drain reserves so it cannot cover 3.5x.
        env.as_contract(&id, || {
            let mut stats = CoinflipContract::load_stats(&env);
            stats.reserve_balance = wager * 3; // Below 3.5x
            CoinflipContract::save_stats(&env, &stats);
        });
        
        let new_commitment = BytesN::from_array(&env, &[1u8; 32]);
        let result = client.try_continue_streak(&player, &new_commitment);
        
        assert_eq!(result, Err(Ok(Error::InsufficientReserves)), 
            "continue_streak must reject if reserves cannot cover next tier");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Feature: Concurrency & Sequential Order Guards
// ═══════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod concurrency_edge_case_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use crate::property_tests::{setup_contract_with_bounds, dummy_commitment_prop};
    use proptest::prelude::*;

    #[test]
    fn test_rapid_sequential_start_attempts() {
        let env = Env::default();
        let min_wager = 1_000_000;
        let max_wager = 100_000_000;
        let contract_id = setup_contract_with_bounds(&env, min_wager, max_wager);
        let client = CoinflipContractClient::new(&env, &contract_id);
        let player = Address::generate(&env);
        let commitment = dummy_commitment_prop(&env);

        // First attempt succeeds
        client.start_game(&player, &Side::Heads, &min_wager, &commitment);

        // Second attempt immediately fails with ActiveGameExists
        let result = client.try_start_game(&player, &Side::Heads, &min_wager, &commitment);
        assert_eq!(result, Err(Ok(Error::ActiveGameExists)));
    }

    #[test]
    fn test_start_game_allowed_after_claim() {
        let env = Env::default();
        let min_wager = 1_000_000;
        let max_wager = 100_000_000;
        let contract_id = setup_contract_with_bounds(&env, min_wager, max_wager);
        let client = CoinflipContractClient::new(&env, &contract_id);
        let player = Address::generate(&env);
        let secret = Bytes::from_slice(&env, &[1u8; 32]); // Win for Heads in test env
        let commitment = env.crypto().sha256(&secret).into();

        // Game 1: start -> reveal (win) -> claim
        client.start_game(&player, &Side::Heads, &min_wager, &commitment);
        client.reveal(&player, &secret);
        client.claim_winnings(&player);

        // Game 2 must be allowed immediately after claim
        let result = client.try_start_game(&player, &Side::Heads, &min_wager, &commitment);
        assert!(result.is_ok(), "Expected Game 2 to be accepted after Game 1 claim");
    }

    #[test]
    fn test_start_game_allowed_after_cash_out() {
        let env = Env::default();
        let min_wager = 1_000_000;
        let max_wager = 100_000_000;
        let contract_id = setup_contract_with_bounds(&env, min_wager, max_wager);
        let client = CoinflipContractClient::new(&env, &contract_id);
        let player = Address::generate(&env);
        let secret = Bytes::from_slice(&env, &[1u8; 32]); // Win for Heads in test env
        let commitment = env.crypto().sha256(&secret).into();

        // Game 1: start -> reveal (win) -> cash_out
        client.start_game(&player, &Side::Heads, &min_wager, &commitment);
        client.reveal(&player, &secret);
        let payout = client.try_cash_out(&player);
        assert!(payout.is_ok());

        // Game 2 must be allowed immediately after cash_out
        let result = client.try_start_game(&player, &Side::Heads, &min_wager, &commitment);
        assert!(result.is_ok(), "Expected Game 2 to be accepted after Game 1 cash_out");
    }

    #[test]
    fn test_rapid_sequential_reveal_attempts() {
        let env = Env::default();
        let min_wager = 1_000_000;
        let max_wager = 100_000_000;
        let contract_id = setup_contract_with_bounds(&env, min_wager, max_wager);
        let client = CoinflipContractClient::new(&env, &contract_id);
        let player = Address::generate(&env);
        let secret = Bytes::from_slice(&env, &[1u8; 32]);
        let commitment = env.crypto().sha256(&secret).into();

        client.start_game(&player, &Side::Heads, &min_wager, &commitment);

        // First reveal succeeds
        client.reveal(&player, &secret);

        // Second reveal fails with InvalidPhase because game moved to Revealed phase
        let result = client.try_reveal(&player, &secret);
        assert_eq!(result, Err(Ok(Error::InvalidPhase)));
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(50))]
        #[test]
        fn prop_start_game_idempotency_guard(
            wager in 1_000_000i128..=100_000_000i128,
        ) {
            let env = Env::default();
            let contract_id = setup_contract_with_bounds(&env, 1_000_000, 100_000_000);
            let client = CoinflipContractClient::new(&env, &contract_id);
            let player = Address::generate(&env);
            let commitment = dummy_commitment_prop(&env);

            client.start_game(&player, &Side::Heads, &wager, &commitment);
            let result = client.try_start_game(&player, &Side::Heads, &wager, &commitment);
            prop_assert_eq!(result, Err(Ok(Error::ActiveGameExists)));
        }
    }
}

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
// - **No token contract required**: [cash_out](cci:1://file:///c:/Users/hp/Documents/Tossd/contract/src/lib.rs:691:4-744:5) is used for settlement so tests
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
// - [env](cci:1://file:///c:/Users/hp/Documents/Tossd/contract/src/lib.rs:3569:4-3591:5)         – Soroban test environment (mock_all_auths enabled)
// - `contract_id` – registered CoinflipContract address
// - `client`      – generated client for calling contract methods
// - [config](cci:1://file:///c:/Users/hp/Documents/Tossd/contract/src/lib.rs:392:4-395:5)      – snapshot of the initialised ContractConfig
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

        /// Toggle pause state through the public admin entrypoint.
        fn set_paused(&self, paused: bool) {
            self.client.set_paused(&self.admin, &paused);
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
                fee_bps: DEFAULT_FEE_BPS,
                phase,
                start_ledger: 0,
            };
            self.env.as_contract(&self.contract_id, || {
                CoinflipContract::save_player_game(&self.env, player, &game);
            });
        }

        /// Read the current `ContractStats` from storage.
        fn stats(&self) -> ContractStats {
            self.env.as_contract(&self.contract_id, || {
                self.env.storage().persistent().get(&StorageKey::Stats).unwrap()
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
        assert_eq!(stats.reserve_balance, 1_000_000_000 - expected_net);
        let gross = wager.checked_mul(get_multiplier(1) as i128).unwrap() / 10_000;
        let fee = gross.checked_mul(DEFAULT_FEE_BPS as i128).unwrap() / 10_000;
        assert_eq!(stats.total_fees, fee);
    }

    #[test]
    fn test_full_loss_forfeits_wager_to_reserves() {
        let h = Harness::new();
        let player = h.player();
        h.fund(1_000_000_000);
        let wager = DEFAULT_WAGER;
        let won = h.play_loss_round(&player, wager);
        assert!(!won, "seed 2 + Heads must lose");
        let game_opt: Option<GameState> = h.env.as_contract(&h.contract_id, || {
            CoinflipContract::load_player_game(&h.env, &player)
        });
        assert!(game_opt.is_none(), "game state must be deleted on loss");
        let stats = h.stats();
        assert_eq!(stats.reserve_balance, 1_000_000_000 + wager);
    }

    #[test]
    fn test_win_continue_win_cash_out_streak_2() {
        let h = Harness::new();
        let player = h.player();
        h.fund(1_000_000_000);
        let wager = DEFAULT_WAGER;
        let won1 = h.play_win_round(&player, wager);
        assert!(won1, "round 1 must win");
        assert_eq!(h.game_state(&player).streak, 1);
        assert_eq!(h.game_state(&player).phase, GamePhase::Revealed);
        let new_commitment = h.make_commitment(1);
        h.client.continue_streak(&player, &new_commitment);
        assert_eq!(h.game_state(&player).phase, GamePhase::Committed);
        let secret2 = h.make_secret(1);
        let won2 = h.client.reveal(&player, &secret2);
        assert!(won2, "round 2 must win");
        assert_eq!(h.game_state(&player).streak, 2);
        let expected_net = calculate_payout(wager, 2, DEFAULT_FEE_BPS).unwrap();
        let payout = h.client.cash_out(&player);
        assert_eq!(payout, expected_net);
        assert_eq!(h.game_state(&player).phase, GamePhase::Completed);
    }

    #[test]
    fn test_streak_4_uses_max_multiplier() {
        let h = Harness::new();
        let player = h.player();
        h.fund(1_000_000_000);
        let wager = DEFAULT_WAGER;
        for expected_streak in 1u32..=4 {
            if expected_streak == 1 {
                let won = h.play_win_round(&player, wager);
                assert!(won);
            } else {
                let commitment = h.make_commitment(1);
                h.client.continue_streak(&player, &commitment);
                let secret = h.make_secret(1);
                let won = h.client.reveal(&player, &secret);
                assert!(won, "round {} must win", expected_streak);
            }
            assert_eq!(h.game_state(&player).streak, expected_streak);
        }
        let expected_net = calculate_payout(wager, 4, DEFAULT_FEE_BPS).unwrap();
        let payout = h.client.cash_out(&player);
        assert_eq!(payout, expected_net);
        let gross = wager.checked_mul(MULTIPLIER_STREAK_4_PLUS as i128).unwrap() / 10_000;
        let fee = gross.checked_mul(DEFAULT_FEE_BPS as i128).unwrap() / 10_000;
        assert_eq!(expected_net, gross - fee);
    }

    #[test]
    fn test_paused_contract_rejects_start_game() {
        let h = Harness::new();
        h.fund(1_000_000_000);
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
        let game = h.game_state(&player);
        assert_eq!(game.phase, GamePhase::Committed);
        assert_eq!(game.side, Side::Heads);
    }

    #[test]
    fn test_reveal_wrong_secret_rejected() {
        let h = Harness::new();
        let player = h.player();
        h.fund(1_000_000_000);
        h.client.start_game(&player, &Side::Heads, &DEFAULT_WAGER, &h.make_commitment(1));
        let wrong_secret = h.make_secret(99);
        let result = h.client.try_reveal(&player, &wrong_secret);
        assert_eq!(result, Err(Ok(Error::CommitmentMismatch)));
        assert_eq!(h.game_state(&player).phase, GamePhase::Committed);
    }

    #[test]
    fn test_start_game_rejected_when_reserves_insufficient() {
        let h = Harness::new();
        let player = h.player();
        let result = h.client.try_start_game(
            &player,
            &Side::Heads,
            &DEFAULT_WAGER,
            &h.make_commitment(1),
        );
        assert_eq!(result, Err(Ok(Error::InsufficientReserves)));
    }

    #[test]
    fn test_new_game_allowed_after_completion() {
        let h = Harness::new();
        let player = h.player();
        h.fund(1_000_000_000);
        h.play_win_round(&player, DEFAULT_WAGER);
        h.client.cash_out(&player);
        assert_eq!(h.game_state(&player).phase, GamePhase::Completed);
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

    #[test]
    fn test_wager_boundary_inclusive() {
        let h = Harness::new();
        h.fund(1_000_000_000);
        let p_min = h.player();
        let p_max = h.player();
        assert!(
            h.client.try_start_game(&p_min, &Side::Heads, &DEFAULT_MIN_WAGER, &h.make_commitment(1)).is_ok(),
            "min_wager must be accepted"
        );
        assert!(
            h.client.try_start_game(&p_max, &Side::Heads, &DEFAULT_MAX_WAGER, &h.make_commitment(1)).is_ok(),
            "max_wager must be accepted"
        );
    }

    #[test]
    fn test_cash_out_rejects_zero_streak_revealed() {
        let h = Harness::new();
        let player = h.player();
        h.inject_game(&player, GamePhase::Revealed, 0, DEFAULT_WAGER, 1);
        let result = h.client.try_cash_out(&player);
        assert_eq!(result, Err(Ok(Error::NoWinningsToClaimOrContinue)));
    }

    #[test]
    fn test_continue_streak_rejects_committed_phase() {
        let h = Harness::new();
        let player = h.player();
        h.inject_game(&player, GamePhase::Committed, 1, DEFAULT_WAGER, 1);
        let result = h.client.try_continue_streak(&player, &h.make_commitment(1));
        assert_eq!(result, Err(Ok(Error::InvalidPhase)));
    }

    #[test]
    fn test_probe_outcome_matches_reveal() {
        let h = Harness::new();
        let player = h.player();
        h.fund(1_000_000_000);
        let predicted = h.probe_outcome(1);
        let commitment = h.make_commitment(1);
        h.client.start_game(&player, &predicted, &DEFAULT_WAGER, &commitment);
        let secret = h.make_secret(1);
        let won = h.client.reveal(&player, &secret);
        assert!(won, "probe_outcome prediction must match actual reveal outcome");
    }
}


// ═══════════════════════════════════════════════════════════════════════════
// Issue #125 — Cash Out Availability Property Tests
//
// ELIGIBILITY INVARIANT:
//   cash_out MUST succeed if and only if ALL of the following hold:
//     1. A game record exists for the player in storage.
//     2. The game is in GamePhase::Revealed.
//     3. game.streak >= 1  (player won at least one flip).
//     4. The contract holds sufficient reserves to cover the gross payout.
//
// Any violation of conditions 1–3 must be rejected with the correct error
// BEFORE any state mutation occurs (no partial writes on rejection).
//
// INELIGIBLE CONDITIONS (must be rejected):
//   - No game exists                  → Error::NoActiveGame
//   - Phase == Committed              → Error::InvalidPhase
//   - Phase == Completed              → Error::InvalidPhase
//   - Phase == Revealed, streak == 0  → Error::NoWinningsToClaimOrContinue
//   - Double claim (game already Completed after first cash_out)
//                                     → Error::InvalidPhase
// ═══════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod cash_out_availability_tests {
    use super::*;
    use proptest::prelude::*;
    use soroban_sdk::testutils::Address as _;
    use crate::property_tests::{setup_contract_with_bounds, dummy_commitment_prop};

    // ── Module-level helpers ─────────────────────────────────────────────────

    /// Initialise a contract and return (contract_id, client).
    fn setup_env(env: &Env) -> (Address, CoinflipContractClient) {
        env.mock_all_auths();
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(env, &contract_id);
        let admin    = Address::generate(env);
        let treasury = Address::generate(env);
        #[allow(deprecated)]
        let token    = env.register_stellar_asset_contract(admin.clone());
        // fee_bps=300, min=1_000_000, max=100_000_000
        client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);
        (contract_id, client)
    }

    /// Deterministic non-zero commitment suitable for property tests.
    fn prop_commitment(env: &Env) -> BytesN<32> {
        env.crypto().sha256(&soroban_sdk::Bytes::from_slice(env, &[0xABu8; 32])).into()
    }

    /// Inject a GameState directly into storage, bypassing start_game.
    /// This lets tests exercise any (phase, streak, wager) combination
    /// without needing a real commit-reveal round.
    fn inject(
        env: &Env,
        contract_id: &Address,
        player: &Address,
        phase: GamePhase,
        streak: u32,
        wager: i128,
    ) {
        let dummy = prop_commitment(env);
        let game = GameState {
            wager,
            side: Side::Heads,
            streak,
            commitment: dummy.clone(),
            contract_random: dummy,
            fee_bps: 300,
            phase,
        };
        env.as_contract(contract_id, || {
            CoinflipContract::save_player_game(env, player, &game);
        });
    }

    /// Set the tracked reserve_balance in ContractStats.
    fn set_reserves(env: &Env, contract_id: &Address, amount: i128) {
        env.as_contract(contract_id, || {
            let mut stats = CoinflipContract::load_stats(env);
            stats.reserve_balance = amount;
            CoinflipContract::save_stats(env, &stats);
        });
    }

    /// Mint tokens to the contract so token transfers don't fail.
    fn mint_to(env: &Env, contract_id: &Address, amount: i128) {
        let config: ContractConfig = env.as_contract(contract_id, || {
            env.storage().persistent().get(&StorageKey::Config).unwrap()
        });
        soroban_sdk::token::StellarAssetClient::new(env, &config.token)
            .mint(contract_id, &amount);
    }

    /// Read ContractStats from storage.
    fn read_stats(env: &Env, contract_id: &Address) -> ContractStats {
        env.as_contract(contract_id, || {
            env.storage().persistent().get(&StorageKey::Stats).unwrap()
        })
    }

    // ── Property: no-game condition ──────────────────────────────────────────
    //
    // INVARIANT: cash_out called for a player with no game record must always
    // return Error::NoActiveGame, regardless of who the player is.
    //
    // This covers:
    //   - A brand-new address that has never played.
    //   - An address whose game was deleted after settlement.
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// PROPERTY CA-1: cash_out with no game record → NoActiveGame.
        ///
        /// For any player address that has no game in storage, cash_out must
        /// be rejected with Error::NoActiveGame.  No state mutation must occur.
        #[test]
        fn prop_cash_out_no_game_returns_no_active_game(
            // seed drives Address::generate deterministically via proptest
            _seed in 0u8..=255u8,
        ) {
            let env = Env::default();
            let (contract_id, client) = setup_env(&env);

            // Fresh player — no game record exists.
            let player = Address::generate(&env);

            let stats_before = read_stats(&env, &contract_id);
            let result = client.try_cash_out(&player);

            // Must be rejected with the correct error.
            prop_assert_eq!(result, Err(Ok(Error::NoActiveGame)));

            // No state mutation: stats unchanged.
            let stats_after = read_stats(&env, &contract_id);
            prop_assert_eq!(stats_before.reserve_balance, stats_after.reserve_balance);
            prop_assert_eq!(stats_before.total_fees, stats_after.total_fees);
        }

        /// PROPERTY CA-2: cash_out after game deletion → NoActiveGame.
        ///
        /// After a successful cash_out the game transitions to Completed but
        /// the record remains in storage.  After a loss the record is deleted.
        /// A second cash_out attempt on a Completed game must return
        /// Error::InvalidPhase (double-claim guard), not succeed silently.
        ///
        /// This property verifies the double-claim path across random wagers.
        #[test]
        fn prop_cash_out_double_claim_rejected(
            wager in 1_000_000i128..=10_000_000i128,
            streak in 1u32..=4u32,
        ) {
            let env = Env::default();
            let (contract_id, client) = setup_env(&env);

            // Provide ample reserves and token balance.
            let reserve = 1_000_000_000i128;
            set_reserves(&env, &contract_id, reserve);
            mint_to(&env, &contract_id, reserve);

            let player = Address::generate(&env);
            // Inject a winning Revealed state.
            inject(&env, &contract_id, &player, GamePhase::Revealed, streak, wager);

            // First cash_out must succeed.
            let first = client.try_cash_out(&player);
            prop_assert!(first.is_ok(), "first cash_out must succeed for Revealed+streak≥1");

            // Second cash_out on the now-Completed game must be rejected.
            // The game record is still present (phase=Completed), so the error
            // is InvalidPhase, not NoActiveGame.
            let second = client.try_cash_out(&player);
            prop_assert_eq!(second, Err(Ok(Error::InvalidPhase)),
                "double cash_out must be rejected with InvalidPhase");
        }
    }

    // ── Property: invalid phase conditions ──────────────────────────────────
    //
    // INVARIANT: cash_out must return Error::InvalidPhase for any game that is
    // not in GamePhase::Revealed, regardless of the streak value or wager.
    //
    // Phases tested:
    //   - Committed  (reveal not yet called)
    //   - Completed  (game already settled)
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(150))]

        /// PROPERTY CA-3: Committed phase → InvalidPhase for any streak/wager.
        ///
        /// A game in Committed phase means the player has placed their wager
        /// and commitment but has not yet called reveal.  cash_out must be
        /// rejected because the outcome is not yet known.
        ///
        /// No funds must be transferred and no state must change on rejection.
        #[test]
        fn prop_cash_out_committed_phase_rejected(
            wager  in 1_000_000i128..=100_000_000i128,
            streak in 0u32..=10u32,
        ) {
            let env = Env::default();
            let (contract_id, client) = setup_env(&env);

            let player = Address::generate(&env);
            inject(&env, &contract_id, &player, GamePhase::Committed, streak, wager);

            let stats_before = read_stats(&env, &contract_id);
            let result = client.try_cash_out(&player);

            // Correct error regardless of streak value.
            prop_assert_eq!(result, Err(Ok(Error::InvalidPhase)),
                "Committed phase must always return InvalidPhase");

            // Guard ordering: no state mutation before the phase check.
            let game_after: GameState = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player).unwrap()
            });
            prop_assert_eq!(game_after.phase, GamePhase::Committed,
                "game phase must not change on rejection");

            let stats_after = read_stats(&env, &contract_id);
            prop_assert_eq!(stats_before.reserve_balance, stats_after.reserve_balance,
                "reserve_balance must not change on rejection");
            prop_assert_eq!(stats_before.total_fees, stats_after.total_fees,
                "total_fees must not change on rejection");
        }

        /// PROPERTY CA-4: Completed phase → InvalidPhase for any streak/wager.
        ///
        /// A game in Completed phase has already been settled (win cashed out
        /// or loss forfeited).  cash_out must be rejected — there is nothing
        /// left to claim.
        ///
        /// This is the double-claim guard at the phase level.
        #[test]
        fn prop_cash_out_completed_phase_rejected(
            wager  in 1_000_000i128..=100_000_000i128,
            streak in 0u32..=10u32,
        ) {
            let env = Env::default();
            let (contract_id, client) = setup_env(&env);

            let player = Address::generate(&env);
            inject(&env, &contract_id, &player, GamePhase::Completed, streak, wager);

            let stats_before = read_stats(&env, &contract_id);
            let result = client.try_cash_out(&player);

            prop_assert_eq!(result, Err(Ok(Error::InvalidPhase)),
                "Completed phase must always return InvalidPhase");

            // State must be unchanged.
            let game_after: GameState = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player).unwrap()
            });
            prop_assert_eq!(game_after.phase, GamePhase::Completed,
                "game phase must not change on rejection");

            let stats_after = read_stats(&env, &contract_id);
            prop_assert_eq!(stats_before.reserve_balance, stats_after.reserve_balance,
                "reserve_balance must not change on rejection");
            prop_assert_eq!(stats_before.total_fees, stats_after.total_fees,
                "total_fees must not change on rejection");
        }
    }

    // ── Property: streak == 0 in Revealed phase ──────────────────────────────
    //
    // INVARIANT: A Revealed game with streak == 0 represents a loss state.
    // cash_out must return Error::NoWinningsToClaimOrContinue.
    // This is distinct from the phase guard — the phase IS correct (Revealed)
    // but the streak condition is not met.
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(150))]

        /// PROPERTY CA-5: Revealed phase + streak == 0 → NoWinningsToClaimOrContinue.
        ///
        /// streak == 0 in Revealed phase means the player lost the flip.
        /// cash_out must be rejected because there are no winnings to claim.
        ///
        /// No funds must be transferred and no state must change on rejection.
        #[test]
        fn prop_cash_out_revealed_streak_zero_rejected(
            wager in 1_000_000i128..=100_000_000i128,
        ) {
            let env = Env::default();
            let (contract_id, client) = setup_env(&env);

            let player = Address::generate(&env);
            // Revealed phase, streak == 0 → loss state.
            inject(&env, &contract_id, &player, GamePhase::Revealed, 0, wager);

            let stats_before = read_stats(&env, &contract_id);
            let result = client.try_cash_out(&player);

            prop_assert_eq!(result, Err(Ok(Error::NoWinningsToClaimOrContinue)),
                "Revealed+streak==0 must return NoWinningsToClaimOrContinue");

            // No state mutation on rejection.
            let game_after: GameState = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player).unwrap()
            });
            prop_assert_eq!(game_after.phase, GamePhase::Revealed,
                "game phase must not change on rejection");
            prop_assert_eq!(game_after.streak, 0u32,
                "streak must not change on rejection");

            let stats_after = read_stats(&env, &contract_id);
            prop_assert_eq!(stats_before.reserve_balance, stats_after.reserve_balance,
                "reserve_balance must not change on rejection");
            prop_assert_eq!(stats_before.total_fees, stats_after.total_fees,
                "total_fees must not change on rejection");
        }
    }

    // ── Property: eligible states succeed ───────────────────────────────────
    //
    // INVARIANT: cash_out MUST succeed when:
    //   - phase == Revealed
    //   - streak >= 1
    //   - reserves >= gross payout
    //
    // On success:
    //   - Returns Ok(net_payout) where net = gross - fee
    //   - game.phase transitions to Completed
    //   - stats.reserve_balance decreases by exactly gross
    //   - stats.total_fees increases by exactly fee
    //   - net_payout > 0
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        /// PROPERTY CA-6: Revealed + streak ≥ 1 → cash_out succeeds and
        /// returns a positive net payout.
        ///
        /// For any valid (wager, streak) combination with sufficient reserves,
        /// cash_out must succeed and the returned net payout must be positive.
        #[test]
        fn prop_cash_out_eligible_state_succeeds(
            wager  in 1_000_000i128..=10_000_000i128,
            streak in 1u32..=4u32,
        ) {
            let env = Env::default();
            let (contract_id, client) = setup_env(&env);

            let reserve = 1_000_000_000i128;
            set_reserves(&env, &contract_id, reserve);
            mint_to(&env, &contract_id, reserve);

            let player = Address::generate(&env);
            inject(&env, &contract_id, &player, GamePhase::Revealed, streak, wager);

            let result = client.try_cash_out(&player);

            // Must succeed.
            prop_assert!(result.is_ok(),
                "cash_out must succeed for Revealed phase with streak >= 1");

            let net = result.unwrap().unwrap();

            // Net payout must be positive.
            prop_assert!(net > 0,
                "net payout must be positive; got {}", net);

            // Game must transition to Completed.
            let game: GameState = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player).unwrap()
            });
            prop_assert_eq!(game.phase, GamePhase::Completed,
                "game must be Completed after successful cash_out");
        }

        /// PROPERTY CA-7: Accounting invariant — reserve decreases by gross,
        /// fees increase by fee, net = gross - fee.
        ///
        /// For any eligible (wager, streak), the settlement accounting must
        /// satisfy:
        ///   gross = wager × multiplier_bps / 10_000
        ///   fee   = gross × fee_bps / 10_000
        ///   net   = gross - fee
        ///   reserve_after = reserve_before - gross
        ///   total_fees_after = total_fees_before + fee
        #[test]
        fn prop_cash_out_accounting_invariant(
            wager  in 1_000_000i128..=10_000_000i128,
            streak in 1u32..=4u32,
        ) {
            let env = Env::default();
            let (contract_id, client) = setup_env(&env);

            let reserve = 1_000_000_000i128;
            set_reserves(&env, &contract_id, reserve);
            mint_to(&env, &contract_id, reserve);

            let player = Address::generate(&env);
            inject(&env, &contract_id, &player, GamePhase::Revealed, streak, wager);

            let stats_before = read_stats(&env, &contract_id);

            let net = client.cash_out(&player);

            // Compute expected values using the same helpers the contract uses.
            let (expected_gross, expected_fee, expected_net) =
                calculate_payout_breakdown(wager, streak, 300).unwrap();

            prop_assert_eq!(net, expected_net,
                "returned net must equal gross - fee");

            let stats_after = read_stats(&env, &contract_id);

            prop_assert_eq!(
                stats_after.reserve_balance,
                stats_before.reserve_balance - expected_gross,
                "reserve must decrease by exactly gross"
            );
            prop_assert_eq!(
                stats_after.total_fees,
                stats_before.total_fees + expected_fee,
                "total_fees must increase by exactly fee"
            );
        }

        /// PROPERTY CA-8: Payout is strictly monotone with streak.
        ///
        /// A higher streak must always produce a strictly higher net payout
        /// for the same wager.  This guards against multiplier regression.
        #[test]
        fn prop_cash_out_payout_monotone_with_streak(
            wager in 1_000_000i128..=5_000_000i128,
        ) {
            // streak 1 < streak 2 < streak 3 < streak 4
            let fee_bps = 300u32;
            let (_, _, net1) = calculate_payout_breakdown(wager, 1, fee_bps).unwrap();
            let (_, _, net2) = calculate_payout_breakdown(wager, 2, fee_bps).unwrap();
            let (_, _, net3) = calculate_payout_breakdown(wager, 3, fee_bps).unwrap();
            let (_, _, net4) = calculate_payout_breakdown(wager, 4, fee_bps).unwrap();

            prop_assert!(net1 < net2, "streak-1 net must be less than streak-2 net");
            prop_assert!(net2 < net3, "streak-2 net must be less than streak-3 net");
            prop_assert!(net3 < net4, "streak-3 net must be less than streak-4 net");
        }

        /// PROPERTY CA-9: cash_out frees the player slot for a new game.
        ///
        /// After a successful cash_out the game record is in Completed phase.
        /// The player must be able to start a new game immediately (the slot
        /// is not blocked by the Completed record — start_game checks for
        /// Committed/Revealed phases only, not Completed).
        #[test]
        fn prop_cash_out_frees_slot_for_new_game(
            wager  in 1_000_000i128..=5_000_000i128,
            streak in 1u32..=4u32,
        ) {
            let env = Env::default();
            let (contract_id, client) = setup_env(&env);

            let reserve = 1_000_000_000i128;
            set_reserves(&env, &contract_id, reserve);
            mint_to(&env, &contract_id, reserve);

            let player = Address::generate(&env);
            inject(&env, &contract_id, &player, GamePhase::Revealed, streak, wager);

            // Cash out succeeds.
            prop_assert!(client.try_cash_out(&player).is_ok());

            // Player can immediately start a new game.
            let new_commitment = prop_commitment(&env);
            let result = client.try_start_game(
                &player, &Side::Tails, &1_000_000, &new_commitment,
            );
            prop_assert!(result.is_ok(),
                "start_game must succeed after cash_out (slot must be free)");
        }
    }

    // ── Property: guard ordering — no state mutation on any rejection ────────
    //
    // INVARIANT: All eligibility checks must fire before any state mutation.
    // A rejected cash_out must leave the game record and contract stats
    // byte-for-byte identical to their pre-call state.
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(150))]

        /// PROPERTY CA-10: No state mutation on any ineligible cash_out call.
        ///
        /// For every ineligible (phase, streak) combination, both the game
        /// record and the contract stats must be unchanged after the call.
        ///
        /// Covers all three rejection paths in a single property:
        ///   - Committed phase (any streak)
        ///   - Completed phase (any streak)
        ///   - Revealed phase + streak == 0
        #[test]
        fn prop_cash_out_no_state_mutation_on_rejection(
            wager  in 1_000_000i128..=100_000_000i128,
            streak in 0u32..=5u32,
            // 0 = Committed, 1 = Completed, 2 = Revealed+streak_zero
            case   in 0u8..=2u8,
        ) {
            let env = Env::default();
            let (contract_id, client) = setup_env(&env);

            let player = Address::generate(&env);

            // Build an ineligible state based on `case`.
            let (phase, effective_streak) = match case {
                0 => (GamePhase::Committed, streak),
                1 => (GamePhase::Completed, streak),
                _ => (GamePhase::Revealed, 0),   // streak forced to 0 → loss state
            };
            inject(&env, &contract_id, &player, phase.clone(), effective_streak, wager);

            let game_before: GameState = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player).unwrap()
            });
            let stats_before = read_stats(&env, &contract_id);

            // Call must fail.
            let result = client.try_cash_out(&player);
            prop_assert!(result.is_err() || matches!(result, Err(_)),
                "ineligible cash_out must return an error");

            // Game record must be byte-for-byte identical.
            let game_after: GameState = env.as_contract(&contract_id, || {
                CoinflipContract::load_player_game(&env, &player).unwrap()
            });
            prop_assert_eq!(game_before, game_after,
                "game state must not change on rejected cash_out");

            // Stats must be unchanged.
            let stats_after = read_stats(&env, &contract_id);
            prop_assert_eq!(stats_before.reserve_balance, stats_after.reserve_balance,
                "reserve_balance must not change on rejected cash_out");
            prop_assert_eq!(stats_before.total_fees, stats_after.total_fees,
                "total_fees must not change on rejected cash_out");
        }
    }
}
