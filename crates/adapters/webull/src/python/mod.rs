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

//! Python bindings from [PyO3](https://pyo3.rs).
//!
//! The module is exposed as `nautilus_trader._libnautilus.webull`.

use std::{str::FromStr, time::Duration};

use nautilus_core::python::to_pyvalue_err;
use nautilus_model::identifiers::{InstrumentId, Symbol, Venue};
use pyo3::{
    IntoPyObjectExt,
    prelude::*,
    types::{PyDict, PyList},
};

use crate::{
    common::Credential,
    historical::{
        Timespan, WebullHistoricalClient as CoreWebullHistoricalClient, bars_to_nautilus,
    },
    http::WebullHttpClient,
    models::{WebullSnapshot, WebullTick},
};

/// Python wrapper for the core Webull historical client.
#[pyo3::pyclass(module = "nautilus_trader.adapters.webull")]
#[pyo3_stub_gen::derive::gen_stub_pyclass(module = "nautilus_trader.adapters.webull")]
pub struct WebullHistoricalClient {
    inner: CoreWebullHistoricalClient,
}

impl std::fmt::Debug for WebullHistoricalClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(stringify!(WebullHistoricalClient))
            .field("inner", &self.inner)
            .finish()
    }
}

#[pymethods]
#[pyo3_stub_gen::derive::gen_stub_pymethods]
impl WebullHistoricalClient {
    /// Core Webull historical client for fetching historical market data.
    ///
    /// The venue's 2FA access token (first line of the SDK token file) is
    /// required by this account and is sent on every request.
    #[new]
    #[pyo3(signature = (api_key, api_secret, access_token=None, base_url=None))]
    fn py_new(
        api_key: String,
        api_secret: String,
        access_token: Option<String>,
        base_url: Option<String>,
    ) -> PyResult<Self> {
        let credential = Credential::new(api_key, api_secret, access_token);
        let http = match base_url {
            Some(url) => WebullHttpClient::new_with_base_url(
                credential,
                &url,
                &host_from_base_url(&url),
                Duration::from_secs(60),
            )
            .map_err(to_pyvalue_err)?,
            None => WebullHttpClient::new(credential).map_err(to_pyvalue_err)?,
        };

        Ok(Self {
            inner: CoreWebullHistoricalClient::new(http),
        })
    }

    /// Fetches historical bars for a symbol in `[start_ms, end_ms]` as Nautilus
    /// `Bar` objects, oldest first.
    ///
    /// `timespan` is one of `M1`, `M5`, `M15`, `M30`, `M60`, `M120`, `M240`,
    /// `D`, `W`, `M`. `trading_sessions` is optional, e.g. `PRE,RTH,ATH`
    /// (omit for RTH-only, the venue default).
    #[expect(clippy::too_many_arguments)]
    #[pyo3(signature = (symbol, category, timespan, start_ms, end_ms, trading_sessions=None, venue=None, price_precision=2))]
    fn get_history_bars<'py>(
        &self,
        py: Python<'py>,
        symbol: String,
        category: String,
        timespan: &str,
        start_ms: u64,
        end_ms: u64,
        trading_sessions: Option<String>,
        venue: Option<&str>,
        price_precision: u8,
    ) -> PyResult<Bound<'py, PyAny>> {
        let parsed_timespan = Timespan::from_str(timespan).map_err(to_pyvalue_err)?;
        let venue = Venue::from(venue.unwrap_or("WEBULL"));
        let instrument_id = InstrumentId::new(Symbol::from(symbol.clone()), venue);

        let inner = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let venue_bars = inner
                .get_history_bars(
                    &symbol,
                    &category,
                    parsed_timespan,
                    start_ms,
                    end_ms,
                    trading_sessions.as_deref(),
                )
                .await
                .map_err(to_pyvalue_err)?;
            let bars = bars_to_nautilus(
                &venue_bars,
                &instrument_id,
                parsed_timespan,
                price_precision,
            )
            .map_err(to_pyvalue_err)?;
            Python::attach(|py| bars.into_py_any(py))
        })
    }

    /// Fetches live quote snapshots for the given symbols.
    ///
    /// Returns a list of dictionaries with the venue's string-typed fields
    /// and millisecond epoch times.
    #[pyo3(signature = (symbols, category))]
    fn get_snapshot<'py>(
        &self,
        py: Python<'py>,
        symbols: Vec<String>,
        category: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let snapshots = inner
                .http()
                .get_snapshot(&symbols, &category)
                .await
                .map_err(to_pyvalue_err)?;
            Python::attach(|py| Ok(snapshot_dicts(snapshots, py)?.unbind()))
        })
    }

    /// Fetches trade ticks ending at `end_ms`, newest first.
    ///
    /// Returns a list of dictionaries with the venue's string-typed fields
    /// and millisecond epoch times.
    #[pyo3(signature = (symbol, category, end_ms, count=200))]
    fn get_ticks<'py>(
        &self,
        py: Python<'py>,
        symbol: String,
        category: String,
        end_ms: u64,
        count: u32,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let ticks = inner
                .http()
                .get_ticks(&symbol, &category, count, end_ms)
                .await
                .map_err(to_pyvalue_err)?;
            Python::attach(|py| Ok(tick_dicts(ticks, py)?.unbind()))
        })
    }
}

