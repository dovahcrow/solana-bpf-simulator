use getset::{Getters, MutGetters};
use solana_program_runtime::{log_collector::LogCollector, sysvar_cache::SysvarCache};
use solana_rbpf::vm::ContextObject;
use solana_sdk::pubkey::Pubkey;

#[derive(MutGetters, Getters)]
pub struct InvokeContext {
    #[getset(get_mut = "pub", get = "pub")]
    sysvars: SysvarCache,
    instruction_remaining: u64,
    #[getset(get_mut = "pub", get = "pub")]
    log_collector: Option<LogCollector>,
    #[getset(get_mut = "pub", get = "pub")]
    program_id: Pubkey,
    #[getset(get_mut = "pub", get = "pub")]
    return_data: (Pubkey, Vec<u8>),
}

impl InvokeContext {
    pub fn new() -> Self {
        Self {
            sysvars: SysvarCache::default(),
            instruction_remaining: u64::MAX / 256,
            log_collector: None,
            program_id: Pubkey::default(),
            return_data: (Pubkey::default(), vec![]),
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
