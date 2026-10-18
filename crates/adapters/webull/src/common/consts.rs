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

//! Webull adapter constants.

use std::num::NonZeroU32;
use std::sync::LazyLock;

use nautilus_model::identifiers::{ClientId, Venue};
use nautilus_network::ratelimiter::quota::Quota;
use ustr::Ustr;

/// The Webull adapter identifier string.
pub const WEBULL: &str = "WEBULL";

/// Static venue label for market data served by the Webull adapter.
pub static WEBULL_VENUE: LazyLock<Venue> = LazyLock::new(|| Venue::new(Ustr::from(WEBULL)));

/// Static client ID instance.
pub static WEBULL_CLIENT_ID: LazyLock<ClientId> =
    LazyLock::new(|| ClientId::new(Ustr::from(WEBULL)));

/// Rate limit key for Webull OpenAPI requests.
pub const WEBULL_REST_RATE_KEY: &str = "webull_rest";

/// Rate limit for the Webull Market Data API (production tier: 60 requests
/// per minute per app key; the sandbox tier is lower).
pub static WEBULL_REST_QUOTA: LazyLock<Quota> =
    LazyLock::new(|| Quota::per_minute(NonZeroU32::new(60).expect("non-zero")));
