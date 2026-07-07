//! Executor key — the trusted public key whose signatures the resource
//! honors on every receipt.
//!
//! Fetched once at startup from the gateway's `GET /v1/executor-key`
//! endpoint. Cached for the lifetime of the resource process. Receipts
//! signed by any other key MUST be rejected.

use std::time::Duration;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::Deserialize;

use maat::{PublicKey, SignatureAlgorithm};
use maat_sdk_core::SdkError;

/// Public key the resource trusts to have signed any receipt it receives.
#[derive(Debug, Clone)]
pub struct ExecutorKey {
    pub key: PublicKey,
    pub key_id: String,
    pub tenant_id: uuid::Uuid,
}

/// Fetch the executor key from the gateway.
///
/// Call this once at resource startup. The returned key is cached for
/// the lifetime of the process; receipts are verified locally against
/// it without further gateway round-trips.
pub async fn fetch_executor_key(
    gateway_url: &str,
    api_key: &str,
) -> Result<ExecutorKey, SdkError> {
    let url = format!(
        "{}/v1/executor-key",
        gateway_url.trim_end_matches('/')
    );

    let resp = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| SdkError::Config(e.to_string()))?
        .get(&url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|e| SdkError::Transport(format!("gateway unreachable: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(SdkError::Transport(format!(
            "executor-key fetch returned {}: {}",
            status, body
        )));
    }

    let view: ExecutorKeyResponse = resp
        .json()
        .await
        .map_err(|e| SdkError::Transport(format!("malformed executor-key response: {}", e)))?;

    if view.algorithm != "ed25519" {
        return Err(SdkError::Crypto(format!(
            "unsupported algorithm: {}",
            view.algorithm
        )));
    }

    let key_data = URL_SAFE_NO_PAD
        .decode(&view.public_key_b64)
        .map_err(|e| SdkError::Crypto(format!("public_key_b64 not base64url: {}", e)))?;

    Ok(ExecutorKey {
        key: PublicKey {
            algorithm: SignatureAlgorithm::Ed25519,
            key_data,
        },
        key_id: view.key_id,
        tenant_id: view.tenant_id,
    })
}

#[derive(Debug, Deserialize)]
struct ExecutorKeyResponse {
    tenant_id: uuid::Uuid,
    key_id: String,
    algorithm: String,
    public_key_b64: String,
}
