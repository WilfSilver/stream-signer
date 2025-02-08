use crate::spec::CredentialOpt;
use identity_credential::credential::Credential;
use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};

/// Stores the defined credentials so it can be easily accessed later on
#[derive(Debug, Default)]
pub struct CredentialStore(HashMap<String, Credential>);

pub enum CredentialError {
    Invalid(&'static str),
    Unknown(String),
}

impl CredentialStore {
    pub fn normalise(&mut self, opt: CredentialOpt) -> Result<&Credential, CredentialError> {
        match opt {
            CredentialOpt::Ref(cred) => self.get(&cred.id).ok_or(CredentialError::Unknown(cred.id)),
            CredentialOpt::Definition(def) => {
                let id = def
                    .id
                    .clone()
                    .ok_or(CredentialError::Invalid(
                        "No id has been specified for the credential",
                    ))?
                    .to_string();
                self.insert(id.clone(), def);
                Ok(self.get(&id).unwrap())
            }
        }
    }
}

impl Deref for CredentialStore {
    type Target = HashMap<String, Credential>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for CredentialStore {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
