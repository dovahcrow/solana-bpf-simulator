mod mem_ops;
mod sysvar;

use std::{
    mem::{align_of, size_of},
    slice::from_raw_parts_mut,
    str::from_utf8,
};

use anyhow::Error;
use fehler::throws;
use solana_rbpf::{
    declare_builtin_function,
    memory_region::{AccessType, MemoryMapping},
    program::{BuiltinFunction, FunctionRegistry},
};
use solana_sdk::{
    program::MAX_RETURN_DATA,
    pubkey::{Pubkey, MAX_SEEDS, MAX_SEED_LEN},
};

use self::{mem_ops::SyscallMemcpy, sysvar::SyscallGetClockSysvar};

use super::context::InvokeContext;
use super::syscall_errors::SyscallError;

type ErrorObj = Box<dyn std::error::Error>;

/// Programs indicate success with a return value of 0
pub const SUCCESS: u64 = 0;

fn address_is_aligned<T>(address: u64) -> bool {
    (address as *mut T as usize)
        .checked_rem(align_of::<T>())
        .map(|rem| rem == 0)
        .expect("T to be non-zero aligned")
}

fn translate(
    memory_mapping: &MemoryMapping,
    access_type: AccessType,
    vm_addr: u64,
    len: u64,
) -> Result<u64, Box<dyn std::error::Error>> {
    memory_mapping
        .map(access_type, vm_addr, len)
        .map_err(|err| err.into())
        .into()
}

fn translate_slice<'a, T>(
    memory_mapping: &MemoryMapping,
    vm_addr: u64,
    len: u64,
    check_aligned: bool,
    check_size: bool,
) -> Result<&'a [T], ErrorObj> {
    translate_slice_inner::<T>(
        memory_mapping,
        AccessType::Load,
        vm_addr,
        len,
        check_aligned,
        check_size,
    )
    .map(|value| &*value)
}

fn translate_type_inner<'a, T>(
    memory_mapping: &MemoryMapping,
    access_type: AccessType,
    vm_addr: u64,
    check_aligned: bool,
) -> Result<&'a mut T, ErrorObj> {
    let host_addr = translate(memory_mapping, access_type, vm_addr, size_of::<T>() as u64)?;
    if !check_aligned {
        Ok(unsafe { std::mem::transmute::<u64, &mut T>(host_addr) })
    } else if !address_is_aligned::<T>(host_addr) {
        Err(SyscallError::UnalignedPointer.into())
    } else {
        Ok(unsafe { &mut *(host_addr as *mut T) })
    }
}

fn translate_slice_mut<'a, T>(
    memory_mapping: &MemoryMapping,
    vm_addr: u64,
    len: u64,
    check_aligned: bool,
    check_size: bool,
) -> Result<&'a mut [T], ErrorObj> {
    translate_slice_inner::<T>(
        memory_mapping,
        AccessType::Store,
        vm_addr,
        len,
        check_aligned,
        check_size,
    )
}

fn translate_slice_inner<'a, T>(
    memory_mapping: &MemoryMapping,
    access_type: AccessType,
    vm_addr: u64,
    len: u64,
    check_aligned: bool,
    check_size: bool,
) -> Result<&'a mut [T], ErrorObj> {
    if len == 0 {
        return Ok(&mut []);
    }

    let total_size = len.saturating_mul(size_of::<T>() as u64);
    if check_size && isize::try_from(total_size).is_err() {
        return Err(SyscallError::InvalidLength.into());
    }

    let host_addr = translate(memory_mapping, access_type, vm_addr, total_size)?;

    if check_aligned && !address_is_aligned::<T>(host_addr) {
        return Err(SyscallError::UnalignedPointer.into());
    }
    Ok(unsafe { from_raw_parts_mut(host_addr as *mut T, len as usize) })
}

fn translate_type_mut<'a, T>(
    memory_mapping: &MemoryMapping,
    vm_addr: u64,
    check_aligned: bool,
) -> Result<&'a mut T, ErrorObj> {
    translate_type_inner::<T>(memory_mapping, AccessType::Store, vm_addr, check_aligned)
}

