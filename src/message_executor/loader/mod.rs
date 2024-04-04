mod program_loader;
mod transaction_loader;

use anyhow::Error;
use fehler::throws;
use solana_program_runtime::loaded_programs::LoadedPrograms;
use solana_sdk::{account::AccountSharedData, feature_set::FeatureSet, pubkey::Pubkey};
use std::collections::HashSet;

use super::{ForkGraph, MessageExecutor};

pub struct AccountLoader<'a, G> {
    g: G,
    feature_set: &'a FeatureSet,
    loaded_programs_cache: &'a mut LoadedPrograms<ForkGraph>,
    program_owners: &'a HashSet<Pubkey>,
    builtin_programs: &'a HashSet<Pubkey>,
}

impl<'a, G> AccountLoader<'a, G> {
    pub fn new(
        g: G,
        loaded_programs: &'a mut LoadedPrograms<ForkGraph>,
        feature_set: &'a FeatureSet,
        program_owners: &'a HashSet<Pubkey>,
        builtin_programs: &'a HashSet<Pubkey>,
    ) -> Self {
        Self {
            g,
            feature_set,
            loaded_programs_cache: loaded_programs,
            program_owners,
            builtin_programs,
        }
    }

    pub fn from_executor(g: G, e: &'a mut MessageExecutor) -> Self {
        Self::new(
            g,
            &mut e.loaded_programs,
            &e.feature_set,
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
    fn get_account_with_fixed_root(&mut self, key: &Pubkey) -> Option<AccountSharedData> {
        (self.g)(&key)
    }
}
