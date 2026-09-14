use std::future::Future;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResponse {
    // `jsonrpc`/`method` are optional so a JSON-RPC error response that omits them
    // (e.g. a protocol-level error, or one relayed by a proxy) still parses and is
    // classified via the `error` arm (503) rather than failing to decode (500).
    #[serde(default)]
    pub jsonrpc: Option<String>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

/// Tuning for the resilient ogmios query path.
#[derive(Debug, Clone, Copy)]
pub struct OgmiosQueryOpts {
    /// Per-attempt timeout for connect + query.
    pub timeout: Duration,
    /// Extra attempts after the first (total attempts = max_retries + 1).
    pub max_retries: u32,
}

/// Error classes from an ogmios query, each mapped to an HTTP status by the caller.
#[derive(Debug)]
pub enum OgmiosError {
    /// Connection/send/recv/peer-close failure (the EAGAIN-close class). Retryable.
    Transient(String),
    /// Per-attempt timeout, retries exhausted.
    Timeout,
    /// ogmios returned a JSON-RPC `error` object. Not retryable.
    JsonRpc { code: i64, message: String },
    /// `result` failed to deserialize into the target type (schema/era drift). Not retryable.
    Decode(String),
}

impl OgmiosError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, OgmiosError::Transient(_))
    }
}

pub fn request(query: &str) -> String {
    json!({
      "jsonrpc" : "2.0",
      "method": query
    })
    .to_string()
}

/// Run `attempt` up to `max_retries + 1` times. Each attempt is bounded by `opts.timeout`.
/// Retries only on retryable errors (Transient) and on per-attempt timeout; returns
/// non-retryable errors (JsonRpc / Decode) immediately. A fixed 50ms backoff separates attempts.
pub async fn run_with_retries<T, F, Fut>(
    opts: &OgmiosQueryOpts,
    attempt: F,
) -> Result<T, OgmiosError>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, OgmiosError>>,
{
    let mut last_err = OgmiosError::Timeout;
    for i in 0..=opts.max_retries {
        if i > 0 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        match tokio::time::timeout(opts.timeout, attempt()).await {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(e)) if e.is_retryable() => {
                tracing::warn!("ogmios attempt {i} failed (retryable): {e:?}");
                last_err = e;
            }
            Ok(Err(e)) => return Err(e),
            Err(_elapsed) => {
                tracing::warn!("ogmios attempt {i} timed out after {:?}", opts.timeout);
                last_err = OgmiosError::Timeout;
            }
        }
    }
    Err(last_err)
}

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

/// One websocket round-trip: connect → send query → read one frame → parse.
async fn single_attempt<T: DeserializeOwned>(
    base_url: &Url,
    query_param: &str,
    method: &str,
) -> Result<T, OgmiosError> {
    let mut url = base_url.clone();
    url.query_pairs_mut().append_pair(query_param, "");

    let (mut ws, _) = connect_async(url)
        .await
        .map_err(|e| OgmiosError::Transient(e.to_string()))?;

    ws.send(Message::Text(request(method)))
        .await
        .map_err(|e| OgmiosError::Transient(e.to_string()))?;

    let msg_opt = ws.next().await;

    // Close is best-effort cleanup. Run it detached so a stalled close can't consume this
    // attempt's timeout budget and cause an already-received response to be dropped as a
    // timeout (`single_attempt` runs inside `run_with_retries`' per-attempt `timeout`).
    // Bound the detached close so a close that hangs against a bad peer can't leak the task
    // and its socket indefinitely.
    tokio::spawn(async move {
        let _ = tokio::time::timeout(Duration::from_secs(1), ws.close(None)).await;
    });

    let msg = match msg_opt {
        None => return Err(OgmiosError::Transient("no ogmios message".into())),
        Some(result) => result.map_err(|e| OgmiosError::Transient(e.to_string()))?,
    };

    let text = msg
        .into_text()
        .map_err(|e| OgmiosError::Transient(e.to_string()))?;

    let response: QueryResponse =
        serde_json::from_str(&text).map_err(|e| OgmiosError::Decode(e.to_string()))?;

    if let Some(err) = response.error {
        return Err(OgmiosError::JsonRpc {
            code: err.code,
            message: err.message,
        });
    }

    let result = response
        .result
        .ok_or_else(|| OgmiosError::Decode("ogmios response missing result".into()))?;

    serde_json::from_value(result).map_err(|e| OgmiosError::Decode(e.to_string()))
}

