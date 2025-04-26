use std::{error::Error, pin::pin, sync::Arc};

use constants::ONE_HUNDRED_MILLIS;
use futures::{StreamExt, TryStreamExt};
use identity_iota::{core::FromJson, credential::Subject, did::DID};
use serde_json::json;
use stream_signer::{
    video::{
        sign,
        verify::{InvalidSignatureError, SignatureState},
        SigOperationError, SigningError,
    },
    SignFile, SignPipeline,
};
use testlibs::{
    client::{get_client, get_resolver},
    identity::TestIdentity,
    issuer::TestIssuer,
    test_video, videos,
};
use utils::{sign_and_verify_int, skip_loading};

mod constants;
mod utils;

#[tokio::test]
async fn sign_and_verify_with_audio() -> Result<(), Box<dyn Error>> {
    sign_and_verify_int(videos::BIG_BUNNY_AUDIO, |i| {
        sign::IntervalController::build(i, ONE_HUNDRED_MILLIS)
    })
    .await
}

#[tokio::test]
async fn sign_and_verify_with_6_channels() -> Result<(), Box<dyn Error>> {
    sign_and_verify_int(videos::BIG_BUNNY_AUDIO_6C, |i| {
        sign::IntervalController::build(i, ONE_HUNDRED_MILLIS)
    })
    .await
}

#[tokio::test]
async fn sign_and_verify_no_channels() -> Result<(), Box<dyn Error>> {
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

    let filepath = test_video(videos::BIG_BUNNY_AUDIO_LEFT);

    let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

    let signfile = pipe
        .sign_with(
            sign::IntervalController::build(Arc::new(identity), ONE_HUNDRED_MILLIS)
                .with_channels(vec![]),
        )?
        .try_collect::<SignFile>()
        .await?;

    let filepath = test_video(videos::BIG_BUNNY_AUDIO_RIGHT);

    let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

    let mut count = 0;
    pipe.verify(resolver.clone(), signfile)?
        .for_each(|v| {
            let v = match v {
                Ok(v) => v,
                Err(e) => {
                    panic!("Frame was invalid: {e}");
                }
            };

            count += 1;

            async move {
                for s in &v.sigs {
                    if skip_loading(s).await {
                        continue;
                    }

                    assert!(
                        matches!(s, SignatureState::Verified(_)),
                        "{s:?} is still valid on a different video with different channel"
                    );
                }
            }
        })
        .await;

    assert!(count > 0, "Signatures have been validated");

    Ok(())
}

#[tokio::test]
async fn sign_with_left_verify_with_right() -> Result<(), Box<dyn Error>> {
    // Please note this requires lossless audio

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

    let filepath = test_video(videos::BIG_BUNNY_AUDIO_LEFT);

    let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

    let signs = pipe
        .sign_with(
            sign::IntervalController::build(Arc::new(identity), ONE_HUNDRED_MILLIS)
                .with_channels(vec![0]),
        )?
        .try_collect::<Vec<_>>()
        .await?;

    // Change the channels in the specification to be the other one
    let signfile = signs
        .iter()
        .cloned()
        .map(|mut i| {
            i.val.channels = vec![1];
            i
        })
        .collect::<SignFile>();

    let filepath = test_video(videos::BIG_BUNNY_AUDIO_RIGHT);

    let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

    let mut count = 0;
    pipe.verify(resolver.clone(), signfile)?
        .for_each(|v| {
            let v = match v {
                Ok(v) => v,
                Err(e) => {
                    panic!("Frame was invalid: {e}");
                }
            };

            count += 1;

            async move {
                for s in &v.sigs {
                    if skip_loading(s).await {
                        continue;
                    }

                    assert!(
                        matches!(s, SignatureState::Verified(_)),
                        "{s:?} is still valid on a different video with different channel"
                    );
                }
            }
        })
        .await;

    assert!(count > 0, "Signatures have been validated");

    Ok(())
}

#[tokio::test]
async fn sign_with_invalid_channel() -> Result<(), Box<dyn Error>> {
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

    let filepath = test_video(videos::BIG_BUNNY_AUDIO);

    let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

    let res = pipe
        .sign_with(
            sign::IntervalController::build(Arc::new(identity), ONE_HUNDRED_MILLIS)
                .with_channels(vec![0, 3]),
        )?
        .try_collect::<Vec<_>>()
        .await;

    assert!(
        matches!(
            &res,
            Err(SigningError::Operation(
                SigOperationError::InvalidChannels(c)
            )) if *c == vec![3]
        ),
        "{:?} reported an invalid channel of [3]",
        res.map(|mut v| v.pop())
    );

    Ok(())
}

#[tokio::test]
async fn verify_with_invalid_channel() -> Result<(), Box<dyn Error>> {
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

    let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

    let signs = pipe
        .sign_with(sign::IntervalController::build(
            Arc::new(identity),
            ONE_HUNDRED_MILLIS,
        ))?
        .try_collect::<Vec<_>>()
        .await
        .unwrap();

    let signfile = signs
        .iter()
        .cloned()
        .map(|mut i| {
            i.val.channels = vec![0, 3];
            i
        })
        .collect::<SignFile>();

    let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

    let mut count = 0;
    let mut stream = pin!(pipe.verify(resolver.clone(), signfile)?);
    while let Some(v) = stream.next().await {
        let v = match v {
            Ok(v) => v,
            Err(e) => {
                panic!("Frame was invalid: {e}");
            }
        };

        let mut validated = false;
        for s in &v.sigs {
            if skip_loading(s).await {
                continue;
            }

            count += 1;

            assert!(
                matches!(
                    s,
                    SignatureState::Invalid(
                        InvalidSignatureError::Operation(
                            SigOperationError::InvalidChannels(
                                c,
                            ),
                        ),
                    ) if *c == vec![3]
                ),
                "{s:?} reported an invalid channel of [3]",
            );
            // Validate one to make the test much faster
            validated = true;
        }

        if validated {
            break;
        }
    }

    assert!(count > 0, "We verified some chunks");

    Ok(())
}
