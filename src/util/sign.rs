use std::str::FromStr;

use bwk_sign::Signer;
use miniscript::{
    bitcoin::{self, hex::DisplayHex, Psbt},
    Descriptor, DescriptorPublicKey,
};

pub fn sign_psbt(
    mnemonic: String,
    psbt_str: String,
    descriptor: String,
    network: bitcoin::Network,
) -> Result<String, String> {
    let mut psbt = Psbt::from_str(psbt_str.trim()).map_err(|e| format!("Invalid PSBT: {}", e))?;
    let descriptor = Descriptor::<DescriptorPublicKey>::from_str(&descriptor)
        .map_err(|e| format!("Invalid descriptor: {e:?}"))?;

    let mut signer = bwk_sign::HotSigner::new_from_mnemonics(network, &mnemonic)
        .map_err(|_| "Fail to create signer")?;
    signer.register_descriptor(descriptor);
    if signer.descriptors().is_empty() {
        return Err("Fail to register descriptor".to_string());
    }
    signer.sign(&mut psbt);
    let signed_tx = signer
        .finalize(&mut psbt)
        .map_err(|e| format!("Fail to finalize transaction {e:#?}"))?;
    let serialized_tx = bitcoin::consensus::serialize(&signed_tx).to_lower_hex_string();

    Ok(serialized_tx)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use bip39::Mnemonic;
    use miniscript::{
        bitcoin::{
            self, absolute,
            bip32::{DerivationPath, Xpriv, Xpub},
            key::Secp256k1,
            transaction::Version,
            Amount, Network, OutPoint, Psbt, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid,
            Witness,
        },
        psbt::PsbtExt,
        Descriptor, DescriptorPublicKey,
    };

    use crate::util::sign::sign_psbt;

    // BIP39 test vector mnemonic — well-known, safe for testing
    const TEST_MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn build_test_psbt(descriptor_str: &str, recv_index: u32) -> (Psbt, String) {
        let descriptor = Descriptor::<DescriptorPublicKey>::from_str(descriptor_str).unwrap();
        let single = descriptor.into_single_descriptors().unwrap();
        let recv_desc = &single[0];
        let concrete = recv_desc.at_derivation_index(recv_index).unwrap();
        let spk = concrete.script_pubkey();

        let fake_txid =
            Txid::from_str("a15d57094aa7a21a28cb20b59aab8fc7d1149a3bdbcddba9c622e4f5f6a99ece")
                .unwrap();

        let txin = TxIn {
            previous_output: OutPoint {
                txid: fake_txid,
                vout: 0,
            },
            script_sig: ScriptBuf::default(),
            sequence: Sequence::ZERO,
            witness: Witness::default(),
        };
        let txout = TxOut {
            value: Amount::from_sat(99_000),
            script_pubkey: spk.clone(), // send back to same address for simplicity
        };
        let tx = Transaction {
            version: Version::TWO,
            lock_time: absolute::LockTime::ZERO,
            input: vec![txin],
            output: vec![txout],
        };

        let mut psbt = Psbt::from_unsigned_tx(tx).unwrap();
        psbt.inputs[0].witness_utxo = Some(TxOut {
            value: Amount::from_sat(100_000),
            script_pubkey: spk,
        });

        // populate bip32_derivation + miniscript descriptor metadata
        PsbtExt::update_input_with_descriptor(&mut psbt, 0, &concrete).unwrap();

        let psbt_str = psbt.to_string();
        (psbt, psbt_str)
    }

    fn test_descriptor() -> String {
        let mnemonic = Mnemonic::from_str(TEST_MNEMONIC).unwrap();
        let seed = mnemonic.to_seed("");
        let secp = Secp256k1::new();
        let master = Xpriv::new_master(Network::Bitcoin, &seed).unwrap();
        let fp = master.fingerprint(&secp);
        let path = DerivationPath::from_str("m/84'/0'/0'").unwrap();
        let account_xpriv = master.derive_priv(&secp, &path).unwrap();
        let account_xpub = Xpub::from_priv(&secp, &account_xpriv);
        format!("wpkh([{}/84'/0'/0']{}/<0;1>/*)", fp, account_xpub)
    }

    #[test]
    fn test_sign_psbt_receive_index_0() {
        let descriptor_str = test_descriptor();
        let (_, psbt_str) = build_test_psbt(&descriptor_str, 0);
        let result = sign_psbt(
            TEST_MNEMONIC.to_string(),
            psbt_str,
            descriptor_str,
            Network::Bitcoin,
        );
        // result is a finalized Transaction string
        let tx_str = result.unwrap();
        let _tx: bitcoin::Transaction =
            bitcoin::consensus::encode::deserialize_hex(&tx_str).unwrap();
    }

    #[test]
    fn test_sign_psbt_wrong_mnemonic() {
        let descriptor_str = test_descriptor();
        let (_, psbt_str) = build_test_psbt(&descriptor_str, 0);
        let wrong_mnemonic = "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong";
        let result = sign_psbt(
            wrong_mnemonic.to_string(),
            psbt_str,
            descriptor_str,
            Network::Bitcoin,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("MissingPubkey"));
    }
}
