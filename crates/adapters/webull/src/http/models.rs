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

//! Webull OpenAPI market data response models.
//!
//! The API preserves precision by representing all numeric values as
//! strings; callers convert them to the Nautilus fixed-point types.

use serde::Deserialize;

/// A historical OHLCV bar as returned by `/openapi/market-data/stock/bars`.
///
/// Bar timestamps are UTC and represent the bar start time.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WebullBar {
    /// The instrument symbol, e.g. `AAPL`.
    pub symbol: String,
    /// Bar start time, e.g. `2026-09-04T14:55:00.000+0000`.
    pub time: String,
    /// Bar open price.
    pub open: String,
    /// Bar high price.
    pub high: String,
    /// Bar low price.
    pub low: String,
    /// Bar close price.
    pub close: String,
    /// Bar volume.
    pub volume: String,
    /// Trading session tag: `PRE`, `RTH`, `ATH`, or `OVN`.
    pub trading_session: String,
    /// The venue's internal instrument identifier.
    #[serde(rename = "instrument_id")]
    pub instrument_id: String,
}

/// A live quote snapshot as returned by `/openapi/market-data/stock/snapshot`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WebullSnapshot {
    /// The instrument symbol, e.g. `AAPL`.
    pub symbol: String,
    /// The latest trade price.
    pub price: String,
    /// The session open price.
    pub open: String,
    /// The session high price.
    pub high: String,
    /// The session low price.
    pub low: String,
    /// The latest close (prior session for pre-market).
    pub close: String,
    /// The previous session close price.
    pub pre_close: String,
    /// The session volume.
    pub volume: String,
    /// The current bid price.
    pub bid: String,
    /// The current ask price.
    pub ask: String,
    /// The current bid size.
    pub bid_size: String,
    /// The current ask size.
    pub ask_size: String,
    /// The latest quote time as a millisecond Unix timestamp.
    pub quote_time: u64,
    /// The last trade time as a millisecond Unix timestamp.
    pub last_trade_time: u64,
    /// The venue's internal instrument identifier.
    #[serde(rename = "instrument_id")]
    pub instrument_id: String,
}

/// The payload of a `/openapi/market-data/stock/tick` response.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WebullTickResponse {
    /// The instrument symbol, e.g. `AAPL`.
    pub symbol: String,
    /// The venue's internal instrument identifier.
    #[serde(rename = "instrument_id")]
    pub instrument_id: String,
    /// The tick records, newest first.
    pub result: Vec<WebullTick>,
}

/// A single trade tick as returned by `/openapi/market-data/stock/tick`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WebullTick {
    /// The tick time as a millisecond Unix timestamp.
    pub time: String,
    /// The tick price.
    pub price: String,
    /// The tick volume.
    pub volume: String,
    /// The aggressor side: `B` (buy), `S` (sell), or `N`/`L` (none/low).
    pub side: String,
    /// Trading session tag: `PRE`, `RTH`, `ATH`, or `OVN`.
    pub trading_session: String,
}

/// An API error payload, e.g. `{"message": "...", "error_code": "UNAUTHORIZED"}`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WebullErrorResponse {
    /// The human-readable error message.
    pub message: String,
    /// The venue error code.
    #[serde(rename = "error_code")]
    pub error_code: String,
}

/// A paginated stock/ETF instrument profile page.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WebullInstrumentPage {
    /// Instrument profiles on this page.
    #[serde(default)]
    pub data: Vec<WebullStockInstrument>,
    /// Cursor for the next page, when more results remain.
    #[serde(default)]
    pub pagination_key: Option<String>,
}

/// A stock/ETF instrument profile from `/trading/instruments/stocks/profiles/list`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WebullStockInstrument {
    /// Human-readable instrument name.
    #[serde(default)]
    pub name: Option<String>,
    /// The venue's internal instrument identifier.
    pub instrument_id: String,
    /// Exchange code when provided by the venue.
    #[serde(default)]
    pub exchange_code: Option<String>,
    /// Instrument category, e.g. `US_STOCK`.
    pub category: String,
    /// Trading symbol, e.g. `AAPL`.
    pub symbol: String,
    /// Listing status when provided, e.g. `ACTIVE`.
    #[serde(default)]
    pub status: Option<String>,
    /// Rounded lot size as a decimal string.
    #[serde(default)]
    pub lot_size: Option<String>,
    /// Quote currency code when provided, e.g. `USD`.
    #[serde(default)]
    pub currency: Option<String>,
    /// Sub-category when provided, e.g. `ETF`.
    #[serde(default)]
    pub sub_category: Option<String>,
}

/// A paginated crypto instrument profile page.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WebullCryptoInstrumentPage {
    /// Instrument profiles on this page.
    #[serde(default)]
    pub data: Vec<WebullCryptoInstrument>,
    /// Cursor for the next page, when more results remain.
    #[serde(default)]
    pub pagination_key: Option<String>,
}

