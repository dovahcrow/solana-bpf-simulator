use std::{fs::OpenOptions, io::Write, path::PathBuf};

use anyhow::{anyhow, Error};
use clap::Parser;
use fehler::{throw, throws};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    account::ReadableAccount, bpf_loader_upgradeable::UpgradeableLoaderState, pubkey::Pubkey,
};
use tracing_subscriber::{
    filter::{EnvFilter, LevelFilter},
    fmt,
    prelude::*,
    Registry,
};
use url::Url;

#[derive(Debug, Clone, Parser)]
struct Cli {
    #[arg(long, env, default_value = "https://api.mainnet-beta.solana.com")]
    solana_rpc: Url,

    #[arg(long)]
    program: Pubkey,

    #[arg(long, default_value = "program.so")]
    dest: PathBuf,
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
                .unwrap_or_else(|_| EnvFilter::builder().parse("simulator=info").unwrap()),
        );

    tracing::subscriber::set_global_default(subscriber).unwrap();

    let rpc = RpcClient::new(cli.solana_rpc.to_string());

    let acc = rpc.get_account(&cli.program)?;
    let state: UpgradeableLoaderState = bincode::deserialize(acc.data())?;

    let address = match state {
        UpgradeableLoaderState::Program {
            programdata_address,
        } => programdata_address,
        _ => throw!(anyhow!("Wrong state")),
    };
    let acc = rpc.get_account(&address)?;

    let mut f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(cli.dest)?;
    f.write_all(acc.data())?;
}
