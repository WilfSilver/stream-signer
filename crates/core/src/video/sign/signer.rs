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

use crate::{file::Timestamp, video::frame::FrameWithAudio};
use crate::{
    spec::{ChunkSignature, PresentationOrId, Vec2u},
    video::audio::AudioSlice,
};

pub trait KeyBound: JwkStorage {}
impl<T: JwkStorage> KeyBound for T {}

pub trait KeyIdBound: KeyIdStorage {}
impl<T: KeyIdStorage> KeyIdBound for T {}

/// This signifies a type which can be used to sign objects and has a
/// verifiable credential attached to it.
///
/// To implement this, it is recommended to implement [IotaSigner], but these
/// have been split to make it easier to integrate into a non-iota library.
///
/// Please see [IotaSigner] for example implemenations
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
/// also implemented.
///
///
/// ## Example
///
/// This is an example of how tests within this library implements [Signer]
///
/// ```
/// use std::sync::Arc;
///
/// use identity_iota::{
///     core::Url,
///     did::DID,
///     credential::{Credential, Jwt, Presentation, PresentationBuilder, JwtPresentationOptions, Subject},
///     storage::{JwkDocumentExt, JwkMemStore, JwkStorage, JwkStorageDocumentError, JwsSignatureOptions, KeyId, KeyIdStorage, KeyIdMemstore, KeyIdStorageResult, KeyStorageResult, MethodDigest, Storage},
///
///     document::CoreDocument,
///     verification::jwk::Jwk,
/// };
/// use iota_sdk::client::secret::SecretManager;
/// use stream_signer::video::sign::IotaSigner;
/// use tokio::sync::Mutex;
///
/// type MemStorage = Storage<JwkMemStore, KeyIdMemstore>;
///
/// struct TestIdentity {
///     pub manager: SecretManager,
///     pub storage: Arc<Mutex<MemStorage>>,
///     pub credential: Credential,
///     pub jwt: Jwt,
///     pub document: CoreDocument,
///     pub fragment: String,
/// }
///
/// impl IotaSigner for TestIdentity {
///     fn document(&self) -> &CoreDocument {
///         &self.document
///     }
///
///     fn fragment(&self) -> &str {
///         &self.fragment
///     }
///
///     async fn get_key_id(&self, digest: &MethodDigest) -> KeyIdStorageResult<KeyId> {
///         self.storage
///             .lock()
///             .await
///             .key_id_storage()
///             .get_key_id(digest)
///             .await
///     }
///
///     async fn sign_with_key(
///         &self,
///         key_id: &KeyId,
///         msg: &[u8],
///         public_key: &Jwk,
///     ) -> KeyStorageResult<Vec<u8>> {
///         self.storage
///             .lock()
///             .await
///             .key_storage()
///             .sign(key_id, msg, public_key)
///             .await
///     }
///
///     async fn presentation(&self) -> Result<Jwt, JwkStorageDocumentError> {
///         let pres: Presentation<Jwt> = PresentationBuilder::new(self.document.id().to_url().into(), Default::default())
///             .credential(self.jwt.clone())
///             .build()
///             .map_err(|_| JwkStorageDocumentError::JwpBuildingError)?;
///
///         let storage = self.storage.lock().await;
///         self.document
///             .create_presentation_jwt(
///                 &pres,
///                 &storage,
///                 &self.fragment,
///                 &JwsSignatureOptions::default(),
///                 &JwtPresentationOptions::default(),
///             )
///             .await
///     }
/// }
/// ````
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

/// Trait which stores the partially created [ChunkSigner] and is used when
/// passing that information around so that the user does not have to specify
/// the size of frames if they do not want to.
#[derive(Debug)]
pub struct ChunkSignerBuilder<S: Signer> {
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

impl<S: Signer> ChunkSignerBuilder<S> {
    /// Sets the presentation to be a reference instead of a full definition
    pub fn as_ref(mut self) -> Self {
        self.is_ref = true;
        self
    }

