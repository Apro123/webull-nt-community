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

//! Python bindings for Webull historical market data.

use std::fmt::Debug;

use nautilus_core::{
    UnixNanos,
    python::{enums::parse_enum, to_pyruntime_err, to_pyvalue_err},
};
use nautilus_model::{
    data::{QuoteTick, TradeTick},
    identifiers::{InstrumentId, Symbol, TradeId},
};
use pyo3::{IntoPyObjectExt, prelude::*};
use ustr::Ustr;

use crate::{
    common::{consts::WEBULL_VENUE, credential::Credential, urls::host_from_base_url},
    data::{aggressor_side_from_str, price_from_str, quantity_from_str},
    historical::{WebullHistoricalClient as CoreWebullHistoricalClient, bars_to_nautilus},
    http::client::{WebullHttpClient, WebullHttpConfig},
};

use super::Timespan;

/// Python wrapper for the core Webull historical client.
#[derive(Clone)]
#[pyo3::pyclass(module = "nautilus_trader.adapters.webull", from_py_object)]
#[pyo3_stub_gen::derive::gen_stub_pyclass(module = "nautilus_trader.adapters.webull")]
pub struct WebullHistoricalClient {
    inner: CoreWebullHistoricalClient,
}

#[pymethods]
#[pyo3_stub_gen::derive::gen_stub_pymethods]
impl WebullHistoricalClient {
    /// Core Webull historical client for fetching historical market data.
    ///
    /// When credentials are `None`, the `WEBULL_API_KEY`, `WEBULL_API_SECRET`,
    /// and `WEBULL_ACCESS_TOKEN` environment variables are used. The venue's
    /// 2FA access token (first line of the SDK token file) is required by
    /// accounts with 2FA enforced and is sent on every request.
    #[new]
    #[pyo3(signature = (api_key = None, api_secret = None, access_token = None, base_url = None))]
    fn py_new(
        api_key: Option<String>,
        api_secret: Option<String>,
        access_token: Option<String>,
        base_url: Option<String>,
    ) -> PyResult<Self> {
        let credential = Credential::resolve(api_key, api_secret, access_token);

        let config = match base_url {
            Some(url) => WebullHttpConfig {
                host: host_from_base_url(&url),
                base_url: url,
                ..Default::default()
            },
            None => WebullHttpConfig::default(),
        };

        if credential.api_key().is_empty() || credential.api_secret().is_empty() {
            return Err(to_pyvalue_err(
                "Webull API key and secret are required (arguments or environment)",
            ));
        }

        let http =
            WebullHttpClient::new_with_config(credential, config).map_err(to_pyruntime_err)?;

        Ok(Self {
            inner: CoreWebullHistoricalClient::new(http),
        })
    }

