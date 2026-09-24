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

//! Historical market data client for Webull.
//!
//! Fetches bars through the venue's paged historical API (newest-first pages
//! walked backwards with `end_time`) and converts them to Nautilus [`Bar`]s.

use std::str::FromStr;

use ahash::AHashSet;
use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{Bar, BarSpecification, BarType},
    enums::{AggregationSource, BarAggregation, PriceType},
    identifiers::InstrumentId,
    types::{Price, Quantity},
};
use rust_decimal::Decimal;

use crate::{http::WebullHttpClient, models::WebullBar};

/// Webull intraday timespans and their Nautilus bar aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timespan {
    /// One-minute bars.
    M1,
    /// Five-minute bars.
    M5,
    /// Fifteen-minute bars.
    M15,
    /// Thirty-minute bars.
    M30,
    /// One-hour bars.
    M60,
    /// Two-hour bars.
    M120,
    /// Four-hour bars.
    M240,
    /// Daily bars.
    Day,
    /// Weekly bars.
    Week,
    /// Monthly bars.
    Month,
}

impl Timespan {
    /// The timespan value accepted by the Webull API.
    #[must_use]
    pub const fn api_value(self) -> &'static str {
        match self {
            Self::M1 => "M1",
            Self::M5 => "M5",
            Self::M15 => "M15",
            Self::M30 => "M30",
            Self::M60 => "M60",
            Self::M120 => "M120",
            Self::M240 => "M240",
            Self::Day => "D",
            Self::Week => "W",
            Self::Month => "M",
        }
    }

    /// The Nautilus bar specification this timespan corresponds to
    /// (last-price aggregation).
    #[must_use]
    pub fn bar_specification(self) -> BarSpecification {
        match self {
            Self::M1 => BarSpecification::new(1, BarAggregation::Minute, PriceType::Last),
            Self::M5 => BarSpecification::new(5, BarAggregation::Minute, PriceType::Last),
            Self::M15 => BarSpecification::new(15, BarAggregation::Minute, PriceType::Last),
            Self::M30 => BarSpecification::new(30, BarAggregation::Minute, PriceType::Last),
            Self::M60 => BarSpecification::new(1, BarAggregation::Hour, PriceType::Last),
            Self::M120 => BarSpecification::new(2, BarAggregation::Hour, PriceType::Last),
            Self::M240 => BarSpecification::new(4, BarAggregation::Hour, PriceType::Last),
            Self::Day => BarSpecification::new(1, BarAggregation::Day, PriceType::Last),
            Self::Week => BarSpecification::new(1, BarAggregation::Week, PriceType::Last),
            Self::Month => BarSpecification::new(1, BarAggregation::Month, PriceType::Last),
        }
    }
}

impl std::str::FromStr for Timespan {
    type Err = String;

    /// Parses a Webull API timespan string (`M1`, `M5`, `M15`, `M30`, `M60`,
    /// `M120`, `M240`, `D`, `W`, or `M`).
    ///
    /// # Errors
    ///
    /// Returns an error if the value is not a known timespan.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "M1" => Ok(Self::M1),
            "M5" => Ok(Self::M5),
            "M15" => Ok(Self::M15),
            "M30" => Ok(Self::M30),
            "M60" => Ok(Self::M60),
            "M120" => Ok(Self::M120),
            "M240" => Ok(Self::M240),
            "D" => Ok(Self::Day),
            "W" => Ok(Self::Week),
            "M" => Ok(Self::Month),
            other => Err(format!(
                "invalid timespan '{other}', expected one of M1, M5, M15, M30, M60, M120, M240, D, W, or M"
            )),
        }
    }
}

/// A historical market data client for Webull.
#[derive(Debug, Clone)]
pub struct WebullHistoricalClient {
    http: WebullHttpClient,
}

impl WebullHistoricalClient {
    /// Creates a new [`WebullHistoricalClient`] wrapping the given HTTP client.
    #[must_use]
    pub fn new(http: WebullHttpClient) -> Self {
        Self { http }
    }

    /// Returns the underlying HTTP client.
    #[must_use]
    pub const fn http(&self) -> &WebullHttpClient {
        &self.http
    }

