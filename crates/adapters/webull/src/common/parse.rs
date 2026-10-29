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

//! Parsers that map Webull instrument profiles onto Nautilus instruments.

use nautilus_core::UnixNanos;
use nautilus_model::{
    identifiers::{InstrumentId, Symbol},
    instruments::{CurrencyPair, Equity, InstrumentAny},
    types::{Currency, Price, Quantity},
};
use rust_decimal::Decimal;

use crate::{
    common::consts::WEBULL_VENUE,
    http::models::{WebullCryptoInstrument, WebullStockInstrument},
};

const DEFAULT_EQUITY_PRICE_PRECISION: u8 = 2;
const DEFAULT_EQUITY_PRICE_INCREMENT: &str = "0.01";
const DEFAULT_EQUITY_LOT_SIZE: &str = "1";
const DEFAULT_CRYPTO_PRICE_STEP: &str = "0.01";
const DEFAULT_CRYPTO_LOT_SIZE: &str = "1";

/// Parses a Webull stock/ETF profile into an [`Equity`] instrument.
///
/// Defaults for US equities: currency `USD`, price precision `2`, price increment
/// `0.01`, and lot size from the profile (or `1` when absent).
///
/// # Errors
///
/// Returns an error if lot size or currency values cannot be parsed, or if the
/// instrument fails validation.
pub fn parse_stock_instrument(
    profile: &WebullStockInstrument,
    ts: UnixNanos,
) -> anyhow::Result<InstrumentAny> {
    let symbol = Symbol::new(profile.symbol.as_str());
    let instrument_id = InstrumentId::new(symbol, *WEBULL_VENUE);
    let currency = parse_stock_currency(profile.currency.as_deref());
    let lot_size = parse_quantity(
        profile
            .lot_size
            .as_deref()
            .unwrap_or(DEFAULT_EQUITY_LOT_SIZE),
        "lot_size",
    )?;

    let equity = Equity::builder()
        .instrument_id(instrument_id)
        .raw_symbol(symbol)
        .currency(currency)
        .price_precision(DEFAULT_EQUITY_PRICE_PRECISION)
        .price_increment(Price::from(DEFAULT_EQUITY_PRICE_INCREMENT))
        .lot_size(lot_size)
        .ts_event(ts)
        .ts_init(ts)
        .build()
        .map_err(|e| anyhow::anyhow!("invalid Webull equity {}: {e}", profile.symbol))?;

    Ok(InstrumentAny::Equity(equity))
}

/// Parses a Webull crypto profile into a [`CurrencyPair`] instrument.
///
/// Price precision is taken from `price_step` decimal scale. Base/quote currencies
/// are derived from a trailing `USD`/`USDT`/`USDC` suffix when possible; otherwise
/// the full symbol is the base and the quote defaults to `USD`.
///
/// # Errors
///
/// Returns an error if price step, lot size, or quantity bounds cannot be parsed,
/// or if the instrument fails validation.
pub fn parse_crypto_instrument(
    profile: &WebullCryptoInstrument,
    ts: UnixNanos,
) -> anyhow::Result<InstrumentAny> {
    let symbol = Symbol::new(profile.symbol.as_str());
    let instrument_id = InstrumentId::new(symbol, *WEBULL_VENUE);
    let (base_currency, quote_currency) =
        parse_crypto_currencies(&profile.symbol, profile.currency.as_deref());

    let price_step = profile
        .price_step
        .as_deref()
        .unwrap_or(DEFAULT_CRYPTO_PRICE_STEP);
    let price_increment = parse_price(price_step, "price_step")?;
    let price_precision = price_increment.precision;

    let size_raw = profile
        .lot_size
        .as_deref()
        .or(profile.min_trade_qty.as_deref())
        .unwrap_or(DEFAULT_CRYPTO_LOT_SIZE);
    let size_increment = parse_quantity(size_raw, "lot_size")?;
    let size_precision = size_increment.precision;

    let min_quantity = profile
        .min_trade_qty
        .as_deref()
        .map(|value| parse_quantity(value, "min_trade_qty"))
        .transpose()?;
    let max_quantity = profile
        .max_trade_qty
        .as_deref()
        .map(|value| parse_quantity(value, "max_trade_qty"))
        .transpose()?;

    let pair = CurrencyPair::builder()
        .instrument_id(instrument_id)
        .raw_symbol(symbol)
        .base_currency(base_currency)
        .quote_currency(quote_currency)
        .price_precision(price_precision)
        .size_precision(size_precision)
        .price_increment(price_increment)
        .size_increment(size_increment)
        .lot_size(size_increment)
        .maybe_min_quantity(min_quantity)
        .maybe_max_quantity(max_quantity)
        .ts_event(ts)
        .ts_init(ts)
        .build()
        .map_err(|e| anyhow::anyhow!("invalid Webull crypto {}: {e}", profile.symbol))?;

    Ok(InstrumentAny::CurrencyPair(pair))
}

