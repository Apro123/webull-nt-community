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

//! Configuration for the Webull data client.

use nautilus_core::string::secret::SecretString;
use serde::{Deserialize, Serialize};

/// Configuration for [`WebullDataClient`](crate::data::WebullDataClient).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "nautilus_trader.adapters.webull", from_py_object)
)]
#[cfg_attr(
    feature = "python",
    pyo3_stub_gen::derive::gen_stub_pyclass(module = "nautilus_trader.adapters.webull")
)]
pub struct WebullDataClientConfig {
    /// App key; `None` falls back to the `WEBULL_API_KEY` environment variable.
    pub api_key: Option<SecretString>,
    /// HMAC signing secret; `None` falls back to the `WEBULL_API_SECRET` environment variable.
    pub api_secret: Option<SecretString>,
    /// Account access token; `None` falls back to the `WEBULL_ACCESS_TOKEN` environment variable.
    pub access_token: Option<SecretString>,
    /// Optional override for the REST base URL.
    pub base_url_http: Option<String>,
    /// Optional explicit proxy URL for the HTTP transport.
    pub proxy_url: Option<SecretString>,
    /// Webull instrument category used for market data requests.
    pub category: String,
    /// Seconds between polling passes for active subscriptions.
    pub poll_interval_secs: u64,
    /// Per-request timeout in seconds; `None` disables the timeout.
    pub timeout_secs: Option<u64>,
    /// Maximum retries for a failed request.
    pub max_retries: u32,
    /// Initial retry delay in milliseconds.
    pub retry_delay_initial_ms: u64,
    /// Maximum retry backoff in milliseconds; venue `Retry-After` delays can exceed it.
    pub retry_delay_max_ms: u64,
}

#[cfg(feature = "python")]
nautilus_core::impl_pyo3_config_getters!(WebullDataClientConfig {
    base_url_http: Option<String>,
    category: String,
    poll_interval_secs: u64,
    timeout_secs: Option<u64>,
    max_retries: u32,
    retry_delay_initial_ms: u64,
    retry_delay_max_ms: u64,
});

impl Default for WebullDataClientConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            api_secret: None,
            access_token: None,
            base_url_http: None,
            proxy_url: None,
            category: "US_STOCK".to_string(),
            poll_interval_secs: 5,
            timeout_secs: Some(60),
            max_retries: 3,
            retry_delay_initial_ms: 1_000,
            retry_delay_max_ms: 60_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    fn test_config_serde_round_trip_with_defaults() {
        let config: WebullDataClientConfig = serde_json::from_str("{}").expect("deserializes");

        assert_eq!(config.category, "US_STOCK");
        assert_eq!(config.poll_interval_secs, 5);
        assert!(config.api_key.is_none());

        let json = serde_json::to_string(&config).expect("serializes");
        let round_tripped: WebullDataClientConfig =
            serde_json::from_str(&json).expect("re-deserializes");

        assert_eq!(round_tripped.timeout_secs, Some(60));
    }

    #[rstest]
    fn test_config_debug_redacts_secrets() {
        let config = WebullDataClientConfig {
            api_secret: Some(SecretString::from("the-api-secret")),
            ..Default::default()
        };

        let debug = format!("{config:?}");

        assert!(!debug.contains("the-api-secret"));
    }
}
