#![no_std]

mod circuit_breaker;
mod drips;
pub mod events;
mod guardian;
mod reentrancy;
mod reputation;
mod storage;
mod task;
mod types;
mod vault;

use soroban_sdk::{contract, contractimpl, Address, Env};
use types::{ContractError, DataKey, RewardStream};

pub use drips::{get_reward_stream, start_drips_stream};
pub use guardian::{add_guardian, is_guardian};
pub use task::{get_task, register_task};

const DEFAULT_WEIGHT_THRESHOLD: u64 = 300;

fn require_not_paused(env: &Env) -> Result<(), ContractError> {
    if env
        .storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        return Err(ContractError::ContractPaused);
    }
    Ok(())
}

fn vote_locked(env: &Env, guardian: &Address, task_id: u64) -> Result<(), ContractError> {
    if !guardian::is_guardian(env, guardian) {
        return Err(ContractError::NotAuthorized);
    }

    let token_key = DataKey::TokenAddress;
    if env.storage().instance().has(&token_key) {
        let threshold: i128 = env
            .storage()
            .instance()
            .get(&DataKey::LockThreshold)
            .unwrap_or(0);
        let balance_key = DataKey::LockedBalance(guardian.clone());
        let locked_balance: i128 = env.storage().instance().get(&balance_key).unwrap_or(0);

        if locked_balance <= threshold {
            return Err(ContractError::InsufficientLockedBalance);
        }
    }

    let voted_key = DataKey::Voted(task_id, guardian.clone());
    if env.storage().instance().has(&voted_key) {
        return Err(ContractError::DuplicateVote);
    }

    let weight = reputation::calculate_voting_power(env, guardian)
        .ok_or(ContractError::NoReputationScore)?;

    if weight == 0 {
        return Err(ContractError::ZeroWeightVote);
    }

    let mut task = storage::get_active_task(env, task_id).ok_or(ContractError::TaskNotFound)?;

    task.total_weight_accrued = task
        .total_weight_accrued
        .checked_add(weight)
        .ok_or(ContractError::WeightOverflow)?;
    task.votes += 1;

    let threshold: u64 = env
        .storage()
        .instance()
        .get(&DataKey::WeightThreshold)
        .unwrap_or(DEFAULT_WEIGHT_THRESHOLD);

    if task.total_weight_accrued >= threshold && !task.is_done {
        task.is_done = true;
        task.resolved_at = env.ledger().timestamp();
        events::emit_task_resolved(env, task_id, task.total_weight_accrued);

        if let Some(vault_addr) = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::VaultAddress)
        {
            let vault_client = vault::VaultClient::new(env, &vault_addr);
            if vault_client.try_release_funds(&task_id).is_err() {
                return Err(ContractError::EscrowUnavailable);
            }
        }
    }

    env.storage().instance().set(&voted_key, &true);
    storage::set_active_task(env, &task);
    events::emit_weighted_vote(env, task_id, guardian, weight);

    Ok(())
}

#[contract]
pub struct VeroContract;

#[contractimpl]
impl VeroContract {
    pub fn initialize(env: Env, token: Address, threshold: i128) -> Result<(), ContractError> {
        let token_key = DataKey::TokenAddress;
        if env.storage().instance().has(&token_key) {
            return Err(ContractError::AlreadyInitialized);
        }
        env.storage().instance().set(&token_key, &token);
        env.storage()
            .instance()
            .set(&DataKey::LockThreshold, &threshold);
        Ok(())
    }

    pub fn lock_tokens(env: Env, guardian: Address, amount: i128) -> Result<(), ContractError> {
        guardian.require_auth();

        let token_key = DataKey::TokenAddress;
        if !env.storage().instance().has(&token_key) {
            return Err(ContractError::NotInitialized);
        }
        let token_address: Address = env.storage().instance().get(&token_key).unwrap();

        let client = soroban_sdk::token::Client::new(&env, &token_address);
        client.transfer(&guardian, &env.current_contract_address(), &amount);

        let balance_key = DataKey::LockedBalance(guardian.clone());
        let current_balance: i128 = env.storage().instance().get(&balance_key).unwrap_or(0);
        env.storage()
            .instance()
            .set(&balance_key, &(current_balance + amount));

        Ok(())
    }

    pub fn resign_guardian(env: Env, guardian: Address) -> Result<(), ContractError> {
        guardian.require_auth();

        let token_key = DataKey::TokenAddress;
        if !env.storage().instance().has(&token_key) {
            return Err(ContractError::NotInitialized);
        }

        if !guardian::is_guardian(&env, &guardian) {
            return Err(ContractError::NotGuardian);
        }

        let key = DataKey::Guardian(guardian.clone());
        env.storage().instance().set(&key, &false);

        let balance_key = DataKey::LockedBalance(guardian.clone());
        let locked_balance: i128 = env.storage().instance().get(&balance_key).unwrap_or(0);
        if locked_balance > 0 {
            let token_address: Address = env.storage().instance().get(&token_key).unwrap();
            let client = soroban_sdk::token::Client::new(&env, &token_address);
            client.transfer(&env.current_contract_address(), &guardian, &locked_balance);
            env.storage().instance().set(&balance_key, &0i128);
        }

        Ok(())
    }