/// Query ogmios with a bounded per-attempt timeout and small retry.
/// `query_param` is the websocket URL query key (e.g. "QueryLedgerStateProtocolParameters");
/// `method` is the JSON-RPC method (e.g. "queryLedgerState/protocolParameters").
pub async fn query<T: DeserializeOwned>(
    base_url: &Url,
    query_param: &str,
    method: &str,
    opts: &OgmiosQueryOpts,
) -> Result<T, OgmiosError> {
    run_with_retries(opts, || single_attempt::<T>(base_url, query_param, method)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    fn opts(max_retries: u32, timeout_ms: u64) -> OgmiosQueryOpts {
        OgmiosQueryOpts {
            timeout: Duration::from_millis(timeout_ms),
            max_retries,
        }
    }

    #[tokio::test]
    async fn succeeds_on_first_attempt() {
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();
        let out: Result<u32, OgmiosError> = run_with_retries(&opts(2, 1000), || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(7u32)
            }
        })
        .await;
        assert_eq!(out.unwrap(), 7);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retries_then_succeeds() {
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();
        let out: Result<u32, OgmiosError> = run_with_retries(&opts(3, 1000), || {
            let c = c.clone();
            async move {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err(OgmiosError::Transient("boom".into()))
                } else {
                    Ok(42u32)
                }
            }
        })
        .await;
        assert_eq!(out.unwrap(), 42);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn gives_up_after_max_retries() {
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();
        let out: Result<u32, OgmiosError> = run_with_retries(&opts(2, 1000), || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err(OgmiosError::Transient("boom".into()))
            }
        })
        .await;
        assert!(matches!(out, Err(OgmiosError::Transient(_))));
        assert_eq!(calls.load(Ordering::SeqCst), 3); // 1 + 2 retries
    }

    #[tokio::test]
    async fn does_not_retry_decode_error() {
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();
        let out: Result<u32, OgmiosError> = run_with_retries(&opts(5, 1000), || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err(OgmiosError::Decode("bad".into()))
            }
        })
        .await;
        assert!(matches!(out, Err(OgmiosError::Decode(_))));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn times_out_slow_attempt() {
        let out: Result<u32, OgmiosError> = run_with_retries(&opts(1, 10), || async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            Ok(1u32)
        })
        .await;
        assert!(matches!(out, Err(OgmiosError::Timeout)));
    }

    #[test]
    fn query_response_parses_error_object() {
        let raw = r#"{"jsonrpc":"2.0","method":"queryLedgerState/protocolParameters","error":{"code":-32000,"message":"acquire failed"}}"#;
        let resp: QueryResponse = serde_json::from_str(raw).unwrap();
        assert!(resp.result.is_none());
        let err = resp.error.expect("error present");
        assert_eq!(err.code, -32000);
        assert_eq!(err.message, "acquire failed");
    }

    #[test]
    fn query_response_parses_error_without_method() {
        // A JSON-RPC error response may omit `method`/`jsonrpc`; it must still parse and
        // surface via the `error` arm (→ 503) rather than failing to decode (→ 500).
        let raw = r#"{"error":{"code":-32700,"message":"parse error"}}"#;
        let resp: QueryResponse = serde_json::from_str(raw).unwrap();
        assert!(resp.method.is_none());
        assert!(resp.result.is_none());
        let err = resp.error.expect("error present");
        assert_eq!(err.code, -32700);
    }

    #[test]
    fn query_response_parses_result_object() {
        let raw = r#"{"jsonrpc":"2.0","method":"queryLedgerState/protocolParameters","result":{"some":"value"}}"#;
        let resp: QueryResponse = serde_json::from_str(raw).unwrap();
        assert!(resp.error.is_none());
        assert!(resp.result.is_some());
    }

    #[test]
    fn only_transient_is_retryable() {
        assert!(OgmiosError::Transient("x".into()).is_retryable());
        assert!(!OgmiosError::Timeout.is_retryable());
        assert!(!OgmiosError::JsonRpc {
            code: 1,
            message: "m".into()
        }
        .is_retryable());
        assert!(!OgmiosError::Decode("d".into()).is_retryable());
    }
}
