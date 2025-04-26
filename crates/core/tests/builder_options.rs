mod constants;
mod utils;

use std::{error::Error, sync::Arc};

use constants::ONE_HUNDRED_MILLIS;
use futures::{StreamExt, TryStreamExt};
use identity_iota::{core::FromJson, credential::Subject, did::DID};
use serde_json::json;
use stream_signer::{
    time::Timestamp,
    utils::UnknownKey,
    video::{
        sign,
        verify::{InvalidSignatureError, SignatureState},
        SigOperationError,
    },
    SignFile, SignPipeline,
};
use testlibs::{
    client::{get_client, get_resolver},
    identity::TestIdentity,
    issuer::TestIssuer,
    test_video, videos,
};
use utils::skip_loading;

#[tokio::test]
async fn sign_and_verify_with_start_offset() -> Result<(), Box<dyn Error>> {
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

    let start = Timestamp::from_secs(2);
    let pipe = SignPipeline::build_from_path(&filepath)
        .unwrap()
        .with_start_offset(start)
        .build()?;

    let signfile = pipe
        .sign_with(sign::IntervalController::build(
            Arc::new(identity),
            ONE_HUNDRED_MILLIS,
        ))?
        .try_collect::<SignFile>()
        .await?;

    let pipe = SignPipeline::build_from_path(&filepath)
        .unwrap()
        .with_start_offset(start)
        .build()?;

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

            async move {
                for s in &v.sigs {
                    if skip_loading(s).await {
                        return;
                    }

                    assert!(
                        matches!(s, SignatureState::Verified(_)),
                        "{s:?} verified correctly"
                    );
                }
            }
        })
        .await;

    assert!(count > 0, "We verified some chunks");

    Ok(())
}

#[tokio::test]
async fn sign_with_start_offset() -> Result<(), Box<dyn Error>> {
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

    let pipe = SignPipeline::build_from_path(&filepath)
        .unwrap()
        .with_start_offset(Timestamp::from_secs(2))
        .build()?;

    let signfile = pipe
        .sign_with(sign::IntervalController::build(
            Arc::new(identity),
            ONE_HUNDRED_MILLIS,
        ))?
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

            async move {
                for s in &v.sigs {
                    if skip_loading(s).await {
                        return;
                    }

                    assert!(
                        matches!(s, SignatureState::Verified(_)),
                        "{s:?} verified correctly"
                    );
                }
            }
        })
        .await;

    assert!(count > 0, "We verified some chunks");

    Ok(())
}

#[tokio::test]
async fn sign_with_start_offset_and_audio() -> Result<(), Box<dyn Error>> {
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

    let filepath = test_video(videos::BIG_BUNNY_AUDIO);

    let pipe = SignPipeline::build_from_path(&filepath)
        .unwrap()
        .with_start_offset(Timestamp::from_secs(2))
        .build()?;

    let signfile = pipe
        .sign_with(sign::IntervalController::build(
            Arc::new(identity),
            ONE_HUNDRED_MILLIS,
        ))?
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

            async move {
                for s in &v.sigs {
                    if skip_loading(s).await {
                        return;
                    }

                    assert!(
                        matches!(s, SignatureState::Verified(_)),
                        "{s:?} verified correctly"
                    );
                }
            }
        })
        .await;

    assert!(count > 0, "We verified some chunks");

    Ok(())
}

#[tokio::test]
async fn sign_and_verify_with_diff_start_offset() -> Result<(), Box<dyn Error>> {
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

    let signfile = pipe
        .sign_with(sign::IntervalController::build(
            Arc::new(identity),
            ONE_HUNDRED_MILLIS,
        ))?
        .try_collect::<SignFile>()
        .await?;

    let pipe = SignPipeline::build_from_path(&filepath)
        .unwrap()
        .with_start_offset(Timestamp::from_secs_f64(2.05)) // Specifically to break the 100ms chunks
        .build()?;

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

                async move {
                    let time = v.state.frame.get_timestamp();
                    let expected_invalid = time < Timestamp::from_secs_f64(2.1);

                    for s in &v.sigs {
                        if skip_loading(s).await {
                            return;
                        }

                        if expected_invalid {
                            assert!(
                                matches!(
                                    s,
                                    SignatureState::Invalid(
                                        InvalidSignatureError::Operation(
                                            SigOperationError::OutOfRange(
                                                _, _,
                                            ),
                                        ),
                                    )
                                ),
                                "{s:?} the first few signatures are invalid due to not having enough data"
                            );
                        } else {
                            // Due to the current code we cannot handle this use case
                            // but this could be changed in the future
                            assert!(
                                matches!(
                                    s,
                                    SignatureState::Invalid(
                                        InvalidSignatureError::Operation(
                                            SigOperationError::UnknownCredential(
                                                UnknownKey(_),
                                            ),
                                        )
                                    )
                                ),
                                "{s:?} has an unknown key"
                            );
                        }
                    }
                }
            })
            .await;

    assert!(count > 0, "We verified some chunks");

    Ok(())
}
