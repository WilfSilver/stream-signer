pub use identity_credential;
pub use identity_credential::credential::Credential;
use serde::{Deserialize, Serialize};

/// Credential information of either the definition (with all the information),
/// or the reference to a defintion (just using the id). Note with the
/// definition an id is required if you wish to reference it later
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum CredentialOpt {
    Ref(CredentialId),
    Definition(Credential),
}

impl From<Credential> for CredentialOpt {
    fn from(value: Credential) -> Self {
        CredentialOpt::Definition(value)
    }
}

impl From<CredentialId> for CredentialOpt {
    fn from(value: CredentialId) -> Self {
        CredentialOpt::Ref(value)
    }
}

/// Stores the id which references a previously defiend definition of the
/// credential
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CredentialId {
    pub id: String,
}
