//! # Governance Tests
//!
//! Tests for the proposal/voting/execution lifecycle, including quadratic voting.

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};

// ── helpers ───────────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (Address, Address, CoinflipContractClient) {
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let treasury = Address::generate(env);
    // Use a real stellar asset contract so token::Client::balance() works.
    let token = env.register_stellar_asset_contract(admin.clone());
    client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);
    (admin, contract_id, client)
}

fn setup_with_token(env: &Env) -> (Address, Address, Address, CoinflipContractClient) {
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let treasury = Address::generate(env);
    let token = env.register_stellar_asset_contract(admin.clone());
    client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);
    (admin, contract_id, token, client)
}

/// Mint `amount` tokens to `addr` using the stellar asset admin.
fn mint(env: &Env, token: &Address, admin: &Address, addr: &Address, amount: i128) {
    soroban_sdk::token::StellarAssetClient::new(env, token).mint(addr, &amount);
    let _ = admin; // admin auth is mocked
}

fn add_voters(client: &CoinflipContractClient, admin: &Address, voters: &[Address]) {
    for v in voters {
        client.add_voter(admin, v);
    }
}

fn advance_past_deadline(env: &Env) {
    env.ledger().with_mut(|l| l.sequence_number += VOTING_PERIOD_LEDGERS + 1);
}

// ── add_voter / remove_voter ──────────────────────────────────────────────────

#[test]
fn test_add_voter_succeeds_for_admin() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let voter = Address::generate(&env);
    assert!(client.try_add_voter(&admin, &voter).is_ok());
    let voters = client.get_voters();
    assert_eq!(voters.len(), 1);
    assert_eq!(voters.get(0).unwrap(), voter);
}

#[test]
fn test_add_voter_rejects_non_admin() {
    let env = Env::default();
    let (_, _, client) = setup(&env);
    let stranger = Address::generate(&env);
    let voter = Address::generate(&env);
    assert_eq!(client.try_add_voter(&stranger, &voter), Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_add_voter_deduplicates() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);
    client.add_voter(&admin, &voter); // second call is a no-op
    assert_eq!(client.get_voters().len(), 1);
}

#[test]
fn test_remove_voter_succeeds() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);
    client.remove_voter(&admin, &voter);
    assert_eq!(client.get_voters().len(), 0);
}

#[test]
fn test_remove_voter_rejects_non_admin() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);
    let stranger = Address::generate(&env);
    assert_eq!(client.try_remove_voter(&stranger, &voter), Err(Ok(Error::Unauthorized)));
}

// ── propose ───────────────────────────────────────────────────────────────────

#[test]
fn test_propose_succeeds_for_admin() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let id = client.propose(&admin, &ProposalAction::SetFee(400));
    assert_eq!(id, 0);
    let p = client.get_proposal(&0).unwrap();
    assert_eq!(p.id, 0);
    assert_eq!(p.status, ProposalStatus::Active);
    assert_eq!(p.votes_for, 0);
}

#[test]
fn test_propose_succeeds_for_registered_voter() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);
    let id = client.propose(&voter, &ProposalAction::SetFee(400));
    assert_eq!(id, 0);
}

#[test]
fn test_propose_rejects_unregistered_caller() {
    let env = Env::default();
    let (_, _, client) = setup(&env);
    let stranger = Address::generate(&env);
    assert_eq!(
        client.try_propose(&stranger, &ProposalAction::SetFee(400)),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn test_propose_increments_id() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let id0 = client.propose(&admin, &ProposalAction::SetFee(400));
    let id1 = client.propose(&admin, &ProposalAction::SetPaused(true));
    assert_eq!(id0, 0);
    assert_eq!(id1, 1);
}

#[test]
fn test_proposal_deadline_is_set() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let current = env.ledger().sequence();
    client.propose(&admin, &ProposalAction::SetFee(400));
    let p = client.get_proposal(&0).unwrap();
    assert_eq!(p.deadline_ledger, current + VOTING_PERIOD_LEDGERS);
}

// ── vote ──────────────────────────────────────────────────────────────────────

#[test]
fn test_vote_approve_increments_votes_for() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);
    client.propose(&admin, &ProposalAction::SetFee(400));
    client.vote(&voter, &0, &true);
    let p = client.get_proposal(&0).unwrap();
    assert_eq!(p.votes_for, 1);
    assert_eq!(p.votes_against, 0);
}

