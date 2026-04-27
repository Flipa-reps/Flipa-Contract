//! # Multi-Signature Admin Control Tests (#470)
//!
//! Tests for the multi-sig proposal/approval/execution system.

use super::*;
use soroban_sdk::{testutils::{Address as _, Ledger}, Env};

fn setup() -> (Env, CoinflipContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(
        &admin, &treasury, &token,
        &300, &1_000_000, &100_000_000,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    (env, client, admin)
}

fn make_signers(env: &Env, n: u32) -> soroban_sdk::Vec<Address> {
    let mut v = soroban_sdk::Vec::new(env);
    for _ in 0..n {
        v.push_back(Address::generate(env));
    }
    v
}

fn configure_2_of_3(
    env: &Env,
    client: &CoinflipContractClient,
    admin: &Address,
) -> (Address, Address, Address) {
    let mut signers = soroban_sdk::Vec::new(env);
    let s1 = Address::generate(env);
    let s2 = Address::generate(env);
    let s3 = Address::generate(env);
    signers.push_back(s1.clone());
    signers.push_back(s2.clone());
    signers.push_back(s3.clone());
    client.configure_multisig(admin, &signers, &2, &0); // threshold=2, no timelock
    (s1, s2, s3)
}

// ── configure_multisig ────────────────────────────────────────────────────────

#[test]
fn test_configure_multisig_succeeds_for_admin() {
    let (env, client, admin) = setup();
    let signers = make_signers(&env, 1);
    assert!(client.try_configure_multisig(&admin, &signers, &1, &0).is_ok());
    let cfg = client.get_multisig_config();
    assert!(cfg.is_some());
    assert_eq!(cfg.unwrap().threshold, 1);
}

#[test]
fn test_configure_multisig_rejected_for_non_admin() {
    let (env, client, _) = setup();
    let non_admin = Address::generate(&env);
    let signers = make_signers(&env, 1);
    assert!(client.try_configure_multisig(&non_admin, &signers, &1, &0).is_err());
}

#[test]
fn test_configure_multisig_threshold_exceeds_signers_rejected() {
    let (env, client, admin) = setup();
    let signers = make_signers(&env, 1);
    // threshold=2 but only 1 signer
    assert!(client.try_configure_multisig(&admin, &signers, &2, &0).is_err());
}

#[test]
fn test_configure_multisig_zero_threshold_rejected() {
    let (env, client, admin) = setup();
    let signers = make_signers(&env, 1);
    assert!(client.try_configure_multisig(&admin, &signers, &0, &0).is_err());
}

// ── get_multisig_config ───────────────────────────────────────────────────────

#[test]
fn test_get_multisig_config_none_before_configure() {
    let (_, client, _) = setup();
    assert!(client.get_multisig_config().is_none());
}

// ── multisig_propose ──────────────────────────────────────────────────────────

#[test]
fn test_multisig_propose_succeeds_for_signer() {
    let (env, client, admin) = setup();
    let (s1, _, _) = configure_2_of_3(&env, &client, &admin);
    let id = client.multisig_propose(&s1, &MultisigAction::SetFee(400));
    assert_eq!(id, 0);
}

#[test]
fn test_multisig_propose_rejected_for_non_signer() {
    let (env, client, admin) = setup();
    configure_2_of_3(&env, &client, &admin);
    let outsider = Address::generate(&env);
    assert!(client.try_multisig_propose(&outsider, &MultisigAction::SetFee(400)).is_err());
}

#[test]
fn test_multisig_propose_rejected_when_not_configured() {
    let (env, client, _) = setup();
    let player = Address::generate(&env);
    assert!(client.try_multisig_propose(&player, &MultisigAction::SetFee(400)).is_err());
}

// ── multisig_approve ──────────────────────────────────────────────────────────

#[test]
fn test_multisig_approve_increments_count() {
    let (env, client, admin) = setup();
    let (s1, s2, _) = configure_2_of_3(&env, &client, &admin);
    let id = client.multisig_propose(&s1, &MultisigAction::SetFee(400));
    client.multisig_approve(&s2, &id);
    let proposal = client.get_multisig_proposal(&id).unwrap();
    assert_eq!(proposal.approvals, 2);
}

#[test]
fn test_multisig_approve_duplicate_rejected() {
    let (env, client, admin) = setup();
    let (s1, _, _) = configure_2_of_3(&env, &client, &admin);
    let id = client.multisig_propose(&s1, &MultisigAction::SetFee(400));
    // s1 already approved via propose; second approval should fail.
    assert_eq!(
        client.try_multisig_approve(&s1, &id),
        Err(Ok(Error::MultisigAlreadyApproved))
    );
}

#[test]
fn test_multisig_approve_non_signer_rejected() {
    let (env, client, admin) = setup();
    let (s1, _, _) = configure_2_of_3(&env, &client, &admin);
    let id = client.multisig_propose(&s1, &MultisigAction::SetFee(400));
    let outsider = Address::generate(&env);
    assert!(client.try_multisig_approve(&outsider, &id).is_err());
}

// ── multisig_execute ──────────────────────────────────────────────────────────

#[test]
fn test_multisig_execute_succeeds_after_threshold_met() {
    let (env, client, admin) = setup();
    let (s1, s2, _) = configure_2_of_3(&env, &client, &admin);
    let id = client.multisig_propose(&s1, &MultisigAction::SetFee(400));
    client.multisig_approve(&s2, &id);
    // No timelock (0 ledgers), so execute immediately.
    assert!(client.try_multisig_execute(&s1, &id).is_ok());
    // Verify the fee was updated.
    assert_eq!(client.get_config().fee_bps, 400);
}

#[test]
fn test_multisig_execute_fails_below_threshold() {
    let (env, client, admin) = setup();
    let (s1, _, _) = configure_2_of_3(&env, &client, &admin);
    let id = client.multisig_propose(&s1, &MultisigAction::SetFee(400));
    // Only 1 approval (proposer), threshold is 2.
    assert_eq!(
        client.try_multisig_execute(&s1, &id),
        Err(Ok(Error::MultisigThresholdNotMet))
    );
}

#[test]
fn test_multisig_execute_fails_during_timelock() {
    let (env, client, admin) = setup();
    let mut signers = soroban_sdk::Vec::new(&env);
    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    signers.push_back(s1.clone());
    signers.push_back(s2.clone());
    // Set a 100-ledger timelock.
    client.configure_multisig(&admin, &signers, &2, &100);
    let id = client.multisig_propose(&s1, &MultisigAction::SetFee(400));
    client.multisig_approve(&s2, &id);
    // Timelock not elapsed yet.
    assert_eq!(
        client.try_multisig_execute(&s1, &id),
        Err(Ok(Error::MultisigTimelockPending))
    );
}

#[test]
fn test_multisig_execute_succeeds_after_timelock() {
    let (env, client, admin) = setup();
    let mut signers = soroban_sdk::Vec::new(&env);
    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    signers.push_back(s1.clone());
    signers.push_back(s2.clone());
    client.configure_multisig(&admin, &signers, &2, &100);
    let id = client.multisig_propose(&s1, &MultisigAction::SetFee(400));
    client.multisig_approve(&s2, &id);
    // Advance past timelock.
    env.ledger().with_mut(|l| l.sequence_number += 101);
    assert!(client.try_multisig_execute(&s1, &id).is_ok());
    assert_eq!(client.get_config().fee_bps, 400);
}

#[test]
fn test_multisig_execute_twice_rejected() {
    let (env, client, admin) = setup();
    let (s1, s2, _) = configure_2_of_3(&env, &client, &admin);
    let id = client.multisig_propose(&s1, &MultisigAction::SetFee(400));
    client.multisig_approve(&s2, &id);
    client.multisig_execute(&s1, &id);
    assert_eq!(
        client.try_multisig_execute(&s1, &id),
        Err(Ok(Error::ProposalAlreadyExecuted))
    );
}

// ── multisig_cancel ───────────────────────────────────────────────────────────

#[test]
fn test_multisig_cancel_by_proposer() {
    let (env, client, admin) = setup();
    let (s1, _, _) = configure_2_of_3(&env, &client, &admin);
    let id = client.multisig_propose(&s1, &MultisigAction::SetFee(400));
    assert!(client.try_multisig_cancel(&s1, &id).is_ok());
    let proposal = client.get_multisig_proposal(&id).unwrap();
    assert_eq!(proposal.status, MultisigProposalStatus::Canceled);
}

#[test]
fn test_multisig_cancel_by_admin() {
    let (env, client, admin) = setup();
    let (s1, _, _) = configure_2_of_3(&env, &client, &admin);
    let id = client.multisig_propose(&s1, &MultisigAction::SetFee(400));
    assert!(client.try_multisig_cancel(&admin, &id).is_ok());
}

#[test]
fn test_multisig_cancel_by_non_proposer_non_admin_rejected() {
    let (env, client, admin) = setup();
    let (s1, s2, _) = configure_2_of_3(&env, &client, &admin);
    let id = client.multisig_propose(&s1, &MultisigAction::SetFee(400));
    assert_eq!(
        client.try_multisig_cancel(&s2, &id),
        Err(Ok(Error::Unauthorized))
    );
}

// ── get_multisig_proposal ─────────────────────────────────────────────────────

#[test]
fn test_get_multisig_proposal_nonexistent_returns_none() {
    let (_, client, _) = setup();
    assert!(client.get_multisig_proposal(&999).is_none());
}

#[test]
fn test_get_multisig_proposal_returns_correct_data() {
    let (env, client, admin) = setup();
    let (s1, _, _) = configure_2_of_3(&env, &client, &admin);
    let id = client.multisig_propose(&s1, &MultisigAction::SetFee(400));
    let proposal = client.get_multisig_proposal(&id).unwrap();
    assert_eq!(proposal.id, id);
    assert_eq!(proposal.proposer, s1);
    assert_eq!(proposal.approvals, 1);
    assert_eq!(proposal.status, MultisigProposalStatus::Pending);
}
