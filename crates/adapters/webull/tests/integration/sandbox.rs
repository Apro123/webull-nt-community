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

//! Sandbox smoke test for the Webull instrument provider.
//!
//! Loads one US equity from `api.sandbox.webull.com`. The test returns
//! immediately unless `WEBULL_API_KEY` and `WEBULL_API_SECRET` are set, so
//! runs that strip adapter environment variables (for example `make pre-flight`)
//! never depend on sandbox credentials. The production 2FA access token is not
//! sent: the sandbox app key authenticates on its own.

use std::time::Duration;

use nautilus_common::providers::InstrumentProvider;
use nautilus_model::{instruments::InstrumentAny, types::Currency};
use nautilus_webull::{
    common::{
        consts::{WEBULL_API_KEY, WEBULL_API_SECRET},
        credential::Credential,
        urls::host_from_base_url,
    },
    http::client::{WebullHttpClient, WebullHttpConfig},
    providers::WebullInstrumentProvider,
};
use rstest::rstest;

const SANDBOX_BASE_URL: &str = "https://api.sandbox.webull.com";
const SYMBOL: &str = "AAPL";
const MAX_ATTEMPTS: u32 = 3;

fn sandbox_credential() -> Option<Credential> {
    let api_key = std::env::var(WEBULL_API_KEY)
        .ok()
        .filter(|value| !value.is_empty())?;
    let api_secret = std::env::var(WEBULL_API_SECRET)
        .ok()
        .filter(|value| !value.is_empty())?;

    // Sandbox app credentials do not use the production 2FA token.
    Some(Credential::new(api_key, api_secret, None))
}

async fn load_aapl(provider: &mut WebullInstrumentProvider) {
    for attempt in 0..MAX_ATTEMPTS {
        match provider.load_all(None).await {
            Ok(()) => return,
            Err(e) if attempt + 1 == MAX_ATTEMPTS => {
                panic!("sandbox instrument load failed: {e}");
            }
            Err(_) => {
                tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
            }
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_sandbox_instrument_provider_loads_aapl() {
    let Some(credential) = sandbox_credential() else {
        return;
    };

    let http = WebullHttpClient::new_with_config(
        credential,
        WebullHttpConfig {
            host: host_from_base_url(SANDBOX_BASE_URL),
            base_url: SANDBOX_BASE_URL.to_string(),
            ..Default::default()
        },
    )
    .expect("sandbox HTTP client");

    let mut provider = WebullInstrumentProvider::new(http, vec![SYMBOL.to_string()], false);
    load_aapl(&mut provider).await;

    assert!(provider.store().is_initialized());
    let instruments = provider.store().list_all();
    assert_eq!(instruments.len(), 1);

    match instruments[0] {
        InstrumentAny::Equity(equity) => {
            assert_eq!(equity.id.to_string(), "AAPL.WEBULL");
            assert_eq!(equity.raw_symbol.as_str(), SYMBOL);
            assert_eq!(equity.currency, Currency::USD());
            assert_eq!(equity.price_precision, 2);
        }
        other => panic!("expected Equity, was {other:?}"),
    }
}
