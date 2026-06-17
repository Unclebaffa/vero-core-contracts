#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env,
};
use vero_core_contracts::VeroContractClient;

const LOCK_THRESHOLD: i128 = 100;
const ARCHIVE_AFTER_SECONDS: u64 = 30 * 24 * 60 * 60;

fn setup() -> (Env, Address, Address, VeroContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    client.initialize(&token, &LOCK_THRESHOLD);

    (env, admin, token, client)
}

fn mint_and_lock(
    env: &Env,
    token: &Address,
    client: &VeroContractClient,
    guardian: &Address,
    amount: i128,
) {
    let asset = StellarAssetClient::new(env, token);
    asset.mint(guardian, &amount);
    client.lock_tokens(guardian, &amount);
}

fn add_guardian_with_rep(
    env: &Env,
    token: &Address,
    client: &VeroContractClient,
    admin: &Address,
    score: u64,
) -> Address {
    let guardian = Address::generate(env);
    client.add_guardian(admin, &guardian);
    client.set_reputation(admin, &guardian, &score);
    mint_and_lock(env, token, client, &guardian, LOCK_THRESHOLD + 1);
    guardian
}

#[test]
fn test_add_guardian_and_register_task() {
    let (env, admin, _token, client) = setup();
    let guardian = Address::generate(&env);

    client.add_guardian(&admin, &guardian);
    client.register_task(&admin, &1u64);

    let task = client.get_task(&1u64).unwrap();
    assert_eq!(task.id, 1);
    assert_eq!(task.votes, 0);
    assert_eq!(task.total_weight_accrued, 0);
    assert_eq!(task.resolved_at, 0);
    assert!(!task.is_done);
}

#[test]
fn test_set_and_get_reputation() {
    let (env, admin, _token, client) = setup();
    let guardian = Address::generate(&env);

    client.add_guardian(&admin, &guardian);
    client.set_reputation(&admin, &guardian, &500u64);

    assert_eq!(client.get_reputation(&guardian), Some(500));
    assert_eq!(client.calculate_voting_power(&guardian), Some(500));
}

