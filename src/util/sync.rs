use std::{collections::BTreeMap, str::FromStr, sync::mpsc, time::SystemTime};

use bwk_tx::{
    transaction::max_input_satisfaction_size, ChangeRecipientProvider, Coin, CoinStatus, KeyChain,
};

use miniscript::{
    bitcoin::{self, Address, Network, OutPoint, Transaction, Txid},
    Descriptor, DescriptorPublicKey,
};

use waterfalls_client::{api::V, Builder};

use super::SyncResult;

type TxMap = BTreeMap<Txid, Transaction>;
type CoinMap = BTreeMap<OutPoint, Coin>;

pub fn sync_wallet(
    descriptor_str: String,
    url: String,
    address: String,
    fee: String,
    to_index: u32,
    log_tx: mpsc::Sender<String>,
    network: Network,
) -> Result<SyncResult, String> {
    let feerate: f32 = fee.parse().map_err(|e| format!("Invalid fee: {}", e))?;
    let feerate = (feerate * 1000.0) as u64;

    let address = Address::from_str(&address).map_err(|e| format!("Invalid address: {}", e))?;
    if !address.is_valid_for_network(network) {
        return Err("Address is for another network".to_string());
    }

    let descriptor = Descriptor::<DescriptorPublicKey>::from_str(descriptor_str.trim())
        .map_err(|e| format!("Invalid descriptor: {}", e))?;
    let satisfaction_size = max_input_satisfaction_size(&descriptor) as u64;

    let start = SystemTime::now();
    let _ = log_tx.send(format!("Connecting to Waterfalls at {}", url));

    let client = Builder::new(&url).build_blocking();

    let _ = log_tx.send(format!(
        "Querying descriptor history (one shot, to_index={})...",
        to_index
    ));

    let resp = client
        .waterfalls_version(
            descriptor_str.trim(),
            4,
            None,
            Some(to_index),
            false,
        )
        .map_err(|e| format!("Waterfalls query failed: {:?}", e))?;

    let elapsed = SystemTime::now().duration_since(start).unwrap();
    let total_seen: usize = resp
        .txs_seen
        .values()
        .flat_map(|v| v.iter())
        .map(|s| s.len())
        .sum();
    let _ = log_tx.send(format!(
        "{:?} -- Waterfalls returned {} TxSeen entries across {} descriptor branches --",
        elapsed,
        total_seen,
        resp.txs_seen.len()
    ));

    // Walk the response: outer key = descriptor branch index ("0" recv, "1" change),
    // inner Vec is per-derivation-index, each containing TxSeen entries.
    let mut funded: Vec<(Txid, u32, KeyChain, u32)> = vec![];
    let mut needed_txids: BTreeMap<Txid, ()> = BTreeMap::new();

    for (branch_key, scripts) in &resp.txs_seen {
        let branch_idx: u32 = branch_key
            .parse()
            .map_err(|e| format!("Invalid branch index '{}': {}", branch_key, e))?;
        let kc = match branch_idx {
            0 => KeyChain::Receive,
            1 => KeyChain::Change,
            n => return Err(format!("Unexpected descriptor branch index {}", n)),
        };

        for (deriv_index, seen_list) in scripts.iter().enumerate() {
            for ts in seen_list {
                needed_txids.insert(ts.txid, ());
                if let V::Vout(vout) = ts.v {
                    funded.push((ts.txid, vout, kc, deriv_index as u32));
                }
            }
        }
    }

    let _ = log_tx.send(format!(
        "Need to fetch {} unique transactions",
        needed_txids.len()
    ));

    let mut tx_map: TxMap = BTreeMap::new();
    for (i, txid) in needed_txids.keys().enumerate() {
        let tx = client
            .get_tx(txid)
            .map_err(|e| format!("Failed to fetch tx {}: {:?}", txid, e))?
            .ok_or_else(|| format!("Tx {} not found on server", txid))?;
        tx_map.insert(*txid, tx);
        if (i + 1) % 50 == 0 {
            let _ = log_tx.send(format!("  fetched {}/{} txs", i + 1, needed_txids.len()));
        }
    }

    let _ = log_tx.send(format!("Fetched {} transactions", tx_map.len()));

    let mut coins_map: CoinMap = BTreeMap::new();
    for (txid, vout, kc, deriv_index) in funded {
        let tx = tx_map
            .get(&txid)
            .ok_or_else(|| format!("Missing tx {} for funded output", txid))?;
        let txout = tx
            .output
            .get(vout as usize)
            .ok_or_else(|| format!("vout {} out of range for tx {}", vout, txid))?
            .clone();
        let outpoint = OutPoint { txid, vout };
        let coin = Coin {
            txout,
            outpoint,
            height: None,
            sequence: Default::default(),
            status: CoinStatus::Confirmed,
            label: None,
            satisfaction_size,
            spend_info: bwk_tx::CoinSpendInfo::Bip32 {
                coin_path: (kc, deriv_index),
                descriptor: descriptor.clone(),
            },
        };
        coins_map.insert(outpoint, coin);
    }

    for tx in tx_map.values() {
        for txin in tx.input.iter() {
            if let Some(coin) = coins_map.get_mut(&txin.previous_output) {
                coin.status = CoinStatus::Spent;
            }
        }
    }

    let unspent_coins: Vec<_> = coins_map
        .into_iter()
        .filter_map(|(_, c)| (c.status != CoinStatus::Spent).then_some(c))
        .collect();

    let _ = log_tx.send(format!("Found {} unspent coins", unspent_coins.len()));

    if unspent_coins.is_empty() {
        return Err("No unspent coins found".to_string());
    }

    let cp = ChangeRecipientProvider::new(descriptor, network);
    let psbt = bwk_tx::TxBuilder::new(Box::new(cp))
        .inputs(unspent_coins)
        .sweep(address.clone().assume_checked(), feerate)
        .unwrap();

    let sum_inputs = psbt.inputs.iter().fold(bitcoin::Amount::ZERO, |a, b| {
        a + b.witness_utxo.as_ref().unwrap().value
    });

    let sum_outputs = psbt
        .unsigned_tx
        .output
        .iter()
        .fold(bitcoin::Amount::ZERO, |a, b| a + b.value);

    let fees = sum_inputs - sum_outputs;

    let _ = log_tx.send(format!("Created PSBT with {} inputs", psbt.inputs.len()));
    let _ = log_tx.send(format!("Total input: {} BTC", sum_inputs.to_btc()));
    let _ = log_tx.send(format!("Fees: {} sats", fees.to_sat()));
    let _ = log_tx.send(format!("Output: {} BTC", sum_outputs.to_btc()));

    Ok(SyncResult {
        psbt: psbt.to_string(),
        num_inputs: psbt.inputs.len(),
        total_value: sum_inputs,
        fees,
        output_value: sum_outputs,
    })
}
