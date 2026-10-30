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

//! Webull OpenAPI endpoint URLs and paths.

/// Default Webull OpenAPI base URL (production, US region).
pub const WEBULL_HTTP_BASE_URL: &str = "https://api.webull.com";

/// Host header used for request signing (must match the base URL host).
pub const WEBULL_HTTP_HOST: &str = "api.webull.com";

/// Path for the historical bars endpoint.
pub const BARS_PATH: &str = "/openapi/market-data/stock/bars";

/// Path for the quote snapshot endpoint.
pub const SNAPSHOT_PATH: &str = "/openapi/market-data/stock/snapshot";

/// Path for the trade tick endpoint.
pub const TICK_PATH: &str = "/openapi/market-data/stock/tick";

/// Stock/ETF instrument profiles (paginated discovery).
pub const STOCK_INSTRUMENTS_PATH: &str = "/trading/instruments/stocks/profiles/list";

/// Crypto instrument profiles (paginated discovery).
pub const CRYPTO_INSTRUMENTS_PATH: &str = "/trading/instruments/crypto/profiles/list";

/// Extracts the signing host from a base URL (scheme stripped, path dropped).
#[must_use]
pub fn host_from_base_url(base_url: &str) -> String {
    let without_scheme = base_url.split_once("//").map_or(base_url, |(_, rest)| rest);

    without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .to_string()
}