/// Extracts the signing host from a base URL (scheme stripped, path dropped).
fn host_from_base_url(base_url: &str) -> String {
    let without_scheme = base_url.split_once("//").map_or(base_url, |(_, rest)| rest);
    without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .to_string()
}

fn snapshot_dicts(snapshots: Vec<WebullSnapshot>, py: Python<'_>) -> PyResult<Bound<'_, PyList>> {
    let list = PyList::empty(py);
    for snapshot in snapshots {
        let dict = PyDict::new(py);
        dict.set_item("symbol", snapshot.symbol)?;
        dict.set_item("price", snapshot.price)?;
        dict.set_item("open", snapshot.open)?;
        dict.set_item("high", snapshot.high)?;
        dict.set_item("low", snapshot.low)?;
        dict.set_item("close", snapshot.close)?;
        dict.set_item("pre_close", snapshot.pre_close)?;
        dict.set_item("volume", snapshot.volume)?;
        dict.set_item("bid", snapshot.bid)?;
        dict.set_item("ask", snapshot.ask)?;
        dict.set_item("bid_size", snapshot.bid_size)?;
        dict.set_item("ask_size", snapshot.ask_size)?;
        dict.set_item("quote_time", snapshot.quote_time)?;
        dict.set_item("last_trade_time", snapshot.last_trade_time)?;
        dict.set_item("instrument_id", snapshot.instrument_id)?;
        list.append(dict)?;
    }
    Ok(list)
}

fn tick_dicts(ticks: Vec<WebullTick>, py: Python<'_>) -> PyResult<Bound<'_, PyList>> {
    let list = PyList::empty(py);
    for tick in ticks {
        let dict = PyDict::new(py);
        dict.set_item("time", tick.time)?;
        dict.set_item("price", tick.price)?;
        dict.set_item("volume", tick.volume)?;
        dict.set_item("side", tick.side)?;
        dict.set_item("trading_session", tick.trading_session)?;
        list.append(dict)?;
    }
    Ok(list)
}

/// Webull Python module.
///
/// The module is exposed as `nautilus_trader._libnautilus.webull`.
///
/// # Errors
///
/// Returns a `PyErr` if registering any module components fails.
#[pymodule]
pub fn webull(_: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<WebullHistoricalClient>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::common::{WEBULL_HTTP_BASE_URL, WEBULL_HTTP_HOST};

    #[rstest]
    fn test_host_from_base_url() {
        assert_eq!(
            host_from_base_url("https://api.webull.com"),
            "api.webull.com"
        );
        assert_eq!(
            host_from_base_url("http://127.0.0.1:12345"),
            "127.0.0.1:12345"
        );
        assert_eq!(
            host_from_base_url("https://api.webull.com/v2"),
            "api.webull.com"
        );
        assert_eq!(host_from_base_url(WEBULL_HTTP_BASE_URL), WEBULL_HTTP_HOST);
    }

    #[rstest]
    fn test_client_construction_without_network() {
        let client = WebullHistoricalClient::py_new(
            "test-key".to_string(),
            "test-secret".to_string(),
            Some("test-token".to_string()),
            None,
        )
        .expect("constructs without network access");

        let debug = format!("{client:?}");
        assert!(debug.contains("********"), "short keys mask to asterisks");
        assert!(!debug.contains("test-key"), "the raw key must not appear");
    }

    #[rstest]
    fn test_client_construction_with_custom_base_url() {
        let client = WebullHistoricalClient::py_new(
            "test-key".to_string(),
            "test-secret".to_string(),
            None,
            Some("http://127.0.0.1:9".to_string()),
        )
        .expect("constructs without network access");

        assert!(format!("{client:?}").contains("127.0.0.1:9"));
    }
}
