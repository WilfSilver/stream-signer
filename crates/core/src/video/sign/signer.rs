use std::{future::Future, sync::Arc};

use futures::{future::BoxFuture, FutureExt};
use identity_iota::{
    credential::Jwt,
    document::CoreDocument,
    storage::{
        JwkStorage, JwkStorageDocumentError, KeyId, KeyIdStorage, KeyIdStorageResult,
        KeyStorageResult, MethodDigest,
    },
    verification::{jwk::Jwk, MethodData},
};

use crate::file::Timestamp;
use crate::spec::{ChunkSignature, PresentationOrId, Vec2u};

pub trait KeyBound: JwkStorage {}
impl<T: JwkStorage> KeyBound for T {}

pub trait KeyIdBound: KeyIdStorage {}
impl<T: KeyIdStorage> KeyIdBound for T {}

/// This signifies a type which can be used to sign objects and has a
/// verifiable credential attached to it.
///
/// To implement this, it is recommended to implement [IotaSigner], but these
/// have been split to make it easier to integrate into a non-iota library.
pub trait Signer: Sync + Send {
    /// Generates the presentation to give attach to the signature proving
    /// the authenticity of the signer
    fn presentation(&self) -> BoxFuture<Result<Jwt, JwkStorageDocumentError>>;

    /// Returns the id that should be given to the presentation and used
    /// for the `id` field within `presentation`
    fn get_presentation_id(&self) -> String;

    /// Wraps all the functions above, producing a clean interface to request
    fn sign<'a>(&'a self, msg: &'a [u8])
        -> BoxFuture<'a, Result<Vec<u8>, JwkStorageDocumentError>>;
}

/// This is a simple trait for creating an easier trait to implement for
/// types from [identity_iota]. If this is implemented, [Signer] is implicitly
/// also implemented
///
/// TODO: Example implementations
pub trait IotaSigner: Sync + Send {
    /// Generates the presentation to give attach to the signature proving
    /// the authenticity of the signer
    fn presentation(&self) -> impl Future<Output = Result<Jwt, JwkStorageDocumentError>> + Send;

    /// Returns the core document for which stores the signing method as well
    /// as public key. This should be accessible via the DID from the verifier
    fn document(&self) -> &CoreDocument;
    fn fragment(&self) -> &str;

    /// Returns the id of the key which will be used to sign the signature
    fn get_key_id(
        &self,
        digest: &MethodDigest,
    ) -> impl Future<Output = KeyIdStorageResult<KeyId>> + Send;

    /// This should sign the `msg` with the given `key_id` from [IotaSigner::get_key_id]
    fn sign_with_key(
        &self,
        key_id: &KeyId,
        msg: &[u8],
        public_key: &Jwk,
    ) -> impl Future<Output = KeyStorageResult<Vec<u8>>> + Send;
}

impl<S: IotaSigner> Signer for S {
    fn presentation(&self) -> BoxFuture<Result<Jwt, JwkStorageDocumentError>> {
        self.presentation().boxed()
    }

    fn get_presentation_id(&self) -> String {
        self.document().id().to_string()
    }

    fn sign<'a>(
        &'a self,
        msg: &'a [u8],
    ) -> BoxFuture<'a, Result<Vec<u8>, JwkStorageDocumentError>> {
        async {
            // Obtain the method corresponding to the given fragment.

            let method = self
                .document()
                .resolve_method(self.fragment(), None)
                .ok_or(JwkStorageDocumentError::MethodNotFound)?;
            let MethodData::PublicKeyJwk(ref jwk) = method.data() else {
                return Err(JwkStorageDocumentError::NotPublicKeyJwk);
            };

            // Get the key identifier corresponding to the given method from the KeyId storage.
            let method_digest = MethodDigest::new(method)
                .map_err(JwkStorageDocumentError::MethodDigestConstructionError)?;
            let key_id = self
                .get_key_id(&method_digest)
                .await
                .map_err(JwkStorageDocumentError::KeyIdStorageError)?;

            let signature = self
                .sign_with_key(&key_id, msg, jwk)
                .await
                .map_err(JwkStorageDocumentError::KeyStorageError)?;

            Ok(signature)
        }
        .boxed()
    }
}