#[test]
fn test_vote_reject_increments_votes_against() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);
    client.propose(&admin, &ProposalAction::SetFee(400));
    client.vote(&voter, &0, &false);
    let p = client.get_proposal(&0).unwrap();
    assert_eq!(p.votes_for, 0);
    assert_eq!(p.votes_against, 1);
}

#[test]
fn test_vote_rejects_non_voter() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    client.propose(&admin, &ProposalAction::SetFee(400));
    let stranger = Address::generate(&env);
    assert_eq!(
        client.try_vote(&stranger, &0, &true),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn test_vote_rejects_double_vote() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);
    client.propose(&admin, &ProposalAction::SetFee(400));
    client.vote(&voter, &0, &true);
    assert_eq!(
        client.try_vote(&voter, &0, &true),
        Err(Ok(Error::AlreadyVoted))
    );
}

#[test]
fn test_vote_rejects_after_deadline() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);
    client.propose(&admin, &ProposalAction::SetFee(400));
    advance_past_deadline(&env);
    assert_eq!(
        client.try_vote(&voter, &0, &true),
        Err(Ok(Error::VotingClosed))
    );
}

#[test]
fn test_vote_rejects_on_nonexistent_proposal() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);
    assert_eq!(
        client.try_vote(&voter, &99, &true),
        Err(Ok(Error::ProposalNotFound))
    );
}

// ── execute_proposal ──────────────────────────────────────────────────────────

#[test]
fn test_execute_set_fee_applies_change() {
    let env = Env::default();
    let (admin, contract_id, client) = setup(&env);
    let voters: Vec<Address> = (0..3).map(|_| Address::generate(&env)).collect();
    add_voters(&client, &admin, &voters);

    client.propose(&admin, &ProposalAction::SetFee(400));
    // 2 yes, 0 against → votes_for > votes_against → passes
    client.vote(&voters[0], &0, &true);
    client.vote(&voters[1], &0, &true);

    advance_past_deadline(&env);
    client.execute_proposal(&admin, &0);

    let cfg = client.get_config();
    assert_eq!(cfg.fee_bps, 400);

    let p = client.get_proposal(&0).unwrap();
    assert_eq!(p.status, ProposalStatus::Executed);
}

#[test]
fn test_execute_set_wager_limits_applies_change() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let voters: Vec<Address> = (0..2).map(|_| Address::generate(&env)).collect();
    add_voters(&client, &admin, &voters);

    client.propose(&admin, &ProposalAction::SetWagerLimits(2_000_000, 200_000_000));
    client.vote(&voters[0], &0, &true);
    client.vote(&voters[1], &0, &true);

    advance_past_deadline(&env);
    client.execute_proposal(&admin, &0);

    let cfg = client.get_config();
    assert_eq!(cfg.min_wager, 2_000_000);
    assert_eq!(cfg.max_wager, 200_000_000);
}

#[test]
fn test_execute_set_paused_applies_change() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);

    client.propose(&admin, &ProposalAction::SetPaused(true));
    client.vote(&voter, &0, &true);

    advance_past_deadline(&env);
    client.execute_proposal(&admin, &0);

    assert!(client.get_config().paused);
}

#[test]
fn test_execute_set_treasury_applies_change() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);
    let new_treasury = Address::generate(&env);

    client.propose(&admin, &ProposalAction::SetTreasury(new_treasury.clone()));
    client.vote(&voter, &0, &true);

    advance_past_deadline(&env);
    client.execute_proposal(&admin, &0);

    assert_eq!(client.get_config().treasury, new_treasury);
}

#[test]
fn test_execute_set_multipliers_applies_change() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);

    let new_m = MultiplierConfig { streak1: 20_000, streak2: 40_000, streak3: 70_000, streak4_plus: 110_000 };
    client.propose(&admin, &ProposalAction::SetMultipliers(new_m.clone()));
    client.vote(&voter, &0, &true);

    advance_past_deadline(&env);
    client.execute_proposal(&admin, &0);

    assert_eq!(client.get_config().multipliers, new_m);
}

