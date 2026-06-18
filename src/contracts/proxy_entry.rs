use crate::contracts::logic;
use crate::types::{ContractError, DataKey, RewardStream, Snapshot};
use crate::DEFAULT_WEIGHT_THRESHOLD;
use crate::{circuit_breaker, drips, events, guardian, reputation, storage, task};
use soroban_sdk::{contract, contractimpl, Address, Env};

#[contract]
pub struct VeroContract;

#[contractimpl]
impl VeroContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        lock_threshold: i128,
    ) -> Result<(), ContractError> {
        if env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Initialized)
            .unwrap_or(false)
        {
            return Err(ContractError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TokenAddress, &token);
        env.storage()
            .instance()
            .set(&DataKey::LockThreshold, &lock_threshold);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage().instance().extend_ttl(100_000, 100_000);
        Ok(())
    }

    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Admin)
    }

    pub fn toggle_pause(env: Env, admin: Address) -> Result<(), ContractError> {
        admin.require_auth();
        let current = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        env.storage().instance().set(&DataKey::Paused, &!current);
        Ok(())
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), ContractError> {
        admin.require_auth();
        env.storage().instance().set(&DataKey::Paused, &true);
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), ContractError> {
        admin.require_auth();
        env.storage().instance().set(&DataKey::Paused, &false);
        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn add_guardian(env: Env, admin: Address, guardian: Address) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        guardian::add_guardian(&env, admin, guardian);
        Ok(())
    }

    pub fn remove_guardian(
        env: Env,
        admin: Address,
        guardian: Address,
    ) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        guardian::remove_guardian(&env, admin, guardian);
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
        circuit_breaker::require_not_paused(&env)?;
        reputation::set_reputation(&env, admin, guardian, score);
        Ok(())
    }

    pub fn get_reputation(env: Env, guardian: Address) -> Option<u64> {
        reputation::get_reputation(&env, &guardian)
    }

    pub fn calculate_voting_power(env: Env, guardian: Address) -> Option<u64> {
        reputation::calculate_voting_power(&env, &guardian)
    }

    pub fn lock_tokens(env: Env, guardian: Address, amount: i128) -> Result<(), ContractError> {
        logic::lock_tokens(&env, guardian, amount)
    }

    pub fn request_unlock(env: Env, guardian: Address) -> Result<(), ContractError> {
        logic::request_unlock(&env, guardian)
    }

    pub fn unlock_tokens(env: Env, guardian: Address) -> Result<(), ContractError> {
        logic::unlock_tokens(&env, guardian)
    }

    pub fn resign_guardian(env: Env, guardian: Address) -> Result<(), ContractError> {
        logic::resign_guardian(&env, guardian)
    }

    pub fn set_weight_threshold(
        env: Env,
        admin: Address,
        threshold: u64,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::WeightThreshold, &threshold);
        Ok(())
    }

    pub fn get_weight_threshold(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::WeightThreshold)
            .unwrap_or(DEFAULT_WEIGHT_THRESHOLD)
    }

    pub fn set_vault_address(env: Env, admin: Address, vault: Address) {
        admin.require_auth();
        env.storage().instance().set(&DataKey::VaultAddress, &vault);
    }

    pub fn register_task(env: Env, admin: Address, task_id: u64) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotInitialized)?;
        if admin != stored_admin {
            return Err(ContractError::NotAuthorized);
        }
        let task_ids = soroban_sdk::vec![&env, task_id];
        task::register_tasks(&env, admin, task_ids)
    }

    pub fn cancel_task(env: Env, admin: Address, task_id: u64) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        task::cancel_task(&env, admin, task_id)
    }

    pub fn vote(env: Env, guardian: Address, task_id: u64) -> Result<(), ContractError> {
        logic::process_vote(&env, guardian, task_id)
    }

    pub fn get_task(env: Env, task_id: u64) -> Option<crate::types::Task> {
        task::get_task(&env, task_id)
    }

    pub fn archive_task(env: Env, task_id: u64) -> Result<(), ContractError> {
        storage::archive_task(&env, task_id)
    }

    pub fn get_archived_task(env: Env, task_id: u64) -> Option<crate::types::Task> {
        storage::get_archived_task(&env, task_id)
    }

    pub fn start_reward_stream(
        env: Env,
        admin: Address,
        drips_address: Address,
        contributor: Address,
        task_id: u64,
    ) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        admin.require_auth();

        let result = drips::start_drips_stream(&env, drips_address, contributor.clone(), task_id);

        match &result {
            Ok(()) => events::emit_reward_stream_started(&env, task_id, &contributor),
            Err(_) => events::emit_reward_stream_failed(&env, task_id, &contributor),
        }

        result
    }

    pub fn get_reward_stream(env: Env, task_id: u64) -> Option<RewardStream> {
        drips::get_reward_stream(&env, task_id)
    }

    pub fn record_failure(env: Env) {
        circuit_breaker::record_failure(&env);
    }

    pub fn reset_circuit_breaker(env: Env, admin: Address) {
        circuit_breaker::reset(&env, admin);
    }

    pub fn get_estimated_cost(_env: Env, op: crate::types::Operation) -> u64 {
        crate::gas::get_estimated_cost(op)
    }

    pub fn upgrade_contract(env: Env, admin: Address, new_wasm_hash: soroban_sdk::BytesN<32>) {
        admin.require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    pub fn get_snapshot(env: Env) -> Snapshot {
        logic::get_snapshot(&env)
    }

    pub fn record_snapshot(env: Env) -> Result<(), ContractError> {
        logic::record_snapshot(&env)
    }

    pub fn get_snapshot_history(env: Env) -> soroban_sdk::Vec<u64> {
        env.storage()
            .instance()
            .get(&DataKey::AllSnapshots)
            .unwrap_or(soroban_sdk::Vec::new(&env))
    }

    pub fn get_snapshot_at(env: Env, timestamp: u64) -> Result<Snapshot, ContractError> {
        env.storage()
            .instance()
            .get(&DataKey::Snapshot(timestamp))
            .ok_or(ContractError::SnapshotNotFound)
    }

    pub fn get_withdrawal_timelock(env: Env, guardian: Address) -> Option<u64> {
        env.storage()
            .instance()
            .get(&DataKey::WithdrawalTimelock(guardian))
    }
}