    /// Allows to customise whether the presentation is a reference or
    /// definition in the final result
    pub fn with_is_ref(mut self, is_ref: bool) -> Self {
        self.is_ref = is_ref;
        self
    }

    /// Assigns the information about the position and size of embedding to
    /// be signed
    pub fn with_embedding(mut self, pos: Vec2u, size: Vec2u) -> Self {
        self.pos = Some(pos);
        self.size = Some(size);
        self
    }

    /// Customises what channels are included within the signature
    pub fn with_channels(mut self, channels: Vec<usize>) -> Self {
        self.channels = Some(channels);
        self
    }

    /// A function that converts the type into a fully fledged [ChunkSigner],
    /// filling in all the defaults.
    ///
    /// This function should not be called directly, instead it should be
    /// managed by the manager as it needs access to the frame information
    pub fn build(self, frame: &FrameWithAudio) -> ChunkSigner<S> {
        ChunkSigner {
            pos: self.pos.unwrap_or_default(),
            size: self.size.unwrap_or(frame.frame.size()),
            signer: self.signer,
            start: self.start,
            is_ref: self.is_ref,
            channels: self.channels.unwrap_or_else(|| {
                (0..frame
                    .audio
                    .as_ref()
                    .map(AudioSlice::channels)
                    .unwrap_or_default())
                    .collect::<Vec<_>>()
            }),
        }
    }
}

/// Stores the information necessary to sign a given second of a video, here
/// note that the end time is implied at when you give this information to the
/// signing algorithm.
///
/// Normally this type is not directly accepted as the returned type, instead
/// [ChunkSignerBuilder] is used in of its place accessed through
/// [ChunkSigner::build]. This is to allow the user not to specify every aspect
/// of the specification.
///
/// ## Examples
///
/// ### Full frame
///
/// ```
/// # use std::error::Error;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn Error>> {
/// use std::sync::Arc;
/// use stream_signer::{
///     file::Timestamp,
///     video::sign::ChunkSigner,
/// #   video::SignPipeline,
///     spec::Vec2u,
/// };
/// # use identity_iota::{did::DID, core::{json, FromJson}, credential::Subject};
/// # use testlibs::{
/// #     client::{get_client, get_resolver},
/// #     identity::TestIdentity,
/// #     issuer::TestIssuer,
/// #     test_video, videos,
/// # };
/// #
/// # let client = get_client();
/// # let issuer = TestIssuer::new(client.clone()).await?;
/// #
/// # let identity = TestIdentity::new(&issuer, |id| {
/// #     Subject::from_json_value(json!({
/// #       "id": id.as_str(),
/// #       "name": "Alice",
/// #       "degree": {
/// #         "type": "BachelorDegree",
/// #         "name": "Bachelor of Science and Arts",
/// #       },
/// #       "GPA": "4.0",
/// #     })).unwrap()
/// # })
/// # .await?;
///
/// let signer = Arc::new(identity);
///
/// let chunk_signer = ChunkSigner::build(Timestamp::from_millis(100), signer);
/// #
/// # stream_signer::gst::init();
/// # let video = test_video(videos::BIG_BUNNY_1080);
/// # let pipe = SignPipeline::build_from_path(&video).unwrap().build().unwrap();
/// # let mut iter = pipe.try_into_iter(()).unwrap();
/// # let frame_with_audio = iter.next().unwrap().unwrap();
///
/// // This is generated by the FrameManager based on this definition
/// let chunk_signer = chunk_signer.build(&frame_with_audio);
/// let msg = vec![0, 0, 0, /* ... */];
/// let signature = chunk_signer.sign(msg).await.unwrap();
///
/// assert_eq!(signature.pos, Vec2u::new(0, 0));
/// // Values are taken from our test video
/// assert_eq!(signature.size, Vec2u::new(1920, 1080));
/// assert_eq!(signature.channels, vec![0, 1]);
/// # Ok(())
/// # }
/// ```
///
/// ### Presentation definition
///
/// ```
/// # use std::error::Error;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn Error>> {
/// use std::sync::Arc;
/// use stream_signer::{
///     file::Timestamp,
///     spec::PresentationOrId,
///     video::sign::ChunkSigner,
/// #   video::SignPipeline,
///     spec::Vec2u,
/// };
/// # use identity_iota::{did::DID, core::{json, FromJson}, credential::Subject};
/// # use testlibs::{
/// #     client::{get_client, get_resolver},
/// #     identity::TestIdentity,
/// #     issuer::TestIssuer,
/// #     test_video, videos,
/// # };
/// #
/// # let client = get_client();
/// # let issuer = TestIssuer::new(client.clone()).await?;
/// #
/// # let identity = TestIdentity::new(&issuer, |id| {
/// #     Subject::from_json_value(json!({
/// #       "id": id.as_str(),
/// #       "name": "Alice",
/// #       "degree": {
/// #         "type": "BachelorDegree",
/// #         "name": "Bachelor of Science and Arts",
/// #       },
/// #       "GPA": "4.0",
/// #     })).unwrap()
/// # })
/// # .await?;
///
/// let signer = Arc::new(identity);
///
/// let chunk_signer = ChunkSigner::build(Timestamp::from_millis(100), signer);
/// #
/// # stream_signer::gst::init();
/// # let video = test_video(videos::BIG_BUNNY);
/// # let pipe = SignPipeline::build_from_path(&video).unwrap().build().unwrap();
/// # let mut iter = pipe.try_into_iter(()).unwrap();
/// # let frame_with_audio = iter.next().unwrap().unwrap();
///
/// // This is generated by the FrameManager based on this definition
/// let chunk_signer = chunk_signer.build(&frame_with_audio);
/// let msg = vec![0, 0, 0, /* ... */];
/// let signature = chunk_signer.sign(msg).await.unwrap();
///
/// assert!(matches!(signature.presentation, PresentationOrId::Def { id: _, jwt: _ }));
/// # Ok(())
/// # }
/// ```
///
/// ### Presentation reference
///
/// ```
/// # use std::error::Error;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn Error>> {
/// use std::sync::Arc;
/// use stream_signer::{
///     file::Timestamp,
///     spec::PresentationOrId,
///     video::sign::ChunkSigner,
/// #   video::SignPipeline,
///     spec::Vec2u,
/// };
/// # use identity_iota::{did::DID, core::{json, FromJson}, credential::Subject};
/// # use testlibs::{
/// #     client::{get_client, get_resolver},
/// #     identity::TestIdentity,
/// #     issuer::TestIssuer,
/// #     test_video, videos,
/// # };
/// #
/// # let client = get_client();
/// # let issuer = TestIssuer::new(client.clone()).await?;
/// #
/// # let identity = TestIdentity::new(&issuer, |id| {
/// #     Subject::from_json_value(json!({
/// #       "id": id.as_str(),
/// #       "name": "Alice",
/// #       "degree": {
/// #         "type": "BachelorDegree",
/// #         "name": "Bachelor of Science and Arts",
/// #       },
/// #       "GPA": "4.0",
/// #     })).unwrap()
/// # })
/// # .await?;
///
/// let signer = Arc::new(identity);
///
/// let chunk_signer = ChunkSigner::build(Timestamp::from_millis(100), signer).as_ref();
/// #
/// # stream_signer::gst::init();
/// # let video = test_video(videos::BIG_BUNNY);
/// # let pipe = SignPipeline::build_from_path(&video).unwrap().build().unwrap();
/// # let mut iter = pipe.try_into_iter(()).unwrap();
/// # let frame_with_audio = iter.next().unwrap().unwrap();
///
/// // This is generated by the FrameManager based on this definition
/// let chunk_signer = chunk_signer.build(&frame_with_audio);
/// let msg = vec![0, 0, 0, /* ... */];
/// let signature = chunk_signer.sign(msg).await.unwrap();
///
/// assert!(matches!(signature.presentation, PresentationOrId::Ref { id: _ }));
/// # Ok(())
/// # }
/// ```
///
/// ### Presentation potentially a reference
///
/// ```
/// # use std::error::Error;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn Error>> {
/// use std::sync::Arc;
/// use stream_signer::{
///     file::Timestamp,
///     spec::PresentationOrId,
///     video::sign::ChunkSigner,
/// #   video::SignPipeline,
///     spec::Vec2u,
/// };
/// # use identity_iota::{did::DID, core::{json, FromJson}, credential::Subject};
/// # use testlibs::{
/// #     client::{get_client, get_resolver},
/// #     identity::TestIdentity,
/// #     issuer::TestIssuer,
/// #     test_video, videos,
/// # };
/// #
/// # let client = get_client();
/// # let issuer = TestIssuer::new(client.clone()).await?;
/// #
/// # let identity = TestIdentity::new(&issuer, |id| {
/// #     Subject::from_json_value(json!({
/// #       "id": id.as_str(),
/// #       "name": "Alice",
/// #       "degree": {
/// #         "type": "BachelorDegree",
/// #         "name": "Bachelor of Science and Arts",
/// #       },
/// #       "GPA": "4.0",
/// #     })).unwrap()
/// # })
/// # .await?;
///
/// let signer = Arc::new(identity);
///
/// let chunk_signer = ChunkSigner::build(Timestamp::from_millis(100), signer)
///     .with_is_ref(true /* Or any other query */);
/// #
/// # stream_signer::gst::init();
/// # let video = test_video(videos::BIG_BUNNY);
/// # let pipe = SignPipeline::build_from_path(&video).unwrap().build().unwrap();
/// # let mut iter = pipe.try_into_iter(()).unwrap();
/// # let frame_with_audio = iter.next().unwrap().unwrap();
///
/// // This is generated by the FrameManager based on this definition
/// let chunk_signer = chunk_signer.build(&frame_with_audio);
/// let msg = vec![0, 0, 0, /* ... */];
/// let signature = chunk_signer.sign(msg).await.unwrap();
///
/// assert!(matches!(signature.presentation, PresentationOrId::Ref { id: _ }));
/// # Ok(())
/// # }
/// ```
///
/// ### With embedding
///
/// ```
/// # use std::error::Error;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn Error>> {
/// use std::sync::Arc;
/// use stream_signer::{
///     file::Timestamp,
///     video::sign::ChunkSigner,
/// #   video::SignPipeline,
///     spec::Vec2u,
/// };
/// # use identity_iota::{did::DID, core::{json, FromJson}, credential::Subject};
/// # use testlibs::{
/// #     client::{get_client, get_resolver},
/// #     identity::TestIdentity,
/// #     issuer::TestIssuer,
/// #     test_video, videos,
/// # };
/// #
/// # let client = get_client();
/// # let issuer = TestIssuer::new(client.clone()).await?;
/// #
/// # let identity = TestIdentity::new(&issuer, |id| {
/// #     Subject::from_json_value(json!({
/// #       "id": id.as_str(),
/// #       "name": "Alice",
/// #       "degree": {
/// #         "type": "BachelorDegree",
/// #         "name": "Bachelor of Science and Arts",
/// #       },
/// #       "GPA": "4.0",
/// #     })).unwrap()
/// # })
/// # .await?;
///
/// let signer = Arc::new(identity);
///
/// let chunk_signer = ChunkSigner::build(Timestamp::from_millis(100), signer)
///     .with_embedding(Vec2u::new(10, 10), Vec2u::new(100, 100));
/// #
/// # stream_signer::gst::init();
/// # let video = test_video(videos::BIG_BUNNY);
/// # let pipe = SignPipeline::build_from_path(&video).unwrap().build().unwrap();
/// # let mut iter = pipe.try_into_iter(()).unwrap();
/// # let frame_with_audio = iter.next().unwrap().unwrap();
///
/// // This is generated by the FrameManager based on this definition
/// let chunk_signer = chunk_signer.build(&frame_with_audio);
/// let msg = vec![0, 0, 0, /* ... */];
/// let signature = chunk_signer.sign(msg).await.unwrap();
///
/// assert_eq!(signature.pos, Vec2u::new(10, 10));
/// assert_eq!(signature.size, Vec2u::new(100, 100));
/// # Ok(())
/// # }
/// ```
///
/// ### Custom channels
///
/// ```
/// # use std::error::Error;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn Error>> {
/// use std::sync::Arc;
/// use stream_signer::{
///     file::Timestamp,
///     video::sign::ChunkSigner,
/// #   video::SignPipeline,
///     spec::Vec2u,
/// };
/// # use identity_iota::{did::DID, core::{json, FromJson}, credential::Subject};
/// # use testlibs::{
/// #     client::{get_client, get_resolver},
/// #     identity::TestIdentity,
/// #     issuer::TestIssuer,
/// #     test_video, videos,
/// # };
/// #
/// # let client = get_client();
/// # let issuer = TestIssuer::new(client.clone()).await?;
/// #
/// # let identity = TestIdentity::new(&issuer, |id| {
/// #     Subject::from_json_value(json!({
/// #       "id": id.as_str(),
/// #       "name": "Alice",
/// #       "degree": {
/// #         "type": "BachelorDegree",
/// #         "name": "Bachelor of Science and Arts",
/// #       },
/// #       "GPA": "4.0",
/// #     })).unwrap()
/// # })
/// # .await?;
///
/// let signer = Arc::new(identity);
///
/// let chunk_signer = ChunkSigner::build(Timestamp::from_millis(100), signer)
///     .with_channels(vec![1]);
/// #
/// # stream_signer::gst::init();
/// # let video = test_video(videos::BIG_BUNNY);
/// # let pipe = SignPipeline::build_from_path(&video).unwrap().build().unwrap();
/// # let mut iter = pipe.try_into_iter(()).unwrap();
/// # let frame_with_audio = iter.next().unwrap().unwrap();
///
/// // This is generated by the FrameManager based on this definition
/// let chunk_signer = chunk_signer.build(&frame_with_audio);
/// let msg = vec![0, 0, 0, /* ... */];
/// let signature = chunk_signer.sign(msg).await.unwrap();
///
/// assert_eq!(signature.channels, vec![1]);
/// # Ok(())
/// # }
/// ```
pub struct ChunkSigner<S: Signer> {
    /// The position of the embedding, if not given, we will assume the top
    /// right
    ///
    /// Note: If this is given, it is assumed you have defined the size
    pub pos: Vec2u,

