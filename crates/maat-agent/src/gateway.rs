//! HTTP client for the Maat gateway.
//!
//! [`GatewayClient`] talks to two endpoints:
//! - `POST /v1/verify` — submit an [`maat::ActionRequest`] and receive a
//!   signed [`maat::Receipt`].
//! - `GET /v1/receipts` and `GET /v1/receipts/{id}` — query the gateway's
//!   receipt log (for agent self-audit).
//!
//! ## Auth
//!
//! All requests carry a tenant API key as a Bearer token. The key
//! identifies the tenant; the agent's identity is asserted by the
//! signed delegation chain in the request body.
//!
//! ## What the client does NOT do
//!
//! - Retries on 5xx or 429. Operators choose retry policy (e.g., via
//!   `tower::retry`).
//! - Composite "verify and act" patterns. The receipt-forwarding step
//!   varies too widely across resources (HTTP header, gRPC metadata,
//!   message-queue payload) to bundle. Operators call
//!   [`GatewayClient::verify_action`], get the receipt JSON, forward it
//!   in whatever shape the resource expects.

use std::time::Duration;

use serde::Deserialize;

use maat::verify::ActionRequest;
use maat::{ObjectId, Receipt};
use maat_sdk_core::{ActionDescriptor, SdkError};

/// Client to the gateway's HTTP API.
#[derive(Debug, Clone)]
pub struct GatewayClient {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

/// Result of a successful gateway verification: the signed receipt as
/// both parsed protocol object and JSON bytes ready to forward to a
/// resource.
#[derive(Debug, Clone)]
pub struct VerifiedReceipt<D> {
    /// The receipt as JSON bytes, suitable for forwarding to the
    /// resource (typically as a header or message-queue payload).
    /// Preserves the gateway's canonicalization byte-for-byte.
    pub receipt_json: Vec<u8>,
    /// The parsed receipt for inspection. Operators usually don't
    /// touch this — `receipt_json` is what gets forwarded.
    pub receipt: Receipt,
    /// The descriptor the agent submitted, returned for convenience
    /// so the agent can log/correlate without re-decoding.
    pub descriptor: D,
}

impl GatewayClient {
    /// Construct a client for the given gateway base URL and tenant API key.
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        GatewayClient {
            base_url: base_url.into(),
            api_key: api_key.into(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("reqwest client should build"),
        }
    }

    /// Submit an action request to the gateway. The descriptor is encoded
    /// into the request's `action_value` field; the gateway copies those
    /// bytes verbatim into the issued receipt's `Action.value`.
    ///
    /// On success, returns the signed receipt as both parsed object and
    /// JSON bytes. On rejection, returns [`SdkError::Rejected`] with
    /// the gateway's structured error.
    pub async fn verify_action<D: ActionDescriptor>(
        &self,
        request: &ActionRequest,
        descriptor: D,
    ) -> Result<VerifiedReceipt<D>, SdkError> {
        // Encode the descriptor and build a request whose action_value
        // carries those bytes. We clone the request and overwrite
        // action_value so callers can pass a request with action_value
        // pre-populated if they want — but the descriptor wins.
        let action_value = descriptor.to_action_value()?;
        let mut req_to_send = request.clone();
        req_to_send.action_value = Some(action_value);

        let url = format!("{}/v1/verify", self.base_url.trim_end_matches('/'));
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&req_to_send)
            .send()
            .await
            .map_err(|e| SdkError::Transport(format!("gateway unreachable: {}", e)))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| SdkError::Transport(e.to_string()))?;

