use anyhow::Error;
use fehler::{throw, throws};
use solana_runtime::accounts::LoadedTransaction;
use solana_sdk::{
    account::{Account, AccountSharedData, ReadableAccount},
    message::SanitizedMessage,
    native_loader,
    pubkey::Pubkey,
    sysvar,
    sysvar::instructions::construct_instructions_data,
    transaction::TransactionError,
};

use super::AccountLoader;

impl<'a, G> AccountLoader<'a, G>
where
    G: FnMut(&Pubkey) -> Option<AccountSharedData>,
{
    // Roughly solana_runtime::accounts::Accounts::load_transaction_accounts
    #[throws(Error)]
    pub fn load_transaction_account(&mut self, msg: &SanitizedMessage) -> LoadedTransaction {
        let mut accounts =
            Vec::with_capacity(msg.account_keys().len() + msg.instructions().len() * 2);

        for &key in msg.account_keys().iter() {
            if solana_sdk::sysvar::instructions::check_id(&key) {
                let acc = Account {
                    data: construct_instructions_data(&msg.decompile_instructions()).into(),
                    owner: sysvar::id(),
                    ..Default::default()
                };
                accounts.push((key, acc.into()));
                continue;
            }

            let account = self
                .get_account(&key)?
                .ok_or(TransactionError::AccountNotFound)?;

            accounts.push((key, account));
        }

        let builtins_start_index = accounts.len();
        let mut program_indices = Vec::with_capacity(msg.instructions().len());
        'OUTER: for ix in msg.instructions() {
            let mut account_indices = Vec::new();
            let mut program_index = ix.program_id_index as usize;

            for _ in 0..5 {
                let (program_id, program_account) = accounts
                    .get(program_index)
                    .ok_or(TransactionError::ProgramAccountNotFound)?;

                // push nothing if the program is native_loader
                if native_loader::check_id(program_id) {
                    program_indices.push(account_indices);
                    continue 'OUTER;
                }

                // push the program
                account_indices.insert(0, program_index as u16);

                let owner_id = program_account.owner();
                if native_loader::check_id(owner_id) {
                    program_indices.push(account_indices);
                    continue 'OUTER;
                }
                program_index = match accounts
                    .get(builtins_start_index..)
                    .ok_or(TransactionError::ProgramAccountNotFound)?
                    .iter()
                    .position(|(key, _)| key == owner_id)
                {
                    Some(owner_index) => builtins_start_index.saturating_add(owner_index),
                    None => {
                        let owner_index = accounts.len();
                        let owner_account = self
                            .get_account(owner_id)?
                            .ok_or(TransactionError::ProgramAccountNotFound)?;

                        accounts.push((*owner_id, owner_account));
                        owner_index
                    }
                };
            }

            throw!(TransactionError::CallChainTooDeep)
        }
        // println!(
        //     "accounts: {:?}, program_indices: {:?}",
        //     accounts.iter().map(|(a, _)| a).collect::<Vec<_>>(),
        //     program_indices
        //         .iter()
        //         .map(|ixs| ixs.iter().map(|i| accounts[*i].0).collect::<Vec<_>>())
        //         .collect::<Vec<_>>()
        // );

        LoadedTransaction {
            accounts,
            program_indices,
            rent: 0,
            rent_debits: Default::default(),
        }
    }
}