    /// Fetches venue bars in `[start_time_ms, end_time_ms]`, oldest first.
    ///
    /// The API pages backwards from `end_time_ms` (up to 200 bars per page,
    /// newest first, start times ignored), so this method walks `end_time`
    /// backwards until the start is reached or the data wall is hit.
    ///
    /// # Errors
    ///
    /// Returns an error if a request fails or a bar time cannot be parsed.
    pub async fn get_history_bars(
        &self,
        symbol: &str,
        category: &str,
        timespan: Timespan,
        start_time_ms: u64,
        end_time_ms: u64,
        trading_sessions: Option<&str>,
    ) -> anyhow::Result<Vec<WebullBar>> {
        const PAGE_SIZE: u32 = 200;

        let mut bars: Vec<(u64, WebullBar)> = Vec::new();
        let mut cursor = end_time_ms;

        loop {
            let page = self
                .http
                .get_bars(
                    symbol,
                    category,
                    timespan.api_value(),
                    PAGE_SIZE,
                    cursor,
                    trading_sessions,
                )
                .await?;

            if page.is_empty() {
                break;
            }

            // Pages are newest first; the oldest bar drives the next cursor.
            let page_len = page.len();
            let oldest_ms = parse_bar_time_ms(&page[page_len - 1].time)?;

            for bar in page {
                let bar_ms = parse_bar_time_ms(&bar.time)?;
                if bar_ms >= start_time_ms {
                    bars.push((bar_ms, bar));
                }
            }

            // A short page means the data wall was reached.
            if oldest_ms <= start_time_ms || page_len < PAGE_SIZE as usize {
                break;
            }

            // Guard against underflow at the epoch boundary.
            if oldest_ms <= 1 {
                break;
            }
            cursor = oldest_ms - 1;
        }

        Ok(sort_and_dedupe(bars))
    }
}

/// Parses a Webull bar time (`2026-09-04T14:55:00.000+0000`) as a
/// millisecond Unix timestamp.
///
/// The API reports all times in UTC with the `+0000` offset.
///
/// # Errors
///
/// Returns an error if the time does not carry the expected offset or is
/// not a valid timestamp.
pub fn parse_bar_time_ms(time: &str) -> anyhow::Result<u64> {
    const OFFSET: &str = "+0000";

    let without_offset = time
        .strip_suffix(OFFSET)
        .ok_or_else(|| anyhow::anyhow!("bar time lacks the '{OFFSET}' offset: {time}"))?;
    let normalized = format!("{without_offset}+00:00");
    let timestamp = jiff::Timestamp::from_str(&normalized)
        .map_err(|e| anyhow::anyhow!("invalid bar time '{time}': {e}"))?;

    u64::try_from(timestamp.as_millisecond())
        .map_err(|_| anyhow::anyhow!("bar time is pre-epoch: {time}"))
}

/// Converts venue bars to Nautilus [`Bar`]s.
///
/// Prices are normalized to the given precision and `ts_event` comes from
/// the bar time. `ts_init` mirrors `ts_event` so the data stays compatible
/// with the catalog, which keys file naming, range queries, and replay
/// ordering on `ts_init` (the Nautilus replay convention).
///
/// # Errors
///
/// Returns an error if a price or volume cannot be parsed, or a bar fails
/// correctness checks (for example a high below the open).
pub fn bars_to_nautilus(
    bars: &[WebullBar],
    instrument_id: &InstrumentId,
    timespan: Timespan,
    price_precision: u8,
) -> anyhow::Result<Vec<Bar>> {
    let bar_type = BarType::new(
        *instrument_id,
        timespan.bar_specification(),
        AggregationSource::External,
    );

    bars.iter()
        .map(|bar| {
            let open = price_from_str(&bar.open, price_precision)?;
            let high = price_from_str(&bar.high, price_precision)?;
            let low = price_from_str(&bar.low, price_precision)?;
            let close = price_from_str(&bar.close, price_precision)?;
            let volume = quantity_from_str(&bar.volume)?;
            let ts_event = UnixNanos::from(parse_bar_time_ms(&bar.time)? * 1_000_000);

            Bar::new_checked(bar_type, open, high, low, close, volume, ts_event, ts_event)
                .map_err(|e| anyhow::anyhow!("invalid bar {}: {e}", bar.time))
        })
        .collect()
}

fn price_from_str(value: &str, precision: u8) -> anyhow::Result<Price> {
    let decimal =
        Decimal::from_str(value).map_err(|e| anyhow::anyhow!("invalid price '{value}': {e}"))?;
    Price::from_decimal_dp(decimal, precision)
        .map_err(|e| anyhow::anyhow!("invalid price '{value}': {e}"))
}

fn quantity_from_str(value: &str) -> anyhow::Result<Quantity> {
    let decimal =
        Decimal::from_str(value).map_err(|e| anyhow::anyhow!("invalid volume '{value}': {e}"))?;
    Quantity::from_decimal(decimal).map_err(|e| anyhow::anyhow!("invalid volume '{value}': {e}"))
}

