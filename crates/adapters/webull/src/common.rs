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

//! Webull adapter constants, credential storage, and token file handling.

use std::{fmt::Debug, num::NonZeroU32, path::Path, sync::LazyLock};

use nautilus_core::{env::get_or_env_var_opt, string::secret::REDACTED};
use nautilus_model::identifiers::ClientId;
use nautilus_network::ratelimiter::quota::Quota;
use ustr::Ustr;
use zeroize::ZeroizeOnDrop;

/// The Webull adapter identifier string.
pub const WEBULL: &str = "WEBULL";

/// Static venue label for market data served by the Webull adapter.
pub static WEBULL_VENUE: LazyLock<nautilus_model::identifiers::Venue> =
    LazyLock::new(|| nautilus_model::identifiers::Venue::new(Ustr::from(WEBULL)));

/// Static client ID instance.
pub static WEBULL_CLIENT_ID: LazyLock<ClientId> =
    LazyLock::new(|| ClientId::new(Ustr::from(WEBULL)));

/// Environment variable name for the Webull API key.
pub const WEBULL_API_KEY: &str = "WEBULL_API_KEY";

/// Environment variable name for the Webull API secret.
pub const WEBULL_API_SECRET: &str = "WEBULL_API_SECRET";

/// Environment variable name for the Webull access token (2FA token).
pub const WEBULL_ACCESS_TOKEN: &str = "WEBULL_ACCESS_TOKEN";

/// Default Webull OpenAPI base URL (production, US region).
pub const WEBULL_HTTP_BASE_URL: &str = "https://api.webull.com";

/// Host header used for request signing (must match the base URL host).
pub const WEBULL_HTTP_HOST: &str = "api.webull.com";

/// Rate limit key for Webull OpenAPI requests.
pub const WEBULL_REST_RATE_KEY: &str = "webull_rest";

/// Rate limit for the Webull Market Data API (300 requests per minute).
pub static WEBULL_REST_QUOTA: LazyLock<Quota> =
    LazyLock::new(|| Quota::per_minute(NonZeroU32::new(300).expect("non-zero")));

/// Webull API credentials: the public app key, the HMAC signing secret,
/// and the account access token (issued by the 2FA token flow).
#[derive(Clone, ZeroizeOnDrop)]
pub struct Credential {
    api_key: Box<[u8]>,
    api_secret: Box<[u8]>,
    access_token: Option<Box<[u8]>>,
}

impl Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(stringify!(Credential))
            .field("api_key", &self.api_key_masked())
            .field("api_secret", &REDACTED)
            .field("access_token", &self.access_token.as_ref().is_some())
            .finish()
    }
}

impl Credential {
    /// Creates a new [`Credential`] instance.
    #[must_use]
    pub fn new(
        api_key: impl Into<String>,
        api_secret: impl Into<String>,
        access_token: Option<String>,
    ) -> Self {
        Self {
            api_key: api_key.into().into_bytes().into_boxed_slice(),
            api_secret: api_secret.into().into_bytes().into_boxed_slice(),
            access_token: access_token.map(|token| token.into_bytes().into_boxed_slice()),
        }
    }

    /// Returns the public API key.
    ///
    /// # Panics
    ///
    /// This method should never panic as the value is always valid UTF-8,
    /// having been created from a `String`.
    #[must_use]
    pub fn api_key(&self) -> &str {
        std::str::from_utf8(&self.api_key).expect("API key is valid UTF-8")
    }

    /// Returns the API signing secret.
    ///
    /// # Panics
    ///
    /// This method should never panic as the value is always valid UTF-8,
    /// having been created from a `String`.
    #[must_use]
    pub fn api_secret(&self) -> &str {
        std::str::from_utf8(&self.api_secret).expect("API secret is valid UTF-8")
    }

    /// Returns the account access token, if any.
    ///
    /// # Panics
    ///
    /// This method should never panic as the value is always valid UTF-8,
    /// having been created from a `String`.
    #[must_use]
    pub fn access_token(&self) -> Option<&str> {
        self.access_token
            .as_ref()
            .map(|token| std::str::from_utf8(token).expect("access token is valid UTF-8"))
    }

    /// Returns a masked version of the API key for logging purposes.
    ///
    /// Shows first 4 and last 4 characters with ellipsis in between.
    /// For keys shorter than 8 characters, shows asterisks only.
    #[must_use]
    pub fn api_key_masked(&self) -> String {
        nautilus_core::string::secret::mask_api_key(self.api_key())
    }

