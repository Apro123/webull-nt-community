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

use std::{collections::HashMap, fmt::Debug};

use nautilus_core::{UUID4, consts::NAUTILUS_USER_AGENT, string::urlencoding};
use nautilus_network::{
    http::{HttpClient, USER_AGENT, parse_retry_after},
    retry::{RetryConfig, RetryManager},
};

use crate::{
    common::{
        consts::{WEBULL_REST_QUOTA, WEBULL_REST_RATE_KEY},
        credential::Credential,
        urls::{
            BARS_PATH, CRYPTO_INSTRUMENTS_PATH, SNAPSHOT_PATH, STOCK_INSTRUMENTS_PATH, TICK_PATH,
            WEBULL_HTTP_BASE_URL, WEBULL_HTTP_HOST,
        },
    },
    http::{
        error::{Error, Result},
        models::{
            WebullBar, WebullCryptoInstrumentPage, WebullErrorResponse, WebullInstrumentPage,
            WebullSnapshot, WebullTick, WebullTickResponse,
        },
    },
    signing::{SIGNATURE_ALGORITHM, SIGNATURE_VERSION, sign_request},
};

const CONTENT_TYPE_JSON: &str = "application/json";

/// Transport configuration for the Webull HTTP client.
#[derive(Clone, Debug)]
pub struct WebullHttpConfig {
    /// The REST base URL.
    pub base_url: String,
    /// The host header used for request signing (must match the base URL host).
    pub host: String,
    /// Optional explicit proxy URL.
    pub proxy_url: Option<String>,
    /// Per-request timeout in seconds; `None` disables the timeout.
    pub timeout_secs: Option<u64>,
    /// Retry policy applied to failed requests, honoring the venue's `Retry-After`.
    pub retry: RetryConfig,
}

impl Default for WebullHttpConfig {
    fn default() -> Self {
        Self {
            base_url: WEBULL_HTTP_BASE_URL.to_string(),
            host: WEBULL_HTTP_HOST.to_string(),
            proxy_url: None,
            timeout_secs: Some(60),
            retry: RetryConfig {
                max_delay_ms: 60_000,
                ..Default::default()
            },
        }
    }
}

