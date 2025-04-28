mod utils;

use std::error::Error;

use futures::StreamExt;
use stream_signer::{
    SignFile, SignPipeline,
    utils::UnknownKey,
    video::{
        SigOperationError,
        verify::{InvalidSignatureError, SignatureState},
    },
};
use testlibs::{client::get_resolver, test_video, videos};
use utils::{skip_loading, test_client};

#[tokio::test]
async fn verify_invalid_pres() -> Result<(), Box<dyn Error>> {
    gst::init()?;

    let client = test_client();
    let resolver = get_resolver(client);

    let signfile = SignFile::from_file("./tests/ssrts/invalid_pres.ssrt")?;

    let filepath = test_video(videos::BIG_BUNNY);
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
                        matches!(s, SignatureState::Invalid(InvalidSignatureError::Jwt(_))),
                        "{s:?} resolved correctly"
                    );
                }
            }
        })
        .await;

    assert!(count > 0, "We verified some chunks");

    Ok(())
}

#[tokio::test]
async fn verify_with_unknown_reference() -> Result<(), Box<dyn Error>> {
    gst::init()?;

    let client = test_client();
    let resolver = get_resolver(client);

    let signfile = SignFile::from_file("./tests/ssrts/undefined_ids.ssrt")?;

    let filepath = test_video(videos::BIG_BUNNY);
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
                        matches!(
                            s,
                            SignatureState::Invalid(InvalidSignatureError::Operation(
                                SigOperationError::UnknownCredential(UnknownKey(
                                    key
                                ))
                            )) if key == "did:test:UNrXvfc7wIvrOk5gmYxke8ahHaLJJjyo"
                        ),
                        "{s:?} resolved correctly"
                    );
                }
            }
        })
        .await;

    assert!(count > 0, "We verified some chunks");

    Ok(())
}
