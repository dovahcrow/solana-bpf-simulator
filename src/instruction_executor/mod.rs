mod account_sizes;
mod context;
mod errors;
mod syscall_errors;
mod syscalls;

use std::{mem::size_of, ptr, sync::Arc};

use anyhow::{anyhow, Error};
use fehler::{throw, throws};
use getset::{Getters, MutGetters};
use solana_rbpf::{
    aligned_memory::{AlignedMemory, Pod},
    ebpf::{HOST_ALIGN, MM_HEAP_START, MM_INPUT_START, MM_STACK_START},
    elf::Executable,
    error::StableResult,
    memory_region::{MemoryMapping, MemoryRegion},
    program::BuiltinProgram,
    vm::{Config, EbpfVm},
};
use solana_sdk::{
    account::{Account, ReadableAccount},
    entrypoint::{
        BPF_ALIGN_OF_U128, HEAP_LENGTH, MAX_PERMITTED_DATA_INCREASE, NON_DUP_MARKER, SUCCESS,
    },
    instruction::InstructionError,
    pubkey::Pubkey,
};

pub use account_sizes::AccountSizes;
pub use errors::InstructionExecutorError;

use context::InvokeContext;
use syscalls::get_syscalls;

/// A faster executor but with limitations.
/// 0. This executor only process a single instruction.
/// 1. You cannot have multiple mutable accounts with same pubkey.
/// 2. Account and instruction spaces are pre-allocated as well as # of accounts.
#[derive(MutGetters, Getters)]
pub struct InstructionExecutor<A> {
    #[getset(get_mut = "pub", get = "pub")]
    context: InvokeContext,
    runtime: Arc<BuiltinProgram<InvokeContext>>,

    instruction_size: usize,
    account_sizes: A,

    instruction_offset: usize,
    account_offsets: Vec<usize>, // offsets
    program_id_offset: usize,

    buffer: AlignedMemory<HOST_ALIGN>,

    stack: AlignedMemory<HOST_ALIGN>,
    heap: AlignedMemory<HOST_ALIGN>,

    executable: Option<Executable<InvokeContext>>,
}