/// A Webull OpenAPI HTTP client with request signing, rate limiting, and retries.
///
/// See <https://developer.webull.com/open-api>.
#[derive(Clone)]
pub struct WebullHttpClient {
    base_url: String,
    host: String,
    credential: Credential,
    client: HttpClient,
    retry: RetryConfig,
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
    /// Creates a new [`WebullHttpClient`] against the production endpoint with default settings.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn new(credential: Credential) -> Result<Self> {
        Self::new_with_config(credential, WebullHttpConfig::default())
    }

    /// Creates a new [`WebullHttpClient`] with an explicit transport configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn new_with_config(credential: Credential, config: WebullHttpConfig) -> Result<Self> {
        let mut headers = HashMap::new();
        headers.insert(USER_AGENT.to_string(), NAUTILUS_USER_AGENT.to_string());

        let keyed_quotas = vec![(WEBULL_REST_RATE_KEY.to_string(), *WEBULL_REST_QUOTA)];
        let client = HttpClient::builder()
            .headers(headers)
            .keyed_quotas(keyed_quotas)
            .default_quota(*WEBULL_REST_QUOTA)
            .maybe_timeout_secs(config.timeout_secs)
            .maybe_proxy_url(config.proxy_url)
            .build()?;

        Ok(Self {
            base_url: config.base_url,
            host: config.host,
            credential,
            client,
            retry: config.retry,
        })
    }

    /// Returns the credential associated with this client.
    #[must_use]
    pub const fn credential(&self) -> &Credential {
        &self.credential
    }

    /// Sends a signed GET request and returns the JSON response body as a string.
    ///
    /// Failed attempts are retried according to the configured [`RetryConfig`], using any
    /// `Retry-After` delay the API reports as a floor on the exponential backoff.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the API returns a non-success status.
    async fn signed_get(&self, path: &str, query: &[(String, String)]) -> Result<String> {
        RetryManager::new(self.retry.clone())
            .invocation(
                "Webull GET",
                || self.signed_get_once(path, query),
                Error::is_retryable,
                Error::Retry,
            )
            .retry_delay(&Error::retry_delay)
            .execute()
            .await
    }

    /// Makes a single signed GET attempt, re-signing with a fresh timestamp and nonce.
    async fn signed_get_once(&self, path: &str, query: &[(String, String)]) -> Result<String> {
        // A fresh signature timestamp and nonce per attempt.
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
            .await?;

        let status = response.status.as_u16();
        let body = String::from_utf8_lossy(&response.body).to_string();

        if response.status.is_success() {
            log::trace!("{body}");
            return Ok(body);
        }

        let (code, message) = serde_json::from_str::<WebullErrorResponse>(&body).map_or_else(
            |_| ("HTTP_STATUS".to_string(), body.clone()),
            |e| (e.error_code, e.message),
        );
        let retry_after_secs = response
            .headers
            .get("retry-after")
            .and_then(|value| parse_retry_after(value, jiff::Timestamp::now()))
            .map(|delay| delay.as_secs());

        let error = Error::ApiError {
            status,
            code,
            message,
            retry_after_secs,
        };

        if error.is_retryable() {
            log::warn!("Webull API error [{status}]: retrying");
        } else {
            log::error!("Webull API error: {error}");
        }

        Err(error)
    }

    /// Sends a signed POST request with a JSON body and returns the response body.
    ///
    /// The body is included in the HMAC-SHA1 signature (MD5 digest) and sent with
    /// `Content-Type: application/json`.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the API returns a non-success status.
    pub async fn signed_post(&self, path: &str, body: &str) -> Result<String> {
        RetryManager::new(self.retry.clone())
            .invocation(
                "Webull POST",
                || self.signed_post_once(path, body),
                Error::is_retryable,
                Error::Retry,
            )
            .retry_delay(&Error::retry_delay)
            .execute()
            .await
    }

    /// Makes a single signed POST attempt, re-signing with a fresh timestamp and nonce.
    async fn signed_post_once(&self, path: &str, body: &str) -> Result<String> {
        let timestamp = current_utc_timestamp();
        let nonce = UUID4::new().as_str().replace('-', "");
        let signature = sign_request(
            self.credential.api_secret(),
            &self.host,
            path,
            &[],
            Some(body),
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
        headers.insert("Content-Type".to_string(), CONTENT_TYPE_JSON.to_string());
        if let Some(token) = self.credential.access_token() {
            headers.insert("x-access-token".to_string(), token.to_string());
        }

        let url = build_url(&self.base_url, path, &[]);
        log::debug!("Requesting POST: {url}");

        let rate_keys = Some(vec![WEBULL_REST_RATE_KEY.to_string()]);
        let response = self
            .client
            .post(
                url,
                None,
                Some(headers),
                Some(body.as_bytes().to_vec()),
                None,
                rate_keys,
            )
            .await?;

        let status = response.status.as_u16();
        let response_body = String::from_utf8_lossy(&response.body).to_string();

        if response.status.is_success() {
            log::trace!("{response_body}");
            return Ok(response_body);
        }

        let (code, message) = serde_json::from_str::<WebullErrorResponse>(&response_body)
            .map_or_else(
                |_| ("HTTP_STATUS".to_string(), response_body.clone()),
                |e| (e.error_code, e.message),
            );
        let retry_after_secs = response
            .headers
            .get("retry-after")
            .and_then(|value| parse_retry_after(value, jiff::Timestamp::now()))
            .map(|delay| delay.as_secs());

        let error = Error::ApiError {
            status,
            code,
            message,
            retry_after_secs,
        };

        if error.is_retryable() {
            log::warn!("Webull API error [{status}]: retrying");
        } else {
            log::error!("Webull API error: {error}");
        }

        Err(error)
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

        let body = self.signed_get(BARS_PATH, &query).await?;
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

        let body = self.signed_get(SNAPSHOT_PATH, &query).await?;
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

        let body = self.signed_get(TICK_PATH, &query).await?;
        let response: WebullTickResponse = serde_json::from_str(&body)?;
        Ok(response.result)
    }

    /// Returns a page of stock/ETF instrument profiles.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response cannot be parsed.
    pub async fn get_stock_instruments(
        &self,
        category: &str,
        symbols: Option<&str>,
        status: Option<&str>,
        sub_category: Option<&str>,
        pagination_key: Option<&str>,
    ) -> Result<WebullInstrumentPage> {
        let mut query = vec![("category".to_string(), category.to_string())];
        push_optional_query(&mut query, "symbols", symbols);
        push_optional_query(&mut query, "status", status);
        push_optional_query(&mut query, "sub_category", sub_category);
        push_optional_query(&mut query, "pagination_key", pagination_key);

        let body = self.signed_get(STOCK_INSTRUMENTS_PATH, &query).await?;
        Ok(serde_json::from_str(&body)?)
    }

    /// Returns a page of crypto instrument profiles.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response cannot be parsed.
    pub async fn get_crypto_instruments(
        &self,
        category: &str,
        symbols: Option<&str>,
        status: Option<&str>,
        pagination_key: Option<&str>,
    ) -> Result<WebullCryptoInstrumentPage> {
        let mut query = vec![("category".to_string(), category.to_string())];
        push_optional_query(&mut query, "symbols", symbols);
        push_optional_query(&mut query, "status", status);
        push_optional_query(&mut query, "pagination_key", pagination_key);

        let body = self.signed_get(CRYPTO_INSTRUMENTS_PATH, &query).await?;
        Ok(serde_json::from_str(&body)?)
    }
}