    /// Fetches historical bars for an instrument in `[start_ms, end_ms]` as
    /// Nautilus `Bar` objects, oldest first.
    ///
    /// `trading_sessions` is optional, e.g. `PRE,RTH,ATH` (omit for RTH-only,
    /// the venue default). `category` selects the venue's instrument category.
    #[expect(clippy::too_many_arguments)]
    #[pyo3(signature = (instrument_id, timespan, start_ms, end_ms, trading_sessions = None, category = String::from("US_STOCK"), price_precision = 2))]
    fn get_history_bars<'py>(
        &self,
        py: Python<'py>,
        instrument_id: InstrumentId,
        timespan: &str,
        start_ms: u64,
        end_ms: u64,
        trading_sessions: Option<String>,
        category: String,
        price_precision: u8,
    ) -> PyResult<Bound<'py, PyAny>> {
        let timespan: Timespan = parse_enum(timespan, stringify!(timespan))?;
        let symbol = instrument_id.symbol.to_string();
        let inner = self.inner.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let venue_bars = inner
                .get_history_bars(
                    &symbol,
                    category.as_str(),
                    timespan,
                    start_ms,
                    end_ms,
                    trading_sessions.as_deref(),
                )
                .await
                .map_err(to_pyruntime_err)?;
            let bars = bars_to_nautilus(&venue_bars, &instrument_id, timespan, price_precision)
                .map_err(to_pyruntime_err)?;

            Python::attach(|py| bars.into_py_any(py))
        })
    }

    /// Fetches live quote snapshots for the given symbols as `QuoteTick`s.
    ///
    /// Instruments are placed on the Webull venue; the venue quotes stock
    /// sizes in whole shares.
    #[pyo3(signature = (symbols, category = String::from("US_STOCK"), price_precision = 2))]
    fn get_snapshot<'py>(
        &self,
        py: Python<'py>,
        symbols: Vec<String>,
        category: String,
        price_precision: u8,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let snapshots = inner
                .http()
                .get_snapshot(&symbols, category.as_str())
                .await
                .map_err(to_pyruntime_err)?;

            let mut ticks = Vec::with_capacity(snapshots.len());

            for snapshot in snapshots {
                let instrument_id = InstrumentId::new(Symbol::from(snapshot.symbol), *WEBULL_VENUE);
                let ts_event = UnixNanos::from(snapshot.quote_time * 1_000_000);

                ticks.push(
                    QuoteTick::new_checked(
                        instrument_id,
                        price_from_str(&snapshot.bid, price_precision).map_err(to_pyruntime_err)?,
                        price_from_str(&snapshot.ask, price_precision).map_err(to_pyruntime_err)?,
                        quantity_from_str(&snapshot.bid_size, 0).map_err(to_pyruntime_err)?,
                        quantity_from_str(&snapshot.ask_size, 0).map_err(to_pyruntime_err)?,
                        ts_event,
                        ts_event,
                    )
                    .map_err(to_pyruntime_err)?,
                );
            }

            Python::attach(|py| ticks.into_py_any(py))
        })
    }

    /// Fetches trade ticks ending at `end_ms` as `TradeTick`s, newest first.
    #[pyo3(signature = (instrument_id, end_ms, category = String::from("US_STOCK"), count = 200, price_precision = 2))]
    fn get_ticks<'py>(
        &self,
        py: Python<'py>,
        instrument_id: InstrumentId,
        end_ms: u64,
        category: String,
        count: u32,
        price_precision: u8,
    ) -> PyResult<Bound<'py, PyAny>> {
        let symbol = instrument_id.symbol.to_string();
        let inner = self.inner.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let ticks = inner
                .http()
                .get_ticks(&symbol, category.as_str(), count, end_ms)
                .await
                .map_err(to_pyruntime_err)?;

            let mut trades = Vec::with_capacity(ticks.len());

            for tick in ticks {
                let ts_event = tick
                    .time
                    .parse::<u64>()
                    .map(|ms| UnixNanos::from(ms * 1_000_000))
                    .map_err(to_pyruntime_err)?;

                trades.push(TradeTick::new(
                    instrument_id,
                    price_from_str(&tick.price, price_precision).map_err(to_pyruntime_err)?,
                    quantity_from_str(&tick.volume, 0).map_err(to_pyruntime_err)?,
                    aggressor_side_from_str(&tick.side),
                    TradeId::new(Ustr::from(&tick.time)),
                    ts_event,
                    ts_event,
                ));
            }

            Python::attach(|py| trades.into_py_any(py))
        })
    }

    fn __repr__(&self) -> String {
        stringify!(WebullHistoricalClient).to_string()
    }
}

impl Debug for WebullHistoricalClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(stringify!(WebullHistoricalClient))
            .field("inner", &self.inner)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    fn test_client_construction_requires_credentials() {
        // Empty strings bypass the environment fallback, so the constructor
        // must fail closed rather than build a client that would only fail on
        // the first request.
        let result =
            WebullHistoricalClient::py_new(Some(String::new()), Some(String::new()), None, None);

        assert!(result.is_err(), "missing credentials must be rejected");
    }

    #[rstest]
    fn test_client_construction_masks_credentials() {
        let client = WebullHistoricalClient::py_new(
            Some("test-key".to_string()),
            Some("test-secret".to_string()),
            Some("test-token".to_string()),
            None,
        )
        .expect("constructs without network access");
        let debug = format!("{client:?}");

        assert!(!debug.contains("test-key"), "the raw key must not appear");
        assert!(!debug.contains("test-secret"), "the secret must not appear");
        assert!(!debug.contains("test-token"), "the token must not appear");
    }
}
