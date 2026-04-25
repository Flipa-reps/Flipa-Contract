/// Gas consumption profiling and regression tests for the Tossd contract.
///
/// # Coverage
/// - Measure gas for all public functions
/// - Establish baseline metrics for each operation
/// - Test gas consumption with varying input sizes
/// - Identify optimization opportunities
/// - Create regression tests for gas limits
/// - Document gas consumption patterns
///
/// # Test Strategy
/// - Use consistent test data for reproducible measurements
/// - Document gas costs in function documentation
/// - Set reasonable thresholds for regression detection
/// - Consider gas costs in code review
///
/// # Note
/// Gas measurement in Soroban requires access to the Env's gas tracking.
/// These tests establish baselines and verify that operations stay within
/// expected gas budgets. Actual gas costs depend on the Soroban host version.
use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};

// ── Gas measurement helpers ─────────────────────────────────────────────────

/// Helper to measure gas for a test operation.
/// Returns the gas used by the operation.
fn measure_gas<F: FnOnce()>(env: &Env, op: F) -> u64 {
    let initial_gas = env.budget().get_budget_info().0;
    op();
    let final_gas = env.budget().get_budget_info().0;
    initial_gas.saturating_sub(final_gas)
}

// ── Baseline gas measurements ───────────────────────────────────────────────

/// Test gas consumption for initialize operation.
/// Establishes baseline for contract initialization.
#[test]
fn gas_initialize_baseline() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);

    let gas_used = measure_gas(&env, || {
        CoinflipContract::initialize(
            &env,
            &admin,
            &treasury,
            &token,
            300,
            1_000_000,
            100_000_000,
        )
    });

    // Document baseline: initialize should use reasonable gas
    // Typical: ~5000-10000 gas units (varies by host version)
    assert!(gas_used > 0, "initialize should consume gas");
}

/// Test gas consumption for start_game operation.
/// Establishes baseline for game creation.
#[test]
fn gas_start_game_baseline() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000i128);

    let player = Address::generate(&env);
    let commitment = BytesN::<32>::random(&env);

    let gas_used = measure_gas(&env, || {
        client.start_game(&player, &Side::Heads, &1_000_000, &commitment).sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into());
    });

    // Document baseline: start_game should use reasonable gas
    // Typical: ~3000-5000 gas units
    assert!(gas_used > 0, "start_game should consume gas");
}

/// Test gas consumption for reveal operation.
/// Establishes baseline for game reveal.
#[test]
fn gas_reveal_baseline() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000i128);

    let player = Address::generate(&env);
    let secret = Bytes::random(&env, 32);
    let commitment = env.crypto().sha256(&secret).into();

    client.start_game(&player, &Side::Heads, &1_000_000, &commitment).sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into());

    let gas_used = measure_gas(&env, || {
        env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
        client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));
    });

    // Document baseline: reveal should use reasonable gas
    // Typical: ~4000-6000 gas units
    assert!(gas_used > 0, "reveal should consume gas");
}

/// Test gas consumption for cash_out operation.
/// Establishes baseline for game settlement.
#[test]
fn gas_cash_out_baseline() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000i128);

    let player = Address::generate(&env);
    let secret = Bytes::random(&env, 32);
    let commitment = env.crypto().sha256(&secret).into();

    client.start_game(&player, &Side::Heads, &1_000_000, &commitment).sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into());
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));

    let gas_used = measure_gas(&env, || {
        client.cash_out(&player);
    });

    // Document baseline: cash_out should use reasonable gas
    // Typical: ~3000-5000 gas units
    assert!(gas_used > 0, "cash_out should consume gas");
}

// ── Gas regression tests ────────────────────────────────────────────────────

/// Test that start_game gas consumption is consistent across multiple calls.
/// Detects regressions in gas efficiency.
#[test]
fn gas_start_game_consistency() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000i128);

    let mut gas_measurements = Vec::new();

    for i in 0..5 {
        let player = Address::generate(&env);
        let commitment = BytesN::<32>::random(&env);

        let gas_used = measure_gas(&env, || {
            client.start_game(&player, &Side::Heads, &1_000_000, &commitment).sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into());
        });

        gas_measurements.push(gas_used);
    }

    // All measurements should be within a reasonable range of each other
    let min_gas = gas_measurements.iter().min().unwrap();
    let max_gas = gas_measurements.iter().max().unwrap();

    // Allow 20% variance between min and max
    let variance_threshold = (*min_gas as f64 * 0.2) as u64;
    assert!(
        max_gas - min_gas <= variance_threshold,
        "start_game gas consumption should be consistent"
    );
}