fn push_optional_query(query: &mut Vec<(String, String)>, key: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        query.push((key.to_string(), value.to_string()));
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
    use rstest::rstest;

    use super::*;
    use crate::common::testing;

    fn test_client(base_url: &str) -> WebullHttpClient {
        let config = WebullHttpConfig {
            base_url: base_url.to_string(),
            host: "127.0.0.1".to_string(),
            retry: RetryConfig {
                max_retries: 1,
                initial_delay_ms: 1,
                max_delay_ms: 1,
                jitter_ms: 0,
                ..Default::default()
            },
            ..Default::default()
        };

        WebullHttpClient::new_with_config(test_credential(), config).expect("builds client")
    }

    fn test_credential() -> Credential {
        Credential::new(
            "test-api-key".to_string(),
            "test-api-secret".to_string(),
            Some("test-access-token".to_string()),
        )
    }

    #[rstest]
    #[tokio::test]
    async fn test_get_bars_signs_and_parses() {
        let bars = r#"[{"symbol":"AAPL","time":"2026-09-04T14:55:00.000+0000","open":"318.98","high":"319.07","low":"318.3","close":"318.44","volume":"619677","trading_session":"RTH","instrument_id":"913256135"}]"#;
        let (base_url, requests) = testing::start(vec![(200, bars.to_string())]);

        let client = test_client(&base_url);

        let bars = client
            .get_bars("AAPL", "US_STOCK", "M5", 200, 1_788_550_800_000, None)
            .await
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
    #[tokio::test]
    async fn test_api_error_is_surfaced() {
        let error_body =
            r#"{"message":"Header x-signature is invalid.","error_code":"UNAUTHORIZED"}"#;
        let (base_url, _requests) = testing::start(vec![(401, error_body.to_string())]);

        let client = test_client(&base_url);

        let result = client
            .get_bars("AAPL", "US_STOCK", "M5", 100, 1_788_550_800_000, None)
            .await;

        match result {
            Err(Error::ApiError { status, code, .. }) => {
                assert_eq!(status, 401);
                assert_eq!(code, "UNAUTHORIZED");
            }
            other => panic!("expected ApiError, got {other:?}"),
        }
    }

    #[rstest]
    #[tokio::test]
    async fn test_rate_limit_retries_once() {
        let bars = "[]";
        let (base_url, requests) = testing::start(vec![
            (429, "rate limited".to_string()),
            (200, bars.to_string()),
        ]);

        let client = test_client(&base_url);

        let bars = client
            .get_bars("AAPL", "US_STOCK", "M5", 100, 1_788_550_800_000, None)
            .await
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

    #[rstest]
    #[tokio::test]
    async fn test_get_stock_instruments_signs_and_parses() {
        let body = r#"{"data":[{"name":"Apple Inc","instrument_id":"913256135","category":"US_STOCK","symbol":"AAPL","lot_size":"1","currency":"USD"}],"pagination_key":null}"#;
        let (base_url, requests) = testing::start(vec![(200, body.to_string())]);
        let client = test_client(&base_url);

        let page = client
            .get_stock_instruments("US_STOCK", Some("AAPL"), None, None, None)
            .await
            .expect("fetches instruments");

        assert_eq!(page.data.len(), 1);
        assert_eq!(page.data[0].symbol, "AAPL");

        let request = requests.recv().expect("captures request");
        assert!(request.contains("GET /trading/instruments/stocks/profiles/list?"));
        assert!(request.contains("category=US_STOCK"));
        assert!(request.contains("symbols=AAPL"));
        assert!(request.contains("x-signature:"));
    }
}
