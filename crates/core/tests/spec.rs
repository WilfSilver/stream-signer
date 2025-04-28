mod constants;
mod utils;

use std::{error::Error, pin::pin, sync::Arc, time::Duration};

use constants::ONE_HUNDRED_MILLIS;
use futures::{StreamExt, TryStreamExt};
use identity_iota::{core::FromJson, credential::Subject, did::DID};
use serde_json::json;
use stream_signer::{
    SignFile, SignPipeline,
    spec::{MAX_CHUNK_LENGTH, MIN_CHUNK_LENGTH},
    video::{
        SigOperationError, SigningError, sign,
        verify::{InvalidSignatureError, SignatureState},
    },
};
use testlibs::{
    client::{get_client, get_resolver},
    identity::TestIdentity,
    issuer::TestIssuer,
    test_video, videos,
};
use utils::skip_loading;

const ONE_MILLI: Duration = Duration::from_millis(1);

#[tokio::test]
async fn sign_too_large_chunk() -> Result<(), Box<dyn Error>> {
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

    let filepath = test_video(videos::BIG_BUNNY_LONG);

    let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

    let length = MAX_CHUNK_LENGTH + ONE_MILLI;

    let res = pipe
        .sign_with(sign::IntervalController::build(Arc::new(identity), length))?
        .try_collect::<Vec<_>>()
        .await;

    // Add the length of a frame due to how IntervalController works
    let expected_chunk_size = MAX_CHUNK_LENGTH + Duration::from_millis(33);

    assert!(
        matches!(
            res,
            Err(SigningError::Operation(
                SigOperationError::InvalidChunkSize(l)
            )) if l == expected_chunk_size
        ),
        "{:?} reported invalid chunk size of {length:.2?}",
        res.map(|mut v| v.pop())
    );

    Ok(())
}

#[tokio::test]
async fn sign_too_small_chunk() -> Result<(), Box<dyn Error>> {
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

    const LENGTH: Duration = Duration::from_millis(4);

    let res = pipe
        .sign_with(sign::IntervalController::build(Arc::new(identity), LENGTH))?
        .try_collect::<Vec<_>>()
        .await;

    // Add the length of a frame and minus some offset due to how IntervalController
    // works
    const EXPECTED_LENGTH: Duration = Duration::from_millis(36);

    assert!(
        matches!(
            res,
            Err(SigningError::Operation(
                SigOperationError::InvalidChunkSize(EXPECTED_LENGTH)
            ))
        ),
        "{:?} reported an invalid chunk size of {LENGTH:?}",
        res.map(|mut v| v.pop())
    );

    Ok(())
}

#[tokio::test]
async fn verify_with_invalid_chunk_length() -> Result<(), Box<dyn Error>> {
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

    let filepath = test_video(videos::BIG_BUNNY_LONG);

    let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

    let signs = pipe
        .sign_with(sign::IntervalController::build(
            Arc::new(identity),
            ONE_HUNDRED_MILLIS,
        ))?
        .try_collect::<Vec<_>>()
        .await?;

    for length in [MIN_CHUNK_LENGTH - ONE_MILLI, MAX_CHUNK_LENGTH + ONE_MILLI] {
        // Secretly just change the length as technically we won't get to the
        // verification stage
        let signfile = signs
            .iter()
            .cloned()
            .map(|mut i| {
                i.stop = i.start + length.as_millis() as u32;
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
                    // Give it time to calculate the signature as this takes
                    // a while
                    tokio::spawn(tokio::time::sleep(Duration::from_millis(100)))
                        .await
                        .unwrap();
                    continue;
                }

                count += 1;

                assert!(
                    matches!(
                        s,
                        SignatureState::Invalid(
                            InvalidSignatureError::Operation(
                                SigOperationError::InvalidChunkSize(
                                    elength,
                                ),
                            ),
                        ) if *elength == length
                    ),
                    "{s:?} marks itself as an invalid chunk length with length {length:.2?}"
                );
                // Validate one to make the test much faster
                validated = true;
            }

            if validated {
                break;
            }
        }

        assert!(count > 0, "We verified some chunks");
    }

    Ok(())
}
