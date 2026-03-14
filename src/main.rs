use clap::Parser;
use miniscript::bitcoin::{self};

mod main_cli;
mod main_gui;
mod styles;
mod util;

#[derive(Parser, Debug)]
#[command(name = "spk_recovery")]
#[command(about = "SPK Recovery Tool - scan and recover Bitcoin from descriptors", long_about = None)]
struct CliArgs {
    /// Run in CLI mode (otherwise runs GUI)
    #[arg(long)]
    cli: bool,
    #[arg(short, long)]
    network: Option<bitcoin::Network>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = CliArgs::parse();

    let network = args.network.unwrap_or(bitcoin::Network::Bitcoin);

    if args.cli {
        main_cli::run(network)?;
    } else {
        main_gui::run(network)?;
    }

    Ok(())
}
