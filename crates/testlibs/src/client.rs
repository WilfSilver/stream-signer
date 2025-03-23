//! Provides a simple implementation for a client which is in memory (meaning
//! that during tests no API calls are made)
use std::{collections::HashMap, sync::Arc};

use identity_iota::{did::CoreDID, document::CoreDocument, prelude::Resolver};
use iota_sdk::types::block::address::Hrp;
use rand::distr::SampleString;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

const METHOD: &str = "test";

/// A client which stores the core ids and their related core documents in
/// memory allowing for use within tests
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MemClient(HashMap<CoreDID, CoreDocument>);

impl MemClient {
    // Resolves some of the DIDs we are interested in.
    pub fn resolve(&self, did: &CoreDID) -> std::result::Result<CoreDocument, std::io::Error> {
        match self.0.get(did) {
            Some(d) => Ok(d.clone()),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Could not find {}", did),
            )),
        }
    }

    pub fn random_id(&self) -> CoreDID {
        // Assumption that we won't have any collisions (bit hopeful but eh)
        let id = rand::distr::Alphanumeric.sample_string(&mut rand::rng(), 32);
        let did = format!("did:{METHOD}:{id}");

        CoreDID::parse(&did).expect("DID failed to parse")
    }

    pub fn publish(&mut self, id: CoreDID, doc: CoreDocument) {
        self.0.insert(id, doc);
    }

    /// This gives an example Bech32 address, please make sure NOT to allow this
    /// to be requested
    pub fn get_bech32_hrp(&self) -> Hrp {
        Hrp::from_str_unchecked("4ef47f6eb681d5d9fa2f7e16336cd629303c635e8da51e425b76088be9c8744c")
    }
}

pub fn get_client() -> Arc<Mutex<MemClient>> {
    Arc::new(Mutex::new(MemClient::default()))
}

pub fn get_resolver(client: Arc<Mutex<MemClient>>) -> Resolver {
    let mut resolver = Resolver::<CoreDocument>::new();

    resolver.attach_handler(METHOD.to_owned(), move |did: CoreDID| {
        let future_client = client.clone();
        async move {
            let future_client = future_client.lock().await;
            future_client.resolve(&did)
        }
    });

    resolver
}
