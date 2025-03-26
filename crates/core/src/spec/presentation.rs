use identity_iota::credential::{Jwt, Presentation as IotaPresentation};
use serde::{Deserialize, Serialize};

pub type Presentation = IotaPresentation<Jwt>;

#[derive(Clone, Debug, Deserialize, Serialize, Eq)]
#[serde(untagged)]
pub enum PresentationOrId {
    Def { id: String, jwt: Jwt },
    Ref { id: String },
}

impl PresentationOrId {
    pub fn new_def<S: ToString>(id: S, jwt: Jwt) -> PresentationOrId {
        PresentationOrId::Def {
            id: id.to_string(),
            jwt,
        }
    }

    pub fn new_ref<S: ToString>(id: S) -> PresentationOrId {
        PresentationOrId::Ref { id: id.to_string() }
    }

    pub fn id(&self) -> &str {
        match self {
            PresentationOrId::Def { id, jwt: _ } => id,
            PresentationOrId::Ref { id } => id,
        }
    }
}

impl PartialEq for PresentationOrId {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}
