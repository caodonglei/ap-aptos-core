// Copyright (c) Aptos
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use aptos_state_view::account_with_state_view::AsAccountWithStateView;
use aptos_types::{
    account_address::AccountAddress,
    account_config::AccountSequenceInfo,
    account_view::AccountView,
    on_chain_config::OnChainConfigPayload,
    transaction::{SignedTransaction, VMValidatorResult},
    vm_status::StatusCode,
};
use aptos_vm::AptosVM;
use fail::fail_point;
use std::sync::Arc;
use storage_interface::state_view::DbStateView;
use storage_interface::{
    cached_state_view::CachedDbStateView, state_view::LatestDbStateCheckpointView, DbReader,
};

/// constance for mock vm_validator
pub const ACCOUNT_DNE_TEST_ADD: AccountAddress =
    AccountAddress::new([0_u8; AccountAddress::LENGTH]);
pub const INVALID_SIG_TEST_ADD: AccountAddress =
    AccountAddress::new([1_u8; AccountAddress::LENGTH]);
pub const INSUFFICIENT_BALANCE_TEST_ADD: AccountAddress =
    AccountAddress::new([2_u8; AccountAddress::LENGTH]);
pub const SEQ_NUMBER_TOO_NEW_TEST_ADD: AccountAddress =
    AccountAddress::new([3_u8; AccountAddress::LENGTH]);
pub const SEQ_NUMBER_TOO_OLD_TEST_ADD: AccountAddress =
    AccountAddress::new([4_u8; AccountAddress::LENGTH]);
pub const TXN_EXPIRATION_TIME_TEST_ADD: AccountAddress =
    AccountAddress::new([5_u8; AccountAddress::LENGTH]);
pub const INVALID_AUTH_KEY_TEST_ADD: AccountAddress =
    AccountAddress::new([6_u8; AccountAddress::LENGTH]);


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



#[derive(Clone)]
pub struct MockVMValidator;

impl VMValidator for MockVMValidator {
    fn validate_transaction(
        &self,
        _transaction: SignedTransaction,
        _state_view: &impl StateView,
    ) -> VMValidatorResult {
        VMValidatorResult::new(None, 0)
    }
}

impl TransactionValidation for MockVMValidator {
    type ValidationInstance = MockVMValidator;
    fn validate_transaction(&self, txn: SignedTransaction) -> Result<VMValidatorResult> {
        let txn = match txn.check_signature() {
            Ok(txn) => txn,
            Err(_) => {
                return Ok(VMValidatorResult::new(
                    Some(StatusCode::INVALID_SIGNATURE),
                    0,
                ))
            }
        };

        let sender = txn.sender();
        let ret = if sender == ACCOUNT_DNE_TEST_ADD {
            Some(StatusCode::SENDING_ACCOUNT_DOES_NOT_EXIST)
        } else if sender == INVALID_SIG_TEST_ADD {
            Some(StatusCode::INVALID_SIGNATURE)
        } else if sender == INSUFFICIENT_BALANCE_TEST_ADD {
            Some(StatusCode::INSUFFICIENT_BALANCE_FOR_TRANSACTION_FEE)
        } else if sender == SEQ_NUMBER_TOO_NEW_TEST_ADD {
            Some(StatusCode::SEQUENCE_NUMBER_TOO_NEW)
        } else if sender == SEQ_NUMBER_TOO_OLD_TEST_ADD {
            Some(StatusCode::SEQUENCE_NUMBER_TOO_OLD)
        } else if sender == TXN_EXPIRATION_TIME_TEST_ADD {
            Some(StatusCode::TRANSACTION_EXPIRED)
        } else if sender == INVALID_AUTH_KEY_TEST_ADD {
            Some(StatusCode::INVALID_AUTH_KEY)
        } else {
            None
        };
        Ok(VMValidatorResult::new(ret, 0))
    }

    fn restart(&mut self, _config: OnChainConfigPayload) -> Result<()> {
        unimplemented!();
    }

    fn notify_commit(&mut self) {}
}
