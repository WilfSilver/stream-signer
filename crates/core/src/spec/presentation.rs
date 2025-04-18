use identity_iota::credential::{Jwt, Presentation as IotaPresentation};
use serde::{Deserialize, Serialize};

/// The expected type of the Verifiable Presentation once it is decoded from
/// its [Jwt] (which is the format it is in within the specification)
pub type Presentation = IotaPresentation<Jwt>;

/// This is the type for `presentation` within the specification.
///
/// It allows for the [PresentationOrId::Def] to be defined once in the source
/// file, with a given [Self::id] and then [PresentationOrId::Ref] to be used
/// later on, for example:
///
/// ```json
/// {
///   // ...
///   "presentation": {
///     "id": "did:test:VNkn4Ma2Chc1gAK7iRRcPDOYoBSsU8M4",
///     "pres": "eyJraWQiOiJkaWQ6dGVzdDpWTmtuNE1hMkNoYzFnQUs3aVJSY1BET1lvQlNzVThNNCNQaDJfYUpzem4wVnZTelgyR29DZVFfYndmZHJrcl9PMnZucXZrcE5vRWlJIiwidHlwIjoiSldUIiwiYWxnIjoiRWREU0EifQ.eyJpc3MiOiJkaWQ6dGVzdDpWTmtuNE1hMkNoYzFnQUs3aVJSY1BET1lvQlNzVThNNCIsIm5iZiI6MTc0MzAwNzc1MiwidnAiOnsiQGNvbnRleHQiOiJodHRwczovL3d3dy53My5vcmcvMjAxOC9jcmVkZW50aWFscy92MSIsInR5cGUiOiJWZXJpZmlhYmxlUHJlc2VudGF0aW9uIiwidmVyaWZpYWJsZUNyZWRlbnRpYWwiOlsiZXlKcmFXUWlPaUprYVdRNmRHVnpkRHBsTkhOdFRUUjRjbVJMZEhwUU16TkJhVzVPUm5ZMk9Fa3hTMGRZVnpoSmVDTTJiR1J5TkRoVE5EaHNkbFF6VUhsMGJUTk9VRGxLV1V4eFVYQk5SSEZ0UzFOSVYydGhaamxxWTBFd0lpd2lkSGx3SWpvaVNsZFVJaXdpWVd4bklqb2lSV1JFVTBFaWZRLmV5SnBjM01pT2lKa2FXUTZkR1Z6ZERwbE5ITnRUVFI0Y21STGRIcFFNek5CYVc1T1JuWTJPRWt4UzBkWVZ6aEplQ0lzSW01aVppSTZNVGMwTXpBd056YzFNU3dpYW5ScElqb2lhSFIwY0hNNkx5OXNiMk5oYkdodmMzUXZZM0psWkdWdWRHbGhiSE12UlRkSVZHNWxaMDFYV1hCRFpEVnJSMnB5WjBkWU5FbFNUR2RNVnpaWFRFZ2lMQ0p6ZFdJaU9pSmthV1E2ZEdWemREcFdUbXR1TkUxaE1rTm9ZekZuUVVzM2FWSlNZMUJFVDFsdlFsTnpWVGhOTkNJc0luWmpJanA3SWtCamIyNTBaWGgwSWpvaWFIUjBjSE02THk5M2QzY3Vkek11YjNKbkx6SXdNVGd2WTNKbFpHVnVkR2xoYkhNdmRqRWlMQ0owZVhCbElqcGJJbFpsY21sbWFXRmliR1ZEY21Wa1pXNTBhV0ZzSWl3aVZXNXBkbVZ5YzJsMGVVUmxaM0psWlVOeVpXUmxiblJwWVd3aVhTd2lZM0psWkdWdWRHbGhiRk4xWW1wbFkzUWlPbnNpUjFCQklqb2lOQzR3SWl3aVpHVm5jbVZsSWpwN0luUjVjR1VpT2lKQ1lXTm9aV3h2Y2tSbFozSmxaU0lzSW01aGJXVWlPaUpDWVdOb1pXeHZjaUJ2WmlCVFkybGxibU5sSUdGdVpDQkJjblJ6SW4wc0ltNWhiV1VpT2lKQmJHbGpaU0o5ZlgwLnRPRlNJSm9YS0JmMk1IR2lPdjJsNzExOXFNU2xEVzJIVWJENjZQSk5RdWlWZ2N4SkdUV2RFcWw3QU5pQjNZbGw3ZkhGb3VJR19FNlNRWTRUbUw2ZkF3Il19fQ.iNgUIRGMKqAo1LaEIe2edn2F07TH1lVIoXm2LZsCkzkYXsQOKrM9TPALqrdeNEvcg_4tMXt44IXOXmtGSAI-CA"
///   },
///   // ...
/// }
/// ```
///
/// And then later on...
///
/// ```json
/// {
///   // ...
///   "presentation": {
///     "id": "did:test:VNkn4Ma2Chc1gAK7iRRcPDOYoBSsU8M4"
///   },
///   // ...
/// }
/// ```
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