#[test]
fn test_execute_rejects_while_voting_open() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);
    client.propose(&admin, &ProposalAction::SetFee(400));
    client.vote(&voter, &0, &true);
    // Do NOT advance past deadline
    assert_eq!(
        client.try_execute_proposal(&admin, &0),
        Err(Ok(Error::VotingOpen))
    );
}

#[test]
fn test_execute_rejects_threshold_not_met() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let voters: Vec<Address> = (0..3).map(|_| Address::generate(&env)).collect();
    add_voters(&client, &admin, &voters);

    client.propose(&admin, &ProposalAction::SetFee(400));
    // 1 yes, 2 against → votes_for (1) <= votes_against (2) → threshold not met
    client.vote(&voters[0], &0, &true);
    client.vote(&voters[1], &0, &false);
    client.vote(&voters[2], &0, &false);

    advance_past_deadline(&env);
    assert_eq!(
        client.try_execute_proposal(&admin, &0),
        Err(Ok(Error::ThresholdNotMet))
    );
}

#[test]
fn test_execute_rejects_already_executed() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);
    client.propose(&admin, &ProposalAction::SetFee(400));
    client.vote(&voter, &0, &true);
    advance_past_deadline(&env);
    client.execute_proposal(&admin, &0);
    assert_eq!(
        client.try_execute_proposal(&admin, &0),
        Err(Ok(Error::ProposalAlreadyExecuted))
    );
}

#[test]
fn test_execute_rejects_nonexistent_proposal() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    assert_eq!(
        client.try_execute_proposal(&admin, &99),
        Err(Ok(Error::ProposalNotFound))
    );
}

// ── cancel_proposal ───────────────────────────────────────────────────────────

#[test]
fn test_cancel_by_admin_succeeds() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    client.propose(&admin, &ProposalAction::SetFee(400));
    client.cancel_proposal(&admin, &0);
    let p = client.get_proposal(&0).unwrap();
    assert_eq!(p.status, ProposalStatus::Canceled);
}

#[test]
fn test_cancel_by_proposer_succeeds() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);
    client.propose(&voter, &ProposalAction::SetFee(400));
    client.cancel_proposal(&voter, &0);
    let p = client.get_proposal(&0).unwrap();
    assert_eq!(p.status, ProposalStatus::Canceled);
}

#[test]
fn test_cancel_rejects_stranger() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    client.propose(&admin, &ProposalAction::SetFee(400));
    let stranger = Address::generate(&env);
    assert_eq!(
        client.try_cancel_proposal(&stranger, &0),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn test_cancel_prevents_execution() {
    let env = Env::default();
    let (admin, _, client) = setup(&env);
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);
    client.propose(&admin, &ProposalAction::SetFee(400));
    client.vote(&voter, &0, &true);
    client.cancel_proposal(&admin, &0);
    advance_past_deadline(&env);
    assert_eq!(
        client.try_execute_proposal(&admin, &0),
        Err(Ok(Error::ProposalAlreadyExecuted))
    );
}

// ── active games unaffected by governance execution ───────────────────────────

#[test]
fn test_governance_fee_change_does_not_reprice_active_game() {
    let env = Env::default();
    let (admin, contract_id, client) = setup(&env);

    // Fund reserves and start a game
    env.as_contract(&contract_id, || {
        let mut stats = CoinflipContract::load_stats(&env);
        stats.reserve_balance = 1_000_000_000;
        CoinflipContract::save_stats(&env, &stats);
    });

    let player = Address::generate(&env);
    let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);
    let commitment: BytesN<32> = env.crypto().sha256(&secret).into();
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment);

    // Governance changes fee to 500 bps
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);
    client.propose(&admin, &ProposalAction::SetFee(500));
    client.vote(&voter, &0, &true);
    advance_past_deadline(&env);
    client.execute_proposal(&admin, &0);
    assert_eq!(client.get_config().fee_bps, 500);

    // The in-flight game still uses the original fee snapshot (300 bps)
    let game: GameState = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap().unwrap()
    });
    assert_eq!(game.fee_bps, 300, "active game fee snapshot must be unchanged");
}

// ── quadratic voting ──────────────────────────────────────────────────────────

#[test]
fn test_get_voting_power_returns_zero_for_non_voter() {
    let env = Env::default();
    let (_, _, _, client) = setup_with_token(&env);
    let stranger = Address::generate(&env);
    assert_eq!(client.get_voting_power(&stranger), 0);
}

