use miniscript::bitcoin::{self, Txid};

pub fn broadcast_psbt(tx: String, ip: String, port: String) -> Result<Txid, String> {
    let tx: bitcoin::Transaction = bitcoin::consensus::encode::deserialize_hex(&tx)
        .map_err(|e| format!("Fail to deserialize transaction : {e:?}"))?;

    let port: u16 = port.parse().map_err(|e| format!("Invalid port: {}", e))?;

    let txid = tx.compute_txid();

    println!("Broadcasting transaction: {}", txid);

    let mut client = bwk_electrum::client::Client::new_local(&ip, port)
        .map_err(|e| format!("Failed to connect to Electrum server: {:?}", e))?;

    client
        .broadcast(&tx)
        .map_err(|e| format!("Failed to broadcast transaction: {:?}", e))?;

    println!("Transaction broadcast successfully: {}", txid);

    Ok(txid)
}
