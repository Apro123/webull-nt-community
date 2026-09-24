// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
//  You may not use this file except in compliance with the License.
//  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
// -------------------------------------------------------------------------------------------------

//! Signed REST client for the Webull OpenAPI market data endpoints.

use std::{collections::HashMap, fmt::Debug, time::Duration};

use nautilus_core::{UUID4, consts::NAUTILUS_USER_AGENT, string::urlencoding};
use nautilus_network::http::{HttpClient, USER_AGENT};

use crate::{
    common::{
        Credential, WEBULL_HTTP_BASE_URL, WEBULL_HTTP_HOST, WEBULL_REST_QUOTA, WEBULL_REST_RATE_KEY,
    },
    models::{WebullBar, WebullErrorResponse, WebullSnapshot, WebullTick, WebullTickResponse},
    signing::{SIGNATURE_ALGORITHM, SIGNATURE_VERSION, sign_request},
};

/// HTTP errors for the Webull OpenAPI client.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An HTTP request failed at the transport level.
    #[error("HTTP request failed: {0}")]
    Request(String),

    /// The Webull API returned an error response.
    #[error("Webull API error [{code}]: {message}")]
    ApiError {
        /// The HTTP status code.
        status: u16,
        /// The Webull error code.
        code: String,
        /// The Webull error message.
        message: String,
    },

    /// Failed to deserialize the JSON response body.
    #[error("Failed to parse response body as JSON: {0}")]
    JsonParse(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Backoff applied after a single HTTP 429 (rate limit) response.
const RATE_LIMIT_BACKOFF_SECS: u64 = 60;

/// A Webull OpenAPI HTTP client with request signing and rate limiting.
///
/// See <https://developer.webull.com/open-api>.
#[derive(Clone)]
pub struct WebullHttpClient {
    base_url: String,
    host: String,
    credential: Credential,
    client: HttpClient,
    rate_limit_backoff: Duration,
}

impl Debug for WebullHttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(stringify!(WebullHttpClient))
            .field("base_url", &self.base_url)
            .field("host", &self.host)
            .field("credential", &self.credential)
            .finish()
    }
}

