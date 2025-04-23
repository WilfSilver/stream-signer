use anyhow::Result;
use identity_iota::credential::{Credential, Jwt};
use identity_iota::document::CoreDocument;
use iota_sdk::types::block::address::Address;
use std::sync::Arc;
use tokio::sync::Mutex;

use identity_iota::storage::{
    JwkDocumentExt, JwkMemStore, JwsSignatureOptions, KeyIdMemstore, Storage,
};
use iota_sdk::client::secret::SecretManager;

use super::client::MemClient;
use super::did;

pub type MemStorage = Storage<JwkMemStore, KeyIdMemstore>;

pub struct TestIssuer {
    client: Arc<Mutex<MemClient>>,
    _manager: SecretManager,
    storage: MemStorage,
    pub document: CoreDocument,
    pub fragment: String,
}

impl TestIssuer {
    pub async fn new(client: Arc<Mutex<MemClient>>) -> Result<Self> {
        let manager: SecretManager = did::get_secret_manager()?;
        let storage: MemStorage = MemStorage::new(JwkMemStore::new(), KeyIdMemstore::new());

        let (_, document, fragment) = async {
            let client = client.lock().await;
            did::create_did(client, &manager, &storage).await
        }
        .await?;

        let issuer = Self {
            client,
            _manager: manager,
            storage,
            document,
            fragment,
        };

        Ok(issuer)
    }

    pub async fn create_credential_jwt(&self, cred: &Credential) -> Result<Jwt> {
        Ok(self
            .document
            .create_credential_jwt(
                cred,
                &self.storage,
                &self.fragment,
                &JwsSignatureOptions::default(),
                None,
            )
            .await?)
    }

    pub async fn create_did(
        &self,
        secret_manager: &SecretManager,
        storage: &MemStorage,
    ) -> Result<(Address, CoreDocument, String)> {
        let client = self.client.lock().await;
        did::create_did(client, secret_manager, storage).await
    }
}