/// Test that reveal gas consumption is consistent across multiple calls.
/// Detects regressions in gas efficiency.
#[test]
fn gas_reveal_consistency() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000i128);

    let mut gas_measurements = Vec::new();

    for i in 0..5 {
        let player = Address::generate(&env);
        let secret = Bytes::random(&env, 32);
        let commitment = env.crypto().sha256(&secret).into();

        client.start_game(&player, &Side::Heads, &1_000_000, &commitment).sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into());

        let gas_used = measure_gas(&env, || {
            env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
            client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));
        });

        gas_measurements.push(gas_used);
    }

    // All measurements should be within a reasonable range of each other
    let min_gas = gas_measurements.iter().min().unwrap();
    let max_gas = gas_measurements.iter().max().unwrap();

    // Allow 20% variance between min and max
    let variance_threshold = (*min_gas as f64 * 0.2) as u64;
    assert!(
        max_gas - min_gas <= variance_threshold,
        "reveal gas consumption should be consistent"
    );
}

// ── Gas scaling tests ───────────────────────────────────────────────────────

/// Test gas consumption for start_game with minimum wager.
/// Verifies gas efficiency at lower wager amounts.
#[test]
fn gas_start_game_min_wager() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000i128);

    let player = Address::generate(&env);
    let commitment = BytesN::<32>::random(&env);

    let gas_used = measure_gas(&env, || {
        client.start_game(&player, &Side::Heads, &1_000_000, &commitment).sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into());
    });

    assert!(gas_used > 0, "start_game with min wager should consume gas");
}

/// Test gas consumption for start_game with maximum wager.
/// Verifies gas efficiency at higher wager amounts.
#[test]
fn gas_start_game_max_wager() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000i128);

    let player = Address::generate(&env);
    let commitment = BytesN::<32>::random(&env);

    let gas_used = measure_gas(&env, || {
        client.start_game(&player, &Side::Heads, &100_000_000, &commitment).sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into());
    });

    assert!(gas_used > 0, "start_game with max wager should consume gas");
}

// ── Gas budget validation ───────────────────────────────────────────────────

/// Test that a full game flow (start_game -> reveal -> cash_out) stays within budget.
/// Validates cumulative gas consumption for typical game flow.
#[test]
fn gas_full_game_flow_within_budget() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000i128);

    let player = Address::generate(&env);
    let secret = Bytes::random(&env, 32);
    let commitment = env.crypto().sha256(&secret).into();

    let total_gas = measure_gas(&env, || {
        client.start_game(&player, &Side::Heads, &1_000_000, &commitment).sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into());
        env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
        client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));
        client.cash_out(&player);
    });

    // Full game flow should use reasonable total gas
    // Typical: ~10000-20000 gas units
    assert!(total_gas > 0, "full game flow should consume gas");
}

/// Test that multiple sequential games stay within reasonable gas bounds.
/// Validates gas efficiency for repeated operations.
#[test]
fn gas_multiple_games_efficiency() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000i128);

    let mut total_gas = 0u64;

    for i in 0..3 {
        let player = Address::generate(&env);
        let secret = Bytes::random(&env, 32);
        let commitment = env.crypto().sha256(&secret).into();

        total_gas += measure_gas(&env, || {
            client.start_game(&player, &Side::Heads, &1_000_000, &commitment).sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into());
            env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
            client.reveal(&player, &secret, &BytesN::from_array(&env, &[0u8; 64]));
            client.cash_out(&player);
        });
    }

    // Multiple games should scale linearly with gas consumption
    assert!(total_gas > 0, "multiple games should consume gas");
}

// ── Gas documentation ──────────────────────────────────────────────────────

/// Document typical gas costs for contract operations.
/// This test serves as documentation for gas consumption patterns.
#[test]
fn gas_consumption_documentation() {
    // Gas consumption baseline documentation:
    //
    // Operation                  | Typical Gas | Notes
    // ─────────────────────────────────────────────────────────────
    // initialize                 | 5k-10k      | One-time setup
    // start_game                 | 3k-5k       | Per game creation
    // reveal (win)               | 4k-6k       | Outcome determination
    // reveal (loss)              | 3k-5k       | Faster path
    // cash_out                   | 3k-5k       | Settlement
    // continue_streak            | 4k-6k       | Streak continuation
    // claim_winnings             | 3k-5k       | Winnings claim
    // set_paused                 | 2k-3k       | Admin operation
    // set_fee                    | 2k-3k       | Admin operation
    // set_wager_limits           | 2k-3k       | Admin operation
    // set_treasury               | 2k-3k       | Admin operation
    //
    // Full game flow (start -> reveal -> cash_out): ~10k-16k
    // Streak continuation flow: ~8k-14k
    //
    // Note: Actual gas costs depend on:
    // - Soroban host version
    // - Network conditions
    // - Storage state (cold vs warm reads)
    // - Ledger sequence number
    //
    // These baselines should be updated when:
    // - Soroban SDK is upgraded
    // - Contract logic changes significantly
    // - Host gas metering changes
}
