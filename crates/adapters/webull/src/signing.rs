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

//! Webull OpenAPI request signing.
//!
//! All requests are signed with HMAC-SHA1. The string to sign is the request
//! path, the query parameters plus six fixed signing headers (keys sorted
//! ascending, joined as `k=v` pairs), and the upper-case MD5 digest of the
//! request body when one is present:
//!
//! ```text
//! str1 = query params + signing headers, sorted, joined as k1=v1&k2=v2
//! str2 = MD5(body).upper()                # empty for GET requests
//! str3 = path + "&" + str1 + ("&" + str2 if body else "")
//! signature = base64(HMAC-SHA1(key = api_secret + "&", msg = percent_encode(str3)))
//! ```

use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use md5::{Digest, Md5};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

/// The algorithm identifier reported in the `x-signature-algorithm` header.
pub const SIGNATURE_ALGORITHM: &str = "HMAC-SHA1";

/// The signature scheme version reported in the `x-signature-version` header.
pub const SIGNATURE_VERSION: &str = "1.0";

/// Computes the Webull request signature for the given inputs.
///
/// # Arguments
///
/// * `api_secret` - the app secret; never sent on the wire.
/// * `host` - the request host header value, e.g. `api.webull.com`.
/// * `path` - the request path, e.g. `/openapi/market-data/stock/bars`.
/// * `query` - the raw (unencoded) query parameters.
/// * `body` - the request body, or `None` for bodyless requests.
/// * `timestamp` - UTC timestamp in `YYYY-MM-DDThh:mm:ssZ` form.
/// * `nonce` - a random per-request nonce.
///
/// # Panics
///
/// Panics only if the HMAC construction rejects the key, which cannot happen
/// with HMAC-SHA1 (keys of any length are accepted).
#[expect(clippy::too_many_arguments)]
#[must_use]
pub fn sign_request(
    api_secret: &str,
    host: &str,
    path: &str,
    query: &[(String, String)],
    body: Option<&str>,
    app_key: &str,
    timestamp: &str,
    nonce: &str,
) -> String {
    let mut items: Vec<(String, String)> = query.to_vec();
    items.push(("x-app-key".to_string(), app_key.to_string()));
    items.push((
        "x-signature-algorithm".to_string(),
        SIGNATURE_ALGORITHM.to_string(),
    ));
    items.push((
        "x-signature-version".to_string(),
        SIGNATURE_VERSION.to_string(),
    ));
    items.push(("x-signature-nonce".to_string(), nonce.to_string()));
    items.push(("x-timestamp".to_string(), timestamp.to_string()));
    items.push(("host".to_string(), host.to_string()));
    items.sort();

    let str1 = items
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&");

    let str3 = match body {
        Some(body) if !body.is_empty() => {
            let str2 = hex_digest_upper(body.as_bytes());
            format!("{path}&{str1}&{str2}")
        }
        _ => format!("{path}&{str1}"),
    };

    let encoded = nautilus_core::string::urlencoding::encode(&str3);
    let mut mac = HmacSha1::new_from_slice(format!("{api_secret}&").as_bytes())
        .expect("HMAC accepts arbitrary keys");
    mac.update(encoded.as_bytes());
    STANDARD.encode(mac.finalize().into_bytes())
}

/// Returns the upper-case MD5 hex digest of `body`.
fn hex_digest_upper(body: &[u8]) -> String {
    let digest = Md5::digest(body);
    digest.iter().map(|byte| format!("{byte:02X}")).collect()
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    /// Reproduces the worked example from the Webull OpenAPI documentation,
    /// verified byte-for-byte against the reference implementation.
    #[rstest]
    fn test_sign_request_documentation_worked_example() {
        let query = vec![
            ("a1".to_string(), "webull".to_string()),
            ("a2".to_string(), "123".to_string()),
            ("a3".to_string(), "xxx".to_string()),
            ("q1".to_string(), "yyy".to_string()),
        ];
        let body = "{\"k1\":123,\"k2\":\"this is the api request body\",\"k3\":true,\"k4\":{\"foo\":[1,2]}}";

        let signature = sign_request(
            "0f50a2e853334a9aae1a783bee120c1f",
            "api.webull.com",
            "/trade/place_order",
            &query,
            Some(body),
            "776da210ab4a452795d74e726ebd74b6",
            "2022-01-04T03:55:31Z",
            "48ef5afed43d4d91ae514aaeafbc29ba",
        );

        assert_eq!(signature, "kvlS6opdZDhEBo5jq40nHYXaLvM=");
    }

    #[rstest]
    fn test_sign_request_bodyless_get_omits_body_digest() {
        let query = vec![("symbol".to_string(), "AAPL".to_string())];

        let signature = sign_request(
            "secret",
            "api.webull.com",
            "/openapi/market-data/stock/snapshot",
            &query,
            None,
            "key",
            "2026-09-04T03:55:31Z",
            "nonce",
        );

        assert_ne!(signature, "");
        assert!(signature.ends_with('='));

        // An empty body must produce the same signature as no body at all.
        let signature_empty = sign_request(
            "secret",
            "api.webull.com",
            "/openapi/market-data/stock/snapshot",
            &query,
            Some(""),
            "key",
            "2026-09-04T03:55:31Z",
            "nonce",
        );
        assert_eq!(signature, signature_empty);
    }

    #[rstest]
    fn test_sign_request_is_deterministic_and_input_sensitive() {
        let query = vec![("symbol".to_string(), "AAPL".to_string())];
        let args = |nonce: &str| {
            sign_request(
                "secret",
                "api.webull.com",
                "/openapi/market-data/stock/bars",
                &query,
                None,
                "key",
                "2026-09-04T03:55:31Z",
                nonce,
            )
        };

        assert_eq!(args("nonce-a"), args("nonce-a"));
        assert_ne!(args("nonce-a"), args("nonce-b"));
        assert_ne!(
            args("nonce-a"),
            sign_request(
                "other-secret",
                "api.webull.com",
                "/openapi/market-data/stock/bars",
                &query,
                None,
                "key",
                "2026-09-04T03:55:31Z",
                "nonce-a",
            )
        );
    }
}
