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

//! Instrument provider for the Webull adapter.

use std::{collections::HashMap, fmt::Debug};

use async_trait::async_trait;
use nautilus_common::providers::{InstrumentProvider, InstrumentStore};
use nautilus_core::time::get_atomic_clock_realtime;
use nautilus_model::{identifiers::InstrumentId, instruments::InstrumentAny};

use crate::{
    common::{
        consts::WEBULL_VENUE,
        parse::{parse_crypto_instrument, parse_stock_instrument},
    },
    http::{
        client::WebullHttpClient,
        models::{WebullCryptoInstrument, WebullStockInstrument},
    },
};

const DEFAULT_STOCK_CATEGORY: &str = "US_STOCK";
const DEFAULT_CRYPTO_CATEGORY: &str = "US_CRYPTO";
const MAX_INSTRUMENT_PAGES: usize = 50;

/// Provides Webull instruments via the REST discovery APIs.
pub struct WebullInstrumentProvider {
    store: InstrumentStore,
    http_client: WebullHttpClient,
    symbols: Vec<String>,
    load_crypto: bool,
}

impl Debug for WebullInstrumentProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(stringify!(WebullInstrumentProvider))
            .field("store", &self.store)
            .field("http_client", &self.http_client)
            .field("symbols", &self.symbols)
            .field("load_crypto", &self.load_crypto)
            .finish()
    }
}

impl WebullInstrumentProvider {
    /// Creates a new provider with an empty store.
    #[must_use]
    pub fn new(http_client: WebullHttpClient, symbols: Vec<String>, load_crypto: bool) -> Self {
        Self {
            store: InstrumentStore::new(),
            http_client,
            symbols: normalize_symbols(symbols),
            load_crypto,
        }
    }

    /// Returns the configured symbol filters.
    #[must_use]
    pub fn symbols(&self) -> &[String] {
        &self.symbols
    }

    /// Returns whether crypto instruments load by default.
    #[must_use]
    pub const fn load_crypto(&self) -> bool {
        self.load_crypto
    }

    /// Returns a reference to the underlying HTTP client.
    #[must_use]
    pub const fn http_client(&self) -> &WebullHttpClient {
        &self.http_client
    }

    /// Adds instruments to the store.
    pub fn add_instruments(&mut self, instruments: Vec<InstrumentAny>) {
        self.store.add_bulk(instruments);
    }

    async fn fetch_stock_instruments(
        &self,
        symbols: Option<&str>,
        status: Option<&str>,
        sub_category: Option<&str>,
    ) -> anyhow::Result<Vec<WebullStockInstrument>> {
        let mut profiles = Vec::new();
        let mut pagination_key: Option<String> = None;

        for _ in 0..MAX_INSTRUMENT_PAGES {
            let page = self
                .http_client
                .get_stock_instruments(
                    DEFAULT_STOCK_CATEGORY,
                    symbols,
                    status,
                    sub_category,
                    pagination_key.as_deref(),
                )
                .await
                .map_err(|e| anyhow::anyhow!("failed to fetch Webull stock instruments: {e}"))?;

            profiles.extend(page.data);

            match page.pagination_key.filter(|key| !key.is_empty()) {
                Some(key) => pagination_key = Some(key),
                None => break,
            }

            // Symbol-filtered requests are expected to fit on a single page
            if symbols.is_some() {
                break;
            }
        }

        Ok(profiles)
    }

    async fn fetch_crypto_instruments(
        &self,
        symbols: Option<&str>,
        status: Option<&str>,
    ) -> anyhow::Result<Vec<WebullCryptoInstrument>> {
        let mut profiles = Vec::new();
        let mut pagination_key: Option<String> = None;

        for _ in 0..MAX_INSTRUMENT_PAGES {
            let page = self
                .http_client
                .get_crypto_instruments(
                    DEFAULT_CRYPTO_CATEGORY,
                    symbols,
                    status,
                    pagination_key.as_deref(),
                )
                .await
                .map_err(|e| anyhow::anyhow!("failed to fetch Webull crypto instruments: {e}"))?;

            profiles.extend(page.data);

            match page.pagination_key.filter(|key| !key.is_empty()) {
                Some(key) => pagination_key = Some(key),
                None => break,
            }

            if symbols.is_some() {
                break;
            }
        }

        Ok(profiles)
    }

    async fn load_instruments(
        &self,
        symbols: &[String],
        status: Option<&str>,
        sub_category: Option<&str>,
        include_crypto: bool,
    ) -> anyhow::Result<Vec<InstrumentAny>> {
        let symbols_param = join_symbols(symbols);
        let stock_profiles = self
            .fetch_stock_instruments(symbols_param.as_deref(), status, sub_category)
            .await?;

        let mut instruments = parse_stock_profiles(stock_profiles);

        if include_crypto {
            let crypto_profiles = self
                .fetch_crypto_instruments(symbols_param.as_deref(), status)
                .await?;
            instruments.extend(parse_crypto_profiles(crypto_profiles));
        }

        Ok(instruments)
    }
}

#[async_trait(?Send)]
impl InstrumentProvider for WebullInstrumentProvider {
    fn store(&self) -> &InstrumentStore {
        &self.store
    }

    fn store_mut(&mut self) -> &mut InstrumentStore {
        &mut self.store
    }