#[allow(dead_code)]
fn translate_type<'a, T>(
    memory_mapping: &MemoryMapping,
    vm_addr: u64,
    check_aligned: bool,
) -> Result<&'a T, ErrorObj> {
    translate_type_inner::<T>(memory_mapping, AccessType::Load, vm_addr, check_aligned)
        .map(|value| &*value)
}

fn translate_string_and_do(
    memory_mapping: &MemoryMapping,
    addr: u64,
    len: u64,
    check_aligned: bool,
    check_size: bool,
    stop_truncating_strings_in_syscalls: bool,
    work: &mut dyn FnMut(&str) -> Result<u64, ErrorObj>,
) -> Result<u64, ErrorObj> {
    let buf = translate_slice::<u8>(memory_mapping, addr, len, check_aligned, check_size)?;
    let msg = if stop_truncating_strings_in_syscalls {
        buf
    } else {
        let i = match buf.iter().position(|byte| *byte == 0) {
            Some(i) => i,
            None => len as usize,
        };
        buf.get(..i).ok_or(SyscallError::InvalidLength)?
    };
    match from_utf8(msg) {
        Ok(message) => work(message),
        Err(err) => Err(SyscallError::InvalidString(err, msg.to_vec()).into()),
    }
}

declare_builtin_function!(
    /// Abort syscall functions, called when the SBF program calls `abort()`
    /// LLVM will insert calls to `abort()` if it detects an untenable situation,
    /// `abort()` is not intended to be called explicitly by the program.
    /// Causes the SBF program to be halted immediately
    SyscallAbort,
    fn rust(
        _invoke_context: &mut InvokeContext,
        _arg1: u64,
        _arg2: u64,
        _arg3: u64,
        _arg4: u64,
        _arg5: u64,
        _memory_mapping: &mut MemoryMapping,
    ) -> Result<u64, ErrorObj> {
        Err(SyscallError::Abort.into())
    }
);

declare_builtin_function!(
    /// Panic syscall function, called when the SBF program calls 'sol_panic_()`
    /// Causes the SBF program to be halted immediately
    SyscallPanic,
    fn rust(
        invoke_context: &mut InvokeContext,
        file: u64,
        len: u64,
        line: u64,
        column: u64,
        _arg5: u64,
        memory_mapping: &mut MemoryMapping,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        let buf = translate_slice::<u8>(
            memory_mapping,
            file,
            len,
            invoke_context.get_check_aligned(),
            invoke_context.get_check_size(),
        )?;
        let i = match buf.iter().position(|byte| *byte == 0) {
            Some(i) => i,
            None => len as usize,
        };
        let msg = buf.get(..i).ok_or(SyscallError::InvalidLength)?;
        match from_utf8(msg) {
            Ok(message) => Err(SyscallError::Panic(message.to_string(), line, column).into()),
            Err(err) => Err(SyscallError::InvalidString(err, msg.to_vec()).into()),
        }
    }
);

declare_builtin_function!(
    /// Log a user's info message
    SyscallLog,
    fn rust(
        invoke_context: &mut InvokeContext,
        addr: u64,
        len: u64,
        _arg3: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &mut MemoryMapping,
    ) -> Result<u64, ErrorObj> {
        translate_string_and_do(
            memory_mapping,
            addr,
            len,
            invoke_context.get_check_aligned(),
            invoke_context.get_check_size(),
            true,
            &mut |string: &str| {
                solana_program_runtime::log_collector::log::debug!(
                    target: "solana_runtime::message_processor::stable_log",
                    "Program log: {}",
                    string
                );

                if let Some(log_collector) = invoke_context.log_collector_mut() {
                    log_collector.log(&format!("Program log: {string}"));
                }

                Ok(0)
            },
        )?;
        Ok(0)
    }
);

