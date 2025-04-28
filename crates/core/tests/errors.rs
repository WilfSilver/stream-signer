mod constants;
mod utils;

use std::{error::Error, sync::Arc};

use constants::ONE_HUNDRED_MILLIS;
use futures::{StreamExt, TryStreamExt};
use identity_iota::{core::FromJson, credential::Subject, did::DID};
use serde_json::json;
use stream_signer::{
    SignFile, SignPipeline,
    video::{sign, verify::SignatureState},
};
use testlibs::{
    client::{get_client, get_resolver},
    identity::TestIdentity,
    issuer::TestIssuer,
    test_video, videos,
};
use utils::skip_loading;

#[tokio::test]
async fn verify_unresolvable() -> Result<(), Box<dyn Error>> {
    gst::init()?;

    let client = get_client();
    let issuer = TestIssuer::new(client.clone()).await?;

    let identity = TestIdentity::new(&issuer, |id| {
        Subject::from_json_value(json!({
          "id": id.as_str(),
          "name": "Alice",
          "degree": {
            "type": "BachelorDegree",
            "name": "Bachelor of Science and Arts",
          },
          "GPA": "4.0",
        }))
        .expect("Invalid subject")
    })
    .await?;

    let filepath = test_video(videos::BIG_BUNNY);

    let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

    let signfile = pipe
        .sign_with(sign::IntervalController::build(
            Arc::new(identity),
            ONE_HUNDRED_MILLIS,
        ))?
        .try_collect::<SignFile>()
        .await?;

    // Use a separate client for verification so that it has no knowledge
    // of the created identity
    let vclient = get_client();
    let resolver = get_resolver(vclient);
    let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

    let mut count = 0;
    pipe.verify(resolver, signfile)?
        .for_each(|v| {
            count += 1;

            let v = match v {
                Ok(v) => v,
                Err(e) => {
                    panic!("Frame was invalid: {e}");
                }
            };

            async move {
                for s in &v.sigs {
                    if skip_loading(s).await {
                        return;
                    }

                    assert!(
                        matches!(s, SignatureState::Unresolved(_)),
                        "{s:?} was unresolved"
                    );
                }
            }
        })
        .await;

    assert!(count > 0, "We verified some chunks");

    Ok(())
}
