use std::{cell::RefCell, rc::Rc};

use getset::{Getters, MutGetters};
use solana_program_runtime::{log_collector::LogCollector, sysvar_cache::SysvarCache};
use solana_rbpf::vm::ContextObject;

#[derive(MutGetters, Getters)]
pub struct InvokeContext {
    #[getset(get_mut = "pub", get = "pub")]
    sysvars: SysvarCache,
    instruction_remaining: u64,
    #[getset(get_mut = "pub", get = "pub")]
    log_collector: Option<Rc<RefCell<LogCollector>>>,
}

impl InvokeContext {
    pub fn new() -> Self {
        Self {
            sysvars: SysvarCache::default(),
            instruction_remaining: u64::MAX / 256,
            log_collector: Some(LogCollector::new_ref()),
        }
    }

    pub fn get_check_aligned(&self) -> bool {
        false
    }

    pub fn get_check_size(&self) -> bool {
        false
    }
}

impl ContextObject for InvokeContext {
    fn trace(&mut self, _state: [u64; 12]) {}

    fn consume(&mut self, amount: u64) {
        self.instruction_remaining = self.instruction_remaining.saturating_sub(amount);
    }

    fn get_remaining(&self) -> u64 {
        self.instruction_remaining
    }
}