    /// Resolves a credential from the provided values or the `WEBULL_API_KEY`,
    /// `WEBULL_API_SECRET`, and `WEBULL_ACCESS_TOKEN` environment variables.
    ///
    /// # Errors
    ///
    /// Returns an error if the API key or API secret cannot be resolved.
    pub fn resolve(
        api_key: Option<String>,
        api_secret: Option<String>,
        access_token: Option<String>,
    ) -> anyhow::Result<Self> {
        let api_key = get_or_env_var_opt(api_key, WEBULL_API_KEY).ok_or_else(|| {
            anyhow::anyhow!(
                "API key must be provided or set in the '{WEBULL_API_KEY}' environment variable"
            )
        })?;
        let api_secret = get_or_env_var_opt(api_secret, WEBULL_API_SECRET)
            .ok_or_else(|| anyhow::anyhow!("API secret must be provided or set in the '{WEBULL_API_SECRET}' environment variable"))?;

        Ok(Self::new(api_key, api_secret, access_token))
    }
}

/// An access token entry as persisted by the Webull OpenAPI SDK token flow.
///
/// The SDK writes a three-line file: the token, its expiry as a millisecond
/// Unix timestamp, and its state (`PENDING`, `NORMAL`, `INVALID`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessTokenFile {
    /// The account access token.
    pub token: String,
    /// Token expiry as a millisecond Unix timestamp.
    pub expiry_ms: u64,
    /// Token state as reported by the API.
    pub state: String,
}

impl AccessTokenFile {
    /// Reads and parses an SDK-style token file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or does not contain
    /// the expected three-line format.
    pub fn read(path: &Path) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let mut lines = content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty());
        let (Some(token), Some(expiry), Some(state)) = (lines.next(), lines.next(), lines.next())
        else {
            return Err(std::io::Error::other(
                "token file must contain three lines: token, expiry, state",
            ));
        };

        Ok(Self {
            token: token.to_string(),
            expiry_ms: expiry.parse().map_err(|_| {
                std::io::Error::other("token expiry is not a millisecond timestamp")
            })?,
            state: state.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rstest::rstest;
    use tempfile::tempdir;

    use super::*;

    #[rstest]
    fn test_credential_accessors() {
        let credential = Credential::new(
            "my_api_key".to_string(),
            "my_api_secret".to_string(),
            Some("my_access_token".to_string()),
        );

        assert_eq!(credential.api_key(), "my_api_key");
        assert_eq!(credential.api_secret(), "my_api_secret");
        assert_eq!(credential.access_token(), Some("my_access_token"));
    }

    #[rstest]
    fn test_credential_no_access_token() {
        let credential = Credential::new("key".to_string(), "secret".to_string(), None);

        assert_eq!(credential.access_token(), None);
    }

    #[rstest]
    fn test_credential_debug_redaction() {
        let credential = Credential::new(
            "abcdef1234567890".to_string(),
            "the-api-secret".to_string(),
            Some("the-access-token".to_string()),
        );
        let debug_str = format!("{credential:?}");

        assert!(debug_str.contains("abcd...7890"));
        assert!(!debug_str.contains("the-api-secret"));
        assert!(!debug_str.contains("the-access-token"));
    }

    #[rstest]
    fn test_credential_resolve_missing_key() {
        assert!(Credential::resolve(None, Some("secret".to_string()), None).is_err());
    }

    #[rstest]
    fn test_credential_resolve_missing_secret() {
        assert!(Credential::resolve(Some("key".to_string()), None, None).is_err());
    }

    #[rstest]
    fn test_credential_resolve_explicit_values() {
        let credential = Credential::resolve(
            Some("key".to_string()),
            Some("secret".to_string()),
            Some("token".to_string()),
        )
        .expect("credential resolves");

        assert_eq!(credential.api_key(), "key");
        assert_eq!(credential.api_secret(), "secret");
        assert_eq!(credential.access_token(), Some("token"));
    }

    #[rstest]
    fn test_access_token_file_read() {
        let dir = tempdir().expect("tempdir");
        let path: PathBuf = dir.path().join("token.txt");
        std::fs::write(&path, "test-token-value\n1750000000000\nNORMAL\n").expect("write");

        let entry = AccessTokenFile::read(&path).expect("reads token file");
        assert_eq!(entry.token, "test-token-value");
        assert_eq!(entry.expiry_ms, 1_750_000_000_000);
        assert_eq!(entry.state, "NORMAL");
    }

    #[rstest]
    fn test_access_token_file_read_malformed() {
        let dir = tempdir().expect("tempdir");
        let path: PathBuf = dir.path().join("token.txt");
        std::fs::write(&path, "only-one-line\n").expect("write");

        assert!(AccessTokenFile::read(&path).is_err());
    }
}
