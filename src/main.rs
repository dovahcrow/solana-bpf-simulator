use std::{
    collections::{hash_map, HashMap},
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
};

use anyhow::{anyhow, Error};
use clap::{Parser, Subcommand};
use fehler::{throw, throws};
use solana_bpf_simulator::{SBPFExecutor, WorkingSlot, FEATURES};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    account::{Account, AccountSharedData, ReadableAccount},
    account_utils::StateMut,
    bpf_loader,
    bpf_loader_upgradeable::{self, UpgradeableLoaderState},
    clock::Clock,
    instruction::{AccountMeta, Instruction},
    message::{LegacyMessage, Message, SanitizedMessage},
    pubkey::Pubkey,
    sysvar::clock,
};
use tracing::{error, info};
use tracing_subscriber::{
    filter::{EnvFilter, LevelFilter},
    fmt,
    prelude::*,
    Registry,
};
use url::Url;

#[derive(Parser)]
struct Cli {
    #[arg(long, env, default_value = "https://api.mainnet-beta.solana.com")]
    solana_rpc: Url,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Simulate(Simulate),
    GetProgramData(GetProgramData),
}

#[throws(Error)]
fn main() {
    let cli = Cli::parse();
    let subscriber = Registry::default()
        .with(
            fmt::layer()
                .with_writer(std::io::stdout)
                .with_filter(LevelFilter::INFO),
        )
        .with(EnvFilter::builder().try_from_env().unwrap_or_else(|_| {
            EnvFilter::builder()
                .parse("solana_bpf_simulator=info")
                .unwrap()
        }));

    tracing::subscriber::set_global_default(subscriber).unwrap();

    let rpc = RpcClient::new(cli.solana_rpc.to_string());

    match cli.command {
        Command::Simulate(c) => c.run(&rpc)?,
        Command::GetProgramData(c) => c.run(&rpc)?,
    }
}

#[derive(Debug, Clone, Parser)]
struct Simulate {
    #[arg(long, default_value = "FAKEPRoGRAM1D111111111111111111111111111111")]
    program_id: Pubkey,

    #[arg(long, default_value = "program.so")]
    program: PathBuf,

    #[arg(long)]
    instruction: String, // base58 string

    #[arg(long)]
    account: Vec<Pubkey>,

    #[arg(long)]
    signer_account: Vec<Pubkey>,

    #[arg(long)]
    writable_account: Vec<Pubkey>,
}

impl Simulate {
    #[throws(Error)]
    fn run(&self, rpc: &RpcClient) {
        let mut sbf = SBPFExecutor::new(FEATURES).unwrap();

        let clock = rpc.get_account(&clock::id())?;
        let clock: Clock = bincode::deserialize(&clock.data())?;

        let slot = clock.slot;
        sbf.sysvar_cache_mut().set_clock(clock);

        let mut data = vec![];
        File::open(&self.program)?.read_to_end(&mut data)?;

        let program_data: AccountSharedData = Account {
            lamports: 1,
            data,
            owner: bpf_loader::id(),
            executable: true,
            rent_epoch: 0,
        }
        .into();

        let mut accounts: HashMap<Pubkey, AccountSharedData> = HashMap::new();
        let mut loader = sbf.loader(|&key| {
            if key == self.program_id {
                return Some(program_data.clone());
            }

            match accounts.entry(key) {
                hash_map::Entry::Occupied(e) => return Some(e.get().clone()),
                hash_map::Entry::Vacant(e) => {
                    let account = rpc.get_account(&key).unwrap_or_default();
                    let account: AccountSharedData = account.into();
                    e.insert(account.clone());
                    return Some(account);
                }
            }
        });

        let ix_data = bs58::decode(&self.instruction).into_vec()?;
        let ix = Instruction::new_with_bytes(
            self.program_id,
            &ix_data,
            self.account
                .iter()
                .map(|a| {
                    let mut signer = false;
                    let mut writable = false;

                    if self.signer_account.contains(a) {
                        signer = true;
                    }

                    if self.writable_account.contains(a) {
                        writable = true;
                    }

                    AccountMeta {
                        pubkey: *a,
                        is_signer: signer,
                        is_writable: writable,
                    }
                })
                .collect(),
        );
        let message = Message::new(&[ix], None);
        let message = SanitizedMessage::Legacy(LegacyMessage::new(message));
        let loaded_transaction = loader.load_transaction_accounts(&message)?;
        let loaded_programs = loader.replenish_program_cache(&WorkingSlot(slot), [&message])?;

        sbf.record_log();
        let res = sbf.process(slot, &message, loaded_transaction, &loaded_programs);

        if let Err(e) = res {
            error!(
                "Invoke errored: {}:\nLogs: {:?}",
                e,
                sbf.logger().get_recorded_content()
            );
        } else {
            info!("{:?}", sbf.logger().get_recorded_content());
        }
    }
}

#[derive(Debug, Clone, Parser)]
struct GetProgramData {
    #[arg(long)]
    program_id: Pubkey,

    #[arg(long, default_value = "program.so")]
    destination: PathBuf,
}

impl GetProgramData {
    #[throws(Error)]
    fn run(&self, rpc: &RpcClient) {
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.destination)?;

        let acc = rpc.get_account(&self.program_id)?;
        if bpf_loader_upgradeable::check_id(acc.owner()) {
            let state: UpgradeableLoaderState = acc.state()?;

            let address = match state {
                UpgradeableLoaderState::Program {
                    programdata_address,
                } => programdata_address,
                _ => throw!(anyhow!("Wrong state")),
            };
            let acc = rpc.get_account(&address)?;
            f.write_all(&acc.data()[UpgradeableLoaderState::size_of_programdata_metadata()..])?;
        } else if bpf_loader::check_id(acc.owner()) {
            f.write_all(&acc.data())?;
        } else {
            throw!(anyhow!("Unknown owner for the program"))
        };
    }
}
