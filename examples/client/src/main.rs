use http_signature_normalization_reqwest::prelude::*;
use reqwest::{
    header::{ACCEPT, USER_AGENT},
    Client,
};
use sha2::{Digest, Sha256};

async fn request(config: Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let digest = Sha256::new();

    let client = Client::new();

    let request = client
        .post("http://127.0.0.1:3001/")
        .header(USER_AGENT, "Reqwest")
        .header(ACCEPT, "text/plain")
        .send()
        .signature_with_digest(config, "my-key-id", digest, "Hewwo-owo", |s| {
            println!("Signing String\n{}", s);
            Ok(base64::encode(s)) as Result<_, MyError>
        })
        .await?;

    let status = client.execute(request).await?.status();

    println!("{:?}", status);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    std::env::set_var("RUST_LOG", "info");
    pretty_env_logger::init();

    tokio::spawn(async move {
        let config = Config::default().require_header("accept");

        request(config.clone()).await?;
        request(config.mastodon_compat()).await
    })
        .await?
}

#[derive(Debug, thiserror::Error)]
pub enum MyError {
    #[error("Failed to create signing string, {0}")]
    Convert(#[from] SignError),

    #[error("Failed to send request")]
    SendRequest(#[from] reqwest::Error),

    #[error("Failed to retrieve request body")]
    Body(reqwest::Error),
}