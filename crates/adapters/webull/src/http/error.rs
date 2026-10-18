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
