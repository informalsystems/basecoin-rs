use std::path::Path;

use bip39::{Language, Mnemonic, Seed};
use bitcoin::bip32::{ChildNumber, DerivationPath, Xpriv, Xpub};
use bitcoin::network::Network;
use digest::Digest;
use generic_array::{typenum::U32, GenericArray};
use hdpath::StandardHDPath;
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use basecoin_modules::error::Error;

/// Represents a JSON seed file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyFile {
    name: String,
    r#type: String,
    address: String,
    pubkey: String,
    mnemonic: String,
}

/// Holds account information and keys needed for signing
/// transactions.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct KeyPair {
    pub public_key: PublicKey,
    pub private_key: SecretKey,
    pub account: String,
    pub address: Vec<u8>,
}

impl KeyPair {
    pub fn new(
        public_key: PublicKey,
        private_key: SecretKey,
        account: String,
        address: Vec<u8>,
    ) -> Self {
        Self {
            public_key,
            private_key,
            account,
            address,
        }
    }

    /// Reads in a JSON seed file and extracts the KeyPair from it.
    pub fn from_seed_file(path: impl AsRef<Path>, hd_path: &StandardHDPath) -> Result<Self, Error> {
        let seed_json = std::fs::read_to_string(&path).map_err(|e| Error::Custom {
            reason: format!("failed to read JSON seed file: {e}"),
        })?;

        let key_file: KeyFile = serde_json::from_str(&seed_json).map_err(|e| Error::Custom {
            reason: format!("failed to deserialize JSON seed file: {e}"),
        })?;

        let account = key_file.address.clone();

        let address = decode_bech32(&key_file.address).map_err(|e| Error::Custom {
            reason: format!("failed to decode key file address: {e}"),
        })?;

        let private_key =
            private_key_from_mnemonic(&key_file.mnemonic, hd_path).map_err(|e| Error::Custom {
                reason: format!("failed to derive private key from mnemonic: {e}"),
            })?;

        let derived_pubkey = Xpub::from_priv(&Secp256k1::signing_only(), &private_key);
        let public_key = derived_pubkey.public_key.clone();

        Ok(KeyPair::new(public_key, private_key, account, address))
    }

    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        let hashed_message: GenericArray<u8, U32> = Sha256::digest(message);

        let message = Message::from_digest_slice(&hashed_message).map_err(|_| Error::Custom {
            reason: format!("attempted to sign an ill-formatted message"),
        })?;

        Ok(Secp256k1::signing_only()
            .sign_ecdsa(&message, &self.private_key)
            .serialize_compact()
            .to_vec())
    }
}

pub fn private_key_from_mnemonic(mnemonic: &str, hd_path: &StandardHDPath) -> Result<Xpriv, Error> {
    let mnemonic =
        Mnemonic::from_phrase(mnemonic, Language::English).map_err(|e| Error::Custom {
            reason: format!("failed to parse mnemonic: {e}"),
        })?;

    let seed = Seed::new(&mnemonic, "");

    let base_key =
        Xpriv::new_master(Network::Bitcoin, seed.as_bytes()).map_err(|e| Error::Custom {
            reason: format!("failed to generate bip32 key: {e}"),
        })?;

    let private_key = base_key
        .derive_priv(
            &Secp256k1::new(),
            &standard_path_to_derivation_path(hd_path),
        )
        .map_err(|e| Error::Custom {
            reason: format!("failed to generate private key from bip32 key: {e}"),
        })?;

    Ok(private_key)
}

pub fn decode_bech32(input: &str) -> Result<Vec<u8>, Error> {
    let (_, data) = bech32::decode(input).map_err(|e| Error::Custom {
        reason: format!("failed to decode bech32 string {input}: {e}"),
    })?;

    Ok(data)
}

fn standard_path_to_derivation_path(path: &StandardHDPath) -> DerivationPath {
    let child_numbers = vec![
        ChildNumber::from_hardened_idx(path.purpose().as_value().as_number())
            .expect("Purpose is not Hardened"),
        ChildNumber::from_hardened_idx(path.coin_type()).expect("Coin Type is not Hardened"),
        ChildNumber::from_hardened_idx(path.account()).expect("Account is not Hardened"),
        ChildNumber::from_normal_idx(path.change()).expect("Change is Hardened"),
        ChildNumber::from_normal_idx(path.index()).expect("Index is Hardened"),
    ];

    DerivationPath::from(child_numbers)
}