impl<A> InstructionExecutor<A>
where
    A: AccountSizes,
{
    #[throws(Error)]
    pub fn new(instruction_size: usize, account_sizes: A) -> Self {
        // if instruction_size > 41 {
        //     throw!(anyhow!("Instruction data too long"))
        // }
        let mut size = size_of::<u64>();
        for i in 0..account_sizes.len() {
            size += size_of::<u8>(); // dup
            size += size_of::<u8>() // is_signer
                + size_of::<u8>() // is_writable
                + size_of::<u8>() // executable
                + size_of::<u32>() // original_data_len
                + size_of::<Pubkey>()  // key
                + size_of::<Pubkey>() // owner
                + size_of::<u64>(); // lamports
            size += size_of::<u64>()
                + account_sizes.size(i)
                + (account_sizes.size(i) as *const u8).align_offset(BPF_ALIGN_OF_U128)
                + MAX_PERMITTED_DATA_INCREASE;
            size += size_of::<u64>(); // rent epoch
        }

        size += size_of::<u64>() + instruction_size;
        size += size_of::<Pubkey>(); // program id;

        let mut buffer = AlignedMemory::with_capacity(size); // Serialize into the buffer

        let mut account_offsets = Vec::new();
        Self::write::<u64>(&mut buffer, None, (account_sizes.len() as u64).to_le());
        for i in 0..account_sizes.len() {
            account_offsets.push(buffer.len());
            Self::write_empty_account(&mut buffer, account_sizes.size(i))?;
        }

        let instruction_offset = buffer.len();
        Self::write::<u64>(&mut buffer, None, (instruction_size as u64).to_le());
        Self::fill_write(&mut buffer, None, instruction_size, 0)?;

        let program_id_offset = buffer.len();
        Self::write_all(&mut buffer, None, Pubkey::default().as_ref());

        let registry = get_syscalls()?;
        let config = Config {
            max_call_depth: 64,
            external_internal_function_hash_collision: true,
            ..Default::default()
        };
        let runtime = Arc::new(BuiltinProgram::<InvokeContext>::new_loader(
            config, registry,
        ));

        let stack = AlignedMemory::<{ HOST_ALIGN }>::zero_filled(config.stack_size());
        let heap =
            AlignedMemory::<{ HOST_ALIGN }>::zero_filled(usize::try_from(HEAP_LENGTH).unwrap());

        InstructionExecutor {
            instruction_size,
            account_sizes,

            instruction_offset,
            account_offsets,
            program_id_offset,

            buffer,
            stack,
            heap,
            runtime,

            executable: None,
            context: InvokeContext::new(),
        }
    }

    #[throws(Error)]
    pub fn update_program<T>(&mut self, program_id: &Pubkey, account: &T, jit: bool)
    where
        T: ReadableAccount,
    {
        let mut executable = Executable::from_elf(&account.data(), self.runtime.clone())
            .map_err(|e| anyhow!("{}", e))?;
        if jit {
            #[cfg(all(not(target_os = "windows"), target_arch = "x86_64"))]
            executable.jit_compile().map_err(|e| anyhow!("{}", e))?;
        }
        self.executable = Some(executable);
        *self.context.program_id_mut() = *program_id;

        Self::write_all(
            &mut self.buffer,
            Some(self.program_id_offset),
            program_id.as_ref(),
        )
    }

    #[throws(Error)]
    pub fn update_instruction(&mut self, instruction: &[u8]) {
        if instruction.len() > self.instruction_size {
            throw!(InstructionExecutorError::InvalidInstruction);
        }

        Self::write::<u64>(
            &mut self.buffer,
            Some(self.instruction_offset),
            (self.instruction_size as u64).to_le(),
        );
        Self::write_all(
            &mut self.buffer,
            Some(self.instruction_offset + size_of::<u64>()),
            instruction,
        )
    }

    #[throws(Error)]
    pub fn update_account<T>(
        &mut self,
        i: usize,
        key: &Pubkey,
        account: &T,
        is_signer: bool,
        is_writable: bool,
        is_executable: bool,
    ) where
        T: ReadableAccount,
    {
        let account_size = self.account_sizes.size(i);
        if account.data().len() > account_size {
            throw!(InstructionExecutorError::InvalidAccount);
        }

        let offset = self.account_offsets[i];

        Self::write_account(
            &mut self.buffer,
            account_size,
            Some(offset),
            key,
            account,
            is_signer,
            is_writable,
            is_executable,
        )?;
    }

    #[throws(Error)]
    pub fn execute(&mut self) {
        let len = self.stack.len();
        self.stack.as_slice_mut()[..len].fill(0);

        let len = self.heap.len();
        self.heap.as_slice_mut()[..len].fill(0);

        let executable = self
            .executable
            .as_ref()
            .ok_or(InstructionExecutorError::MissingProgram)?;

        let config = executable.get_config();
        let sbpf_version = executable.get_sbpf_version();
        let len = self.buffer.len();
        let regions: Vec<MemoryRegion> = vec![
            executable.get_ro_region(),
            MemoryRegion::new_writable_gapped(self.stack.as_slice_mut(), MM_STACK_START, 0),
            MemoryRegion::new_writable(self.heap.as_slice_mut(), MM_HEAP_START),
            MemoryRegion::new_writable(
                self.buffer.as_slice_mut().get_mut(0..len).unwrap(),
                MM_INPUT_START,
            ),
        ];

        *self.context.return_data_mut() = (Pubkey::default(), vec![]);

        let mm = MemoryMapping::new(regions, config, sbpf_version).unwrap();
        let mut vm = EbpfVm::new(
            self.runtime.clone(),
            executable.get_sbpf_version(),
            &mut self.context,
            mm,
            config.stack_size(),
        );

        #[cfg(all(not(target_os = "windows"), target_arch = "x86_64"))]
        let (_, result) =
            vm.execute_program(&executable, executable.get_compiled_program().is_none());
        #[cfg(any(target_os = "windows", not(target_arch = "x86_64")))]
        let (_, result) = vm.execute_program(&executable, true);

        match result {
            StableResult::Ok(code) if code == SUCCESS => {}
            StableResult::Ok(code) => throw!(InstructionError::from(code)),
            StableResult::Err(e) => throw!(anyhow!(e.to_string())),
        }
    }

    pub fn get_return_data(&self) -> Option<&(Pubkey, Vec<u8>)> {
        let return_data = self.context.return_data();
        if return_data.1.is_empty() {
            return None;
        }

        Some(return_data)
    }

    pub fn get_account(&self, i: usize) -> Account {
        let mut offset = self.account_offsets[i] + 3;
        let executable = self.buffer.as_slice()[offset] == 1;
        offset += 37;

        let owner = Pubkey::try_from(&self.buffer.as_slice()[offset..offset + 32]).unwrap();
        offset += 32;

        let lamports = u64::from_le_bytes(
            self.buffer.as_slice()[offset..offset + 8]
                .try_into()
                .unwrap(),
        );
        offset += 8;

        let data_len = u64::from_le_bytes(
            self.buffer.as_slice()[offset..offset + 8]
                .try_into()
                .unwrap(),
        ) as usize;
        offset += 8;

        let data = self.buffer.as_slice()[offset..offset + data_len].to_vec();
        offset += data_len;

        let align_offset = (data_len as *const u8).align_offset(BPF_ALIGN_OF_U128);
        offset += align_offset + MAX_PERMITTED_DATA_INCREASE;

        let rent_epoch = u64::from_le_bytes(
            self.buffer.as_slice()[offset..offset + 8]
                .try_into()
                .unwrap(),
        );

        Account {
            owner,
            lamports,
            data,
            executable,
            rent_epoch,
        }
    }

    fn fill_write(
        buffer: &mut AlignedMemory<HOST_ALIGN>,
        at: Option<usize>,
        num: usize,
        value: u8,
    ) -> std::io::Result<usize> {
        match at {
            Some(at) => {
                let size = num;
                buffer.as_slice_mut()[at..at + size].fill(value);
                Ok(at + size)
            }
            None => {
                buffer.fill_write(num, value)?;
                Ok(buffer.len())
            }
        }
    }

    fn write<T: Pod>(buffer: &mut AlignedMemory<HOST_ALIGN>, at: Option<usize>, value: T) -> usize {
        // self.debug_assert_alignment::<T>();
        match at {
            Some(at) => unsafe {
                let size = size_of::<T>();
                ptr::write_unaligned(
                    buffer.as_slice_mut()[at..at + size].as_mut_ptr().cast(),
                    value,
                );
                at + size
            },
            None => unsafe {
                buffer.write_unchecked(value);
                buffer.len()
            },
        }
    }

    fn write_all(buffer: &mut AlignedMemory<HOST_ALIGN>, at: Option<usize>, value: &[u8]) -> usize {
        match at {
            Some(at) => {
                buffer.as_slice_mut()[at..at + value.len()].copy_from_slice(value);
                at + value.len()
            }
            None => {
                unsafe { buffer.write_all_unchecked(value) }
                buffer.len()
            }
        }
    }

    #[throws(Error)]
    fn write_account<T>(
        buffer: &mut AlignedMemory<HOST_ALIGN>,
        account_size: usize,
        mut at: Option<usize>,
        key: &Pubkey,
        account: &T,
        is_signer: bool,
        is_writable: bool,
        is_executable: bool,
    ) -> usize
    where
        T: ReadableAccount,
    {
        let next = Self::write::<u8>(buffer, at, NON_DUP_MARKER);
        at = at.map(|_| next);
        let next = Self::write::<u8>(buffer, at, is_signer as u8);
        at = at.map(|_| next);
        let next = Self::write::<u8>(buffer, at, is_writable as u8);
        at = at.map(|_| next);
        let next = Self::write::<u8>(buffer, at, is_executable as u8);
        at = at.map(|_| next);
        let next = Self::write_all(buffer, at, &[0u8, 0, 0, 0]);
        at = at.map(|_| next);
        let next = Self::write_all(buffer, at, key.as_ref());
        at = at.map(|_| next);
        let next = Self::write_all(buffer, at, account.owner().as_ref());
        at = at.map(|_| next);
        let next = Self::write::<u64>(buffer, at, account.lamports().to_le());
        at = at.map(|_| next);
        let next = Self::write::<u64>(buffer, at, (account_size as u64).to_le());
        at = at.map(|_| next);
        let next = Self::write_all(buffer, at, account.data());
        at = at.map(|_| next);
        let align_offset = (account_size as *const u8).align_offset(BPF_ALIGN_OF_U128);
        let next = Self::fill_write(
            buffer,
            at,
            account_size - account.data().len() + MAX_PERMITTED_DATA_INCREASE + align_offset,
            0,
        )
        .map_err(|_| InstructionError::InvalidArgument)?;
        at = at.map(|_| next);
        let next = Self::write::<u64>(buffer, at, (account.rent_epoch()).to_le());
        next
    }

    #[throws(Error)]
    fn write_empty_account(buffer: &mut AlignedMemory<HOST_ALIGN>, data_size: usize) {
        Self::write::<u8>(buffer, None, NON_DUP_MARKER);
        Self::write::<u8>(buffer, None, 0);
        Self::write::<u8>(buffer, None, 0);
        Self::write::<u8>(buffer, None, 0);
        Self::write_all(buffer, None, &[0u8, 0, 0, 0]);
        Self::write_all(buffer, None, Pubkey::default().as_ref());
        Self::write_all(buffer, None, Pubkey::default().as_ref());
        Self::write::<u64>(buffer, None, 0u64.to_le());
        Self::write::<u64>(buffer, None, (data_size as u64).to_le());
        let align_offset = (data_size as *const u8).align_offset(BPF_ALIGN_OF_U128);
        Self::fill_write(
            buffer,
            None,
            data_size + MAX_PERMITTED_DATA_INCREASE + align_offset,
            0,
        )
        .map_err(|_| InstructionError::InvalidArgument)?;
        Self::write::<u64>(buffer, None, 0u64.to_le());
    }
}