#[test]
fn test_get_voting_power_returns_one_for_voter_with_no_balance() {
    let env = Env::default();
    let (admin, _, _, client) = setup_with_token(&env);
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);
    // balance = 0 → isqrt(0).max(1) = 1
    assert_eq!(client.get_voting_power(&voter), 1);
}

#[test]
fn test_get_voting_power_reflects_token_balance() {
    let env = Env::default();
    let (admin, _, token, client) = setup_with_token(&env);
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);
    // mint 100 tokens → isqrt(100) = 10
    mint(&env, &token, &admin, &voter, 100);
    assert_eq!(client.get_voting_power(&voter), 10);
}

#[test]
fn test_quadratic_vote_weight_accumulates_correctly() {
    let env = Env::default();
    let (admin, _, token, client) = setup_with_token(&env);

    let voter_a = Address::generate(&env);
    let voter_b = Address::generate(&env);
    client.add_voter(&admin, &voter_a);
    client.add_voter(&admin, &voter_b);

    // voter_a: balance 100 → weight 10
    // voter_b: balance 400 → weight 20
    mint(&env, &token, &admin, &voter_a, 100);
    mint(&env, &token, &admin, &voter_b, 400);

    client.propose(&admin, &ProposalAction::SetFee(400));
    client.vote(&voter_a, &0, &true);
    client.vote(&voter_b, &0, &true);

    let p = client.get_proposal(&0).unwrap();
    assert_eq!(p.votes_for, 30);   // 10 + 20
    assert_eq!(p.votes_against, 0);
}

#[test]
fn test_quadratic_large_balance_voter_can_outweigh_many_small_voters() {
    let env = Env::default();
    let (admin, _, token, client) = setup_with_token(&env);

    // whale: balance 10_000 → weight 100
    // 3 minnows: balance 1 each → weight 1 each
    let whale = Address::generate(&env);
    let minnows: Vec<Address> = (0..3).map(|_| Address::generate(&env)).collect();

    client.add_voter(&admin, &whale);
    for m in &minnows { client.add_voter(&admin, m); }

    mint(&env, &token, &admin, &whale, 10_000);
    for m in &minnows { mint(&env, &token, &admin, m, 1); }

    client.propose(&admin, &ProposalAction::SetFee(400));
    // whale votes against, minnows vote for
    client.vote(&whale, &0, &false);
    for m in &minnows { client.vote(m, &0, &true); }

    let p = client.get_proposal(&0).unwrap();
    // votes_for = 3 (3 minnows × 1), votes_against = 100 (whale)
    assert_eq!(p.votes_for, 3);
    assert_eq!(p.votes_against, 100);

    advance_past_deadline(&env);
    // votes_for (3) <= votes_against (100) → threshold not met
    assert_eq!(
        client.try_execute_proposal(&admin, &0),
        Err(Ok(Error::ThresholdNotMet))
    );
}

#[test]
fn test_quadratic_execute_passes_when_for_exceeds_against() {
    let env = Env::default();
    let (admin, _, token, client) = setup_with_token(&env);

    let voter_a = Address::generate(&env);
    let voter_b = Address::generate(&env);
    client.add_voter(&admin, &voter_a);
    client.add_voter(&admin, &voter_b);

    // voter_a: 10_000 → weight 100 (votes for)
    // voter_b: 100   → weight 10  (votes against)
    mint(&env, &token, &admin, &voter_a, 10_000);
    mint(&env, &token, &admin, &voter_b, 100);

    client.propose(&admin, &ProposalAction::SetFee(400));
    client.vote(&voter_a, &0, &true);
    client.vote(&voter_b, &0, &false);

    advance_past_deadline(&env);
    client.execute_proposal(&admin, &0);

    assert_eq!(client.get_config().fee_bps, 400);
}

#[test]
fn test_quadratic_no_votes_fails_threshold() {
    let env = Env::default();
    let (admin, _, _, client) = setup_with_token(&env);
    let voter = Address::generate(&env);
    client.add_voter(&admin, &voter);

    client.propose(&admin, &ProposalAction::SetFee(400));
    // No votes cast at all
    advance_past_deadline(&env);
    assert_eq!(
        client.try_execute_proposal(&admin, &0),
        Err(Ok(Error::ThresholdNotMet))
    );
}
