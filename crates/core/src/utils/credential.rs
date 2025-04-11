use identity_eddsa_verifier::EdDSAJwsVerifier;
use identity_iota::{
    core::Object,
    credential::{
        CompoundJwtPresentationValidationError, DecodedJwtCredential, DecodedJwtPresentation,
        FailFast, Jwt, JwtCredentialValidationOptions, JwtCredentialValidator,
        JwtCredentialValidatorUtils, JwtPresentationValidationOptions, JwtPresentationValidator,
        JwtPresentationValidatorUtils, JwtValidationError, SubjectHolderRelationship,
    },
    did::{CoreDID, DID},
    document::{verifiable::JwsVerificationOptions, DIDUrlQuery},
    prelude::Resolver,
    resolver::Error as ResolverError,
    verification::{jwk::Jwk, jws::Decoder},
};
use thiserror::Error;

use crate::spec::PresentationOrId;
use std::{collections::HashMap, ops::Deref, sync::Arc};

pub type Credential = DecodedJwtCredential;

#[derive(Error, Clone, Debug)]
#[error("Unknown key id used: {0}")]
pub struct UnknownKey(String);

/// Quick bridge to [SubjectState], allowing to use the type `Result<Subject, SubjectError>`
enum SubjectError {
    Validation(Vec<JwtValidationError>),
    Resolver(ResolverError),
}

impl From<CompoundJwtPresentationValidationError> for SubjectError {
    fn from(value: CompoundJwtPresentationValidationError) -> Self {
        Self::Validation(value.presentation_validation_errors)
    }
}

impl From<JwtValidationError> for SubjectError {
    fn from(value: JwtValidationError) -> Self {
        Self::Validation(vec![value])
    }
}

impl From<ResolverError> for SubjectError {
    fn from(value: ResolverError) -> Self {
        Self::Resolver(value)
    }
}

/// Stores information about the [Subject] and any result determined when trying
/// to validate a signature.
/// TODO: FIX
#[derive(Debug, Clone)]
pub enum SubjectState {
    Invalid(Vec<Arc<JwtValidationError>>),
    ResolverFailed(Arc<ResolverError>),
    Valid(Box<Subject>),
}

impl From<SubjectError> for SubjectState {
    #[inline]
    fn from(value: SubjectError) -> Self {
        match value {
            SubjectError::Resolver(e) => SubjectState::ResolverFailed(Arc::new(e)),
            SubjectError::Validation(e) => {
                SubjectState::Invalid(e.into_iter().map(Arc::new).collect::<Vec<_>>())
            }
        }
    }
}

impl From<Result<Subject, SubjectError>> for SubjectState {
    #[inline]
    fn from(value: Result<Subject, SubjectError>) -> Self {
        match value {
            Ok(signer) => Self::Valid(Box::new(signer)),
            Err(e) => e.into(),
        }
    }
}

/// Stores the information which is required to verify a signature for some individual or
/// organisation.
#[derive(Debug, Clone)]
pub struct Subject {
    creds: Vec<Credential>,
    pub public_key: Jwk,
}

impl PartialEq for Subject {
    fn eq(&self, other: &Self) -> bool {
        self.public_key == other.public_key
    }
}

impl Subject {
    /// Returns a list of verified credentials for the signer
    pub fn creds(&self) -> &[Credential] {
        &self.creds
    }
}

type IntMap = HashMap<String, SubjectState>;

/// Stores the defined credentials so it can be easily accessed later on
#[derive(Debug)]
pub struct CredentialStore {
    resolver: Arc<Resolver>,
    map: IntMap,
}

impl CredentialStore {
    /// Creates a new [CredentialStore] with the given [Resolver].
    ///
    /// ## Example
    ///
    /// ### With Iota
    ///
    /// ```no_run
    /// # use std::error::Error;
    /// #
    /// # const API_ENDPOINT: &str = "http://localhost";
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use iota_sdk::client::Client;
    /// use identity_iota::resolver::Resolver;
    /// use stream_signer::utils::CredentialStore;
    ///
    /// let client: Client = Client::builder()
    ///   .with_primary_node(API_ENDPOINT, None)?
    ///   .finish()
    ///   .await?;
    ///
    /// let mut resolver = Resolver::new();
    /// resolver.attach_iota_handler(client);
    ///
    /// let resolver = Arc::new(resolver);
    ///
    /// let store = CredentialStore::new(resolver);
    ///
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ### With Custom Client
    ///
    /// For extra examples see [Resolver::attach_handler]
    ///
    /// ```
    /// use identity_iota::did::CoreDID;
    /// use identity_iota::document::CoreDocument;
    /// use identity_iota::resolver::Resolver;
    /// use stream_signer::utils::CredentialStore;
    ///
    /// // A client that can resolve DIDs of our invented "foo" method.
    /// struct Client;
    ///
    /// impl Client {
    ///   // Resolves some of the DIDs we are interested in.
    ///   async fn resolve(&self, _did: &CoreDID) -> std::result::Result<CoreDocument, std::io::Error> {
    ///     todo!()
    ///   }
    /// }
    ///
    /// // This way we can essentially produce (cheap) clones of our client.
    /// let client = std::sync::Arc::new(Client {});
    ///
    /// // Get a clone we can move into a handler.
    /// let client_clone = client.clone();
    ///
    /// // Construct a resolver that resolves documents of type `CoreDocument`.
    /// let mut resolver = Resolver::<CoreDocument>::new();
    ///
    /// // Now we want to attach a handler that uses the client to resolve DIDs whose method is "foo".
    /// resolver.attach_handler("foo".to_owned(), move |did: CoreDID| {
    ///   // We want to resolve the did asynchronously, but since we do not know when it will be awaited we
    ///   // let the future take ownership of the client by moving a clone into the asynchronous block.
    ///   let future_client = client_clone.clone();
    ///   async move { future_client.resolve(&did).await }
    /// });
    ///
    /// let store = CredentialStore::new(&resolver);
    /// ```
    pub fn new(resolver: Arc<Resolver>) -> Self {
        Self {
            resolver,
            map: HashMap::default(),
        }
    }

