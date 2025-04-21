mod utils;

use std::{error::Error, sync::Arc};

use futures::{StreamExt, TryStreamExt};
use identity_iota::{core::FromJson, credential::Subject, did::DID};
use serde_json::json;
use stream_signer::{
    spec::{MAX_CHUNK_LENGTH, MIN_CHUNK_LENGTH},
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
use utils::skip_loading;

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

    let res = pipe
        .sign_with(sign::IntervalController::build(
            Arc::new(identity),
            MAX_CHUNK_LENGTH as u32 + 1,
        ))?
        .try_collect::<Vec<_>>()
        .await;

    const LENGTH: usize = MAX_CHUNK_LENGTH + 1;

    assert!(
        matches!(
            res,
            Err(SigningError::Operation(
                SigOperationError::InvalidChunkSize(LENGTH)
            ))
        ),
        "{:?} reported invalid chunk size of {LENGTH}",
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

    let res = pipe
        .sign_with(sign::IntervalController::build(Arc::new(identity), 4))?
        .try_collect::<Vec<_>>()
        .await;

    const LENGTH: usize = 4;

    assert!(
        matches!(
            res,
            Err(SigningError::Operation(
                SigOperationError::InvalidChunkSize(LENGTH)
            ))
        ),
        "{:?} reported an invalid chunk size of {LENGTH}",
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
        .sign_with(sign::IntervalController::build(Arc::new(identity), 100))?
        .try_collect::<Vec<_>>()
        .await?;

    for length in [MIN_CHUNK_LENGTH - 1, MAX_CHUNK_LENGTH + 1] {
        // Secretly just change the length as technically we won't get to the
        // verification stage
        let signfile = signs
            .iter()
            .cloned()
            .map(|mut i| {
                i.stop = i.start + length as u32;
                i
            })
            .collect::<SignFile>();

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
                            "{s:?} marks itself as an invalid chunk length with length {length}ms"
                        );
                    }
                }
            })
            .await;

        assert!(count > 0, "We verified some chunks");
    }

    Ok(())
}
