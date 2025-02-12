pub use pqc_dilithium::Keypair;

use crate::spec::{Coord, CredentialOpt, SignatureInfo};
use crate::Timestamp;

/// Stores the information necessary to sign a given second of a video, here
/// note that the end time is implied at when you give this information to the
/// signing algorithm
pub struct SignInfo {
    /// The position of the embedding, if not given, we will assume the top
    /// right
    ///
    /// Note: If this is given, it is assumed you have defined the size
    pub pos: Option<Coord>,
    /// The information about the credential, note that we do not manage the
    /// credential definitions and references here
    pub credential: CredentialOpt,
    /// The keypair to sign with
    pub keypair: Keypair,
    /// The size of the embedding, if not given, we will assume the size of
    /// the window.
    ///
    /// Note: If this is given, it is assumed you have defined the position
    pub size: Option<Coord>,
    pub start: Timestamp,
}

impl SignInfo {
    /// Creates a new object, with pos and size set to None (assuming they
    /// will be defined later with [SignInfo::with_embedding])
    pub fn new(start: Timestamp, credential: CredentialOpt, keypair: Keypair) -> Self {
        Self {
            credential,
            keypair,
            pos: None,
            size: None,
            start,
        }
    }

    /// Assigns the information about the position and size of embedding to
    /// be signed
    pub fn with_embedding(mut self, coord: Coord, size: Coord) -> Self {
        self.pos = Some(coord);
        self.size = Some(size);
        self
    }

    /// Signs a given stream and generates [SignatureInfo] which can be stored
    /// in the files
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
