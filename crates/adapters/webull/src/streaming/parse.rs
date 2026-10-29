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

//! Parse Webull streaming Protobuf/JSON payloads into Nautilus types.

use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{BookOrder, OrderBookDelta, OrderBookDeltas, QuoteTick, TradeTick},
    enums::{BookAction, OrderSide, RecordFlag},
    identifiers::{InstrumentId, TradeId},
};
use prost::Message;
use ustr::Ustr;

use crate::{
    data::{aggressor_side_from_str, price_from_str, quantity_from_str},
    http::models::{
        WebullDepthBook, WebullDepthBroker, WebullDepthLevel, WebullDepthOrder,
    },
    streaming::{
        client::WebullStreamMessage,
        proto::{AskBid, Quote, Snapshot, Tick},
    },
};

/// MQTT / payload topic names used by the Data Streaming API.
pub const TOPIC_QUOTE: &str = "quote";
pub const TOPIC_SNAPSHOT: &str = "snapshot";
pub const TOPIC_TICK: &str = "tick";
pub const TOPIC_NOTICE: &str = "notice";
pub const TOPIC_ECHO: &str = "echo";

/// Domain view of a streaming snapshot payload (no bid/ask in the venue proto).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebullStreamSnapshot {
    /// Instrument symbol.
    pub symbol: String,
    /// Venue instrument identifier.
    pub instrument_id: String,
    /// Basic header timestamp (ms string).
    pub timestamp: String,
    /// Last trade time (ms string).
    pub trade_time: String,
    /// Last trade / mark price.
    pub price: String,
    /// Session open.
    pub open: String,
    /// Session high.
    pub high: String,
    /// Session low.
    pub low: String,
    /// Previous close.
    pub pre_close: String,
    /// Session volume.
    pub volume: String,
    /// Absolute change.
    pub change: String,
    /// Change ratio.
    pub change_ratio: String,
}

/// Domain view of a streaming trade tick payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebullStreamTick {
    /// Instrument symbol.
    pub symbol: String,
    /// Venue instrument identifier.
    pub instrument_id: String,
    /// Trade time (ms string).
    pub time: String,
    /// Trade price.
    pub price: String,
    /// Trade volume.
    pub volume: String,
    /// Aggressor side: `B`, `S`, or `N`/`L`.
    pub side: String,
}

/// Top-of-book fields extracted from a depth quote for [`QuoteTick`] construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepthTopQuoteFields {
    /// Best bid price.
    pub bid_price: String,
    /// Best ask price.
    pub ask_price: String,
    /// Best bid size.
    pub bid_size: String,
    /// Best ask size.
    pub ask_size: String,
    /// Quote time in milliseconds.
    pub quote_time_ms: u64,
}

/// Last-price fields from a streaming snapshot useful as QuoteTick inputs.
///
/// Streaming snapshots do not carry bid/ask; callers that need a full
/// [`QuoteTick`] should prefer [`depth_book_top_quote_fields`] from a `QUOTE`
/// message, or combine with REST snapshot data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotTopQuoteFields {
    /// Instrument symbol.
    pub symbol: String,
    /// Last / mark price.
    pub price: String,
    /// Trade time in milliseconds when parseable.
    pub trade_time_ms: Option<u64>,
    /// Basic header timestamp in milliseconds when parseable.
    pub timestamp_ms: Option<u64>,
}

/// Parses an MQTT topic + payload into a [`WebullStreamMessage`].
///
/// # Errors
///
/// Returns an error if the Protobuf or JSON payload cannot be decoded, or the
/// topic is unrecognized.
pub fn parse_stream_topic_payload(topic: &str, payload: &[u8]) -> anyhow::Result<WebullStreamMessage> {
    // Event-contract topics share the same payload families with an `event-` prefix.
    let topic = topic.strip_prefix("event-").unwrap_or(topic);

    match topic {
        TOPIC_QUOTE => {
            let quote = Quote::decode(payload)
                .map_err(|e| anyhow::anyhow!("failed to decode quote protobuf: {e}"))?;
            Ok(WebullStreamMessage::Quote(quote_to_depth_book(&quote)?))
        }
        TOPIC_SNAPSHOT => {
            let snapshot = Snapshot::decode(payload)
                .map_err(|e| anyhow::anyhow!("failed to decode snapshot protobuf: {e}"))?;
            Ok(WebullStreamMessage::Snapshot(snapshot_to_domain(&snapshot)))
        }
        TOPIC_TICK => {
            let tick = Tick::decode(payload)
                .map_err(|e| anyhow::anyhow!("failed to decode tick protobuf: {e}"))?;
            Ok(WebullStreamMessage::Tick(tick_to_domain(&tick)))
        }
        TOPIC_NOTICE => {
            let value: serde_json::Value = if payload.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::from_slice(payload)
                    .map_err(|e| anyhow::anyhow!("failed to decode notice JSON: {e}"))?
            };
            Ok(WebullStreamMessage::Notice(value))
        }
        TOPIC_ECHO => Ok(WebullStreamMessage::Echo),
        other => Err(anyhow::anyhow!("unrecognized streaming topic '{other}'")),
    }
}