fn parse_stock_currency(currency: Option<&str>) -> Currency {
    match currency.map(str::trim).filter(|value| !value.is_empty()) {
        Some(code) => Currency::from(code),
        None => Currency::USD(),
    }
}

fn parse_crypto_currencies(symbol: &str, currency: Option<&str>) -> (Currency, Currency) {
    let upper = symbol.trim().to_uppercase();
    for quote in ["USDT", "USDC", "USD"] {
        if let Some(base) = upper.strip_suffix(quote)
            && !base.is_empty()
        {
            return (
                Currency::get_or_create_crypto(base),
                Currency::get_or_create_crypto(quote),
            );
        }
    }

    let quote = match currency.map(str::trim).filter(|value| !value.is_empty()) {
        Some(code) => Currency::get_or_create_crypto(code),
        None => Currency::USD(),
    };

    (Currency::get_or_create_crypto(upper), quote)
}

fn parse_price(value: &str, field: &str) -> anyhow::Result<Price> {
    let decimal = Decimal::from_str_exact(value.trim())
        .map_err(|e| anyhow::anyhow!("invalid Webull {field} '{value}': {e}"))?
        .normalize();
    let precision = u8::try_from(decimal.scale())
        .map_err(|_| anyhow::anyhow!("Webull {field} '{value}' precision exceeds u8"))?;
    Price::from_decimal_dp(decimal, precision)
        .map_err(|e| anyhow::anyhow!("invalid Webull {field} '{value}': {e}"))
}