#[test]
fn test_weighted_vote_resolves_task_and_sets_resolved_at() {
    let (env, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    let guardian = add_guardian_with_rep(&env, &token, &client, &admin, 300);
    client.register_task(&admin, &42u64);
    client.vote(&guardian, &42u64);

    let task = client.get_task(&42u64).unwrap();
    assert_eq!(task.votes, 1);
    assert_eq!(task.total_weight_accrued, 300);
    assert_eq!(task.resolved_at, env.ledger().timestamp());
    assert!(task.is_done);
}

#[test]
fn test_multiple_low_rep_guardians_accumulate_weight() {
    let (env, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    let g1 = add_guardian_with_rep(&env, &token, &client, &admin, 100);
    let g2 = add_guardian_with_rep(&env, &token, &client, &admin, 100);
    let g3 = add_guardian_with_rep(&env, &token, &client, &admin, 100);

    client.register_task(&admin, &7u64);
    client.vote(&g1, &7u64);
    client.vote(&g2, &7u64);
    client.vote(&g3, &7u64);

    let task = client.get_task(&7u64).unwrap();
    assert_eq!(task.votes, 3);
    assert_eq!(task.total_weight_accrued, 300);
    assert!(task.is_done);
}

#[test]
fn test_vote_rejects_non_guardian_duplicate_and_missing_task() {
    let (env, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &token, &client, &admin, 300);
    let stranger = Address::generate(&env);

    client.register_task(&admin, &10u64);

    assert!(client.try_vote(&stranger, &10u64).is_err());
    client.vote(&guardian, &10u64);
    assert!(client.try_vote(&guardian, &10u64).is_err());

    let other_guardian = add_guardian_with_rep(&env, &token, &client, &admin, 300);
    assert!(client.try_vote(&other_guardian, &999u64).is_err());
}

#[test]
fn test_vote_rejects_missing_or_zero_reputation() {
    let (env, admin, token, client) = setup();
    let no_rep = Address::generate(&env);
    let zero_rep = Address::generate(&env);

    client.add_guardian(&admin, &no_rep);
    mint_and_lock(&env, &token, &client, &no_rep, LOCK_THRESHOLD + 1);

    client.add_guardian(&admin, &zero_rep);
    client.set_reputation(&admin, &zero_rep, &0u64);
    mint_and_lock(&env, &token, &client, &zero_rep, LOCK_THRESHOLD + 1);

    client.register_task(&admin, &11u64);

    assert!(client.try_vote(&no_rep, &11u64).is_err());
    assert!(client.try_vote(&zero_rep, &11u64).is_err());
}

#[test]
fn test_token_locking_rules() {
    let (env, admin, token, client) = setup();
    let guardian = Address::generate(&env);

    client.add_guardian(&admin, &guardian);
    client.set_reputation(&admin, &guardian, &100u64);
    client.register_task(&admin, &12u64);

    assert!(client.try_vote(&guardian, &12u64).is_err());

    mint_and_lock(&env, &token, &client, &guardian, LOCK_THRESHOLD);
    assert!(client.try_vote(&guardian, &12u64).is_err());

    mint_and_lock(&env, &token, &client, &guardian, 1);
    client.vote(&guardian, &12u64);

    assert_eq!(client.get_task(&12u64).unwrap().votes, 1);
}

#[test]
fn test_resign_and_unlock_refund_locked_tokens() {
    let (env, admin, token, client) = setup();
    let guardian = Address::generate(&env);
    let non_guardian = Address::generate(&env);

    client.add_guardian(&admin, &guardian);
    mint_and_lock(&env, &token, &client, &guardian, 200);
    client.resign_guardian(&guardian);

    let token_client = TokenClient::new(&env, &token);
    assert!(!client.is_guardian(&guardian));
    assert_eq!(token_client.balance(&guardian), 200);

    mint_and_lock(&env, &token, &client, &non_guardian, 150);
    client.unlock_tokens(&non_guardian);
    assert_eq!(token_client.balance(&non_guardian), 150);
}

#[test]
fn test_reward_stream_rejected_until_task_verified() {
    let (env, admin, _token, client) = setup();
    let contributor = Address::generate(&env);
    let drips_addr = Address::generate(&env);

    client.register_task(&admin, &20u64);

    assert!(client
        .try_start_reward_stream(&admin, &drips_addr, &contributor, &20u64)
        .is_err());
    assert!(client
        .try_start_reward_stream(&admin, &drips_addr, &contributor, &999u64)
        .is_err());
}

#[test]
fn test_reward_stream_stored_after_success() {
    let (env, admin, token, client) = setup();
    let contributor = Address::generate(&env);
    let guardian = add_guardian_with_rep(&env, &token, &client, &admin, 300);

    client.register_task(&admin, &21u64);
    client.vote(&guardian, &21u64);

    let drips_contract_id = env.register_contract(None, MockDripsContract);
    client.start_reward_stream(&admin, &drips_contract_id, &contributor, &21u64);

    let stream = client.get_reward_stream(&21u64).unwrap();
    assert_eq!(stream.task_id, 21);
    assert_eq!(stream.contributor, contributor);
    assert!(stream.active);

    assert!(client
        .try_start_reward_stream(&admin, &drips_contract_id, &contributor, &21u64)
        .is_err());
}

#[test]
fn test_admin_pause_and_circuit_breaker() {
    let (env, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &token, &client, &admin, 300);

    assert!(!client.is_paused());
    client.toggle_pause(&admin);
    assert!(client.is_paused());
    assert!(client.try_register_task(&admin, &30u64).is_err());

    client.toggle_pause(&admin);
    client.register_task(&admin, &30u64);
    client.vote(&guardian, &30u64);
    assert!(client.get_task(&30u64).unwrap().is_done);

    for _ in 0..51 {
        client.record_failure();
    }
    assert!(client.is_paused());

    client.reset_circuit_breaker(&admin);
    assert!(!client.is_paused());
}

#[test]
fn test_archive_moves_stale_resolved_task_and_frees_active_storage() {
    let (env, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &token, &client, &admin, 300);

    client.register_task(&admin, &40u64);
    client.vote(&guardian, &40u64);

    let resolved = client.get_task(&40u64).unwrap();
    env.ledger()
        .set_timestamp(resolved.resolved_at + ARCHIVE_AFTER_SECONDS + 1);

    client.archive_task(&40u64);

    assert!(client.get_task(&40u64).is_none());

    let archived = client.get_archived_task(&40u64).unwrap();
    assert_eq!(archived.id, resolved.id);
    assert_eq!(archived.votes, resolved.votes);
    assert_eq!(archived.is_done, resolved.is_done);
    assert_eq!(archived.resolved_at, resolved.resolved_at);
    assert_eq!(archived.total_weight_accrued, resolved.total_weight_accrued);
}

#[test]
fn test_archive_rejects_unresolved_recent_and_duplicate_tasks() {
    let (env, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &token, &client, &admin, 300);

    client.register_task(&admin, &50u64);
    env.ledger()
        .set_timestamp(1_000 + ARCHIVE_AFTER_SECONDS + 1);
    assert!(client.try_archive_task(&50u64).is_err());
    assert!(client.get_task(&50u64).is_some());

    client.register_task(&admin, &51u64);
    client.vote(&guardian, &51u64);
    let resolved = client.get_task(&51u64).unwrap();

    env.ledger()
        .set_timestamp(resolved.resolved_at + ARCHIVE_AFTER_SECONDS);
    assert!(client.try_archive_task(&51u64).is_err());
    assert!(client.get_archived_task(&51u64).is_none());

    env.ledger()
        .set_timestamp(resolved.resolved_at + ARCHIVE_AFTER_SECONDS + 1);
    client.archive_task(&51u64);
    assert!(client.try_archive_task(&51u64).is_err());
}

#[contract]
pub struct MockDripsContract;

#[contractimpl]
impl MockDripsContract {
    pub fn start_stream(_env: Env, _contributor: Address, _task_id: u64, _resolution_status: u32) {}
}