impl WebullHttpClient {
    /// Creates a new [`WebullHttpClient`] against the production base URL.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn new(credential: Credential) -> anyhow::Result<Self> {
        Self::new_with_base_url(
            credential,
            WEBULL_HTTP_BASE_URL,
            WEBULL_HTTP_HOST,
            Duration::from_secs(RATE_LIMIT_BACKOFF_SECS),
        )
    }

    /// Creates a new [`WebullHttpClient`] against an explicit base URL and host.
    ///
    /// The `host` is used for request signing and must match the base URL host.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn new_with_base_url(
        credential: Credential,
        base_url: &str,
        host: &str,
        rate_limit_backoff: Duration,
    ) -> anyhow::Result<Self> {
        let mut headers = HashMap::new();
        headers.insert(USER_AGENT.to_string(), NAUTILUS_USER_AGENT.to_string());

        let keyed_quotas = vec![(WEBULL_REST_RATE_KEY.to_string(), *WEBULL_REST_QUOTA)];
        let client = HttpClient::builder()
            .headers(headers)
            .keyed_quotas(keyed_quotas)
            .default_quota(*WEBULL_REST_QUOTA)
            .maybe_timeout_secs(Some(60))
            .build()?;

        Ok(Self {
            base_url: base_url.to_string(),
            host: host.to_string(),
            credential,
            client,
            rate_limit_backoff,
        })
    }

    /// Returns the credential associated with this client.
    #[must_use]
    pub const fn credential(&self) -> &Credential {
        &self.credential
    }

    /// Sends a signed GET request and returns the JSON response body as a string.
    ///
    /// Retries once after a 60-second backoff when the API responds with 429.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the API returns a non-success status.
    async fn signed_get(&self, path: &str, query: &[(String, String)]) -> Result<String> {
        let mut retried = false;

        loop {
            let timestamp = current_utc_timestamp();
            let nonce = UUID4::new().as_str().replace('-', "");
            let signature = sign_request(
                self.credential.api_secret(),
                &self.host,
                path,
                query,
                None,
                self.credential.api_key(),
                &timestamp,
                &nonce,
            );

            let mut headers = HashMap::new();
            headers.insert(
                "x-app-key".to_string(),
                self.credential.api_key().to_string(),
            );
            headers.insert(
                "x-signature-algorithm".to_string(),
                SIGNATURE_ALGORITHM.to_string(),
            );
            headers.insert(
                "x-signature-version".to_string(),
                SIGNATURE_VERSION.to_string(),
            );
            headers.insert("x-signature-nonce".to_string(), nonce);
            headers.insert("x-version".to_string(), "v2".to_string());
            headers.insert("x-timestamp".to_string(), timestamp);
            headers.insert("host".to_string(), self.host.clone());
            headers.insert("x-signature".to_string(), signature);
            if let Some(token) = self.credential.access_token() {
                headers.insert("x-access-token".to_string(), token.to_string());
            }

            let url = build_url(&self.base_url, path, query);
            log::debug!("Requesting: {url}");

            let rate_keys = Some(vec![WEBULL_REST_RATE_KEY.to_string()]);
            let response = self
                .client
                .get(url, None, Some(headers), None, rate_keys)
                .await
                .map_err(|e| Error::Request(e.to_string()))?;

            let status = response.status.as_u16();
            let body = String::from_utf8_lossy(&response.body).to_string();

            if response.status.is_success() {
                log::trace!("{body}");
                return Ok(body);
            }

            let (code, message) = serde_json::from_str::<WebullErrorResponse>(&body).map_or_else(
                |_| ("HTTP_STATUS".to_string(), body.clone()),
                |error| (error.error_code, error.message),
            );

            if status == 429 && !retried {
                log::warn!("Webull API rate limit hit; retrying once after backoff");
                retried = true;
                tokio::time::sleep(self.rate_limit_backoff).await;
                continue;
            }

            log::error!("Webull API error [{status} {code}]: {message}");
            return Err(Error::ApiError {
                status,
                code,
                message,
            });
        }
    }

    /// Returns historical bars ending at `end_time_ms`, newest first.
    ///
    /// The API returns at most `count` bars at or before `end_time_ms` and
    /// ignores any start time; call with a decreasing `end_time_ms` to page.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response cannot be parsed.
    pub async fn get_bars(
        &self,
        symbol: &str,
        category: &str,
        timespan: &str,
        count: u32,
        end_time_ms: u64,
        trading_sessions: Option<&str>,
    ) -> Result<Vec<WebullBar>> {
        let mut query = vec![
            ("symbol".to_string(), symbol.to_string()),
            ("category".to_string(), category.to_string()),
            ("timespan".to_string(), timespan.to_string()),
            ("count".to_string(), count.to_string()),
            ("end_time".to_string(), end_time_ms.to_string()),
        ];

        if let Some(sessions) = trading_sessions {
            query.push(("trading_sessions".to_string(), sessions.to_string()));
        }

        let body = self
            .signed_get("/openapi/market-data/stock/bars", &query)
            .await?;
        Ok(serde_json::from_str(&body)?)
    }

    /// Returns quote snapshots for the given symbols.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response cannot be parsed.
    pub async fn get_snapshot(
        &self,
        symbols: &[String],
        category: &str,
    ) -> Result<Vec<WebullSnapshot>> {
        let query = vec![
            ("symbols".to_string(), symbols.join(",")),
            ("category".to_string(), category.to_string()),
            ("extend_hour_required".to_string(), "false".to_string()),
            ("overnight_required".to_string(), "false".to_string()),
        ];

        let body = self
            .signed_get("/openapi/market-data/stock/snapshot", &query)
            .await?;
        Ok(serde_json::from_str(&body)?)
    }

    /// Returns trade ticks ending at `end_time_ms`, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response cannot be parsed.
    pub async fn get_ticks(
        &self,
        symbol: &str,
        category: &str,
        count: u32,
        end_time_ms: u64,
    ) -> Result<Vec<WebullTick>> {
        let query = vec![
            ("symbol".to_string(), symbol.to_string()),
            ("category".to_string(), category.to_string()),
            ("count".to_string(), count.to_string()),
            ("end_time".to_string(), end_time_ms.to_string()),
        ];

        let body = self
            .signed_get("/openapi/market-data/stock/tick", &query)
            .await?;
        let response: WebullTickResponse = serde_json::from_str(&body)?;
        Ok(response.result)
    }
}

