use soroban_sdk::{Address, Env, Vec};

use crate::events;
use crate::reentrancy;
use crate::storage;
use crate::types::{ContractError, Task};
use crate::validation;
use crate::types::{ContractError, DataKey, Task};

const MAX_REGISTER_TASK_BATCH_SIZE: u32 = 32;

pub fn register_tasks(env: &Env, admin: Address, task_ids: Vec<u64>) -> Result<(), ContractError> {
    if task_ids.is_empty() || task_ids.len() > MAX_REGISTER_TASK_BATCH_SIZE {
        return Err(ContractError::BatchTooLarge);
    }

    validation::validate_admin_address(env, &admin)?;
    admin.require_auth();

    let mut seen_task_ids = Vec::new(env);
    for task_id in task_ids.iter() {
        validation::validate_task_id(task_id)?;
        if seen_task_ids.contains(task_id) {
            return Err(ContractError::InvalidConfig);
        }
        if storage::has_active_task(env, task_id)
            || storage::get_archived_task(env, task_id).is_some()
        {
            return Err(ContractError::InvalidConfig);
        }
        seen_task_ids.push_back(task_id);
    }

    reentrancy::lock(env)?;

    let mut all_tasks: Vec<u64> = env
        .storage()
        .instance()
        .get(&crate::types::DataKey::AllTasks)
        .unwrap_or(Vec::new(env));

    for task_id in task_ids.iter() {
    for task_id in task_ids.into_iter() {
        if storage::get_active_task(env, task_id).is_some() {
            reentrancy::unlock(env);
            return Err(ContractError::NotAuthorized);
        }

        all_tasks.push_back(task_id);

        let task = Task {
            id: task_id,
            votes: 0,
            is_done: false,
            resolved_at: 0,
            total_weight_accrued: 0,
            is_cancelled: false,
        };
        storage::set_active_task(env, &task);
        all_tasks.push_back(task_id);
    }

    env.storage()
        .instance()
        .set(&crate::types::DataKey::AllTasks, &all_tasks);
    }

    env.storage().instance().set(&DataKey::AllTasks, &all_tasks);

    reentrancy::unlock(env);
    Ok(())
}

pub fn cancel_task(env: &Env, admin: Address, task_id: u64) -> Result<(), ContractError> {
    admin.require_auth();
    reentrancy::lock(env)?;

    let mut task = storage::get_active_task(env, task_id).ok_or(ContractError::TaskNotFound)?;
    if task.is_cancelled || task.is_done {
        reentrancy::unlock(env);
        return Err(ContractError::NotAuthorized);
    }

    task.is_cancelled = true;
    storage::set_active_task(env, &task);
    events::emit_task_cancelled(env, task_id);

    reentrancy::unlock(env);
    Ok(())
}

pub fn cancel_task(env: &Env, admin: Address, task_id: u64) -> Result<(), ContractError> {
    validation::validate_admin_address(env, &admin)?;
    admin.require_auth();
    validation::validate_task_id(task_id)?;

    let mut task = storage::get_active_task(env, task_id).ok_or(ContractError::TaskNotFound)?;
    if task.is_cancelled {
        return Err(ContractError::TaskCancelled);
    }

    task.is_cancelled = true;
    storage::set_active_task(env, &task);
    events::emit_task_cancelled(env, task_id);
    Ok(())
}

pub fn get_task(env: &Env, task_id: u64) -> Option<Task> {
    storage::get_active_task(env, task_id)
}

pub fn get_all_tasks(env: &Env) -> Vec<u64> {
    env.storage()
        .instance()
        .get(&crate::types::DataKey::AllTasks)
        .unwrap_or(Vec::new(env))
}