fn translate_and_check_program_address_inputs<'a>(
    seeds_addr: u64,
    seeds_len: u64,
    program_id_addr: u64,
    memory_mapping: &mut MemoryMapping,
    check_aligned: bool,
    check_size: bool,
) -> Result<(Vec<&'a [u8]>, &'a Pubkey), ErrorObj> {
    let untranslated_seeds = translate_slice::<&[u8]>(
        memory_mapping,
        seeds_addr,
        seeds_len,
        check_aligned,
        check_size,
    )?;
    if untranslated_seeds.len() > MAX_SEEDS {
        return Err(SyscallError::BadSeeds.into());
    }
    let seeds = untranslated_seeds
        .iter()
        .map(|untranslated_seed| {
            if untranslated_seed.len() > MAX_SEED_LEN {
                return Err(SyscallError::BadSeeds.into());
            }
            translate_slice::<u8>(
                memory_mapping,
                untranslated_seed.as_ptr() as *const _ as u64,
                untranslated_seed.len() as u64,
                check_aligned,
                check_size,
            )
        })
        .collect::<Result<Vec<_>, ErrorObj>>()?;
    let program_id = translate_type::<Pubkey>(memory_mapping, program_id_addr, check_aligned)?;
    Ok((seeds, program_id))
}

pub fn is_nonoverlapping<N>(src: N, src_len: N, dst: N, dst_len: N) -> bool
where
    N: Ord + num_traits::SaturatingSub,
{
    // If the absolute distance between the ptrs is at least as big as the size of the other,
    // they do not overlap.
    if src > dst {
        src.saturating_sub(&dst) >= dst_len
    } else {
        dst.saturating_sub(&src) >= src_len
    }
}

declare_builtin_function!(
    /// Create a program address
    SyscallTryFindProgramAddress,
    fn rust(
        _invoke_context: &mut InvokeContext,
        seeds_addr: u64,
        seeds_len: u64,
        program_id_addr: u64,
        address_addr: u64,
        bump_seed_addr: u64,
        memory_mapping: &mut MemoryMapping,
    ) -> Result<u64, ErrorObj> {
        let (seeds, program_id) = translate_and_check_program_address_inputs(
            seeds_addr,
            seeds_len,
            program_id_addr,
            memory_mapping,
            false,
            false,
        )?;

        let mut bump_seed = [std::u8::MAX];
        for _ in 0..std::u8::MAX {
            {
                let mut seeds_with_bump = seeds.to_vec();
                seeds_with_bump.push(&bump_seed);

                if let Ok(new_address) =
                    Pubkey::create_program_address(&seeds_with_bump, program_id)
                {
                    let bump_seed_ref =
                        translate_type_mut::<u8>(memory_mapping, bump_seed_addr, false)?;
                    let address = translate_slice_mut::<u8>(
                        memory_mapping,
                        address_addr,
                        std::mem::size_of::<Pubkey>() as u64,
                        false,
                        false,
                    )?;
                    if !is_nonoverlapping(
                        bump_seed_ref as *const _ as usize,
                        std::mem::size_of_val(bump_seed_ref),
                        address.as_ptr() as usize,
                        std::mem::size_of::<Pubkey>(),
                    ) {
                        return Err(SyscallError::CopyOverlapping.into());
                    }
                    *bump_seed_ref = bump_seed[0];
                    address.copy_from_slice(new_address.as_ref());
                    return Ok(0);
                }
            }
            bump_seed[0] = bump_seed[0].saturating_sub(1);
        }
        Ok(1)
    }
);

declare_builtin_function!(
    /// Set return data
    SyscallSetReturnData,
    fn rust(
        invoke_context: &mut InvokeContext,
        addr: u64,
        len: u64,
        _arg3: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &mut MemoryMapping,
    ) -> Result<u64, ErrorObj> {
        if len > MAX_RETURN_DATA as u64 {
            return Err(SyscallError::ReturnDataTooLarge(len, MAX_RETURN_DATA as u64).into());
        }

        let return_data = if len == 0 {
            Vec::new()
        } else {
            translate_slice::<u8>(
                memory_mapping,
                addr,
                len,
                invoke_context.get_check_aligned(),
                invoke_context.get_check_size(),
            )?
            .to_vec()
        };

        let program_id = *invoke_context.program_id();
        *invoke_context.return_data_mut() = (program_id, return_data);

        Ok(0)
    }
);

