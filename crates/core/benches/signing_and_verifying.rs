use std::{error::Error, fmt::Display, future, sync::Arc, time::Duration};

use criterion::BenchmarkId;
use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, Criterion, criterion_group, criterion_main};
use futures::{StreamExt, TryStreamExt};
use identity_iota::{core::FromJson, credential::Subject, did::DID, prelude::Resolver};
use serde_json::json;
use stream_signer::{
    SignFile, SignPipeline,
    spec::{MAX_CHUNK_LENGTH, MIN_CHUNK_LENGTH},
    video::sign,
};
use testlibs::{
    client::{get_client, get_resolver},
    identity::TestIdentity,
    issuer::TestIssuer,
    test_video, videos,
};
use tokio::runtime::Runtime;

pub fn criterion_benchmark(c: &mut Criterion) {
    gst::init().unwrap();

    let runtime = Runtime::new().unwrap();
    let client = get_client();
    let issuer = runtime.block_on(TestIssuer::new(client.clone())).unwrap();
    let resolver = get_resolver(client);

    let identity = runtime
        .block_on(TestIdentity::new(&issuer, |id| {
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
        }))
        .unwrap();

    let signer = Arc::new(identity);

    // These are really slow tests
    let mut group = c.benchmark_group("Integration");
    group
        .sample_size(10)
        .measurement_time(Duration::from_secs(7 * 10));

    test_all_chunk_sizes(
        &runtime,
        &mut group,
        videos::BIG_BUNNY_1080,
        signer.clone(),
        resolver.clone(),
    );
    test_all_chunk_sizes(
        &runtime,
        &mut group,
        videos::BIG_BUNNY,
        signer.clone(),
        resolver.clone(),
    );
    test_all_chunk_sizes(
        &runtime,
        &mut group,
        videos::BIG_BUNNY_LONG,
        signer.clone(),
        resolver.clone(),
    );

    group.finish();
}

fn test_all_chunk_sizes(
    runtime: &Runtime,
    c: &mut BenchmarkGroup<'_, WallTime>,
    video: &str,
    signer: Arc<TestIdentity>,
    resolver: Arc<Resolver>,
) {
    for duration in [
        MIN_CHUNK_LENGTH + Duration::from_millis(1),
        Duration::from_millis(100),
        Duration::from_millis(1000),
        MAX_CHUNK_LENGTH - Duration::from_millis(41),
    ] {
        struct SignParams {
            pub video: String,
            pub interval: Duration,
            pub signer: Arc<TestIdentity>,
        }

        impl Display for SignParams {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{} | {:?}", &self.video, self.interval)
            }
        }

        let params = SignParams {
            video: video.to_owned(),
            interval: duration,
            signer: signer.clone(),
        };

        c.bench_with_input(BenchmarkId::new("Signing", &params), &params, |b, info| {
            b.to_async(runtime).iter(|| async {
                let res = sign(&info.video, info.interval, info.signer.clone()).await;
                assert!(res.is_ok(), "{:?} was able to be created", res.err())
            })
        });

        // We have to get the signfile separately
        if let Ok(signfile) = runtime.block_on(sign(video, duration, signer.clone())) {
            struct VerifyParams {
                pub video: String,
                pub resolver: Arc<Resolver>,
                pub interval: Duration,
                pub signfile: SignFile,
            }

            impl Display for VerifyParams {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{} | {:?}", &self.video, self.interval)
                }
            }

            let params = VerifyParams {
                video: video.to_owned(),
                interval: duration,
                resolver: resolver.clone(),
                signfile,
            };

            c.bench_with_input(BenchmarkId::new("Verify", &params), &params, |b, info| {
                b.to_async(runtime).iter(|| async {
                    let res = verify(video, info.resolver.clone(), info.signfile.clone()).await;
                    assert!(res.is_ok(), "{:?} was able to be created", res.err())
                })
            });
        }
    }

    c.bench_with_input(BenchmarkId::new("Decoding", video), video, |b, video| {
        b.to_async(runtime).iter(|| async {
            let res = decode(video).await;
            assert!(res.is_ok(), "{:?} was able to be decoded", res.err())
        })
    });
}

async fn decode(video: &str) -> Result<usize, Box<dyn Error>> {
    let filepath = test_video(video);

    let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

    let mut i = 0;
    for s in pipe.try_into_iter(())? {
        i = s.unwrap().idx;
    }

    Ok(i)
}

async fn sign(
    video: &str,
    interval: Duration,
    signer: Arc<TestIdentity>,
) -> Result<SignFile, Box<dyn Error>> {
    let filepath = test_video(video);

    let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

    let signfile = pipe
        .sign_with(sign::IntervalController::build(signer, interval))?
        .try_collect::<SignFile>()
        .await?;

    Ok(signfile)
}

async fn verify(
    video: &str,
    resolver: Arc<Resolver>,
    signfile: SignFile,
) -> Result<(), Box<dyn Error>> {
    let filepath = test_video(video);

    let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

    let mut count = 0;
    pipe.verify(resolver, signfile)?
        .for_each(|v| {
            count += 1;

            let _ = match v {
                Ok(v) => v,
                Err(e) => {
                    panic!("Frame was invalid: {e}");
                }
            };

            future::ready(())
        })
        .await;

    assert!(count > 0, "We verified some chunks");

    Ok(())
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
