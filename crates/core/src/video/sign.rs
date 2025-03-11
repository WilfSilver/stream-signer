use identity_iota::{
    credential::{Jwt, JwtPresentationOptions, Presentation},
    document::CoreDocument,
    storage::{
        JwkDocumentExt, JwkStorage, JwkStorageDocumentError, JwsSignatureOptions, KeyIdStorage,
        MethodDigest, Storage,
    },
    verification::MethodData,
};

use crate::spec::{Coord, PresentationOrId, PresentationReference, SignatureInfo};
use crate::{file::Timestamp, spec::PresentationDefinition};

pub trait KeyBound: JwkStorage {}
impl<T: JwkStorage> KeyBound for T {}

pub trait KeyIdBound: KeyIdStorage {}
impl<T: KeyIdStorage> KeyIdBound for T {}

pub struct SignerInfo<'a, K, I>
where
    K: KeyBound,
    I: KeyIdBound,
{
    pub document: &'a CoreDocument,
    pub presentation: Presentation<Jwt>,
    pub storage: &'a Storage<K, I>,
    pub fragment: &'a str,
}

impl<K, I> SignerInfo<'_, K, I>
where
    K: KeyBound,
    I: KeyIdBound,
{
    async fn create_def(&self) -> Result<PresentationDefinition, JwkStorageDocumentError> {
        let jwt = self
            .document
            .create_presentation_jwt(
                &self.presentation,
                self.storage,
                self.fragment,
                &JwsSignatureOptions::default(),
                &JwtPresentationOptions::default(),
            )
            .await?;

        Ok(PresentationDefinition {
            id: self.gen_id(),
            pres: jwt,
        })
    }

    fn create_ref(&self) -> PresentationReference {
        PresentationReference { id: self.gen_id() }
    }

    fn gen_id(&self) -> String {
        "test".to_string()
    }

    async fn sign(&self, msg: &[u8]) -> Result<Vec<u8>, JwkStorageDocumentError> {
        // Obtain the method corresponding to the given fragment.

        let method = self
            .document
            .resolve_method(self.fragment, None)
            .ok_or(JwkStorageDocumentError::MethodNotFound)?;
        let MethodData::PublicKeyJwk(ref jwk) = method.data() else {
            return Err(JwkStorageDocumentError::NotPublicKeyJwk);
        };

        // Get the key identifier corresponding to the given method from the KeyId storage.
        let method_digest = MethodDigest::new(method)
            .map_err(JwkStorageDocumentError::MethodDigestConstructionError)?;
        let key_id = <I as KeyIdStorage>::get_key_id(self.storage.key_id_storage(), &method_digest)
            .await
            .map_err(JwkStorageDocumentError::KeyIdStorageError)?;

        // TODO: This is really slow (takes about 1 second to sign a 100ms chunk)
        let signature = <K as JwkStorage>::sign(self.storage.key_storage(), &key_id, msg, jwk)
            .await
            .map_err(JwkStorageDocumentError::KeyStorageError)?;

        Ok(signature)
    }
}

/// Stores the information necessary to sign a given second of a video, here
/// note that the end time is implied at when you give this information to the
/// signing algorithm
pub struct ChunkSigner<'a, K, I>
where
    K: KeyBound,
    I: KeyIdBound,
{
    /// The position of the embedding, if not given, we will assume the top
    /// right
    ///
    /// Note: If this is given, it is assumed you have defined the size
    pub pos: Option<Coord>,

    /// The given information to prove your credability as well as the
    /// information to create any signatures
    pub signer: &'a SignerInfo<'a, K, I>,

    /// The size of the embedding, if not given, we will assume the size of
    /// the window.
    ///
    /// Note: If this is given, it is assumed you have defined the position
    pub size: Option<Coord>,
    pub start: Timestamp,

    /// If this is set, we will only return a reference of the definition
    pub is_ref: bool,
}

impl<'a, K, I> ChunkSigner<'a, K, I>
where
    K: KeyBound,
    I: KeyIdBound,
{
    /// Creates a new object, with pos and size set to None (assuming they
    /// will be defined later with [SignInfo::with_embedding])
    pub fn new(start: Timestamp, signer: &'a SignerInfo<'a, K, I>, is_ref: bool) -> Self {
        Self {
            signer,
            pos: None,
            size: None,
            start,
            is_ref,
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
    pub async fn sign(
        self,
        msg: Vec<u8>,
        size: Coord,
    ) -> Result<SignatureInfo, JwkStorageDocumentError> {
        let signature = self.signer.sign(&msg).await?;
        let presentation: PresentationOrId = if self.is_ref {
            self.signer.create_ref().into()
        } else {
            self.signer.create_def().await?.into()
        };

        Ok(SignatureInfo {
            pos: self.pos.unwrap_or_default(),
            size: self.size.unwrap_or(size),
            presentation,
            signature,
        })
    }
}

#[cfg(any(test, feature = "testlibs"))]
pub use testlib_extras::*;

#[cfg(any(test, feature = "testlibs"))]
mod testlib_extras {
    use super::*;
    use identity_iota::storage::{JwkMemStore, KeyIdMemstore};
    use testlibs::anyhow;
    use testlibs::identity::TestIdentity;

    pub trait GenSignerInfo {
        fn gen_signer_info(&self) -> anyhow::Result<SignerInfo<'_, JwkMemStore, KeyIdMemstore>>;
    }
    impl GenSignerInfo for TestIdentity {
        fn gen_signer_info(&self) -> anyhow::Result<SignerInfo<'_, JwkMemStore, KeyIdMemstore>> {
            Ok(SignerInfo {
                document: &self.document,
                presentation: self.build_presentation()?,
                storage: &self.storage,
                fragment: &self.fragment,
            })
        }
    }
}
