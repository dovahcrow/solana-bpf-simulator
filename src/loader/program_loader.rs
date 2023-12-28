use std::sync::{atomic::Ordering, Arc};

use anyhow::Error;
use fehler::throws;
use solana_program_runtime::loaded_programs::{
    LoadProgramMetrics, LoadedProgram, LoadedProgramMatchCriteria, LoadedProgramType,
    LoadedProgramsForTxBatch, WorkingSlot, DELAY_VISIBILITY_SLOT_OFFSET,
};
use solana_sdk::{
    account::{AccountSharedData, ReadableAccount},
    account_utils::StateMut,
    bpf_loader_upgradeable::{self, UpgradeableLoaderState},
    epoch_schedule::DEFAULT_SLOTS_PER_EPOCH,
    feature_set,
    instruction::InstructionError,
    loader_v4::{self, LoaderV4State, LoaderV4Status},
    message::SanitizedMessage,
    pubkey::Pubkey,
    slot_history::Slot,
    transaction::TransactionError,
};

use super::AccountLoader;

impl<'a, G> AccountLoader<'a, G>
where
    G: FnMut(&Pubkey) -> Option<AccountSharedData>,
{
    // Bank::replenish_program_cache
    #[throws(Error)]
    pub fn replenish_program_cache<'b, I, S>(
        &mut self,
        s: &S,
        messages: I,
    ) -> LoadedProgramsForTxBatch
    where
        I: IntoIterator<Item = &'b SanitizedMessage>,
        S: WorkingSlot,
    {
        let mut missing_programs: Vec<(Pubkey, (LoadedProgramMatchCriteria, u64))> = vec![];

        for msg in messages {
            for &key in msg.account_keys().iter() {
                let acc = self
                    .get_account_with_fixed_root(&key)?
                    .ok_or(TransactionError::AccountNotFound)?;
                if self.program_owners.contains(&acc.owner()) {
                    if let Err(i) = missing_programs.binary_search_by_key(&key, |(key, _)| *key) {
                        missing_programs
                            .insert(i, (key, (LoadedProgramMatchCriteria::NoCriteria, 0)));
                    }
                }
            }
        }
        for builtin_program in self.builtin_programs.iter() {
            if let Err(i) = missing_programs.binary_search_by_key(builtin_program, |(key, _)| *key)
            {
                missing_programs.insert(
                    i,
                    (
                        *builtin_program,
                        (LoadedProgramMatchCriteria::NoCriteria, 0),
                    ),
                );
            }
        }

        let mut loaded_programs_for_txs = LoadedProgramsForTxBatch::new(
            s.current_slot(),
            self.loaded_programs_cache.environments.clone(),
        );

        // Load programs from cache
        self.loaded_programs_cache
            .extract(s, &mut missing_programs, &mut loaded_programs_for_txs);

        // Load programs from account
        let loaded_programs: Vec<(Pubkey, Arc<LoadedProgram>)> = missing_programs
            .iter()
            .map(|(key, (_match_criteria, count))| {
                let program = self.load_program(s.current_slot(), key)?;
                program.tx_usage_counter.store(*count, Ordering::Relaxed);
                Result::<_, Error>::Ok((*key, Arc::new(program)))
            })
            .collect::<Result<_, _>>()?;

        for (key, program) in loaded_programs {
            let (_, entry) = self.loaded_programs_cache.replenish(key, program);
            // Use the returned entry as that might have been deduplicated globally
            loaded_programs_for_txs.replenish(key, entry);
        }

        loaded_programs_for_txs
    }

    #[throws(Error)]
    fn load_program_accounts(&mut self, pubkey: &Pubkey) -> ProgramAccountLoadResult {
        let program_account = match self.get_account_with_fixed_root(pubkey)? {
            None => return ProgramAccountLoadResult::AccountNotFound,
            Some(account) => account,
        };

        if loader_v4::check_id(program_account.owner()) {
            return solana_loader_v4_program::get_state(program_account.data())
                .ok()
                .and_then(|state| {
                    (!matches!(state.status, LoaderV4Status::Retracted)).then_some(state.slot)
                })
                .map(|slot| ProgramAccountLoadResult::ProgramOfLoaderV4(program_account, slot))
                .unwrap_or(ProgramAccountLoadResult::InvalidV4Program);
        }

        if !bpf_loader_upgradeable::check_id(program_account.owner()) {
            return ProgramAccountLoadResult::ProgramOfLoaderV1orV2(program_account);
        }

        if let Ok(UpgradeableLoaderState::Program {
            programdata_address,
        }) = program_account.state()
        {
            let programdata_account =
                match self.get_account_with_fixed_root(&programdata_address)? {
                    None => return ProgramAccountLoadResult::AccountNotFound,
                    Some(account) => account,
                };

            if let Ok(UpgradeableLoaderState::ProgramData {
                slot,
                upgrade_authority_address: _,
            }) = programdata_account.state()
            {
                return ProgramAccountLoadResult::ProgramOfLoaderV3(
                    program_account,
                    programdata_account,
                    slot,
                );
            }
        }
        ProgramAccountLoadResult::InvalidAccountData
    }

    // Roughly Bank::load_program
    #[throws(Error)]
    fn load_program(&mut self, slot: Slot, pubkey: &Pubkey) -> LoadedProgram {
        let environments = self
            .loaded_programs_cache
            .get_environments_for_epoch(slot / DEFAULT_SLOTS_PER_EPOCH)
            .clone();

        let mut load_program_metrics = LoadProgramMetrics {
            program_id: pubkey.to_string(),
            ..LoadProgramMetrics::default()
        };

        let reload = false;

        let loaded_program = match self.load_program_accounts(pubkey)? {
            ProgramAccountLoadResult::AccountNotFound => Ok(LoadedProgram::new_tombstone(
                slot,
                LoadedProgramType::Closed,
            )),

            ProgramAccountLoadResult::InvalidAccountData => {
                Err(InstructionError::InvalidAccountData)
            }

            ProgramAccountLoadResult::ProgramOfLoaderV1orV2(program_account) => {
                solana_bpf_loader_program::load_program_from_bytes(
                    self.feature_set
                        .is_active(&feature_set::delay_visibility_of_program_deployment::id()),
                    None,
                    &mut load_program_metrics,
                    program_account.data(),
                    program_account.owner(),
                    program_account.data().len(),
                    0,
                    environments.program_runtime_v1.clone(),
                    reload,
                )
            }

            ProgramAccountLoadResult::ProgramOfLoaderV3(
                program_account,
                programdata_account,
                slot,
            ) => programdata_account
                .data()
                .get(UpgradeableLoaderState::size_of_programdata_metadata()..)
                .ok_or(InstructionError::InvalidAccountData)
                .and_then(|programdata| {
                    solana_bpf_loader_program::load_program_from_bytes(
                        self.feature_set
                            .is_active(&feature_set::delay_visibility_of_program_deployment::id()),
                        None,
                        &mut load_program_metrics,
                        programdata,
                        program_account.owner(),
                        program_account
                            .data()
                            .len()
                            .saturating_add(programdata_account.data().len()),
                        slot,
                        environments.program_runtime_v1.clone(),
                        reload,
                    )
                }),

            ProgramAccountLoadResult::ProgramOfLoaderV4(program_account, slot) => {
                let loaded_program = program_account
                    .data()
                    .get(LoaderV4State::program_data_offset()..)
                    .and_then(|elf_bytes| {
                        if reload {
                            // Safety: this is safe because the program is being reloaded in the cache.
                            unsafe {
                                LoadedProgram::reload(
                                    &loader_v4::id(),
                                    environments.program_runtime_v2.clone(),
                                    slot,
                                    slot.saturating_add(DELAY_VISIBILITY_SLOT_OFFSET),
                                    None,
                                    elf_bytes,
                                    program_account.data().len(),
                                    &mut load_program_metrics,
                                )
                            }
                        } else {
                            LoadedProgram::new(
                                &loader_v4::id(),
                                environments.program_runtime_v2.clone(),
                                slot,
                                slot.saturating_add(DELAY_VISIBILITY_SLOT_OFFSET),
                                None,
                                elf_bytes,
                                program_account.data().len(),
                                &mut load_program_metrics,
                            )
                        }
                        .ok()
                    })
                    .unwrap_or(LoadedProgram::new_tombstone(
                        slot,
                        LoadedProgramType::FailedVerification(
                            environments.program_runtime_v2.clone(),
                        ),
                    ));
                Ok(loaded_program)
            }

            ProgramAccountLoadResult::InvalidV4Program => Ok(LoadedProgram::new_tombstone(
                slot,
                LoadedProgramType::FailedVerification(environments.program_runtime_v2.clone()),
            )),
        }
        .unwrap_or_else(|_| {
            LoadedProgram::new_tombstone(
                slot,
                LoadedProgramType::FailedVerification(environments.program_runtime_v1.clone()),
            )
        });

        loaded_program
    }
}

enum ProgramAccountLoadResult {
    AccountNotFound,
    InvalidAccountData,
    InvalidV4Program,
    ProgramOfLoaderV1orV2(AccountSharedData),
    ProgramOfLoaderV3(AccountSharedData, AccountSharedData, Slot),
    ProgramOfLoaderV4(AccountSharedData, Slot),
}