    async fn load_all(&mut self, filters: Option<&HashMap<String, String>>) -> anyhow::Result<()> {
        let resolved = resolve_load_filters(&self.symbols, self.load_crypto, filters)?;
        let instruments = self
            .load_instruments(
                &resolved.symbols,
                resolved.status.as_deref(),
                resolved.sub_category.as_deref(),
                resolved.include_crypto,
            )
            .await?;

        self.store.clear();
        self.add_instruments(instruments);
        self.store.set_initialized();

        Ok(())
    }

    async fn load(
        &mut self,
        instrument_id: &InstrumentId,
        filters: Option<&HashMap<String, String>>,
    ) -> anyhow::Result<()> {
        if self.store.contains(instrument_id) {
            return Ok(());
        }

        if instrument_id.venue != *WEBULL_VENUE {
            anyhow::bail!(
                "WebullInstrumentProvider cannot load non-WEBULL instrument {instrument_id}"
            );
        }

        let resolved = resolve_load_filters(
            &[instrument_id.symbol.to_string()],
            self.load_crypto,
            filters,
        )?;
        let instruments = self
            .load_instruments(
                &resolved.symbols,
                resolved.status.as_deref(),
                resolved.sub_category.as_deref(),
                resolved.include_crypto,
            )
            .await?;

        self.add_instruments(instruments);

        if self.store.contains(instrument_id) {
            Ok(())
        } else {
            anyhow::bail!("Webull instrument not found: {instrument_id}")
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedLoadFilters {
    symbols: Vec<String>,
    status: Option<String>,
    sub_category: Option<String>,
    include_crypto: bool,
}

fn resolve_load_filters(
    default_symbols: &[String],
    default_include_crypto: bool,
    filters: Option<&HashMap<String, String>>,
) -> anyhow::Result<ResolvedLoadFilters> {
    let symbols = filters
        .and_then(|map| map.get("symbols"))
        .map(|value| split_symbols(value))
        .unwrap_or_else(|| default_symbols.to_vec());

    let status = filters
        .and_then(|map| map.get("status"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let sub_category = filters
        .and_then(|map| map.get("sub_category"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let include_crypto = filters
        .and_then(|map| map.get("include_crypto"))
        .map(|value| value.parse::<bool>())
        .transpose()
        .map_err(|e| anyhow::anyhow!("invalid Webull `include_crypto` filter: {e}"))?
        .unwrap_or(default_include_crypto);

    Ok(ResolvedLoadFilters {
        symbols: normalize_symbols(symbols),
        status,
        sub_category,
        include_crypto,
    })
}

fn parse_stock_profiles(profiles: Vec<WebullStockInstrument>) -> Vec<InstrumentAny> {
    let ts_init = get_atomic_clock_realtime().get_time_ns();
    let mut instruments = Vec::with_capacity(profiles.len());

    for profile in profiles {
        match parse_stock_instrument(&profile, ts_init) {
            Ok(instrument) => instruments.push(instrument),
            Err(e) => log::warn!(
                "Failed to parse Webull stock instrument {}: {e}",
                profile.symbol
            ),
        }
    }

    instruments
}

fn parse_crypto_profiles(profiles: Vec<WebullCryptoInstrument>) -> Vec<InstrumentAny> {
    let ts_init = get_atomic_clock_realtime().get_time_ns();
    let mut instruments = Vec::with_capacity(profiles.len());

    for profile in profiles {
        match parse_crypto_instrument(&profile, ts_init) {
            Ok(instrument) => instruments.push(instrument),
            Err(e) => log::warn!(
                "Failed to parse Webull crypto instrument {}: {e}",
                profile.symbol
            ),
        }
    }

    instruments
}

fn join_symbols(symbols: &[String]) -> Option<String> {
    if symbols.is_empty() {
        None
    } else {
        Some(symbols.join(","))
    }
}

fn split_symbols(value: &str) -> Vec<String> {
    normalize_symbols(value.split(',').map(ToOwned::to_owned).collect())
}

fn normalize_symbols(symbols: Vec<String>) -> Vec<String> {
    let mut symbols: Vec<_> = symbols
        .into_iter()
        .map(|symbol| symbol.trim().to_string())
        .filter(|symbol| !symbol.is_empty())
        .collect();
    symbols.sort();
    symbols.dedup();
    symbols
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    fn test_resolve_load_filters_defaults() {
        let resolved = resolve_load_filters(&["AAPL".to_string()], false, None).unwrap();

        assert_eq!(resolved.symbols, vec!["AAPL".to_string()]);
        assert!(!resolved.include_crypto);
        assert!(resolved.status.is_none());
        assert!(resolved.sub_category.is_none());
    }

    #[rstest]
    fn test_resolve_load_filters_from_map() {
        let filters = HashMap::from([
            ("symbols".to_string(), " MSFT, aapl ,MSFT ".to_string()),
            ("status".to_string(), "ACTIVE".to_string()),
            ("sub_category".to_string(), "ETF".to_string()),
            ("include_crypto".to_string(), "true".to_string()),
        ]);

        let resolved = resolve_load_filters(&[], false, Some(&filters)).unwrap();

        assert_eq!(
            resolved.symbols,
            vec!["MSFT".to_string(), "aapl".to_string()]
        );
        assert_eq!(resolved.status.as_deref(), Some("ACTIVE"));
        assert_eq!(resolved.sub_category.as_deref(), Some("ETF"));
        assert!(resolved.include_crypto);
    }

    #[rstest]
    fn test_resolve_load_filters_rejects_invalid_include_crypto() {
        let filters = HashMap::from([("include_crypto".to_string(), "yes".to_string())]);
        let error = resolve_load_filters(&[], false, Some(&filters)).unwrap_err();
        assert!(error.to_string().contains("include_crypto"));
    }
}
