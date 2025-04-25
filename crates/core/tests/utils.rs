#![allow(dead_code)]

use std::{error::Error, fs::File, sync::Arc, time::Duration};

use futures::{StreamExt, TryStreamExt};
use identity_iota::{core::FromJson, credential::Subject, did::DID};
use serde_json::json;
use stream_signer::{
    video::{
        sign::{self, Controller},
        verify::SignatureState,
    },
    SignFile, SignPipeline,
};
use testlibs::{
    client::{get_client, get_resolver, MemClient},
    identity::TestIdentity,
    issuer::TestIssuer,
    test_video, videos,
};
use tokio::sync::Mutex;

pub async fn skip_loading(state: &SignatureState) -> bool {
    match state {
        SignatureState::Loading => {
            tokio::spawn(tokio::time::sleep(Duration::from_millis(5)))
                .await
                .unwrap();
            true
        }
        _ => false,
    }
}

pub async fn sign_and_verify_int<F, C>(get_controller: F) -> Result<(), Box<dyn Error>>
where
    F: Fn(Arc<TestIdentity>) -> C,
    C: Controller<TestIdentity> + 'static,
{
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
        .sign_with(get_controller(Arc::new(identity)))?
        .try_collect::<SignFile>()
        .await?;

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
                        matches!(s, SignatureState::Verified(_)),
                        "{s:?} resolved correctly"
                    );
                }
            }
        })
        .await;

    assert!(count > 0, "We verified some chunks");

    Ok(())
}

pub async fn sign_and_verify_multi(
    alice_chunk_size: Duration,
    bob_chunk_size: Duration,
) -> Result<(), Box<dyn Error>> {
    gst::init()?;

    let client = get_client();
    let issuer = TestIssuer::new(client.clone()).await?;
    let resolver = get_resolver(client);

    let alice_identity = TestIdentity::new(&issuer, |id| {
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

    let bob_identity = TestIdentity::new(&issuer, |id| {
        Subject::from_json_value(json!({
            "id": id.as_str(),
            "name": "Bob",
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

    let alice_signfile = pipe
        .sign_with(sign::IntervalController::build(
            Arc::new(alice_identity),
            alice_chunk_size,
        ))?
        .try_collect::<SignFile>()
        .await?;

    let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

    let bob_signfile = pipe
        .sign_with(sign::IntervalController::build(
            Arc::new(bob_identity),
            bob_chunk_size,
        ))?
        .try_collect::<SignFile>()
        .await?;

    let mut signfile = alice_signfile;
    signfile.extend(bob_signfile.into_iter());

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

pub async fn sign_and_verify_multi_together(
    alice_chunk_size: Duration,
    bob_chunk_size: Duration,
) -> Result<(), Box<dyn Error>> {
    gst::init()?;

    let client = get_client();
    let issuer = TestIssuer::new(client.clone()).await?;
    let resolver = get_resolver(client);

    let alice_identity = TestIdentity::new(&issuer, |id| {
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

    let bob_identity = TestIdentity::new(&issuer, |id| {
        Subject::from_json_value(json!({
            "id": id.as_str(),
            "name": "Bob",
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
        .sign_with_all(vec![
            Box::new(sign::IntervalController::build(
                Arc::new(alice_identity),
                alice_chunk_size,
            )),
            Box::new(sign::IntervalController::build(
                Arc::new(bob_identity),
                bob_chunk_size,
            )),
        ])?
        .try_collect::<SignFile>()
        .await?;

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

pub fn test_client() -> Arc<Mutex<MemClient>> {
    let client = {
        let f = File::open("./tests/ssrts/test_client.json").unwrap();
        serde_json::from_reader::<_, MemClient>(f).unwrap()
    };

    Arc::new(Mutex::new(client))
}
