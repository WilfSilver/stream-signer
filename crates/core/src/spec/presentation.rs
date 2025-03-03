use identity_iota::credential::{Jwt, Presentation as IotaPresentation};
use serde::{Deserialize, Serialize};

pub type Presentation = IotaPresentation<Jwt>;

/// TODO: Swap Eq to be checking the id, not the whole object please
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum PresentationOrId {
    Def(PresentationDefinition),
    Ref(PresentationReference),
}

impl From<PresentationReference> for PresentationOrId {
    fn from(value: PresentationReference) -> Self {
        Self::Ref(value)
    }
}

impl From<PresentationDefinition> for PresentationOrId {
    fn from(value: PresentationDefinition) -> Self {
        Self::Def(value)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PresentationReference {
    pub id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PresentationDefinition {
    pub id: String,
    pub pres: Jwt,
}
