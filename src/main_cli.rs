use crate::util::sync::sync_wallet;
use clap::Parser;
use miniscript::bitcoin;
use std::{fs, path::PathBuf, time::SystemTime};
use tokio::sync::mpsc as tokio_mpsc;

#[derive(Parser, Debug)]
#[command(name = "spk_recovery")]
#[command(about = "SPK Recovery Tool - scan and recover Bitcoin from descriptors", long_about = None)]
struct Args {
    #[arg(short, long)]
    /// Path to the file containing the descriptor
    descriptor: PathBuf,

    #[arg(short, long)]
    /// IP of the electrum server
    ip: String,

    #[arg(short, long)]
    /// Port of the electrum server
    port: u16,

    #[arg(short, long)]
    /// Target derivation index
    target: u32,

    #[arg(short, long)]
    /// Address where the coins will be spent
    address: String,

    #[arg(short, long, default_value = "20000")]
    /// Max subscription accepted by the server for each connection
    max: u32,

    #[arg(short, long, default_value = "10000")]
    /// Batch size - how many spk we ask for each request
    batch: u32,

    #[arg(short, long, default_value = "1")]
    /// Fee rate in sats/vb
    fee: u64,
}

pub fn run(network: bitcoin::Network) -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let start = SystemTime::now();

    println!("Open descriptor file at {}", args.descriptor.display());
    let path = args.descriptor.canonicalize()?;
    let descriptor_str = fs::read_to_string(path)?;

    let (log_tx, mut log_rx) = tokio_mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        while let Some(msg) = log_rx.blocking_recv() {
            println!("{}", msg);
        }
    });

    let result = sync_wallet(
        descriptor_str,
        args.ip,
        args.port.to_string(),
        args.target.to_string(),
        args.address,
        args.max.to_string(),
        args.batch.to_string(),
        args.fee.to_string(),
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