/// Converts a streaming depth book into Nautilus [`OrderBookDeltas`].
///
/// Follows the Clear + Add pattern used by the Bybit adapter: one clear delta,
/// then `BookAction::Add` levels with `F_MBP`, and `F_LAST` on the final level.
///
/// # Errors
///
/// Returns an error if a price or size cannot be parsed, or the deltas cannot
/// be assembled.
pub fn parse_depth_book_to_order_book_deltas(
    book: &WebullDepthBook,
    instrument_id: InstrumentId,
    price_precision: u8,
    size_precision: u8,
    sequence: u64,
    ts_init: Option<UnixNanos>,
) -> anyhow::Result<OrderBookDeltas> {
    let ts_event = UnixNanos::from(book.quote_time.saturating_mul(1_000_000));
    let ts_init = ts_init.unwrap_or(ts_event);

    let total_levels = book.bids.len() + book.asks.len();
    let mut deltas = Vec::with_capacity(total_levels + 1);

    let mut clear = OrderBookDelta::clear(instrument_id, sequence, ts_event, ts_init);
    if total_levels == 0 {
        clear.flags |= RecordFlag::F_LAST as u8;
    }
    deltas.push(clear);

    let mut processed = 0_usize;

    let mut push_level = |level: &WebullDepthLevel, side: OrderSide| -> anyhow::Result<()> {
        let price = price_from_str(&level.price, price_precision)?;
        let size = quantity_from_str(&level.size, size_precision)?;
        processed += 1;

        let mut flags = RecordFlag::F_MBP as u8;
        if processed == total_levels {
            flags |= RecordFlag::F_LAST as u8;
        }

        let order = BookOrder::new(side, price, size, sequence);
        let delta = OrderBookDelta::new_checked(
            instrument_id,
            BookAction::Add,
            order,
            flags,
            sequence,
            ts_event,
            ts_init,
        )
        .map_err(|e| anyhow::anyhow!("failed to construct OrderBookDelta: {e}"))?;
        deltas.push(delta);
        Ok(())
    };

    for level in &book.bids {
        push_level(level, OrderSide::Buy)?;
    }

    for level in &book.asks {
        push_level(level, OrderSide::Sell)?;
    }

    OrderBookDeltas::new_checked(instrument_id, deltas)
        .map_err(|e| anyhow::anyhow!("failed to assemble OrderBookDeltas: {e}"))
}

/// Extracts top-of-book bid/ask fields from a depth book for [`QuoteTick`].
///
/// # Errors
///
/// Returns an error when either side of the book is empty.
pub fn depth_book_top_quote_fields(book: &WebullDepthBook) -> anyhow::Result<DepthTopQuoteFields> {
    let bid = book
        .bids
        .first()
        .ok_or_else(|| anyhow::anyhow!("depth book missing bid for {}", book.symbol))?;
    let ask = book
        .asks
        .first()
        .ok_or_else(|| anyhow::anyhow!("depth book missing ask for {}", book.symbol))?;

    Ok(DepthTopQuoteFields {
        bid_price: bid.price.clone(),
        ask_price: ask.price.clone(),
        bid_size: bid.size.clone(),
        ask_size: ask.size.clone(),
        quote_time_ms: book.quote_time,
    })
}

/// Builds a [`QuoteTick`] from depth top-of-book fields.
///
/// # Errors
///
/// Returns an error if prices/sizes are invalid for a quote tick.
pub fn parse_depth_top_to_quote_tick(
    book: &WebullDepthBook,
    instrument_id: InstrumentId,
    price_precision: u8,
    size_precision: u8,
    ts_init: Option<UnixNanos>,
) -> anyhow::Result<QuoteTick> {
    let top = depth_book_top_quote_fields(book)?;
    let ts_event = UnixNanos::from(top.quote_time_ms.saturating_mul(1_000_000));
    let ts_init = ts_init.unwrap_or(ts_event);

    QuoteTick::new_checked(
        instrument_id,
        price_from_str(&top.bid_price, price_precision)?,
        price_from_str(&top.ask_price, price_precision)?,
        quantity_from_str(&top.bid_size, size_precision)?,
        quantity_from_str(&top.ask_size, size_precision)?,
        ts_event,
        ts_init,
    )
    .map_err(|e| anyhow::anyhow!("invalid quote tick for {instrument_id}: {e}"))
}

