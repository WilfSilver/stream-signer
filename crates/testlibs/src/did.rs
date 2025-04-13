use std::collections::BTreeMap;

use anyhow::Result;
use identity_iota::{
    did::CoreDID,
    document::CoreDocument,
    storage::{JwkDocumentExt, JwkMemStore},
    verification::{MethodScope, jws::JwsAlgorithm},
};
use iota_sdk::{
    client::{
        api::GetAddressesOptions,
        secret::{SecretManager, mnemonic::MnemonicSecretManager},
    },
    crypto::keys::bip39,
    types::block::address::{Address, Bech32Address, Hrp},
};
use tokio::sync::MutexGuard;

use super::{client::MemClient, issuer::MemStorage};

/// Creates a core document with the given id
pub async fn create_did_document(
    id: CoreDID,
    storage: &MemStorage,
) -> Result<(CoreDocument, String)> {
    let mut document = CoreDocument::builder(BTreeMap::new()).id(id).build()?;

    let fragment: String = document
        .generate_method(
            storage,
            JwkMemStore::ED25519_KEY_TYPE,
            JwsAlgorithm::EdDSA,
            None,
            MethodScope::VerificationMethod,
        )
        .await?;

    Ok((document, fragment))
}

pub async fn create_did(
    mut client: MutexGuard<'_, MemClient>,
    secret_manager: &SecretManager,
    storage: &MemStorage,
) -> Result<(Address, CoreDocument, String)> {
    let address: Address = get_address(&client, secret_manager).await?.into();

    let id = client.random_id();

    let (doc, frag) = create_did_document(id.clone(), storage).await?;

    client.publish(id, doc.clone());

    Ok((address, doc, frag))
}

pub fn get_secret_manager() -> Result<SecretManager> {
    let random: [u8; 32] = rand::random();
    let mnemonic = bip39::wordlist::encode(random.as_ref(), &bip39::wordlist::ENGLISH)
        .map_err(|err| anyhow::anyhow!(format!("{err:?}")))?;

    let res = MnemonicSecretManager::try_from_mnemonic(mnemonic)?;

    Ok(res.into())
}

/// Initializes the [`SecretManager`] with a new mnemonic, if necessary,
/// and generates an address from the given [`SecretManager`].
pub async fn get_address(
    client: &MutexGuard<'_, MemClient>,
    secret_manager: &SecretManager,
) -> Result<Bech32Address> {
    let bech32_hrp: Hrp = client.get_bech32_hrp();
    let address: Bech32Address = secret_manager
        .generate_ed25519_addresses(
            GetAddressesOptions::default()
                .with_range(0..1)
                .with_bech32_hrp(bech32_hrp),
        )
        .await?[0];

    Ok(address)
}
