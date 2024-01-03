mod program_loader;
mod transaction_loader;

use anyhow::Error;
use fehler::throws;
use solana_program_runtime::{invoke_context::InvokeContext, loaded_programs::LoadedPrograms};
use solana_rbpf06::vm::BuiltinProgram;
use solana_sdk::{account::AccountSharedData, feature_set::FeatureSet, pubkey::Pubkey};
use std::{collections::HashSet, sync::Arc};

use super::SBPFMessageExecutor;

pub struct AccountLoader<'a, G> {
    g: G,
    feature_set: &'a FeatureSet,
    environment: Arc<BuiltinProgram<InvokeContext<'static>>>,
    loaded_programs: &'a mut LoadedPrograms,
    program_owners: &'a HashSet<Pubkey>,
    builtin_programs: &'a HashSet<Pubkey>,
}

impl<'a, G> AccountLoader<'a, G> {
    pub fn new(
        g: G,
        loaded_programs: &'a mut LoadedPrograms,
        feature_set: &'a FeatureSet,
        environment: Arc<BuiltinProgram<InvokeContext<'static>>>,
        program_owners: &'a HashSet<Pubkey>,
        builtin_programs: &'a HashSet<Pubkey>,
    ) -> Self {
        Self {
            g,
            feature_set,
            environment,
            loaded_programs,
            program_owners,
            builtin_programs,
        }
    }

    pub fn from_executor(g: G, e: &'a mut SBPFMessageExecutor) -> Self {
        Self::new(
            g,
            &mut e.loaded_programs,
            &e.feature_set,
            e.environment.clone(),
            &e.program_owners,
            &e.builtin_programs,
        )
    }
}

impl<'a, G> AccountLoader<'a, G>
where
    G: FnMut(&Pubkey) -> Option<AccountSharedData>,
{
    #[throws(Error)]
    fn get_account(&mut self, key: &Pubkey) -> Option<AccountSharedData> {
        (self.g)(&key)
    }
}
