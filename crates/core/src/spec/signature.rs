use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{coord::Coord, credential::CredentialOpt};

const SIGNATURE_LEN: usize = 4595;
type Signature = [u8; SIGNATURE_LEN];

/// Information stored about the signature of a video/embedding for a given
/// time range in a the video
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SignatureInfo {
    /// The pixel coordinate of the top left hand corner where the embedding
    /// starts
    pub pos: Coord,
    /// The pixel width and height of the area which is signed
    pub size: Coord,
    /// Reference to or the definition of a verified credential (id field is
    /// required).
    pub credential: CredentialOpt,
    /// The signature information in base64
    #[serde(
        serialize_with = "signature_serialise",
        deserialize_with = "from_signature"
    )]
    pub signature: Signature, // TODO: Swap to bin
}

fn signature_serialise<S>(x: &Signature, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let encoded = STANDARD_NO_PAD.encode(x);
    s.serialize_str(&encoded)
}

fn from_signature<'de, D>(deserializer: D) -> Result<Signature, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;

    // TODO: Handle error
    let decoded = STANDARD_NO_PAD.decode(s).unwrap();
    let signature: Signature = decoded.try_into().unwrap();
    Ok(signature)
}
