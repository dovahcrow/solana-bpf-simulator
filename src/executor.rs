use std::{
    cell::{Ref, RefCell},
    collections::HashSet,
    rc::Rc,
    sync::Arc,
};

use anyhow::{anyhow, Error};
use fehler::throws;
use getset::{Getters, MutGetters};
use solana_bpf_loader_program::syscalls::create_program_runtime_environment;
use solana_program_runtime::{
    compute_budget::ComputeBudget,
    invoke_context::InvokeContext,
    loaded_programs::{LoadedProgram, LoadedPrograms, LoadedProgramsForTxBatch},
    log_collector::LogCollector,
    sysvar_cache::SysvarCache,
    timings::ExecuteTimings,
};
use solana_rbpf::vm::BuiltinProgram;
use solana_runtime::{
    accounts::LoadedTransaction, builtins::BUILTINS, message_processor::MessageProcessor,
};
use solana_sdk::{
    account::AccountSharedData, bpf_loader, bpf_loader_deprecated, bpf_loader_upgradeable,
    feature_set::FeatureSet, message::SanitizedMessage, pubkey::Pubkey, rent::Rent,
    slot_history::Slot, transaction_context::TransactionContext,
};

use crate::AccountLoader;

#[derive(Debug)]
pub struct ExecutionRecord {
    pub keys: Vec<Pubkey>,
    pub datas: Vec<AccountSharedData>,
    pub cu: u64,
}

#[derive(Getters, MutGetters)]
pub struct SBFExecutor {
    pub(crate) feature_set: Arc<FeatureSet>,
    #[getset(get_mut = "pub", get = "pub")]
    sysvar_cache: SysvarCache,
    pub(crate) logger: Option<Rc<RefCell<LogCollector>>>,
    pub(crate) environment: Arc<BuiltinProgram<InvokeContext<'static>>>,
    pub(crate) program_owners: HashSet<Pubkey>, // a set of program loaders that owns all the programs (except for native)
    pub(crate) builtin_programs: HashSet<Pubkey>,
    pub(crate) loaded_programs: LoadedPrograms,
}

unsafe impl Send for SBFExecutor {}

impl SBFExecutor {
    #[throws(Error)]
    pub fn new(enabled_features: &[Pubkey]) -> Self {
        let mut features = FeatureSet::default();
        for feat in enabled_features {
            features.activate(&feat, 0);
        }

        let environment =
            create_program_runtime_environment(&features, &Default::default(), false, false)
                .map_err(|e| anyhow!("{}", e))?;

        let program_owners = HashSet::from_iter(vec![
            bpf_loader_upgradeable::id(),
            bpf_loader::id(),
            bpf_loader_deprecated::id(),
        ]);

        let mut this = Self {
            feature_set: Arc::new(features),
            sysvar_cache: SysvarCache::default(),
            // logger: None,
            logger: Some(LogCollector::new_ref()),
            environment: Arc::new(environment),
            program_owners,
            loaded_programs: LoadedPrograms::default(),
            builtin_programs: HashSet::new(),
        };

        // Bank::apply_builtin_program_feature_transitions
        for builtin in BUILTINS.iter() {
            let should_apply_action_for_feature_transition = builtin
                .feature_id
                .map(|f| this.feature_set.is_active(&f))
                .unwrap_or(true);

            {
                if should_apply_action_for_feature_transition {
                    // debug!("Adding program {} under {:?}", name, program_id);
                    // self.add_builtin_account(name.as_str(), &program_id, false);
                    this.builtin_programs.insert(builtin.program_id);
                    this.loaded_programs.replenish(
                        builtin.program_id,
                        Arc::new(LoadedProgram::new_builtin(
                            0,
                            builtin.name.len(),
                            builtin.entrypoint,
                        )),
                    );
                }
            }
        }

        this
    }

    pub fn loader<'a, G>(&'a mut self, g: G) -> AccountLoader<'a, G>
    where
        G: FnMut(&Pubkey) -> Option<AccountSharedData>,
    {
        AccountLoader::from_executor(g, self)
    }

    pub fn logger(&self) -> Ref<LogCollector> {
        self.logger.as_ref().unwrap().borrow()
    }

    pub fn record_log(&mut self) {
        self.logger = Some(LogCollector::new_ref());
    }

    #[throws(Error)]
    pub fn process(
        &self,
        slot: Slot,
        message: &SanitizedMessage,
        loaded_transaction: LoadedTransaction,
        loaded_programs: &LoadedProgramsForTxBatch,
    ) -> ExecutionRecord {
        let compute_budget = ComputeBudget::default();
        let mut transaction_context = TransactionContext::new(
            loaded_transaction.accounts,
            // Some(Rent::default()),
            None,
            10,
            usize::MAX,
        );

        let mut units = 0;
        let mut timing = ExecuteTimings::default();

        let mut p1 = LoadedProgramsForTxBatch::new(slot);
        let mut p2 = LoadedProgramsForTxBatch::new(slot);
        MessageProcessor::process_message(
            message,
            &loaded_transaction.program_indices,
            &mut transaction_context,
            Rent::default(),
            self.logger.clone(),
            loaded_programs,
            &mut p1,
            &mut p2,
            self.feature_set.clone(),
            compute_budget,
            &mut timing,
            &self.sysvar_cache,
            *message.recent_blockhash(),
            0,
            0,
            &mut units,
        )?;

        let keys = message.account_keys().iter().copied().collect();
        let datas: Vec<_> = transaction_context.deconstruct_without_keys()?;

        ExecutionRecord {
            keys,
            datas,
            cu: units,
        }
    }

    // pub fn prune<F: ForkGraph>(&mut self, fork_graph: &F, new_root: Slot) {
    //     self.loaded_programs.prune(fork_graph, new_root);
    // }
}
