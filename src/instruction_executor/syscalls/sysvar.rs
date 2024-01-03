use std::sync::Arc;

use solana_rbpf::{declare_builtin_function, memory_region::MemoryMapping};
use solana_sdk::{
    instruction::InstructionError,
    sysvar::{Sysvar, SysvarId},
};

use super::super::context::InvokeContext;
use super::{translate_type_mut, ErrorObj, SUCCESS};

fn get_sysvar<T: std::fmt::Debug + Sysvar + SysvarId + Clone>(
    sysvar: Result<Arc<T>, InstructionError>,
    var_addr: u64,
    check_aligned: bool,
    memory_mapping: &mut MemoryMapping,
    _invoke_context: &mut InvokeContext,
) -> Result<u64, ErrorObj> {
    let var = translate_type_mut::<T>(memory_mapping, var_addr, check_aligned)?;

    let sysvar: Arc<T> = sysvar?;
    *var = T::clone(sysvar.as_ref());

    Ok(SUCCESS)
}

declare_builtin_function!(
    /// Get a Clock sysvar
    SyscallGetClockSysvar,
    fn rust(
        invoke_context: &mut InvokeContext,
        var_addr: u64,
        _arg2: u64,
        _arg3: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &mut MemoryMapping,
    ) -> Result<u64, ErrorObj> {
        get_sysvar(
            invoke_context.sysvars().get_clock(),
            var_addr,
            invoke_context.get_check_aligned(),
            memory_mapping,
            invoke_context,
        )
    }
);
