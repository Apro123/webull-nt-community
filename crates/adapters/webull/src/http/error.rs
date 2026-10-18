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

//! Error types for the Webull HTTP client.

use std::time::Duration;

use nautilus_network::{http::HttpClientError, retry::RetryError};

/// Errors returned by the Webull market data client.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An HTTP request failed at the transport level.
    #[error("HTTP request failed: {0}")]
    Http(#[from] HttpClientError),

    /// The Webull API returned an error response.
    #[error("Webull API error [{status} {code}]: {message}")]
    ApiError {
        /// The HTTP status code.
        status: u16,
        /// The Webull error code.
        code: String,
        /// The Webull error message.
        message: String,
        /// The server-advised retry delay in seconds, if any.
        retry_after_secs: Option<u64>,
    },

    /// Failed to deserialize the JSON response body.
    #[error("Failed to parse response body as JSON: {0}")]
    JsonParse(#[from] serde_json::Error),

    /// A venue bar time could not be parsed.
    #[error("Invalid bar time: {0}")]
    BarTime(String),

    /// A venue price could not be parsed as a decimal.
    #[error("Invalid price: {0}")]
    Price(String),

    /// A venue volume could not be parsed as a decimal.
    #[error("Invalid volume: {0}")]
    Volume(String),

    /// A venue bar failed the engine's bar validity checks.
    #[error("Invalid bar {time}: {message}")]
    InvalidBar {
        /// The bar time of the rejected bar.
        time: String,
        /// The validity failure reported by the engine.
        message: String,
    },

    /// A retryable operation exhausted its retry policy.
    #[error("Retry failed: {0}")]
    Retry(RetryError),
}

/// The result type alias for the Webull HTTP client.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Returns `true` if the operation that produced this error may succeed when retried.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Http(_) => true,
            Self::ApiError { status, .. } => *status == 429 || *status >= 500,
            Self::Retry(error) => {
                matches!(error, RetryError::OperationTimeout { .. })
            }
            Self::JsonParse(_)
            | Self::BarTime(_)
            | Self::Price(_)
            | Self::Volume(_)
            | Self::InvalidBar { .. } => false,
        }
    }

    /// Returns the server-advised retry delay, if the venue provided one.
    #[must_use]
    pub fn retry_delay(&self) -> Option<Duration> {
        match self {
            Self::ApiError {
                retry_after_secs, ..
            } => retry_after_secs.map(Duration::from_secs),
            _ => None,
        }
    }
}
