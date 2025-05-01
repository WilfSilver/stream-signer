mod constants;

use std::{error::Error, time::Duration};

use constants::ONE_HUNDRED_MILLIS;
use stream_signer::{
    spec::{MAX_CHUNK_LENGTH, MIN_CHUNK_LENGTH},
    video::sign,
};
use testlibs::videos;
use utils::{sign_and_verify_int, sign_and_verify_multi, sign_and_verify_multi_together};

mod utils;

#[tokio::test]
async fn sign_and_verify() -> Result<(), Box<dyn Error>> {
    sign_and_verify_int(videos::BIG_BUNNY, |i| {
        sign::IntervalController::build(i, ONE_HUNDRED_MILLIS)
    })
    .await
}

#[tokio::test]
async fn sign_and_verify_near_max() -> Result<(), Box<dyn Error>> {
    sign_and_verify_int(videos::BIG_BUNNY_LONG, |i| {
        sign::IntervalController::build(i, MAX_CHUNK_LENGTH - Duration::from_millis(34))
    })
    .await
}

#[tokio::test]
async fn sign_and_verify_near_min() -> Result<(), Box<dyn Error>> {
    sign_and_verify_int(videos::BIG_BUNNY_LONG, |i| {
        sign::IntervalController::build(i, MIN_CHUNK_LENGTH + Duration::from_millis(1))
    })
    .await
}

#[tokio::test]
async fn sign_and_verify_weird_interval() -> Result<(), Box<dyn Error>> {
    sign_and_verify_int(videos::BIG_BUNNY_LONG, |i| {
        sign::IntervalController::build(i, Duration::from_millis(179))
    })
    .await
}

#[tokio::test]
async fn sign_and_verify_multi_same() -> Result<(), Box<dyn Error>> {
    sign_and_verify_multi(ONE_HUNDRED_MILLIS, ONE_HUNDRED_MILLIS).await
}

#[tokio::test]
async fn sign_and_verify_multi_same_together() -> Result<(), Box<dyn Error>> {
    sign_and_verify_multi_together(ONE_HUNDRED_MILLIS, ONE_HUNDRED_MILLIS).await
}

#[tokio::test]
async fn sign_and_verify_multi_diff() -> Result<(), Box<dyn Error>> {
    sign_and_verify_multi(ONE_HUNDRED_MILLIS, Duration::from_millis(179)).await
}

#[tokio::test]
async fn sign_and_verify_multi_diff_together() -> Result<(), Box<dyn Error>> {
    sign_and_verify_multi_together(ONE_HUNDRED_MILLIS, Duration::from_millis(179)).await
}