    pub fn unlock_tokens(env: Env, guardian: Address) -> Result<(), ContractError> {
        guardian.require_auth();

        let token_key = DataKey::TokenAddress;
        if !env.storage().instance().has(&token_key) {
            return Err(ContractError::NotInitialized);
        }

        if guardian::is_guardian(&env, &guardian) {
            return Err(ContractError::StillGuardian);
        }

        let balance_key = DataKey::LockedBalance(guardian.clone());
        let locked_balance: i128 = env.storage().instance().get(&balance_key).unwrap_or(0);
        if locked_balance > 0 {
            let token_address: Address = env.storage().instance().get(&token_key).unwrap();
            let client = soroban_sdk::token::Client::new(&env, &token_address);
            client.transfer(&env.current_contract_address(), &guardian, &locked_balance);
            env.storage().instance().set(&balance_key, &0i128);
        }

        Ok(())
    }

    pub fn toggle_pause(env: Env, admin: Address) {
        admin.require_auth();
        let current: bool = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        env.storage().instance().set(&DataKey::Paused, &!current);
        events::emit_pause_toggled(&env, !current);
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn add_guardian(env: Env, admin: Address, guardian: Address) -> Result<(), ContractError> {
        require_not_paused(&env)?;
        guardian::add_guardian(&env, admin, guardian);
        Ok(())
    }

    pub fn is_guardian(env: Env, guardian: Address) -> bool {
        guardian::is_guardian(&env, &guardian)
    }

    pub fn set_reputation(
        env: Env,
        admin: Address,
        guardian: Address,
        score: u64,
    ) -> Result<(), ContractError> {
        require_not_paused(&env)?;
        reputation::set_reputation(&env, admin, guardian, score);
        Ok(())
    }

    pub fn get_reputation(env: Env, guardian: Address) -> Result<Option<u64>, ContractError> {
        require_not_paused(&env)?;
        Ok(reputation::get_reputation(&env, &guardian))
    }

    pub fn calculate_voting_power(
        env: Env,
        guardian: Address,
    ) -> Result<Option<u64>, ContractError> {
        require_not_paused(&env)?;
        Ok(reputation::calculate_voting_power(&env, &guardian))
    }

    pub fn set_weight_threshold(
        env: Env,
        admin: Address,
        threshold: u64,
    ) -> Result<(), ContractError> {
        require_not_paused(&env)?;
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::WeightThreshold, &threshold);
        Ok(())
    }

    pub fn get_weight_threshold(env: Env) -> Result<u64, ContractError> {
        require_not_paused(&env)?;
        Ok(env
            .storage()
            .instance()
            .get(&DataKey::WeightThreshold)
            .unwrap_or(DEFAULT_WEIGHT_THRESHOLD))
    }

    pub fn set_vault_address(env: Env, admin: Address, vault: Address) {
        admin.require_auth();
        env.storage().instance().set(&DataKey::VaultAddress, &vault);
    }

    pub fn register_task(env: Env, admin: Address, task_id: u64) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        task::register_task(&env, admin, task_id)
    }

    pub fn vote(env: Env, guardian: Address, task_id: u64) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        guardian.require_auth();
        reentrancy::lock(&env)?;

        let result = vote_locked(&env, &guardian, task_id);
        reentrancy::unlock(&env);
        result
    }

    pub fn get_task(env: Env, task_id: u64) -> Result<Option<types::Task>, ContractError> {
        require_not_paused(&env)?;
        Ok(task::get_task(&env, task_id))
    }

    pub fn archive_task(env: Env, task_id: u64) -> Result<(), ContractError> {
        require_not_paused(&env)?;
        let task = storage::get_active_task(&env, task_id).ok_or(ContractError::TaskNotFound)?;
        storage::archive_task(&env, task_id)?;
        events::emit_task_archived(&env, task_id, task.resolved_at);
        Ok(())
    }

    pub fn get_archived_task(env: Env, task_id: u64) -> Result<Option<types::Task>, ContractError> {
        require_not_paused(&env)?;
        Ok(storage::get_archived_task(&env, task_id))
    }

    pub fn start_reward_stream(
        env: Env,
        admin: Address,
        drips_address: Address,
        contributor: Address,
        task_id: u64,
    ) -> Result<(), ContractError> {
        require_not_paused(&env)?;
        admin.require_auth();

        let result = drips::start_drips_stream(&env, drips_address, contributor.clone(), task_id);

        match &result {
            Ok(()) => events::emit_reward_stream_started(&env, task_id, &contributor),
            Err(_) => events::emit_reward_stream_failed(&env, task_id, &contributor),
        }

        result
    }

    pub fn get_reward_stream(
        env: Env,
        task_id: u64,
    ) -> Result<Option<RewardStream>, ContractError> {
        require_not_paused(&env)?;
        Ok(drips::get_reward_stream(&env, task_id))
    }

    pub fn record_failure(env: Env) {
        circuit_breaker::record_failure(&env);
    }

    pub fn reset_circuit_breaker(env: Env, admin: Address) {
        circuit_breaker::reset(&env, admin);
    }

    pub fn upgrade_contract(env: Env, admin: Address, new_wasm_hash: soroban_sdk::BytesN<32>) {
        admin.require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}