/// Builds the request URL with a percent-encoded query string.
fn build_url(base_url: &str, path: &str, query: &[(String, String)]) -> String {
    if query.is_empty() {
        return format!("{base_url}{path}");
    }

    let query_string = query
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                urlencoding::encode(key),
                urlencoding::encode(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&");

    format!("{base_url}{path}?{query_string}")
}

/// Returns the current UTC time as `YYYY-MM-DDThh:mm:ssZ`.
fn current_utc_timestamp() -> String {
    jiff::Timestamp::now()
        .strftime("%Y-%m-%dT%H:%M:%SZ")
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::mpsc,
        thread,
    };

    use rstest::rstest;

    use super::*;

    /// Spins up a single-request loopback HTTP server returning `responses`
    /// in order and capturing each raw request.
    fn mock_server(responses: Vec<(u16, String)>) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("binds loopback port");
        let addr = listener.local_addr().expect("local address");
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().expect("accepts connection");
                let request = read_request_head(&mut stream);
                tx.send(request).expect("sends captured request");
                let response = format!(
                    "HTTP/1.1 {status} Mock\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("writes response");
            }
        });

        (format!("http://127.0.0.1:{}", addr.port()), rx)
    }

    fn read_request_head(stream: &mut TcpStream) -> String {
        let mut buffer = vec![0u8; 8192];
        let mut collected = Vec::new();

        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    collected.extend_from_slice(&buffer[..n]);
                    if String::from_utf8_lossy(&collected).contains("\r\n\r\n") {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        String::from_utf8_lossy(&collected).to_string()
    }

    fn test_credential() -> Credential {
        Credential::new(
            "test-api-key".to_string(),
            "test-api-secret".to_string(),
            Some("test-access-token".to_string()),
        )
    }

    #[rstest]
    fn test_get_bars_signs_and_parses() {
        let bars = r#"[{"symbol":"AAPL","time":"2026-09-04T14:55:00.000+0000","open":"318.98","high":"319.07","low":"318.3","close":"318.44","volume":"619677","trading_session":"RTH","instrument_id":"913256135"}]"#;
        let (base_url, requests) = mock_server(vec![(200, bars.to_string())]);

        let client = WebullHttpClient::new_with_base_url(
            test_credential(),
            &base_url,
            "127.0.0.1",
            Duration::from_millis(1),
        )
        .expect("builds client");

        let bars = tokio_test_block_on(async move {
            client
                .get_bars("AAPL", "US_STOCK", "M5", 200, 1_788_550_800_000, None)
                .await
        })
        .expect("fetches bars");

        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].close, "318.44");

        let request = requests.recv().expect("captures request");
        assert!(request.contains("GET /openapi/market-data/stock/bars?"));
        assert!(request.contains("symbol=AAPL"));
        assert!(request.contains("timespan=M5"));
        assert!(request.contains("end_time=1788550800000"));
        assert!(request.contains("x-signature:"));
        assert!(request.contains("x-signature-algorithm: HMAC-SHA1"));
        assert!(request.contains("x-access-token: test-access-token"));
        assert!(request.contains("x-app-key: test-api-key"));
        assert!(
            !request.contains("test-api-secret"),
            "secret must not be sent"
        );
    }

    #[rstest]
    fn test_api_error_is_surfaced() {
        let error_body =
            r#"{"message":"Header x-signature is invalid.","error_code":"UNAUTHORIZED"}"#;
        let (base_url, _requests) = mock_server(vec![(401, error_body.to_string())]);

        let client = WebullHttpClient::new_with_base_url(
            test_credential(),
            &base_url,
            "127.0.0.1",
            Duration::from_millis(1),
        )
        .expect("builds client");

        let result = tokio_test_block_on(async move {
            client
                .get_bars("AAPL", "US_STOCK", "M5", 100, 1_788_550_800_000, None)
                .await
        });

        match result {
            Err(Error::ApiError { status, code, .. }) => {
                assert_eq!(status, 401);
                assert_eq!(code, "UNAUTHORIZED");
            }
            other => panic!("expected ApiError, got {other:?}"),
        }
    }

    #[rstest]
    fn test_rate_limit_retries_once() {
        let bars = "[]";
        let (base_url, requests) = mock_server(vec![
            (429, "rate limited".to_string()),
            (200, bars.to_string()),
        ]);

        let client = WebullHttpClient::new_with_base_url(
            test_credential(),
            &base_url,
            "127.0.0.1",
            Duration::from_millis(1),
        )
        .expect("builds client");

        let bars = tokio_test_block_on(async move {
            client
                .get_bars("AAPL", "US_STOCK", "M5", 100, 1_788_550_800_000, None)
                .await
        })
        .expect("succeeds after 429 retry");

        assert!(bars.is_empty());
        let first = requests.recv().expect("first request");
        let second = requests.recv().expect("second request");
        assert_ne!(first, second, "the retry carries a fresh signature nonce");
        assert!(
            requests.try_recv().is_err(),
            "exactly two requests were made"
        );
    }

    #[rstest]
    fn test_build_url_encodes_query_values() {
        let query = vec![("trading_sessions".to_string(), "PRE,RTH,ATH".to_string())];
        let url = build_url(
            "https://api.webull.com",
            "/openapi/market-data/stock/bars",
            &query,
        );

        assert_eq!(
            url,
            "https://api.webull.com/openapi/market-data/stock/bars?trading_sessions=PRE%2CRTH%2CATH"
        );
    }

    #[rstest]
    fn test_build_url_without_query() {
        let url = build_url("https://api.webull.com", "/path", &[]);
        assert_eq!(url, "https://api.webull.com/path");
    }

    /// Runs an async block on a fresh single-threaded runtime.
    fn tokio_test_block_on<T: Send + 'static>(
        fut: impl core::future::Future<Output = T> + Send + 'static,
    ) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("builds runtime")
            .block_on(fut)
    }
}
