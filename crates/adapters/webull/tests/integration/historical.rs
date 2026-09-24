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

//! Integration tests for the paged historical bars fetch, against a loopback mock server.

use std::time::Duration;

use nautilus_webull::{
    common::Credential,
    historical::{Timespan, WebullHistoricalClient, parse_bar_time_ms},
    http::WebullHttpClient,
};

use crate::common::mock_server;

const END_MS: u64 = 1_788_534_000_000; // 2026-09-04T15:00:00Z
const STEP_MS: u64 = 300_000; // 5 minutes

fn ms_to_time_str(ms: u64) -> String {
    format!(
        "{}+0000",
        jiff::Timestamp::from_millisecond(ms as i64)
            .expect("valid millisecond timestamp")
            .strftime("%Y-%m-%dT%H:%M:%S")
    )
}

fn page_json(times_ms: &[u64]) -> String {
    let bars = times_ms
        .iter()
        .map(|ms| {
            format!(
                r#"{{"symbol":"AAPL","time":"{}","open":"100.0","high":"100.5","low":"99.5","close":"100.2","volume":"100","trading_session":"RTH","instrument_id":"123"}}"#,
                ms_to_time_str(*ms)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{bars}]")
}

fn mock_client(
    responses: Vec<(u16, String)>,
) -> (WebullHttpClient, std::sync::mpsc::Receiver<String>) {
    let (base_url, requests) = mock_server::start(responses);
    let client = WebullHttpClient::new_with_base_url(
        Credential::new(
            "test-api-key".to_string(),
            "test-api-secret".to_string(),
            Some("test-access-token".to_string()),
        ),
        &base_url,
        "127.0.0.1",
        Duration::from_millis(1),
    )
    .expect("builds client");
    (client, requests)
}

fn tokio_test_block_on<T: Send + 'static>(
    fut: impl core::future::Future<Output = T> + Send + 'static,
) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("builds runtime")
        .block_on(fut)
}

#[test]
fn test_get_history_bars_pages_backwards_to_start() {
    // Page 1: 200 bars (full page) ending at END_MS.
    let page1: Vec<u64> = (0..200).map(|i| END_MS - i * STEP_MS).collect();
    // Page 2: 150 bars (short page = data wall) continuing backwards.
    let page2: Vec<u64> = (0..150).map(|i| END_MS - (200 + i) * STEP_MS).collect();
    let start_ms = *page2.last().expect("page has bars");

    let (client, requests) = mock_client(vec![(200, page_json(&page1)), (200, page_json(&page2))]);
    let historical = WebullHistoricalClient::new(client);

    let bars = tokio_test_block_on(async move {
        historical
            .get_history_bars("AAPL", "US_STOCK", Timespan::M5, start_ms, END_MS, None)
            .await
    })
    .expect("fetches paged bars");

    assert_eq!(bars.len(), 350, "all bars across both pages");
    assert_eq!(
        bars.first().expect("non-empty").time,
        ms_to_time_str(start_ms)
    );
    assert_eq!(bars.last().expect("non-empty").time, ms_to_time_str(END_MS));

    // Ascending (oldest first)
    for (a, b) in bars.iter().zip(bars.iter().skip(1)) {
        let a_ms = parse_bar_time_ms(&a.time).expect("parses");
        let b_ms = parse_bar_time_ms(&b.time).expect("parses");
        assert!(a_ms < b_ms);
    }

    // Second request walks backwards from the oldest page-1 bar.
    let first_request = requests.recv().expect("first request");
    let second_request = requests.recv().expect("second request");
    assert!(first_request.contains(&format!("end_time={END_MS}")));
    assert!(second_request.contains(&format!("end_time={}", END_MS - 199 * STEP_MS - 1)));
    assert!(requests.try_recv().is_err(), "no further requests");
}

#[test]
fn test_get_history_bars_stops_on_short_page() {
    let page: Vec<u64> = (0..2).map(|i| END_MS - i * STEP_MS).collect();
    let start_ms = *page.last().expect("page has bars");

    let (client, requests) = mock_client(vec![(200, page_json(&page))]);
    let historical = WebullHistoricalClient::new(client);

    let bars = tokio_test_block_on(async move {
        historical
            .get_history_bars(
                "AAPL",
                "US_STOCK",
                Timespan::M5,
                start_ms,
                END_MS,
                Some("PRE,RTH,ATH"),
            )
            .await
    })
    .expect("fetches bars");

    assert_eq!(bars.len(), 2);
    let request = requests.recv().expect("request captured");
    assert!(request.contains("trading_sessions=PRE%2CRTH%2CATH"));
    assert!(requests.try_recv().is_err(), "a short page stops the walk");
}

#[test]
fn test_get_history_bars_empty_page() {
    let (client, _requests) = mock_client(vec![(200, String::from("[]"))]);
    let historical = WebullHistoricalClient::new(client);

    let bars = tokio_test_block_on(async move {
        historical
            .get_history_bars("AAPL", "US_STOCK", Timespan::M5, 1_000, 2_000, None)
            .await
    })
    .expect("empty result");

    assert!(bars.is_empty());
}