fn parse_quantity(value: &str, field: &str) -> anyhow::Result<Quantity> {
    let decimal = Decimal::from_str_exact(value.trim())
        .map_err(|e| anyhow::anyhow!("invalid Webull {field} '{value}': {e}"))?
        .normalize();
    let precision = u8::try_from(decimal.scale())
        .map_err(|_| anyhow::anyhow!("Webull {field} '{value}' precision exceeds u8"))?;
    Quantity::from_decimal_dp(decimal, precision)
        .map_err(|e| anyhow::anyhow!("invalid Webull {field} '{value}': {e}"))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn stock_profile() -> WebullStockInstrument {
        WebullStockInstrument {
            name: Some("Apple Inc".to_string()),
            instrument_id: "913256135".to_string(),
            exchange_code: Some("NAS".to_string()),
            category: "US_STOCK".to_string(),
            symbol: "AAPL".to_string(),
            status: Some("ACTIVE".to_string()),
            lot_size: Some("1".to_string()),
            currency: Some("USD".to_string()),
            sub_category: Some("STOCK".to_string()),
        }
    }

    fn crypto_profile(symbol: &str, price_step: &str, lot_size: &str) -> WebullCryptoInstrument {
        WebullCryptoInstrument {
            name: Some(symbol.to_string()),
            instrument_id: "crypto-1".to_string(),
            symbol: symbol.to_string(),
            category: "CRYPTO".to_string(),
            status: Some("ACTIVE".to_string()),
            min_trade_qty: Some(lot_size.to_string()),
            max_trade_qty: Some("100".to_string()),
            price_step: Some(price_step.to_string()),
            lot_size: Some(lot_size.to_string()),
            currency: Some("USD".to_string()),
        }
    }

    #[rstest]
    fn test_parse_stock_instrument_defaults() {
        let instrument =
            parse_stock_instrument(&stock_profile(), UnixNanos::from(1)).expect("parses equity");

        match instrument {
            InstrumentAny::Equity(equity) => {
                assert_eq!(equity.id.to_string(), "AAPL.WEBULL");
                assert_eq!(equity.raw_symbol.as_str(), "AAPL");
                assert_eq!(equity.currency, Currency::USD());
                assert_eq!(equity.price_precision, 2);
                assert_eq!(equity.price_increment, Price::from("0.01"));
                assert_eq!(equity.lot_size, Some(Quantity::from("1")));
            }
            other => panic!("expected Equity, was {other:?}"),
        }
    }

    #[rstest]
    fn test_parse_stock_instrument_missing_currency_defaults_usd() {
        let mut profile = stock_profile();
        profile.currency = None;
        profile.lot_size = None;

        let instrument =
            parse_stock_instrument(&profile, UnixNanos::from(2)).expect("parses equity");

        match instrument {
            InstrumentAny::Equity(equity) => {
                assert_eq!(equity.currency, Currency::USD());
                assert_eq!(equity.lot_size, Some(Quantity::from("1")));
            }
            other => panic!("expected Equity, was {other:?}"),
        }
    }

    #[rstest]
    fn test_parse_crypto_instrument_usd_pair() {
        let profile = crypto_profile("BTCUSD", "0.01", "0.0001");
        let instrument =
            parse_crypto_instrument(&profile, UnixNanos::from(3)).expect("parses crypto");

        match instrument {
            InstrumentAny::CurrencyPair(pair) => {
                assert_eq!(pair.id.to_string(), "BTCUSD.WEBULL");
                assert_eq!(pair.base_currency.code.as_str(), "BTC");
                assert_eq!(pair.quote_currency.code.as_str(), "USD");
                assert_eq!(pair.price_precision, 2);
                assert_eq!(pair.price_increment, Price::from("0.01"));
                assert_eq!(pair.size_precision, 4);
                assert_eq!(pair.size_increment, Quantity::from("0.0001"));
                assert_eq!(pair.min_quantity, Some(Quantity::from("0.0001")));
                assert_eq!(pair.max_quantity, Some(Quantity::from("100")));
            }
            other => panic!("expected CurrencyPair, was {other:?}"),
        }
    }

    #[rstest]
    fn test_parse_crypto_instrument_usdt_suffix() {
        let profile = crypto_profile("ETHUSDT", "0.001", "0.01");
        let instrument =
            parse_crypto_instrument(&profile, UnixNanos::from(4)).expect("parses crypto");

        match instrument {
            InstrumentAny::CurrencyPair(pair) => {
                assert_eq!(pair.base_currency.code.as_str(), "ETH");
                assert_eq!(pair.quote_currency.code.as_str(), "USDT");
                assert_eq!(pair.price_precision, 3);
            }
            other => panic!("expected CurrencyPair, was {other:?}"),
        }
    }

    #[rstest]
    fn test_parse_crypto_instrument_fallback_base_symbol() {
        let profile = crypto_profile("SOL", "0.01", "1");
        let instrument =
            parse_crypto_instrument(&profile, UnixNanos::from(5)).expect("parses crypto");

        match instrument {
            InstrumentAny::CurrencyPair(pair) => {
                assert_eq!(pair.base_currency.code.as_str(), "SOL");
                assert_eq!(pair.quote_currency, Currency::USD());
            }
            other => panic!("expected CurrencyPair, was {other:?}"),
        }
    }
}
