use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};

use super::{PresentationOrId, Vec2u};

type Signature = Vec<u8>;

/// Information stored about the signature of a video/embedding for a given
/// time range in a the video
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ChunkSignature {
    /// The pixel Vec2uinate of the top left hand corner where the embedding
    /// starts
    pub pos: Vec2u,
    /// The pixel width and height of the area which is signed
    pub size: Vec2u,
    /// Reference to or the definition of a verified credential (id field is
    /// required).
    pub presentation: PresentationOrId,
    /// The signature information in base64
    #[serde(
        serialize_with = "signature_serialise",
        deserialize_with = "from_signature"
    )]
    pub signature: Signature,
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

    let decoded = STANDARD_NO_PAD.decode(s).map_err(D::Error::custom)?;
    Ok(decoded)
}
