use std::{
    collections::{hash_map::Entry, HashMap},
    sync::{atomic::Ordering, Arc},
};

use anyhow::Error;
use fehler::throws;
use solana_program_runtime::loaded_programs::{
    LoadedProgram, LoadedProgramMatchCriteria, LoadedProgramType, LoadedProgramsForTxBatch,
    WorkingSlot,
};
use solana_sdk::{
    account::{AccountSharedData, ReadableAccount},
    account_utils::StateMut,
    bpf_loader_upgradeable::{self, UpgradeableLoaderState},
    message::SanitizedMessage,
    pubkey::Pubkey,
    slot_history::Slot,
    sysvar,
    transaction::TransactionError,
    transaction_context::TransactionContext,
};

use super::AccountLoader;

impl<'a, G> AccountLoader<'a, G>
where
    G: FnMut(&Pubkey) -> Option<AccountSharedData>,
{
    #[throws(Error)]
    pub fn load_programs<'b, I, S>(&mut self, s: &S, messages: I) -> LoadedProgramsForTxBatch
    where
        I: IntoIterator<Item = &'b SanitizedMessage>,
        S: WorkingSlot,
    {
        let mut programs_and_slots = HashMap::new();

        for msg in messages {
            for &key in msg.account_keys().iter() {
                let acc = self
                    .get_account(&key)?
                    .ok_or(TransactionError::AccountNotFound)?;
                if self.program_owners.contains(&acc.owner()) {
                    match programs_and_slots.entry(key) {
                        Entry::Vacant(e) => {
                            e.insert((LoadedProgramMatchCriteria::NoCriteria, 0));
                        }
                        Entry::Occupied(mut e) => e.get_mut().1 += 1,
                    }
                }
            }
        }
        for builtin_program in self.builtin_programs.iter() {
            programs_and_slots.insert(
                *builtin_program,
                (LoadedProgramMatchCriteria::NoCriteria, 0),
            );
        }

        let (mut loaded_programs_for_txs, missing_programs) = {
            // Lock the global cache to figure out which programs need to be loaded
            self.loaded_programs
                .extract(s, programs_and_slots.into_iter())
        };

        let slot = s.current_slot();

        // Load missing programs while global cache is unlocked
        let mut loaded_missing_programs = vec![];
        for (key, count) in missing_programs {
            let program = self.load_program(slot, &key)?;
            program.tx_usage_counter.store(count, Ordering::Relaxed);
            loaded_missing_programs.push((key, program))
        }

        // Lock the global cache again to replenish the missing programs
        for (key, program) in loaded_missing_programs {
            let (_was_occupied, entry) = self.loaded_programs.replenish(key, program);
            // Use the returned entry as that might have been deduplicated globally
            loaded_programs_for_txs.replenish(key, entry);
        }

        loaded_programs_for_txs
    }

    // Roughly Bank::load_program
    #[throws(Error)]
    pub fn load_program(&mut self, slot: Slot, pubkey: &Pubkey) -> Arc<LoadedProgram> {
        let program = self
            .get_account(pubkey)?
            .ok_or(TransactionError::AccountNotFound)?;

        let mut transaction_accounts = vec![(*pubkey, program)];
        let is_upgradeable_loader =
            bpf_loader_upgradeable::check_id(transaction_accounts[0].1.owner());
        if is_upgradeable_loader {
            let programdata_address = match transaction_accounts[0].1.state() {
                Ok(UpgradeableLoaderState::Program {
                    programdata_address,
                }) => programdata_address,
                _ => {
                    return Arc::new(LoadedProgram::new_tombstone(
                        slot,
                        LoadedProgramType::Closed,
                    ));
                }
            };

            let programdata_account = self
                .get_account(&programdata_address)?
                .ok_or(TransactionError::AccountNotFound)?;

            transaction_accounts.push((programdata_address, programdata_account));
        }

        let mut transaction_context = TransactionContext::new(
            transaction_accounts,
            Some(sysvar::rent::Rent::default()),
            1,
            1,
        );
        let instruction_context = transaction_context.get_next_instruction_context().unwrap();
        instruction_context.configure(if is_upgradeable_loader { &[0, 1] } else { &[0] }, &[], &[]);
        transaction_context.push().unwrap();
        let instruction_context = transaction_context
            .get_current_instruction_context()
            .unwrap();
        let program = instruction_context
            .try_borrow_program_account(&transaction_context, 0)
            .unwrap();
        let programdata = if is_upgradeable_loader {
            Some(
                instruction_context
                    .try_borrow_program_account(&transaction_context, 1)
                    .unwrap(),
            )
        } else {
            None
        };
        solana_bpf_loader_program::load_program_from_account(
            &self.feature_set,
            None, // log_collector
            &program,
            programdata.as_ref().unwrap_or(&program),
            self.environment.clone(),
        )
        .map(|(loaded_program, _)| loaded_program)
        .unwrap_or_else(|_| {
            Arc::new(LoadedProgram::new_tombstone(
                slot,
                LoadedProgramType::FailedVerification(self.environment.clone()),
            ))
        })
    }
}
