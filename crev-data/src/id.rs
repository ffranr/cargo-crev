use base64;
use blake2;
use ed25519_dalek::{self, SecretKey, PublicKey};
use rand::OsRng;
use std::{
    fmt
};
use crev_common::{self, serde::{as_base64, from_base64}};
use Result;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum IdType {
#[serde(rename = "crev")]
    Crev
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Public CrevId of someone
pub struct PubId {
    pub url: String,
    #[serde(rename = "type")]
    pub type_: IdType,

    #[serde(
        serialize_with = "as_base64",
        deserialize_with = "from_base64"
    )]
    pub id: Vec<u8>,
}

impl PubId {
    pub fn new(url: String, id: Vec<u8>) -> Self {
        Self {
            url,
            id,
            type_: IdType::Crev,
        }
    }
}

impl fmt::Display for PubId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        crev_common::serde::write_as_headerless_yaml(self, f)
    }
}

#[derive(Debug)]
pub struct OwnId {
    pub id: PubId,
    pub keypair: ed25519_dalek::Keypair,
}

impl OwnId {
    pub fn new(url: String, sec_key: Vec<u8>) -> Result<Self> {

        let sec_key = SecretKey::from_bytes(&sec_key)?;
        let calculated_pub_key: PublicKey = PublicKey::from_secret::<blake2::Blake2b>(&sec_key);

        Ok(Self {

            id: PubId::new(url, calculated_pub_key.as_bytes().to_vec()),
            keypair: ed25519_dalek::Keypair {
                secret: sec_key,
                public: calculated_pub_key,
            },
        })
    }

    pub fn sign(&self, msg: &[u8]) -> Vec<u8> {
        self.keypair.sign::<blake2::Blake2b>(&msg).to_bytes().to_vec()
    }

    pub fn type_as_string(&self) -> String {
        "crev".into()
    }


    pub fn pub_key_as_base64(&self) -> String {
        base64::encode_config(&self.id.id, base64::URL_SAFE)
    }

    pub fn generate(url: String) -> Self {
        let mut csprng: OsRng = OsRng::new().unwrap();
        let keypair = ed25519_dalek::Keypair::generate::<blake2::Blake2b, _>(&mut csprng);
        Self {
            id: PubId::new(url, keypair.public.as_bytes().to_vec()),
            keypair,
        }
    }
}