declare_builtin_function!(
    /// Get return data
    SyscallGetReturnData,
    fn rust(
        invoke_context: &mut InvokeContext,
        return_data_addr: u64,
        length: u64,
        program_id_addr: u64,
        _arg4: u64,
        _arg5: u64,
        memory_mapping: &mut MemoryMapping,
    ) -> Result<u64, ErrorObj> {
        let (program_id, return_data) = invoke_context.return_data();
        let length = length.min(return_data.len() as u64);
        if length != 0 {
            let return_data_result = translate_slice_mut::<u8>(
                memory_mapping,
                return_data_addr,
                length,
                invoke_context.get_check_aligned(),
                invoke_context.get_check_size(),
            )?;

            let to_slice = return_data_result;
            let from_slice = return_data
                .get(..length as usize)
                .ok_or(SyscallError::InvokeContextBorrowFailed)?;
            if to_slice.len() != from_slice.len() {
                return Err(SyscallError::InvalidLength.into());
            }
            to_slice.copy_from_slice(from_slice);

            let program_id_result = translate_type_mut::<Pubkey>(
                memory_mapping,
                program_id_addr,
                invoke_context.get_check_aligned(),
            )?;

            if !is_nonoverlapping(
                to_slice.as_ptr() as usize,
                length as usize,
                program_id_result as *const _ as usize,
                std::mem::size_of::<Pubkey>(),
            ) {
                return Err(SyscallError::CopyOverlapping.into());
            }

            *program_id_result = *program_id;
        }

        // Return the actual length, rather the length returned
        Ok(return_data.len() as u64)
    }
);

