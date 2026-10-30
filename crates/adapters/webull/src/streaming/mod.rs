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

//! Webull MQTT market data streaming client.
//!
//! Connects to the venue MQTT broker, then uses signed HTTP
//! `/openapi/market-data/streaming/subscribe` and
//! `/openapi/market-data/streaming/unsubscribe` as the control plane.
//! Publish payloads on topics `quote`, `snapshot`, and `tick` are
//! Protobuf; `notice` is JSON; `echo` is empty.

pub mod client;
pub mod parse;
pub mod proto;

pub use client::{Error, Result, WebullStreamMessage, WebullStreamingClient};
pub use parse::{
    DepthTopQuoteFields, SnapshotTopQuoteFields, WebullStreamSnapshot, WebullStreamTick,
    depth_book_top_quote_fields, parse_depth_book_to_order_book_deltas,
    parse_depth_top_to_quote_tick, parse_stream_tick_to_trade_tick, parse_stream_topic_payload,
    snapshot_top_quote_fields,
};
pub use proto::{AskBid, Basic, Broker, Order, Quote, Snapshot, Tick};
