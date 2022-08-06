/*
 * 将原vm_validator/src/vm_validator.rs中交易验证的相关的代码
 * copy到mempool代码中，避免依赖整个vm-validator子模块
 *
 * 将vm-validator/src/mocks/mock_vm_validator.rs复制到mempool，
 * 用于测试。
 */
use fail::fail_point;
use std::sync::Arc;
use anyhow::Result;

use aptos_vm::AptosVM;
use aptos_types::{
    account_address::AccountAddress,
    account_config::AccountSequenceInfo,
    on_chain_config::OnChainConfigPayload,
    transaction::{SignedTransaction, VMValidatorResult},
};
use storage_interface::{
    cached_state_view::CachedDbStateView, state_view::LatestDbStateCheckpointView, DbReader,
};
use storage_interface::state_view::DbStateView;

pub trait TransactionValidation: Send + Sync + Clone {
    type ValidationInstance: aptos_vm::VMValidator;

    /// Validate a txn from client
    fn validate_transaction(&self, _txn: SignedTransaction) -> Result<VMValidatorResult>;

    /// Restart the transaction validation instance
    fn restart(&mut self, config: OnChainConfigPayload) -> Result<()>;

    /// Notify about new commit
    fn notify_commit(&mut self);
}

pub struct VMValidator {
    db_reader: Arc<dyn DbReader>,
    state_view: CachedDbStateView,
    vm: AptosVM,
}

impl Clone for VMValidator {
    fn clone(&self) -> Self {
        Self::new(self.db_reader.clone())
    }
}

impl VMValidator {
    pub fn new(db_reader: Arc<dyn DbReader>) -> Self {
        let db_state_view = db_reader
            .latest_state_checkpoint_view()
            .expect("Get db view cannot fail");

        let vm = AptosVM::new_for_validation(&db_state_view);
        VMValidator {
            db_reader,
            state_view: db_state_view.into(),
            vm,
        }
    }
}

impl TransactionValidation for VMValidator {
    type ValidationInstance = AptosVM;

    fn validate_transaction(&self, txn: SignedTransaction) -> Result<VMValidatorResult> {
        fail_point!("vm_validator::validate_transaction", |_| {
            Err(anyhow::anyhow!(
                "Injected error in vm_validator::validate_transaction"
            ))
        });
        use aptos_vm::VMValidator;

        Ok(self.vm.validate_transaction(txn, &self.state_view))
    }

    fn restart(&mut self, _config: OnChainConfigPayload) -> Result<()> {
        self.notify_commit();

        self.vm = AptosVM::new_for_validation(&self.state_view);
        Ok(())
    }

    fn notify_commit(&mut self) {
        self.state_view = self
            .db_reader
            .latest_state_checkpoint_view()
            .expect("Get db view cannot fail")
            .into();
    }
}

/// returns account's sequence number from storage
pub fn get_account_sequence_number(
    state_view: &DbStateView,
    address: AccountAddress,
) -> Result<AccountSequenceInfo> {
    fail_point!("vm_validator::get_account_sequence_number", |_| {
        Err(anyhow::anyhow!(
            "Injected error in get_account_sequence_number"
        ))
    });

    let account_state_view = state_view.as_account_with_state_view(&address);

    if let Ok(Some(crsn)) = account_state_view.get_crsn_resource() {
        return Ok(AccountSequenceInfo::CRSN {
            min_nonce: crsn.min_nonce(),
            size: crsn.size(),
        });
    }

    match account_state_view.get_account_resource()? {
        Some(account_resource) => Ok(AccountSequenceInfo::Sequential(
            account_resource.sequence_number(),
        )),
        None => Ok(AccountSequenceInfo::Sequential(0)),
    }
}