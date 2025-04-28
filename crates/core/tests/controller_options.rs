mod constants;
mod utils;

use std::{error::Error, sync::Arc};

use constants::ONE_HUNDRED_MILLIS;
use futures::{StreamExt, TryStreamExt};
use identity_iota::{core::FromJson, credential::Subject, did::DID};
use serde_json::json;
use stream_signer::{
    SignFile, SignPipeline,
    time::Timestamp,
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
async fn sign_and_verify_with_range() -> Result<(), Box<dyn Error>> {
    gst::init()?;

    let client = get_client();
    let issuer = TestIssuer::new(client.clone()).await?;
    let resolver = get_resolver(client);

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

    let range = Timestamp::from_millis(105)..Timestamp::from_millis(110);
    let controller = sign::IntervalController::build(Arc::new(identity), ONE_HUNDRED_MILLIS)
        .with_range(range.clone());

    let signfile = pipe
        .sign_with(controller)?
        .try_collect::<SignFile>()
        .await?;

    let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

    let mut count = 0;
    pipe.verify(resolver.clone(), signfile)?
        .for_each(|v| {
            count += 1;

            let v = match v {
                Ok(v) => v,
                Err(e) => {
                    panic!("Frame was invalid: {e}");
                }
            };

            let range = range.clone();
            async move {
                if range.contains(&v.state.time.start()) {
                    assert_eq!(v.sigs.len(), 1, "Signatures outside the range");
                    for s in &v.sigs {
                        if skip_loading(s).await {
                            return;
                        }

                        assert!(
                            matches!(s, SignatureState::Verified(_)),
                            "Within the range {s:?} verified correctly"
                        );
                    }
                } else {
                    assert_eq!(v.sigs.len(), 0, "No signatures outside the range");
                }
            }
        })
        .await;

    assert!(count > 0, "We verified some chunks");

    Ok(())
}

// TODO: Use all other controllers