        if status.is_success() {
            let parsed: VerifyResponseEnvelope = serde_json::from_str(&body)
                .map_err(|e| SdkError::Transport(format!("malformed verify response: {}", e)))?;
            let receipt_json = serde_json::to_vec(&parsed.receipt)
                .map_err(|e| SdkError::Transport(e.to_string()))?;
            Ok(VerifiedReceipt {
                receipt_json,
                receipt: parsed.receipt,
                descriptor,
            })
        } else {
            let rejection: RejectionEnvelope = serde_json::from_str(&body).unwrap_or(RejectionEnvelope {
                reason: Some("unknown".into()),
                detail: Some(body.clone()),
                error: None,
            });
            Err(SdkError::Rejected {
                status: status.as_u16(),
                reason: rejection
                    .reason
                    .or(rejection.error)
                    .unwrap_or_else(|| "rejected".into()),
                detail: rejection.detail.unwrap_or_default(),
            })
        }
    }

    /// List receipts for the calling tenant. Most recent first.
    pub async fn list_receipts(
        &self,
        filter: ReceiptFilter,
    ) -> Result<Vec<Receipt>, SdkError> {
        let mut url = format!("{}/v1/receipts", self.base_url.trim_end_matches('/'));
        let mut params: Vec<(&str, String)> = Vec::new();
        if let Some(limit) = filter.limit {
            params.push(("limit", limit.to_string()));
        }
        if let Some(after_id) = &filter.after_id {
            params.push(("after_id", after_id.clone()));
        }
        if !params.is_empty() {
            url.push('?');
            url.push_str(
                &params
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, urlencode(v)))
                    .collect::<Vec<_>>()
                    .join("&"),
            );
        }
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| SdkError::Transport(format!("gateway unreachable: {}", e)))?;
        if !resp.status().is_success() {
            return Err(SdkError::Transport(format!(
                "list_receipts returned {}",
                resp.status()
            )));
        }
        let body = resp
            .json::<ReceiptListEnvelope>()
            .await
            .map_err(|e| SdkError::Transport(e.to_string()))?;
        Ok(body.receipts)
    }

    /// Fetch a single receipt by ID.
    pub async fn get_receipt(&self, id: &ObjectId) -> Result<Receipt, SdkError> {
        let id_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            id.0,
        );
        let url = format!(
            "{}/v1/receipts/{}",
            self.base_url.trim_end_matches('/'),
            id_b64
        );
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| SdkError::Transport(format!("gateway unreachable: {}", e)))?;
        if !resp.status().is_success() {
            return Err(SdkError::Transport(format!(
                "get_receipt returned {}",
                resp.status()
            )));
        }
        resp.json::<Receipt>()
            .await
            .map_err(|e| SdkError::Transport(e.to_string()))
    }
}

// ─── Filter for list_receipts ──────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct ReceiptFilter {
    pub limit: Option<u32>,
    pub after_id: Option<String>,
}

impl ReceiptFilter {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }
    pub fn after(mut self, id: impl Into<String>) -> Self {
        self.after_id = Some(id.into());
        self
    }
}

// ─── Wire envelopes ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct VerifyResponseEnvelope {
    #[allow(dead_code)]
    #[serde(default)]
    verified: bool,
    receipt: Receipt,
}

#[derive(Debug, Deserialize)]
struct RejectionEnvelope {
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    detail: Option<String>,
    /// Some gateways return `{ "error": "..." }` instead of structured
    /// `{ reason, detail }`. Accept both.
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReceiptListEnvelope {
    receipts: Vec<Receipt>,
}

fn urlencode(s: &str) -> String {
    // Minimal — only encode characters that would break a URL query.
    s.bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{:02X}", b),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_construction() {
        let c = GatewayClient::new("http://gateway.test", "test-key");
        assert_eq!(c.base_url, "http://gateway.test");
    }

    #[test]
    fn receipt_filter_builder_composes() {
        let f = ReceiptFilter::new().with_limit(50).after("abc");
        assert_eq!(f.limit, Some(50));
        assert_eq!(f.after_id.as_deref(), Some("abc"));
    }

    #[test]
    fn url_encoding_handles_special_chars() {
        assert_eq!(urlencode("hello world"), "hello%20world");
        assert_eq!(urlencode("a+b=c"), "a%2Bb%3Dc");
        assert_eq!(urlencode("simple"), "simple");
    }

    // Live HTTP behavior is exercised in the migration tests once
    // demos rebuild on the SDK in Slice 10. Here we keep tests
    // dependency-free.
}
