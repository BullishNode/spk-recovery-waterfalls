use miniscript::bitcoin::{self, Txid};
use waterfalls_client::Builder;

pub fn broadcast_psbt(tx: String, url: String) -> Result<Txid, String> {
    let tx: bitcoin::Transaction = bitcoin::consensus::encode::deserialize_hex(&tx)
        .map_err(|e| format!("Fail to deserialize transaction : {e:?}"))?;

    let txid = tx.compute_txid();

    println!("Broadcasting transaction: {}", txid);

    let client = Builder::new(&url).build_blocking();

    client
        .broadcast(&tx)
        .map_err(|e| format!("Failed to broadcast transaction: {:?}", e))?;

    println!("Transaction broadcast successfully: {}", txid);

    Ok(txid)
}
