use std::{fs::File, io::Read};

use anyhow::Error;
use clap::Parser;
use fehler::throws;
use solana_bpf_simulator::{SBPFInstructionExecutor, SBPFMessageExecutor, WorkingSlot, FEATURES};
use solana_client::rpc_client::RpcClient;
use solana_program::instruction::AccountMeta;
use solana_sdk::{
    account::{Account, AccountSharedData, ReadableAccount},
    clock::Clock,
    instruction::Instruction,
    message::{LegacyMessage, Message, SanitizedMessage},
    pubkey, system_program,
    sysvar::clock,
};

#[derive(Parser)]
struct Cli {
    #[arg(long, env, default_value = "https://api.mainnet-beta.solana.com")]
    solana_rpc: String,

    #[arg(long)]
    ro: bool,
}

#[throws(Error)]
fn main() {
    let cli = Cli::parse();

    if cli.ro {
        run_ro(cli)?;
    } else {
        run_full(cli)?;
    }
}

#[throws(Error)]
fn run_ro(cli: Cli) {
    let rpc = RpcClient::new(cli.solana_rpc.to_string());

    let program_id = pubkey!("DUMMYPRoGRAM1111111111111111111111111111111");

    let mut data = vec![];
    File::open("target/deploy/example.so")?.read_to_end(&mut data)?;

    let program_data: AccountSharedData = Account {
        data,
        ..Default::default()
    }
    .into();

    let accounts = vec![(
        AccountMeta::new_readonly(system_program::ID, false),
        rpc.get_account(&system_program::ID).unwrap_or_default(),
    )];

    let mut exe = SBPFInstructionExecutor::new(
        41,
        Vec::from_iter(accounts.iter().map(|(_, a)| a.data().len())),
    )?;

    exe.update_program(&program_id, &program_data, true)?;

    let ix_data = vec![1, 2, 3, 4];
    exe.update_instruction(&ix_data)?;

    for (i, (meta, account)) in accounts.into_iter().enumerate() {
        exe.update_account(
            i,
            &meta.pubkey,
            &account.into(),
            meta.is_signer,
            meta.is_writable,
            false,
        )?;
    }

    if let Err(e) = exe.execute() {
        println!(
            "Invoke errored: {}:\nLogs: {:?}",
            e,
            exe.context()
                .log_collector()
                .as_ref()
                .unwrap()
                .borrow()
                .get_recorded_content()
        );
    } else {
        println!(
            "{:?}",
            exe.context()
                .log_collector()
                .as_ref()
                .unwrap()
                .borrow()
                .get_recorded_content()
        );
    }
}

#[throws(Error)]
fn run_full(cli: Cli) {
    let rpc = RpcClient::new(cli.solana_rpc.to_string());
    let mut sbf = SBPFMessageExecutor::new(FEATURES).unwrap();

    let clock = rpc.get_account(&clock::id())?;
    let clock: Clock = bincode::deserialize(&clock.data())?;

    let slot = clock.slot;
    sbf.sysvar_cache_mut().set_clock(clock);

    let program_id = pubkey!("DUMMYPRoGRAM1111111111111111111111111111111");

    let mut data = vec![];
    File::open("target/deploy/example.so")?.read_to_end(&mut data)?;

    let program_data: AccountSharedData = Account {
        lamports: 0,
        data,
        owner: solana_sdk::bpf_loader::id(),
        executable: true,
        rent_epoch: 0,
    }
    .into();

    let mut loader = sbf.loader(|&key| {
        if key == program_id {
            return Some(program_data.clone());
        }

        let account = rpc.get_account(&key).unwrap_or_default();
        return Some(account.into());
    });

    let ix_data = vec![1, 2, 3, 4];
    let accounts = vec![AccountMeta::new_readonly(system_program::ID, false)];
    let ix = Instruction::new_with_bytes(program_id, &ix_data, accounts);
    let message = Message::new(&[ix], None);
    let message = SanitizedMessage::Legacy(LegacyMessage::new(message));
    let loaded_transaction = loader.load_transaction_account(&message)?;
    let loaded_programs = loader.load_programs(&WorkingSlot(slot), [&message])?;

    sbf.record_log();
    let res = sbf.process(slot, &message, loaded_transaction, &loaded_programs);

    if let Err(e) = res {
        println!(
            "Invoke errored: {}:\nLogs: {:?}",
            e,
            sbf.logger().get_recorded_content()
        );
    } else {
        println!("{:?}", sbf.logger().get_recorded_content());
    }
}
