pub use pqc_dilithium::Keypair;

use crate::spec::{Coord, CredentialOpt, SignatureInfo};
use crate::Timestamp;

pub struct SignInfo {
    pub pos: Option<Coord>,
    pub credential: CredentialOpt,
    pub keypair: Keypair,
    pub size: Option<Coord>,
    pub start: Timestamp,
}

impl SignInfo {
    pub fn new(start: Timestamp, credential: CredentialOpt, keypair: Keypair) -> Self {
        Self {
            credential,
            keypair,
            pos: None,
            size: None,
            start,
        }
    }

    pub fn with_embedding(mut self, coord: Coord, size: Coord) -> Self {
        self.pos = Some(coord);
        self.size = Some(size);
        self
    }

    pub fn sign(self, msg: &[u8], size: Coord) -> SignatureInfo {
        let signature = self.keypair.sign(msg);
        SignatureInfo {
            pos: self.pos.unwrap_or_default(),
            size: self.size.unwrap_or(size),
            credential: self.credential,
            signature,
        }
    }
}
