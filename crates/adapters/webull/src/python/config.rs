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

//! Python bindings for Webull configuration.

use nautilus_core::string::secret::SecretString;
use pyo3::pymethods;

use crate::config::WebullDataClientConfig;

#[pymethods]
#[pyo3_stub_gen::derive::gen_stub_pymethods]
impl WebullDataClientConfig {
    /// Configuration for the Webull live data client.
    ///
    /// When credentials are `None`, the client reads the `WEBULL_API_KEY`,
    /// `WEBULL_API_SECRET`, and `WEBULL_ACCESS_TOKEN` environment variables
    /// at construction.
    #[new]
    #[pyo3(signature = (
        api_key = None,
        api_secret = None,
        access_token = None,
        base_url_http = None,
        proxy_url = None,
        category = None,
        poll_interval_secs = None,
        timeout_secs = None,
        max_retries = None,
        retry_delay_initial_ms = None,
        retry_delay_max_ms = None,
    ))]
    #[expect(clippy::too_many_arguments)]
    fn py_new(
        api_key: Option<String>,
        api_secret: Option<String>,
        access_token: Option<String>,
        base_url_http: Option<String>,
        proxy_url: Option<String>,
        category: Option<String>,
        poll_interval_secs: Option<u64>,
        timeout_secs: Option<u64>,
        max_retries: Option<u32>,
        retry_delay_initial_ms: Option<u64>,
        retry_delay_max_ms: Option<u64>,
    ) -> Self {
        let defaults = Self::default();

        Self {
            api_key: api_key.map(SecretString::from),
            api_secret: api_secret.map(SecretString::from),
            access_token: access_token.map(SecretString::from),
            base_url_http,
            proxy_url: proxy_url.map(SecretString::from),
            category: category.unwrap_or(defaults.category),
            poll_interval_secs: poll_interval_secs.unwrap_or(defaults.poll_interval_secs),
            timeout_secs: timeout_secs.or(defaults.timeout_secs),
            max_retries: max_retries.unwrap_or(defaults.max_retries),
            retry_delay_initial_ms: retry_delay_initial_ms
                .unwrap_or(defaults.retry_delay_initial_ms),
            retry_delay_max_ms: retry_delay_max_ms.unwrap_or(defaults.retry_delay_max_ms),
        }
    }

    #[getter]
    const fn has_proxy_url(&self) -> bool {
        self.proxy_url.is_some()
    }

    fn __repr__(&self) -> String {
        stringify!(WebullDataClientConfig).to_string()
    }
}