/// Extracts last-price helper fields from a streaming snapshot.
#[must_use]
pub fn snapshot_top_quote_fields(snapshot: &WebullStreamSnapshot) -> SnapshotTopQuoteFields {
    SnapshotTopQuoteFields {
        symbol: snapshot.symbol.clone(),
        price: snapshot.price.clone(),
        trade_time_ms: parse_ms_opt(&snapshot.trade_time),
        timestamp_ms: parse_ms_opt(&snapshot.timestamp),
    }
}

/// Converts a streaming tick into a Nautilus [`TradeTick`].
///
/// # Errors
///
/// Returns an error if the price, volume, or timestamp cannot be parsed.
pub fn parse_stream_tick_to_trade_tick(
    tick: &WebullStreamTick,
    instrument_id: InstrumentId,
    price_precision: u8,
    size_precision: u8,
    ts_init: Option<UnixNanos>,
) -> anyhow::Result<TradeTick> {
    let time_ms = tick
        .time
        .parse::<u64>()
        .map_err(|e| anyhow::anyhow!("invalid tick time '{}': {e}", tick.time))?;
    let ts_event = UnixNanos::from(time_ms.saturating_mul(1_000_000));
    let ts_init = ts_init.unwrap_or(ts_event);

    let price = price_from_str(&tick.price, price_precision)?;
    let size = quantity_from_str(&tick.volume, size_precision)?;
    let aggressor = aggressor_side_from_str(&tick.side);
    let trade_id = TradeId::new(Ustr::from(&tick.time));

    Ok(TradeTick::new(
        instrument_id,
        price,
        size,
        aggressor,
        trade_id,
        ts_event,
        ts_init,
    ))
}

fn quote_to_depth_book(quote: &Quote) -> anyhow::Result<WebullDepthBook> {
    let basic = quote
        .basic
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("quote protobuf missing basic header"))?;
    let quote_time = parse_ms_opt(&basic.timestamp).unwrap_or(0);

    Ok(WebullDepthBook {
        symbol: basic.symbol.clone(),
        instrument_id: basic.instrument_id.clone(),
        quote_time,
        asks: quote.asks.iter().map(ask_bid_to_level).collect(),
        bids: quote.bids.iter().map(ask_bid_to_level).collect(),
    })
}

fn ask_bid_to_level(level: &AskBid) -> WebullDepthLevel {
    WebullDepthLevel {
        price: level.price.clone(),
        size: level.size.clone(),
        order: level
            .order
            .iter()
            .map(|o| WebullDepthOrder {
                mpid: o.mpid.clone(),
                size: o.size.clone(),
            })
            .collect(),
        broker: level
            .broker
            .iter()
            .map(|b| WebullDepthBroker {
                bid: b.bid.clone(),
                name: b.name.clone(),
            })
            .collect(),
    }
}

fn snapshot_to_domain(snapshot: &Snapshot) -> WebullStreamSnapshot {
    let basic = snapshot.basic.as_ref();
    WebullStreamSnapshot {
        symbol: basic.map(|b| b.symbol.clone()).unwrap_or_default(),
        instrument_id: basic.map(|b| b.instrument_id.clone()).unwrap_or_default(),
        timestamp: basic.map(|b| b.timestamp.clone()).unwrap_or_default(),
        trade_time: snapshot.trade_time.clone(),
        price: snapshot.price.clone(),
        open: snapshot.open.clone(),
        high: snapshot.high.clone(),
        low: snapshot.low.clone(),
        pre_close: snapshot.pre_close.clone(),
        volume: snapshot.volume.clone(),
        change: snapshot.change.clone(),
        change_ratio: snapshot.change_ratio.clone(),
    }
}

fn tick_to_domain(tick: &Tick) -> WebullStreamTick {
    let basic = tick.basic.as_ref();
    WebullStreamTick {
        symbol: basic.map(|b| b.symbol.clone()).unwrap_or_default(),
        instrument_id: basic.map(|b| b.instrument_id.clone()).unwrap_or_default(),
        time: tick.time.clone(),
        price: tick.price.clone(),
        volume: tick.volume.clone(),
        side: tick.side.clone(),
    }
}

fn parse_ms_opt(value: &str) -> Option<u64> {
    if value.is_empty() {
        return None;
    }
    value.parse::<u64>().ok()
}

#[cfg(test)]
mod tests {
    use nautilus_model::{
        enums::AggressorSide,
        identifiers::{InstrumentId, Symbol, Venue},
        types::{Price, Quantity},
    };
    use prost::Message;
    use rstest::rstest;
    use ustr::Ustr;

