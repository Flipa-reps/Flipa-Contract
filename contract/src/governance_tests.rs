//! # Governance Tests
//!
//! Tests for the proposal/voting/execution lifecycle.

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};

// ── helpers ───────────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (Address, Address, CoinflipContractClient) {
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let treasury = Address::generate(env);
    let token = Address::generate(env);
    client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);
    (admin, contract_id, client)
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
    // 2 of 3 vote yes → 66% > 51%
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
    // Only 1 of 3 votes yes → 33% < 51%
    client.vote(&voters[0], &0, &true);

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