/// A crypto instrument profile from `/trading/instruments/crypto/profiles/list`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WebullCryptoInstrument {
    /// Human-readable instrument name.
    #[serde(default)]
    pub name: Option<String>,
    /// The venue's internal instrument identifier.
    pub instrument_id: String,
    /// Trading symbol, e.g. `BTCUSD`.
    pub symbol: String,
    /// Instrument category, e.g. `CRYPTO`.
    pub category: String,
    /// Minimum price increment as a decimal string.
    #[serde(default)]
    pub price_step: Option<String>,
    /// Size increment / lot size as a decimal string.
    #[serde(default)]
    pub lot_size: Option<String>,
    /// Quote currency code when provided.
    #[serde(default)]
    pub currency: Option<String>,
    /// Minimum trade quantity as a decimal string.
    #[serde(default)]
    pub min_trade_qty: Option<String>,
    /// Maximum trade quantity as a decimal string.
    #[serde(default)]
    pub max_trade_qty: Option<String>,
    /// Listing status when provided.
    #[serde(default)]
    pub status: Option<String>,
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const BAR_JSON: &str = r#"{"symbol":"AAPL","time":"2026-09-04T14:55:00.000+0000","open":"318.98","close":"318.44","high":"319.07","low":"318.3","volume":"619677","trading_session":"RTH","instrument_id":"913256135","tickerId":"913256135"}"#;

    #[rstest]
    fn test_bar_deserializes_with_unknown_fields_ignored() {
        let bar: WebullBar = serde_json::from_str(BAR_JSON).expect("parses bar");

        assert_eq!(bar.symbol, "AAPL");
        assert_eq!(bar.time, "2026-09-04T14:55:00.000+0000");
        assert_eq!(bar.open, "318.98");
        assert_eq!(bar.high, "319.07");
        assert_eq!(bar.low, "318.3");
        assert_eq!(bar.close, "318.44");
        assert_eq!(bar.volume, "619677");
        assert_eq!(bar.trading_session, "RTH");
        assert_eq!(bar.instrument_id, "913256135");
    }

    #[rstest]
    fn test_snapshot_deserializes() {
        let json = r#"[{"symbol":"AAPL","price":"319.97","open":"328.305","high":"328.93","low":"317.86","volume":"39606884","close":"319.97","pre_close":"328.21","bid":"320.01","ask":"320.07","bid_size":"50","ask_size":"145","quote_time":1788566391309,"last_trade_time":1788552001325,"instrument_id":"913256135","list_status":"LISTED"}]"#;

        let snapshots: Vec<WebullSnapshot> = serde_json::from_str(json).expect("parses snapshot");

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].symbol, "AAPL");
        assert_eq!(snapshots[0].price, "319.97");
        assert_eq!(snapshots[0].quote_time, 1_788_566_391_309);
    }

    #[rstest]
    fn test_tick_response_deserializes() {
        let json = r#"{"symbol":"AAPL","instrument_id":"913256135","result":[{"time":"1788552058699","price":"319.78","volume":"2","side":"N","trading_session":"RTH"}]}"#;

        let response: WebullTickResponse = serde_json::from_str(json).expect("parses ticks");

        assert_eq!(response.symbol, "AAPL");
        assert_eq!(response.result.len(), 1);
        assert_eq!(response.result[0].time, "1788552058699");
        assert_eq!(response.result[0].price, "319.78");
        assert_eq!(response.result[0].side, "N");
    }

    #[rstest]
    fn test_error_response_deserializes() {
        let json = r#"{"message":"Header x-signature is invalid.","error_code":"UNAUTHORIZED"}"#;

        let error: WebullErrorResponse = serde_json::from_str(json).expect("parses error");

        assert_eq!(error.error_code, "UNAUTHORIZED");
        assert!(error.message.contains("x-signature"));
    }

    #[rstest]
    fn test_stock_instrument_page_deserializes() {
        let json = r#"{
            "data": [{
                "name": "Apple Inc",
                "instrument_id": "913256135",
                "exchange_code": "NAS",
                "category": "US_STOCK",
                "symbol": "AAPL",
                "status": "ACTIVE",
                "lot_size": "1",
                "currency": "USD",
                "sub_category": "STOCK",
                "extra_field": "ignored"
            }],
            "pagination_key": "next-page"
        }"#;

        let page: WebullInstrumentPage = serde_json::from_str(json).expect("parses page");

        assert_eq!(page.data.len(), 1);
        assert_eq!(page.data[0].symbol, "AAPL");
        assert_eq!(page.data[0].instrument_id, "913256135");
        assert_eq!(page.data[0].lot_size.as_deref(), Some("1"));
        assert_eq!(page.pagination_key.as_deref(), Some("next-page"));
    }

    #[rstest]
    fn test_crypto_instrument_page_deserializes() {
        let json = r#"{
            "data": [{
                "name": "Bitcoin",
                "instrument_id": "crypto-1",
                "symbol": "BTCUSD",
                "category": "CRYPTO",
                "price_step": "0.01",
                "lot_size": "0.0001",
                "currency": "USD",
                "min_trade_qty": "0.0001",
                "max_trade_qty": "100"
            }],
            "pagination_key": null
        }"#;

        let page: WebullCryptoInstrumentPage = serde_json::from_str(json).expect("parses page");

        assert_eq!(page.data.len(), 1);
        assert_eq!(page.data[0].symbol, "BTCUSD");
        assert_eq!(page.data[0].price_step.as_deref(), Some("0.01"));
        assert!(page.pagination_key.is_none());
    }
}
