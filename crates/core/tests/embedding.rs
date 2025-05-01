mod constants;
mod utils;

use std::{error::Error, sync::Arc};

use constants::ONE_HUNDRED_MILLIS;
use futures::{StreamExt, TryStreamExt};
use identity_iota::{core::FromJson, credential::Subject, did::DID};
use serde_json::json;
use stream_signer::{
    SignFile, SignPipeline,
    spec::Vec2u,
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
use utils::{sign_and_verify_int, skip_loading};

#[tokio::test]
async fn sign_and_verify_embedding() -> Result<(), Box<dyn Error>> {
    sign_and_verify_int(videos::BIG_BUNNY, |i| {
        sign::IntervalController::build(i, ONE_HUNDRED_MILLIS)
            .with_embedding(Vec2u::new(10, 10), Vec2u::new(100, 100))
    })
    .await
}

#[tokio::test]
async fn sign_with_video_verify_with_embedded() -> Result<(), Box<dyn Error>> {
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

    // We sadly have to use a custom source video due to discrepencies from how
    // (seemingly) GStreamer reads lossless video formats
    let filepath = test_video(videos::BIG_BUNNY_EMBED_SRC);

    let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

    let signs = pipe
        .sign_with(sign::IntervalController::build(
            Arc::new(identity),
            ONE_HUNDRED_MILLIS,
        ))?
        .try_collect::<Vec<_>>()
        .await?;

    // Change the channels in the specification to be the other one
    let signfile = signs
        .iter()
        .cloned()
        .map(|mut i| {
            i.val.pos = Vec2u::new(24, 24);
            i
        })
        .collect::<SignFile>();

    let filepath = test_video(videos::BIG_BUNNY_EMBED);

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
async fn sign_with_too_large_embedding() -> Result<(), Box<dyn Error>> {
    gst::init()?;

    let client = get_client();
    let issuer = TestIssuer::new(client.clone()).await?;

    let identity = Arc::new(
        TestIdentity::new(&issuer, |id| {
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
        .await?,
    );

    let filepath = test_video(videos::BIG_BUNNY);

    for size in [(1000, 50), (50, 1000), (1000, 1000)]
        .into_iter()
        .map(Vec2u::from)
    {
        let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

        let ctrl = sign::IntervalController::build(identity.clone(), ONE_HUNDRED_MILLIS)
            .with_embedding(Vec2u::new(0, 0), size);

        let res = pipe.sign_with(ctrl)?.try_collect::<SignFile>().await;

        const POS: Vec2u = Vec2u::new(0, 0);

        assert!(
            matches!(
                res,
                Err(SigningError::Operation(SigOperationError::InvalidCrop(
                    POS, esize
                ))) if esize == size
            ),
            "{:?} responded as an invalid crop of position {POS:?} and size {size:?}",
            res.map(|s| s.into_iter().next())
        );
    }

    Ok(())
}

#[tokio::test]
async fn sign_with_too_small_embedding() -> Result<(), Box<dyn Error>> {
    gst::init()?;

    let client = get_client();
    let issuer = TestIssuer::new(client.clone()).await?;

    let identity = Arc::new(
        TestIdentity::new(&issuer, |id| {
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
        .await?,
    );

    let filepath = test_video(videos::BIG_BUNNY);

    for size in [(0, 50), (50, 0), (0, 0)].into_iter().map(Vec2u::from) {
        let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

        let ctrl = sign::IntervalController::build(identity.clone(), ONE_HUNDRED_MILLIS)
            .with_embedding(Vec2u::new(0, 0), size);

        let res = pipe.sign_with(ctrl)?.try_collect::<SignFile>().await;

        const POS: Vec2u = Vec2u::new(0, 0);
        assert!(
            matches!(
                res,
                Err(SigningError::Operation(SigOperationError::InvalidCrop(
                    POS, esize
                ))) if esize == size
            ),
            "{:?} responded as an invalid crop of position {POS:?} and size {size:?}",
            res.map(|s| s.into_iter().next())
        );
    }

    Ok(())
}

#[tokio::test]
async fn sign_with_too_large_position() -> Result<(), Box<dyn Error>> {
    gst::init()?;

    let client = get_client();
    let issuer = TestIssuer::new(client.clone()).await?;

    let identity = Arc::new(
        TestIdentity::new(&issuer, |id| {
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
        .await?,
    );

    let filepath = test_video(videos::BIG_BUNNY);

    for pos in [(1000, 50), (50, 1000), (1000, 1000)]
        .into_iter()
        .map(Vec2u::from)
    {
        let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

        let ctrl = sign::IntervalController::build(identity.clone(), ONE_HUNDRED_MILLIS)
            .with_embedding(pos, Vec2u::new(1, 1));

        let res = pipe.sign_with(ctrl)?.try_collect::<SignFile>().await;

        const SIZE: Vec2u = Vec2u::new(1, 1);
        assert!(
            matches!(
                res,
                Err(SigningError::Operation(SigOperationError::InvalidCrop(
                    epos, SIZE
                ))) if epos == pos
            ),
            "{:?} responded as an invalid crop of position {pos:?} and size {SIZE:?}",
            res.map(|s| s.into_iter().next())
        );
    }

    Ok(())
}

#[tokio::test]
async fn verify_with_invalid_embedding() -> Result<(), Box<dyn Error>> {
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

    let signs = pipe
        .sign_with(sign::IntervalController::build(
            Arc::new(identity),
            ONE_HUNDRED_MILLIS,
        ))?
        .try_collect::<Vec<_>>()
        .await?;

    for size in [
        (0, 50),
        (50, 0),
        (0, 0),
        (1000, 50),
        (50, 1000),
        (1000, 1000),
    ]
    .into_iter()
    .map(Vec2u::from)
    {
        // Secretly just change the size as technically we won't get to the
        // verification stage
        let signfile = signs
            .iter()
            .cloned()
            .map(|mut i| {
                i.val.size = size;
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
                                        SigOperationError::InvalidCrop(
                                            Vec2u { x: 0, y: 0},
                                            esize,
                                        ),
                                    ),
                                ) if *esize == size
                            ),
                            "{s:?} marks itself as an invalid crop with size {size:?}"
                        );
                    }
                }
            })
            .await;

        assert!(count > 0, "We verified some chunks");
    }

    Ok(())
}

#[tokio::test]
async fn verify_with_invalid_pos() -> Result<(), Box<dyn Error>> {
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

    let signs = pipe
        .sign_with(sign::IntervalController::build(
            Arc::new(identity),
            ONE_HUNDRED_MILLIS,
        ))?
        .try_collect::<Vec<_>>()
        .await?;

    for pos in [(1000, 50), (50, 1000), (1000, 1000)]
        .into_iter()
        .map(Vec2u::from)
    {
        // Secretly just change the pos as technically we won't get to the
        // verification stage
        let signfile = signs
            .iter()
            .cloned()
            .map(|mut i| {
                i.val.size = Vec2u::new(1, 1);
                i.val.pos = pos;
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
                                        SigOperationError::InvalidCrop(
                                            epos,
                                            Vec2u { x: 1, y: 1},
                                        ),
                                    ),
                                ) if *epos == pos
                            ),
                            "{s:?} marks itself as an invalid crop with position {pos:?}"
                        );
                    }
                }
            })
            .await;

        assert!(count > 0, "We verified some chunks");
    }

    Ok(())
}
