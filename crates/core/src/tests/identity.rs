use anyhow::Result;
use identity_iota::{
    core::Url,
    credential::{Credential, CredentialBuilder, Jwt, Presentation, PresentationBuilder, Subject},
    did::{CoreDID, DID},
    document::CoreDocument,
    storage::{JwkMemStore, KeyIdMemstore},
};
use iota_sdk::client::secret::SecretManager;
use rand::distr::SampleString;

#[cfg(feature = "signing")]
use crate::video::SignerInfo;

use super::{
    did,
    issuer::{MemStorage, TestIssuer},
};

pub struct TestIdentity {
    manager: SecretManager,
    storage: MemStorage,
    credential: Credential,
    jwt: Jwt,
    pub document: CoreDocument,
    pub fragment: String,
}

impl TestIdentity {
    pub async fn new<F>(issuer: &TestIssuer, gen_subject: F) -> Result<Self>
    where
        F: Fn(&CoreDID) -> Subject,
    {
        let manager: SecretManager = did::get_secret_manager()?;
        let storage: MemStorage = MemStorage::new(JwkMemStore::new(), KeyIdMemstore::new());

        let (_, document, fragment) = issuer.create_did(&manager, &storage).await?;

        let id = rand::distr::Alphanumeric.sample_string(&mut rand::rng(), 32);

        let subject = gen_subject(document.id());
        let credential: Credential = CredentialBuilder::default()
            .id(Url::parse(&format!("https://localhost/credentials/{id}"))?)
            .issuer(Url::parse(issuer.document.id().as_str())?)
            .type_("UniversityDegreeCredential")
            .subject(subject)
            .build()?;

        let jwt = issuer.create_credential_jwt(&credential).await?;

        Ok(Self {
            credential,
            manager,
            storage,
            document,
            fragment,
            jwt,
        })
    }

    pub fn build_presentation(&self) -> Result<Presentation<Jwt>> {
        Ok(
            PresentationBuilder::new(self.document.id().to_url().into(), Default::default())
                .credential(self.jwt.clone())
                .build()?,
        )
    }

    #[cfg(feature = "signing")]
    pub fn gen_signer_info(&self) -> Result<SignerInfo<'_, JwkMemStore, KeyIdMemstore>> {
        Ok(SignerInfo {
            document: &self.document,
            presentation: self.build_presentation()?,
            storage: &self.storage,
            fragment: &self.fragment,
        })
    }
}
