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

//! Webull API credential storage.

use std::fmt::Debug;

use nautilus_core::{env::get_or_env_var_opt, string::secret::REDACTED};
use zeroize::ZeroizeOnDrop;

use super::consts::{WEBULL_ACCESS_TOKEN, WEBULL_API_KEY, WEBULL_API_SECRET};

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

    /// Resolves credentials from the provided values, falling back to the
    /// `WEBULL_API_KEY`, `WEBULL_API_SECRET`, and `WEBULL_ACCESS_TOKEN`
    /// environment variables.
    #[must_use]
    pub fn resolve(
        api_key: Option<String>,
        api_secret: Option<String>,
        access_token: Option<String>,
    ) -> Self {
        Self::new(
            get_or_env_var_opt(api_key, WEBULL_API_KEY).unwrap_or_default(),
            get_or_env_var_opt(api_secret, WEBULL_API_SECRET).unwrap_or_default(),
            get_or_env_var_opt(access_token, WEBULL_ACCESS_TOKEN),
        )
    }

    /// Returns the public API key.
    ///
    /// The API key is always valid UTF-8 because it is created from a `String`.
    #[allow(
        clippy::missing_panics_doc,
        reason = "the API key is created from a UTF-8 String"
    )]
    #[must_use]
    pub fn api_key(&self) -> &str {
        std::str::from_utf8(&self.api_key).expect("API key is valid UTF-8")
    }

    /// Returns the API signing secret.
    ///
    /// The API secret is always valid UTF-8 because it is created from a `String`.
    #[allow(
        clippy::missing_panics_doc,
        reason = "the API secret is created from a UTF-8 String"
    )]
    #[must_use]
    pub fn api_secret(&self) -> &str {
        std::str::from_utf8(&self.api_secret).expect("API secret is valid UTF-8")
    }

    /// Returns the account access token, if any.
    ///
    /// The access token is always valid UTF-8 because it is created from a `String`.
    #[allow(
        clippy::missing_panics_doc,
        reason = "the access token is created from a UTF-8 String"
    )]
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
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

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
}
