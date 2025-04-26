// TODO:
// - Count the signatures generated

mod constants;

use std::{error::Error, time::Duration};

use constants::ONE_HUNDRED_MILLIS;
use stream_signer::video::sign;
use utils::{sign_and_verify_int, sign_and_verify_multi, sign_and_verify_multi_together};

mod utils;

#[tokio::test]
async fn sign_and_verify() -> Result<(), Box<dyn Error>> {
    sign_and_verify_int(|i| sign::IntervalController::build(i, ONE_HUNDRED_MILLIS)).await
}

#[tokio::test]
async fn sign_and_verify_multi_same() -> Result<(), Box<dyn Error>> {
    sign_and_verify_multi(ONE_HUNDRED_MILLIS, ONE_HUNDRED_MILLIS).await?;
    sign_and_verify_multi_together(ONE_HUNDRED_MILLIS, ONE_HUNDRED_MILLIS).await
}

#[tokio::test]
async fn sign_and_verify_multi_diff() -> Result<(), Box<dyn Error>> {
    sign_and_verify_multi(ONE_HUNDRED_MILLIS, Duration::from_millis(179)).await?;
    sign_and_verify_multi_together(ONE_HUNDRED_MILLIS, Duration::from_millis(179)).await
}