#[throws(Error)]
pub fn get_syscalls() -> FunctionRegistry<BuiltinFunction<InvokeContext>> {
    let mut result = FunctionRegistry::<BuiltinFunction<InvokeContext>>::default();

    // Abort
    result.register_function_hashed(*b"abort", SyscallAbort::vm)?;

    // Panic
    result.register_function_hashed(*b"sol_panic_", SyscallPanic::vm)?;

    // Logging
    result.register_function_hashed(*b"sol_log_", SyscallLog::vm)?;
    // result.register_function_hashed(*b"sol_log_64_", SyscallLogU64::vm)?;
    // result.register_function_hashed(*b"sol_log_compute_units_", SyscallLogBpfComputeUnits::vm)?;
    // result.register_function_hashed(*b"sol_log_pubkey", SyscallLogPubkey::vm)?;

    // Program defined addresses (PDA)
    // result.register_function_hashed(
    //     *b"sol_create_program_address",
    //     SyscallCreateProgramAddress::vm,
    // )?;
    result.register_function_hashed(
        *b"sol_try_find_program_address",
        SyscallTryFindProgramAddress::vm,
    )?;

    // Sha256
    // result.register_function_hashed(*b"sol_sha256", SyscallHash::vm::<Sha256Hasher>)?;

    // Keccak256
    // result.register_function_hashed(*b"sol_keccak256", SyscallHash::vm::<Keccak256Hasher>)?;

    // Secp256k1 Recover
    // result.register_function_hashed(*b"sol_secp256k1_recover", SyscallSecp256k1Recover::vm)?;

    // Blake3
    // register_feature_gated_function!(
    //     result,
    //     blake3_syscall_enabled,
    //     *b"sol_blake3",
    //     SyscallHash::vm::<Blake3Hasher>,
    // )?;

    // Elliptic Curve Operations
    // register_feature_gated_function!(
    //     result,
    //     curve25519_syscall_enabled,
    //     *b"sol_curve_validate_point",
    //     SyscallCurvePointValidation::vm,
    // )?;
    // register_feature_gated_function!(
    //     result,
    //     curve25519_syscall_enabled,
    //     *b"sol_curve_group_op",
    //     SyscallCurveGroupOps::vm,
    // )?;
    // register_feature_gated_function!(
    //     result,
    //     curve25519_syscall_enabled,
    //     *b"sol_curve_multiscalar_mul",
    //     SyscallCurveMultiscalarMultiplication::vm,
    // )?;

    // Sysvars
    result.register_function_hashed(*b"sol_get_clock_sysvar", SyscallGetClockSysvar::vm)?;
    // result.register_function_hashed(
    //     *b"sol_get_epoch_schedule_sysvar",
    //     SyscallGetEpochScheduleSysvar::vm,
    // )?;
    // register_feature_gated_function!(
    //     result,
    //     !disable_fees_sysvar,
    //     *b"sol_get_fees_sysvar",
    //     SyscallGetFeesSysvar::vm,
    // )?;
    // result.register_function_hashed(*b"sol_get_rent_sysvar", SyscallGetRentSysvar::vm)?;

    // register_feature_gated_function!(
    //     result,
    //     last_restart_slot_syscall_enabled,
    //     *b"sol_get_last_restart_slot",
    //     SyscallGetLastRestartSlotSysvar::vm,
    // )?;

    // register_feature_gated_function!(
    //     result,
    //     epoch_rewards_syscall_enabled,
    //     *b"sol_get_epoch_rewards_sysvar",
    //     SyscallGetEpochRewardsSysvar::vm,
    // )?;

    // Memory ops
    result.register_function_hashed(*b"sol_memcpy_", SyscallMemcpy::vm)?;
    // result.register_function_hashed(*b"sol_memmove_", SyscallMemmove::vm)?;
    // result.register_function_hashed(*b"sol_memcmp_", SyscallMemcmp::vm)?;
    // result.register_function_hashed(*b"sol_memset_", SyscallMemset::vm)?;

    // Processed sibling instructions
    // result.register_function_hashed(
    //     *b"sol_get_processed_sibling_instruction",
    //     SyscallGetProcessedSiblingInstruction::vm,
    // )?;

    // Stack height
    // result.register_function_hashed(*b"sol_get_stack_height", SyscallGetStackHeight::vm)?;

    // Return data
    result.register_function_hashed(*b"sol_set_return_data", SyscallSetReturnData::vm)?;
    result.register_function_hashed(*b"sol_get_return_data", SyscallGetReturnData::vm)?;

    // Cross-program invocation
    // result.register_function_hashed(*b"sol_invoke_signed_c", SyscallInvokeSignedC::vm)?;
    // result.register_function_hashed(*b"sol_invoke_signed_rust", SyscallInvokeSignedRust::vm)?;

    // Memory allocator
    // register_feature_gated_function!(
    //     result,
    //     !disable_deploy_of_alloc_free_syscall,
    //     *b"sol_alloc_free_",
    //     SyscallAllocFree::vm,
    // )?;

    // Alt_bn128
    // register_feature_gated_function!(
    //     result,
    //     enable_alt_bn128_syscall,
    //     *b"sol_alt_bn128_group_op",
    //     SyscallAltBn128::vm,
    // )?;

    // Big_mod_exp
    // register_feature_gated_function!(
    //     result,
    //     enable_big_mod_exp_syscall,
    //     *b"sol_big_mod_exp",
    //     SyscallBigModExp::vm,
    // )?;

    // Poseidon
    // register_feature_gated_function!(
    //     result,
    //     enable_poseidon_syscall,
    //     *b"sol_poseidon",
    //     SyscallPoseidon::vm,
    // )?;

    // Accessing remaining compute units
    // register_feature_gated_function!(
    //     result,
    //     remaining_compute_units_syscall_enabled,
    //     *b"sol_remaining_compute_units",
    //     SyscallRemainingComputeUnits::vm
    // )?;

    // Alt_bn128_compression
    // register_feature_gated_function!(
    //     result,
    //     enable_alt_bn128_compression_syscall,
    //     *b"sol_alt_bn128_compression",
    //     SyscallAltBn128Compression::vm,
    // )?;

    // Log data
    // result.register_function_hashed(*b"sol_log_data", SyscallLogData::vm)?;

    result
}
