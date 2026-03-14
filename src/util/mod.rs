pub mod broadcast;
pub mod sign;
pub mod sync;

use miniscript::bitcoin::Amount;
use miniscript::bitcoin::ScriptBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct SyncResult {
    pub psbt: String,
    pub num_inputs: usize,
    pub total_value: Amount,
    pub fees: Amount,
    pub output_value: Amount,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SpkEntry {
    pub spk: ScriptBuf,
    pub change: bool,
    pub index: u32,
}
