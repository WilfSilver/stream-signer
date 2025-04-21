use std::error::Error;

use stream_signer::video::sign;
use utils::{sign_and_verify_int, sign_and_verify_multi, sign_and_verify_multi_together};

mod utils;

#[tokio::test]
async fn sign_and_verify() -> Result<(), Box<dyn Error>> {
    sign_and_verify_int(|i| sign::IntervalController::build(i, 100)).await
}

#[tokio::test]
async fn sign_and_verify_multi_same() -> Result<(), Box<dyn Error>> {
    sign_and_verify_multi(100, 100).await?;
    sign_and_verify_multi_together(100, 100).await
}

#[tokio::test]
async fn sign_and_verify_multi_diff() -> Result<(), Box<dyn Error>> {
    sign_and_verify_multi(100, 179).await?;
    sign_and_verify_multi_together(100, 179).await
}