/// Stores the information necessary to sign a given second of a video, here
/// note that the end time is implied at when you give this information to the
/// signing algorithm
#[derive(Debug)]
pub struct ChunkSigner<S: Signer> {
    /// The position of the embedding, if not given, we will assume the top
    /// right
    ///
    /// Note: If this is given, it is assumed you have defined the size
    pub pos: Option<Vec2u>,

    /// The given information to prove your credability as well as the
    /// information to create any signatures
    pub signer: Arc<S>,

    /// The size of the embedding, if not given, we will assume the size of
    /// the window.
    ///
    /// Note: If this is given, it is assumed you have defined the position
    pub size: Option<Vec2u>,
    pub start: Timestamp,

    /// If this is set, we will only return a reference of the definition
    pub is_ref: bool,

    /// The audio channels to include when signing, in the order that they will
    /// be added to the buffer to be signed. If [None] is given, then it assumes
    /// all buffers should be included
    pub channels: Option<Vec<usize>>,
}

impl<S: Signer> ChunkSigner<S> {
    /// Creates a new object, with pos and size set to None (assuming they
    /// will be defined later with [Self::with_embedding])
    pub const fn new(
        start: Timestamp,
        signer: Arc<S>,
        channels: Option<Vec<usize>>,
        is_ref: bool,
    ) -> Self {
        Self {
            signer,
            pos: None,
            size: None,
            start,
            is_ref,
            channels,
        }
    }

    /// Assigns the information about the position and size of embedding to
    /// be signed
    pub fn with_embedding(mut self, pos: Vec2u, size: Vec2u) -> Self {
        self.pos = Some(pos);
        self.size = Some(size);
        self
    }

    /// Signs a given stream and generates [ChunkSignature] which can be stored
    /// in the files
    pub async fn sign(
        self,
        msg: Vec<u8>,
        size: Vec2u,
        channels: usize,
    ) -> Result<ChunkSignature, JwkStorageDocumentError> {
        let presentation: PresentationOrId = if self.is_ref {
            PresentationOrId::new_ref(self.signer.get_presentation_id())
        } else {
            let jwt = self.signer.presentation().await?;
            PresentationOrId::new_def(self.signer.get_presentation_id(), jwt)
        };

        let signature = self.signer.sign(&msg).await?;

        Ok(ChunkSignature {
            pos: self.pos.unwrap_or_default(),
            size: self.size.unwrap_or(size),
            channels: self.channels.unwrap_or((0..channels).collect::<Vec<_>>()),
            presentation,
            signature,
        })
    }
}

impl<S: Signer> Clone for ChunkSigner<S> {
    fn clone(&self) -> Self {
        Self {
            signer: self.signer.clone(),
            channels: self.channels.clone(),
            pos: self.pos,
            size: self.size,
            start: self.start,
            is_ref: self.is_ref,
        }
    }
}

#[cfg(any(test, doctest, feature = "testlibs"))]
mod testlib_extras {
    use super::*;
    use identity_iota::{
        credential::JwtPresentationOptions,
        storage::{JwkDocumentExt, JwsSignatureOptions},
    };
    use testlibs::identity::TestIdentity;

    impl IotaSigner for TestIdentity {
        fn document(&self) -> &CoreDocument {
            &self.document
        }

        fn fragment(&self) -> &str {
            &self.fragment
        }

        async fn get_key_id(&self, digest: &MethodDigest) -> KeyIdStorageResult<KeyId> {
            self.storage
                .lock()
                .await
                .key_id_storage()
                .get_key_id(digest)
                .await
        }

        async fn sign_with_key(
            &self,
            key_id: &KeyId,
            msg: &[u8],
            public_key: &Jwk,
        ) -> KeyStorageResult<Vec<u8>> {
            self.storage
                .lock()
                .await
                .key_storage()
                .sign(key_id, msg, public_key)
                .await
        }

        async fn presentation(&self) -> Result<Jwt, JwkStorageDocumentError> {
            let pres = self
                .build_presentation()
                .map_err(|_| JwkStorageDocumentError::JwpBuildingError)?;

            let storage = self.storage.lock().await;
            self.document
                .create_presentation_jwt(
                    &pres,
                    &storage,
                    &self.fragment,
                    &JwsSignatureOptions::default(),
                    &JwtPresentationOptions::default(),
                )
                .await
        }
    }
}