    /// This checks if the given operation is a reference or definition and extracts the signer,
    /// either from the presentation in the [PresentationOrId::Def] or from the stored information
    /// in the case for the [PresentationOrId::Ref].
    ///
    /// Note that if a definition is given, we must make all the necessary calls to verify the
    /// presentation itself
    ///
    /// The returned [SubjectState] must then be check for how valid the credentials are
    pub async fn normalise(&mut self, opt: PresentationOrId) -> Result<&SubjectState, UnknownKey> {
        match opt {
            PresentationOrId::Ref { id } => self.get(&id).ok_or(UnknownKey(id)),
            PresentationOrId::Def { id, jwt: pres } => {
                self.map
                    .insert(id.clone(), self.validate_pres(pres).await.into());

                Ok(self.get(&id).unwrap())
            }
        }
    }

    /// Validates the given [Presentation] (as a [Jwt]) and correctly converts
    /// it into a [Subject] which can then be stored in the system
    ///
    /// A lot of the code has been taken from
    /// <https://wiki.iota.org/identity.rs/1.5/how-tos/verifiable-presentations/create-and-validate/?language=rust>
    async fn validate_pres(&self, pres_jwt: Jwt) -> Result<Subject, SubjectError> {
        // Resolve the holder's document.
        let holder_did: CoreDID = JwtPresentationValidatorUtils::extract_holder(&pres_jwt)?;
        let holder = self.resolver.resolve(&holder_did).await?;

        // Validate presentation. Note that this doesn't validate the included credentials.
        let presentation_validation_options = JwtPresentationValidationOptions::default()
            .presentation_verifier_options(JwsVerificationOptions::default());
        let presentation: DecodedJwtPresentation<Jwt> =
            JwtPresentationValidator::with_signature_verifier(EdDSAJwsVerifier::default())
                .validate(&pres_jwt, &holder, &presentation_validation_options)?;

        // This is safe to ignore all errors due to the presentation running the
        // same code
        let validation_item = Decoder::new()
            .decode_compact_serialization(pres_jwt.as_str().as_bytes(), None)
            .unwrap();

        let method_url_query: DIDUrlQuery<'_> = validation_item.kid().unwrap().into();

        let public_key: &Jwk = holder
            .resolve_method(method_url_query, None)
            .unwrap()
            .data()
            .try_public_key_jwk()
            .unwrap();

        // Concurrently resolve the issuers' documents.
        let jwt_credentials = &presentation.presentation.verifiable_credential;
        let issuers = jwt_credentials
            .iter()
            .map(JwtCredentialValidatorUtils::extract_issuer_from_jwt)
            .collect::<Result<Vec<CoreDID>, _>>()?;
        let issuers_documents = self.resolver.resolve_multiple(&issuers).await?;

        // Validate the credentials in the presentation.
        let credential_validator =
            JwtCredentialValidator::with_signature_verifier(EdDSAJwsVerifier::default());
        let validation_options = JwtCredentialValidationOptions::default()
            .subject_holder_relationship(
                holder_did.to_url().into(),
                SubjectHolderRelationship::AlwaysSubject,
            );

        let creds = jwt_credentials
            .iter()
            .enumerate()
            .map(|(index, jwt_vc)| {
                // SAFETY: Indexing should be fine since we extracted the DID from each credential and resolved it.
                let issuer_document = &issuers_documents[&issuers[index]];

                credential_validator
                    .validate::<_, Object>(
                        jwt_vc,
                        issuer_document,
                        &validation_options,
                        FailFast::FirstError,
                    )
                    .unwrap()
            })
            .collect::<Vec<_>>();

        Ok(Subject {
            creds,
            public_key: public_key.clone(),
        })
    }
}

impl Deref for CredentialStore {
    type Target = IntMap;
    fn deref(&self) -> &Self::Target {
        &self.map
    }
}
