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

//! Manual prost message types for Webull streaming Protobuf payloads.
//!
//! Field numbers and types match the venue `message.proto` used by the
//! official Webull Data Streaming SDK (no `build.rs` / `.proto` codegen).

#![allow(
    missing_docs,
    reason = "prost Message field docs mirror the venue schema verbatim"
)]

use prost::Message;

/// Common header carried by quote, snapshot, and tick payloads.
#[derive(Clone, PartialEq, Eq, Message)]
pub struct Basic {
    #[prost(string, tag = "1")]
    pub symbol: String,
    #[prost(string, tag = "2")]
    pub instrument_id: String,
    #[prost(string, tag = "3")]
    pub timestamp: String,
    #[prost(string, tag = "4")]
    pub trading_session: String,
}

/// Full depth quote (`QUOTE` / topic `quote`).
#[derive(Clone, PartialEq, Eq, Message)]
pub struct Quote {
    #[prost(message, optional, tag = "1")]
    pub basic: Option<Basic>,
    #[prost(message, repeated, tag = "2")]
    pub asks: Vec<AskBid>,
    #[prost(message, repeated, tag = "3")]
    pub bids: Vec<AskBid>,
}

/// A single ask or bid price level.
#[derive(Clone, PartialEq, Eq, Message)]
pub struct AskBid {
    #[prost(string, tag = "1")]
    pub price: String,
    #[prost(string, tag = "2")]
    pub size: String,
    #[prost(message, repeated, tag = "3")]
    pub order: Vec<Order>,
    #[prost(message, repeated, tag = "4")]
    pub broker: Vec<Broker>,
}

/// Market-participant order queue entry.
#[derive(Clone, PartialEq, Eq, Message)]
pub struct Order {
    #[prost(string, tag = "1")]
    pub mpid: String,
    #[prost(string, tag = "2")]
    pub size: String,
}

/// Broker attribution entry.
#[derive(Clone, PartialEq, Eq, Message)]
pub struct Broker {
    #[prost(string, tag = "1")]
    pub bid: String,
    #[prost(string, tag = "2")]
    pub name: String,
}

/// Session snapshot (`SNAPSHOT` / topic `snapshot`).
#[derive(Clone, PartialEq, Eq, Message)]
pub struct Snapshot {
    #[prost(message, optional, tag = "1")]
    pub basic: Option<Basic>,
    #[prost(string, tag = "2")]
    pub trade_time: String,
    #[prost(string, tag = "3")]
    pub price: String,
    #[prost(string, tag = "4")]
    pub open: String,
    #[prost(string, tag = "5")]
    pub high: String,
    #[prost(string, tag = "6")]
    pub low: String,
    #[prost(string, tag = "7")]
    pub pre_close: String,
    #[prost(string, tag = "8")]
    pub volume: String,
    #[prost(string, tag = "9")]
    pub change: String,
    #[prost(string, tag = "10")]
    pub change_ratio: String,
    #[prost(string, tag = "11")]
    pub ext_trade_time: String,
    #[prost(string, tag = "12")]
    pub ext_price: String,
    #[prost(string, tag = "13")]
    pub ext_high: String,
    #[prost(string, tag = "14")]
    pub ext_low: String,
    #[prost(string, tag = "15")]
    pub ext_volume: String,
    #[prost(string, tag = "16")]
    pub ext_change: String,
    #[prost(string, tag = "17")]
    pub ext_change_ratio: String,
    #[prost(string, tag = "18")]
    pub ovn_trade_time: String,
    #[prost(string, tag = "19")]
    pub ovn_price: String,
    #[prost(string, tag = "20")]
    pub ovn_high: String,
    #[prost(string, tag = "21")]
    pub ovn_low: String,
    #[prost(string, tag = "22")]
    pub ovn_volume: String,
    #[prost(string, tag = "23")]
    pub ovn_change: String,
    #[prost(string, tag = "24")]
    pub ovn_change_ratio: String,
}

/// Per-trade tick (`TICK` / topic `tick`).
#[derive(Clone, PartialEq, Eq, Message)]
pub struct Tick {
    #[prost(message, optional, tag = "1")]
    pub basic: Option<Basic>,
    #[prost(string, tag = "2")]
    pub time: String,
    #[prost(string, tag = "3")]
    pub price: String,
    #[prost(string, tag = "4")]
    pub volume: String,
    #[prost(string, tag = "5")]
    pub side: String,
}

#[cfg(test)]
mod tests {
    use prost::Message;
    use rstest::rstest;

    use super::*;

    #[rstest]
    fn test_quote_roundtrip() {
        let quote = Quote {
            basic: Some(Basic {
                symbol: "AAPL".to_string(),
                instrument_id: "913256135".to_string(),
                timestamp: "1788552000000".to_string(),
                trading_session: "RTH".to_string(),
            }),
            asks: vec![AskBid {
                price: "190.10".to_string(),
                size: "100".to_string(),
                order: vec![Order {
                    mpid: "NSDQ".to_string(),
                    size: "100".to_string(),
                }],
                broker: vec![],
            }],
            bids: vec![AskBid {
                price: "190.00".to_string(),
                size: "200".to_string(),
                order: vec![],
                broker: vec![Broker {
                    bid: "1".to_string(),
                    name: "TEST".to_string(),
                }],
            }],
        };

        let encoded = quote.encode_to_vec();
        let decoded = Quote::decode(encoded.as_slice()).expect("decodes quote");

        assert_eq!(decoded, quote);
        assert_eq!(decoded.basic.as_ref().unwrap().symbol, "AAPL");
        assert_eq!(decoded.asks[0].order[0].mpid, "NSDQ");
        assert_eq!(decoded.bids[0].broker[0].name, "TEST");
    }

    #[rstest]
    fn test_snapshot_and_tick_roundtrip() {
        let snapshot = Snapshot {
            basic: Some(Basic {
                symbol: "BTCUSD".to_string(),
                instrument_id: "1".to_string(),
                timestamp: "1788552000000".to_string(),
                trading_session: String::new(),
            }),
            trade_time: "1788552000123".to_string(),
            price: "65000.5".to_string(),
            open: "64000".to_string(),
            high: "66000".to_string(),
            low: "63000".to_string(),
            pre_close: "64500".to_string(),
            volume: "12.5".to_string(),
            change: "500.5".to_string(),
            change_ratio: "0.0077".to_string(),
            ..Default::default()
        };
        let tick = Tick {
            basic: snapshot.basic.clone(),
            time: "1788552000456".to_string(),
            price: "65001".to_string(),
            volume: "0.01".to_string(),
            side: "B".to_string(),
        };

        let snapshot_bytes = snapshot.encode_to_vec();
        let tick_bytes = tick.encode_to_vec();

        assert_eq!(
            Snapshot::decode(snapshot_bytes.as_slice()).expect("snapshot"),
            snapshot
        );
        assert_eq!(Tick::decode(tick_bytes.as_slice()).expect("tick"), tick);
        assert_eq!(snapshot.ovn_change_ratio, "");
    }
}
