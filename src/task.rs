use soroban_sdk::{Address, Env};

use crate::reentrancy;
use crate::storage;
use crate::types::{ContractError, Task};

pub fn register_task(env: &Env, admin: Address, task_id: u64) -> Result<(), ContractError> {
    admin.require_auth();

    reentrancy::lock(env)?;

    if storage::has_active_task(env, task_id) || storage::get_archived_task(env, task_id).is_some()
    {
        reentrancy::unlock(env);
        return Err(ContractError::NotAuthorized);
    }

    let task = Task {
        id: task_id,
        votes: 0,
        is_done: false,
        resolved_at: 0,
        total_weight_accrued: 0,
    };
    storage::set_active_task(env, &task);

    reentrancy::unlock(env);
    Ok(())
}

pub fn get_task(env: &Env, task_id: u64) -> Option<Task> {
    storage::get_active_task(env, task_id)
}