    use super::*;
    use crate::streaming::proto::{AskBid, Basic, Quote, Snapshot, Tick};

    fn aapl_id() -> InstrumentId {
        InstrumentId::new(Symbol::from("AAPL"), Venue::new(Ustr::from("NASDAQ")))
    }

    fn sample_quote_bytes() -> Vec<u8> {
        Quote {
            basic: Some(Basic {
                symbol: "AAPL".to_string(),
                instrument_id: "913256135".to_string(),
                timestamp: "1788552000000".to_string(),
                trading_session: "RTH".to_string(),
            }),
            asks: vec![AskBid {
                price: "190.10".to_string(),
                size: "100".to_string(),
                order: vec![],
                broker: vec![],
            }],
            bids: vec![AskBid {
                price: "190.00".to_string(),
                size: "200".to_string(),
                order: vec![],
                broker: vec![],
            }],
        }
        .encode_to_vec()
    }

    #[rstest]
    fn test_parse_quote_topic_to_depth_and_deltas() {
        let message =
            parse_stream_topic_payload(TOPIC_QUOTE, &sample_quote_bytes()).expect("parses quote");

        let WebullStreamMessage::Quote(book) = message else {
            panic!("expected Quote message");
        };
        assert_eq!(book.symbol, "AAPL");
        assert_eq!(book.bids[0].price, "190.00");
        assert_eq!(book.asks[0].size, "100");

        let deltas =
            parse_depth_book_to_order_book_deltas(&book, aapl_id(), 2, 0, 1, None).expect("deltas");
        assert_eq!(deltas.deltas.len(), 3);
        assert_eq!(deltas.deltas[0].action, BookAction::Clear);
        assert_eq!(deltas.deltas[1].action, BookAction::Add);
        assert_eq!(deltas.deltas[1].order.side, Some(OrderSide::Buy));
        assert!(RecordFlag::F_MBP.matches(deltas.deltas[1].flags));
        assert!(!RecordFlag::F_LAST.matches(deltas.deltas[1].flags));
        assert!(RecordFlag::F_LAST.matches(deltas.deltas[2].flags));

        let top = depth_book_top_quote_fields(&book).expect("top");
        assert_eq!(top.bid_price, "190.00");
        assert_eq!(top.ask_price, "190.10");
    }

    #[rstest]
    fn test_parse_snapshot_and_tick_topics() {
        let snapshot = Snapshot {
            basic: Some(Basic {
                symbol: "AAPL".to_string(),
                instrument_id: "913256135".to_string(),
                timestamp: "1788552000000".to_string(),
                trading_session: "RTH".to_string(),
            }),
            trade_time: "1788552000123".to_string(),
            price: "190.05".to_string(),
            open: "189".to_string(),
            high: "191".to_string(),
            low: "188".to_string(),
            pre_close: "189.5".to_string(),
            volume: "1000".to_string(),
            change: "0.55".to_string(),
            change_ratio: "0.0029".to_string(),
            ..Default::default()
        };
        let tick = Tick {
            basic: snapshot.basic.clone(),
            time: "1788552000456".to_string(),
            price: "190.06".to_string(),
            volume: "2".to_string(),
            side: "B".to_string(),
        };

        let snap_msg = parse_stream_topic_payload(TOPIC_SNAPSHOT, &snapshot.encode_to_vec())
            .expect("snapshot");
        let tick_msg =
            parse_stream_topic_payload(TOPIC_TICK, &tick.encode_to_vec()).expect("tick");

        let WebullStreamMessage::Snapshot(snap) = snap_msg else {
            panic!("expected Snapshot");
        };
        let fields = snapshot_top_quote_fields(&snap);
        assert_eq!(fields.price, "190.05");
        assert_eq!(fields.trade_time_ms, Some(1_788_552_000_123));

        let WebullStreamMessage::Tick(stream_tick) = tick_msg else {
            panic!("expected Tick");
        };
        let trade =
            parse_stream_tick_to_trade_tick(&stream_tick, aapl_id(), 2, 0, None).expect("trade");
        assert_eq!(trade.aggressor_side, AggressorSide::Buy);
        assert_eq!(trade.price, Price::from("190.06"));
        assert_eq!(trade.size, Quantity::from("2"));
    }

    #[rstest]
    fn test_notice_and_echo() {
        let notice = parse_stream_topic_payload(TOPIC_NOTICE, br#"{"code":"OK"}"#).expect("notice");
        assert!(matches!(notice, WebullStreamMessage::Notice(_)));

        let echo = parse_stream_topic_payload(TOPIC_ECHO, &[]).expect("echo");
        assert!(matches!(echo, WebullStreamMessage::Echo));
    }
}