/// Sorts bars globally oldest first and removes duplicate times,
/// keeping the first occurrence.
///
/// Pages arrive as separate newest-first groups, so a stable sort on the
/// timestamp restores the global order; fetch order wins ties.
fn sort_and_dedupe(bars: Vec<(u64, WebullBar)>) -> Vec<WebullBar> {
    let mut bars = bars;
    bars.sort_by_key(|(bar_ms, _)| *bar_ms);
    let mut seen: AHashSet<String> = AHashSet::with_capacity(bars.len());
    bars.into_iter()
        .map(|(_, bar)| bar)
        .filter(|bar| seen.insert(bar.time.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn fixture_bar(time: &str, close: &str) -> WebullBar {
        WebullBar {
            symbol: "AAPL".to_string(),
            time: time.to_string(),
            open: close.to_string(),
            high: close.to_string(),
            low: close.to_string(),
            close: close.to_string(),
            volume: "100".to_string(),
            trading_session: "RTH".to_string(),
            instrument_id: "913256135".to_string(),
        }
    }

    #[rstest]
    fn test_timespan_api_values() {
        assert_eq!(Timespan::M1.api_value(), "M1");
        assert_eq!(Timespan::M5.api_value(), "M5");
        assert_eq!(Timespan::M60.api_value(), "M60");
        assert_eq!(Timespan::Day.api_value(), "D");
        assert_eq!(Timespan::Week.api_value(), "W");
        assert_eq!(Timespan::Month.api_value(), "M");
    }

    #[rstest]
    fn test_timespan_bar_specifications() {
        let spec = Timespan::M5.bar_specification();
        assert_eq!(spec.step.get(), 5);
        assert_eq!(spec.aggregation, BarAggregation::Minute);
        assert_eq!(spec.price_type, PriceType::Last);

        let spec = Timespan::M120.bar_specification();
        assert_eq!(spec.step.get(), 2);
        assert_eq!(spec.aggregation, BarAggregation::Hour);

        let spec = Timespan::Month.bar_specification();
        assert_eq!(spec.aggregation, BarAggregation::Month);
    }

    #[rstest]
    fn test_parse_bar_time_ms() {
        assert_eq!(
            parse_bar_time_ms("2026-09-04T14:55:00.000+0000").expect("parses"),
            1_788_533_700_000
        );
        assert_eq!(
            parse_bar_time_ms("1990-01-01T00:00:00.000+0000").expect("parses"),
            631_152_000_000
        );
    }

    #[rstest]
    fn test_parse_bar_time_ms_rejects_bad_offset() {
        assert!(parse_bar_time_ms("2026-09-04T14:55:00.000+0200").is_err());
        assert!(parse_bar_time_ms("not-a-time").is_err());
    }

    #[rstest]
    fn test_bars_to_nautilus_values() {
        let bars = vec![
            fixture_bar("2026-09-04T14:55:00.000+0000", "318.44"),
            fixture_bar("2026-09-04T15:00:00.000+0000", "318.90"),
        ];

        let instrument_id = nautilus_model::identifiers::InstrumentId::from_as_ref("AAPL.WEBULL")
            .expect("parses instrument id");
        let converted = bars_to_nautilus(&bars, &instrument_id, Timespan::M5, 2).expect("converts");

        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].close, Price::from("318.44"));
        assert_eq!(converted[1].close, Price::from("318.90"));
        assert_eq!(converted[0].volume, Quantity::from(100u64));
        assert_eq!(
            converted[0].ts_event,
            UnixNanos::from(1_788_533_700_000 * 1_000_000)
        );
        assert_eq!(converted[0].bar_type.instrument_id(), instrument_id);
        assert_eq!(
            converted[0].ts_init, converted[0].ts_event,
            "ts_init mirrors ts_event for catalog compatibility"
        );
    }

    #[rstest]
    fn test_bars_to_nautilus_rejects_inconsistent_ohlc() {
        let mut bar = fixture_bar("2026-09-04T14:55:00.000+0000", "318.44");
        bar.high = "318.00".to_string(); // high below open/close

        let instrument_id = nautilus_model::identifiers::InstrumentId::from_as_ref("AAPL.WEBULL")
            .expect("parses instrument id");

        assert!(
            bars_to_nautilus(std::slice::from_ref(&bar), &instrument_id, Timespan::M5, 2).is_err()
        );
    }

    #[rstest]
    fn test_sort_and_dedupe_orders_and_keeps_first() {
        // Two newest-first page groups with one overlapping boundary time.
        let bars = vec![
            (
                parse_bar_time_ms("2026-09-04T15:00:00.000+0000").expect("parses"),
                fixture_bar("2026-09-04T15:00:00.000+0000", "a"),
            ),
            (
                parse_bar_time_ms("2026-09-04T14:55:00.000+0000").expect("parses"),
                fixture_bar("2026-09-04T14:55:00.000+0000", "b"),
            ),
            (
                parse_bar_time_ms("2026-09-04T14:55:00.000+0000").expect("parses"),
                fixture_bar("2026-09-04T14:55:00.000+0000", "c"),
            ),
            (
                parse_bar_time_ms("2026-09-04T14:50:00.000+0000").expect("parses"),
                fixture_bar("2026-09-04T14:50:00.000+0000", "d"),
            ),
        ];

        let deduped = sort_and_dedupe(bars);
        assert_eq!(deduped.len(), 3);
        assert_eq!(deduped[0].time, "2026-09-04T14:50:00.000+0000");
        assert_eq!(
            deduped[1].close, "b",
            "the first occurrence of the duplicate wins"
        );
        assert_eq!(deduped[2].time, "2026-09-04T15:00:00.000+0000");
    }
}