    /// The given information to prove your credability as well as the
    /// information to create any signatures
    pub signer: Arc<S>,

    /// The size of the embedding, if not given, we will assume the size of
    /// the window.
    ///
    /// Note: If this is given, it is assumed you have defined the position
    pub size: Vec2u,
    pub start: Timestamp,

    /// If this is set, we will only return a reference of the definition
    pub is_ref: bool,

    /// The audio channels to include when signing, in the order that they will
    /// be added to the buffer to be signed. If [None] is given, then it assumes
    /// all buffers should be included
    pub channels: Vec<usize>,
}

impl<S: Signer> ChunkSigner<S> {
    /// Creates a new object, with pos and size set to None (assuming they
    /// will be defined later with [ChunkSignerBuilder::with_embedding])
    pub const fn build(start: Timestamp, signer: Arc<S>) -> ChunkSignerBuilder<S> {
        ChunkSignerBuilder {
            signer,
            pos: None,
            size: None,
            start,
            is_ref: false,
            channels: None,
        }
    }

    /// Signs a given stream and generates [ChunkSignature] which can be stored
    /// in the files
    pub async fn sign(self, msg: Vec<u8>) -> Result<ChunkSignature, JwkStorageDocumentError> {
        let presentation: PresentationOrId = if self.is_ref {
            PresentationOrId::new_ref(self.signer.get_presentation_id())
        } else {
            let jwt = self.signer.presentation().await?;
            PresentationOrId::new_def(self.signer.get_presentation_id(), jwt)
        };

        let signature = self.signer.sign(&msg).await?;

        Ok(ChunkSignature {
            pos: self.pos,
            size: self.size,
            channels: self.channels,
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
