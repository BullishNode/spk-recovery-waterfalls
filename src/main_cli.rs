use crate::util::sync::sync_wallet;
use clap::Parser;
use miniscript::bitcoin;
use std::{fs, path::PathBuf, sync::mpsc, time::SystemTime};

#[derive(Parser, Debug)]
#[command(name = "spk_recovery")]
#[command(about = "SPK Recovery Tool - scan and recover Bitcoin from descriptors via Waterfalls", long_about = None)]
struct Args {
    #[arg(short, long)]
    /// Path to the file containing the descriptor
    descriptor: PathBuf,

    #[arg(short, long)]
    /// Waterfalls server URL (e.g. https://waterfalls.example.com/api)
    url: String,

    #[arg(short, long)]
    /// Address where the coins will be swept to
    address: String,

    #[arg(short, long, default_value = "1")]
    /// Fee rate in sats/vb
    fee: u64,

    #[arg(short, long, default_value = "100000")]
    /// Minimum derivation index to scan up to. Forces the server past gap-limit-stop within this range.
    /// Set high to catch funds past wide unused gaps. The server still applies its 20-address gap limit
    /// past this index for the tail stop.
    to_index: u32,

    #[arg(short, long, default_value = "bitcoin")]
    /// Bitcoin network: bitcoin, testnet, signet, regtest
    network: bitcoin::Network,
}

pub fn run(_default_network: bitcoin::Network) -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let network = args.network;
    let start = SystemTime::now();

    println!("Open descriptor file at {}", args.descriptor.display());
    let path = args.descriptor.canonicalize()?;
    let descriptor_str = fs::read_to_string(path)?;

    let (log_tx, log_rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        while let Ok(msg) = log_rx.recv() {
            println!("{}", msg);
        }
    });

    let result = sync_wallet(
        descriptor_str,
        args.url,
        args.address,
        args.fee.to_string(),
        args.to_index,
        log_tx,
        network,
    )?;

    println!("\n{} inputs: {}", result.num_inputs, result.total_value);
    println!("Fees: {}", result.fees);
    println!("Output: {}", result.output_value);
    println!("\nSweep psbt:\n{}", result.psbt);

    let now = SystemTime::now();
    let time = now.duration_since(start).unwrap();
    println!("\nCompleted in {:?}", time);

    Ok(())
}
