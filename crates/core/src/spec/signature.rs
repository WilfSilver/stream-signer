use serde::{Deserialize, Serialize};

use super::{coord::Coord, credential::CredentialOpt};

/// Information stored about the signature of a video/embedding for a given
/// time range in a the video
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Signature {
    /// The pixel coordinate of the top left hand corner where the embedding
    /// starts
    pub pos: Coord,
    /// The pixel width and height of the area which is signed
    pub size: Coord,
    /// Reference to or the definition of a verified credential (id field is
    /// required).
    pub credential: CredentialOpt,
    /// The signature information in base64
    pub signature: String, // TODO: Swap to bin
}
