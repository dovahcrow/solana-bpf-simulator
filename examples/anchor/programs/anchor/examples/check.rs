use std::{fs::File, io::Read};

use anchor_lang::{InstructionData, ToAccountMetas};
use anyhow::Error;
use clap::Parser;
use fehler::throws;
use solana_bpf_simulator::{SBFExecutor, WrappedSlot, FEATURES};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    account::{Account, AccountSharedData, ReadableAccount},
    bpf_loader_upgradeable::UpgradeableLoaderState,
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
}

#[throws(Error)]
fn main() {
    let cli = Cli::parse();
    let rpc = RpcClient::new(cli.solana_rpc.to_string());

    let mut sbf = SBFExecutor::new(FEATURES).unwrap();

    let clock = rpc.get_account(&clock::id())?;
    let clock: Clock = bincode::deserialize(&clock.data())?;

    let slot = clock.slot;
    sbf.sysvar_cache_mut().set_clock(clock);

    let bpf_upgradable_loader = pubkey!("BPFLoaderUpgradeab1e11111111111111111111111");
    let program_id = pubkey!("DUMMYPRoGRAM1111111111111111111111111111111");
    let programdata_address = pubkey!("DUMMYPRoGRAMDATA111111111111111111111111111");

    let program: AccountSharedData = Account {
        lamports: 0,
        data: bincode::serialize(&UpgradeableLoaderState::Program {
            programdata_address,
        })?,
        owner: bpf_upgradable_loader,
        executable: true,
        rent_epoch: 0,
    }
    .into();

    let mut data = vec![];
    File::open("target/deploy/anchor_example.so")?.read_to_end(&mut data)?;

    let program_data: AccountSharedData = Account {
        lamports: 0,
        data,
        owner: bpf_upgradable_loader,
        executable: true,
        rent_epoch: 0,
    }
    .into();

    let mut loader = sbf.loader(|&key| {
        dbg!(key);
        if key == program_id {
            return Some(program.clone());
        }

        if key == programdata_address {
            return Some(program_data.clone());
        }

        let account = rpc.get_account(&key).unwrap_or_default();
        return Some(account.into());
    });

    let ix_data = anchor_example::instruction::Initialize {}.data();
    let accounts = anchor_example::accounts::Initialize {
        account: system_program::ID,
    }
    .to_account_metas(None);
    let ix = Instruction::new_with_bytes(program_id, &ix_data, accounts);
    let message = Message::new(&[ix], None);
    let message = SanitizedMessage::Legacy(LegacyMessage::new(message));
    let loaded_transaction = loader.load_transaction_account(&message)?;
    let loaded_programs = loader.load_programs(&WrappedSlot(slot), [&message])?;

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
