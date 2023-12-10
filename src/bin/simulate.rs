use std::{
    collections::{hash_map, HashMap},
    fs::File,
    io::Read,
    path::PathBuf,
};

use anyhow::Error;
use clap::Parser;
use fehler::throws;
use once_cell::sync::Lazy;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    account::{Account, AccountSharedData, ReadableAccount},
    bpf_loader_upgradeable::UpgradeableLoaderState,
    clock::Clock,
    instruction::{AccountMeta, Instruction},
    message::{LegacyMessage, Message, SanitizedMessage},
    native_loader, pubkey,
    pubkey::Pubkey,
    slot_history::Slot,
    sysvar::clock,
};
use solana_simulator::{SBFExecutor, WorkingSlot, FEATURES};
use tracing::{error, info};
use tracing_subscriber::{
    filter::{EnvFilter, LevelFilter},
    fmt,
    prelude::*,
    Registry,
};
use url::Url;

static BPF_LOADER: Lazy<AccountSharedData> = Lazy::new(|| {
    Account {
        owner: native_loader::ID,
        executable: true,
        rent_epoch: 0,
        data: b"solana_bpf_loader_upgradeable_program".to_vec(),
        lamports: 1,
    }
    .into()
});

#[derive(Debug, Clone, Parser)]
struct Cli {
    #[arg(long, env, default_value = "https://api.mainnet-beta.solana.com")]
    solana_rpc: Url,

    #[arg(long)]
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

#[throws(Error)]
fn main() {
    let cli = Cli::parse();
    let subscriber = Registry::default()
        .with(
            fmt::layer()
                .with_writer(std::io::stdout)
                .with_filter(LevelFilter::INFO),
        )
        .with(
            EnvFilter::builder()
                .try_from_env()
                .unwrap_or_else(|_| EnvFilter::builder().parse("simulate=info").unwrap()),
        );

    tracing::subscriber::set_global_default(subscriber).unwrap();

    let rpc = RpcClient::new(cli.solana_rpc.to_string());

    let mut sbf = SBFExecutor::new(FEATURES).unwrap();

    let clock = rpc.get_account(&clock::id())?;
    let clock: Clock = bincode::deserialize(&clock.data())?;

    let slot = clock.slot;
    sbf.sysvar_cache_mut().set_clock(clock);

    let bpf_upgradable_loader = pubkey!("BPFLoaderUpgradeab1e11111111111111111111111");
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
    File::open(cli.program)?.read_to_end(&mut data)?;

    let program_data: AccountSharedData = Account {
        lamports: 0,
        data,
        owner: bpf_upgradable_loader,
        executable: true,
        rent_epoch: 0,
    }
    .into();

    let mut accounts: HashMap<Pubkey, AccountSharedData> = HashMap::new();
    let mut loader = sbf.loader(|&key| {
        if key == pubkey!("BPFLoaderUpgradeab1e11111111111111111111111") {
            return Some(BPF_LOADER.clone());
        }

        if key == cli.program_id {
            return Some(program.clone());
        }

        if key == programdata_address {
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

    let ix_data = bs58::decode(cli.instruction).into_vec()?;
    let ix = Instruction::new_with_bytes(
        cli.program_id,
        &ix_data,
        cli.account
            .iter()
            .map(|a| {
                let mut signer = false;
                let mut writable = false;

                if cli.signer_account.contains(a) {
                    signer = true;
                }

                if cli.writable_account.contains(a) {
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
    let loaded_transaction = loader.load_transaction_account(&message)?;
    let loaded_programs = loader.load_programs(&WrappedSlot(slot), [&message])?;

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

struct WrappedSlot(Slot);
impl WorkingSlot for WrappedSlot {
    fn current_slot(&self) -> Slot {
        self.0
    }

    fn is_ancestor(&self, _: Slot) -> bool {
        true
    }
}
