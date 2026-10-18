// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
//  you may not use this file except in compliance with the License.
//  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
// -------------------------------------------------------------------------------------------------

//! A Webull market data client which polls the venue's REST endpoints.
//!
//! The Webull OpenAPI exposes only request/response market data, so live data
//! is delivered by polling the snapshot, tick, and bar endpoints at a fixed
//! interval. Instruments must already be loaded in the cache: the venue
//! exposes no instrument discovery endpoint for this fork's scope.

use std::{str::FromStr, time::Duration};

use ahash::AHashMap;
use anyhow::Context;
use async_trait::async_trait;
use jiff::Timestamp;
use nautilus_common::{
    cache::CacheView,
    clients::DataClient,
    live::{dst::time, runner::get_data_event_sender, sender::EventSender},
    messages::{
        DataEvent,
        data::{
            SubscribeBars, SubscribeQuotes, SubscribeTrades, UnsubscribeBars, UnsubscribeQuotes,
            UnsubscribeTrades,
        },
    },
};
use nautilus_core::UnixNanos;
use nautilus_live::task::TaskGroup;
use nautilus_model::{
    data::{BarType, Data, QuoteTick, TradeTick},
    enums::{AggressorSide, BarAggregation},
    identifiers::{ClientId, InstrumentId, TradeId, Venue},
    instruments::Instrument,
    types::{Price, Quantity},
};
use nautilus_network::retry::RetryConfig;
use parking_lot::Mutex;
use rust_decimal::Decimal;
use tokio_util::sync::CancellationToken;
use ustr::Ustr;

use crate::{
    common::{
        credential::Credential,
        urls::{WEBULL_HTTP_BASE_URL, WEBULL_HTTP_HOST, host_from_base_url},
    },
    config::WebullDataClientConfig,
    historical::{Timespan, bars_to_nautilus},
    http::client::{WebullHttpClient, WebullHttpConfig},
};

/// Maps a bar aggregation step and cadence back to a Webull timespan.
fn timespan_for_bar_spec(step: u32, aggregation: BarAggregation) -> Option<Timespan> {
    match (step, aggregation) {
        (1, BarAggregation::Minute) => Some(Timespan::M1),
        (5, BarAggregation::Minute) => Some(Timespan::M5),
        (15, BarAggregation::Minute) => Some(Timespan::M15),
        (30, BarAggregation::Minute) => Some(Timespan::M30),
        (1, BarAggregation::Hour) => Some(Timespan::M60),
        (2, BarAggregation::Hour) => Some(Timespan::M120),
        (4, BarAggregation::Hour) => Some(Timespan::M240),
        (1, BarAggregation::Day) => Some(Timespan::Day),
        (1, BarAggregation::Week) => Some(Timespan::Week),
        (1, BarAggregation::Month) => Some(Timespan::Month),
        _ => None,
    }
}

/// An instrument subscription with precisions resolved from the cache at
/// subscribe time, keeping the polling task independent of the cache.
#[derive(Debug, Clone, PartialEq, Eq)]
struct InstrumentSpec {
    instrument_id: InstrumentId,
    price_precision: u8,
    size_precision: u8,
}

/// A bar subscription with its native timespan and precisions resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BarSpec {
    bar_type: BarType,
    timespan: Timespan,
    price_precision: u8,
    size_precision: u8,
}

#[derive(Debug, Default)]
struct Subscriptions {
    quotes: Vec<InstrumentSpec>,
    trades: Vec<InstrumentSpec>,
    bars: Vec<BarSpec>,
    last_trade_ms: AHashMap<InstrumentId, u64>,
    last_bar_ns: AHashMap<BarType, u64>,
}

/// State shared between the client and its polling task.
#[derive(Clone, Debug)]
struct PollState {
    http: WebullHttpClient,
    sender: EventSender<DataEvent>,
    category: Ustr,
    subs: std::sync::Arc<Mutex<Subscriptions>>,
}

impl PollState {
    /// Polls once for every active subscription, emitting only data newer
    /// than what was emitted before.
    async fn poll_once(&self) -> anyhow::Result<()> {
        let quotes = { self.subs.lock().quotes.clone() };
        let trades = { self.subs.lock().trades.clone() };
        let bars = { self.subs.lock().bars.clone() };

        if !quotes.is_empty() {
            self.poll_quotes(&quotes).await?;
        }

        for spec in trades {
            self.poll_trades(&spec).await?;
        }

        for spec in bars {
            self.poll_bars(&spec).await?;
        }

        Ok(())
    }

    async fn poll_quotes(&self, instruments: &[InstrumentSpec]) -> anyhow::Result<()> {
        let symbols: Vec<String> = instruments
            .iter()
            .map(|spec| spec.instrument_id.symbol.to_string())
            .collect();
        let snapshots = self
            .http
            .get_snapshot(&symbols, self.category.as_str())
            .await?;

        for snapshot in snapshots {
            let Some(spec) = instruments
                .iter()
                .find(|spec| spec.instrument_id.symbol.as_str() == snapshot.symbol)
            else {
                continue;
            };
            let instrument_id = spec.instrument_id;
            let (price_precision, size_precision) = (spec.price_precision, spec.size_precision);

            let tick = QuoteTick::new_checked(
                instrument_id,
                price_from_str(&snapshot.bid, price_precision)?,
                price_from_str(&snapshot.ask, price_precision)?,
                quantity_from_str(&snapshot.bid_size, size_precision)?,
                quantity_from_str(&snapshot.ask_size, size_precision)?,
                UnixNanos::from(snapshot.quote_time * 1_000_000),
                UnixNanos::from(Timestamp::now().as_nanosecond() as u64),
            )
            .map_err(|e| anyhow::anyhow!("invalid snapshot quote for {instrument_id}: {e}"))?;

            self.sender
                .send(DataEvent::Data(Data::Quote(tick)))
                .map_err(|e| anyhow::anyhow!("failed to send quote tick: {e}"))?;
        }

        Ok(())
    }

    async fn poll_trades(&self, spec: &InstrumentSpec) -> anyhow::Result<()> {
        let instrument_id = spec.instrument_id;
        let last_ms = self.subs.lock().last_trade_ms.get(&instrument_id).copied();
        let ticks = self
            .http
            .get_ticks(
                instrument_id.symbol.as_str(),
                self.category.as_str(),
                100,
                now_ms(),
            )
            .await?;

        let (price_precision, size_precision) = (spec.price_precision, spec.size_precision);
        let mut newest_ms = last_ms.unwrap_or(0);

        for tick in ticks.iter().rev() {
            // Ticks arrive newest first; walk oldest to newest so
            // `ts_event` is nondecreasing and emit only unseen trades.
            let time_ms = tick.time.parse::<u64>().unwrap_or(0);

            if time_ms <= newest_ms {
                continue;
            }

            let trade = TradeTick::new(
                instrument_id,
                price_from_str(&tick.price, price_precision)?,
                quantity_from_str(&tick.volume, size_precision)?,
                aggressor_side_from_str(&tick.side),
                TradeId::new(Ustr::from(&time_ms.to_string())),
                UnixNanos::from(time_ms * 1_000_000),
                UnixNanos::from(Timestamp::now().as_nanosecond() as u64),
            );

            self.sender
                .send(DataEvent::Data(Data::Trade(trade)))
                .map_err(|e| anyhow::anyhow!("failed to send trade tick: {e}"))?;
            newest_ms = time_ms;
        }

        self.subs
            .lock()
            .last_trade_ms
            .insert(instrument_id, newest_ms);
        Ok(())
    }

    async fn poll_bars(&self, spec: &BarSpec) -> anyhow::Result<()> {
        let instrument_id = spec.bar_type.instrument_id();
        let last_ns = self.subs.lock().last_bar_ns.get(&spec.bar_type).copied();

        let venue_bars = self
            .http
            .get_bars(
                instrument_id.symbol.as_str(),
                self.category.as_str(),
                spec.timespan.api_value(),
                10,
                now_ms(),
                None,
            )
            .await?;

        let bars = bars_to_nautilus(
            &venue_bars,
            &instrument_id,
            spec.timespan,
            spec.price_precision,
        )?;

        for bar in bars {
            if bar.ts_event.as_u64() <= last_ns.unwrap_or(0) {
                continue;
            }

            self.sender
                .send(DataEvent::Data(Data::Bar(bar)))
                .map_err(|e| anyhow::anyhow!("failed to send bar: {e}"))?;
            self.subs
                .lock()
                .last_bar_ns
                .insert(spec.bar_type, bar.ts_event.as_u64());
        }

        Ok(())
    }
}

/// A Webull data client which polls snapshot, tick, and bar endpoints for
/// active subscriptions.
#[derive(Debug)]
pub struct WebullDataClient {
    client_id: ClientId,
    config: WebullDataClientConfig,
    cache: CacheView,
    state: PollState,
    poll_tasks: TaskGroup,
    is_connected: bool,
}

impl WebullDataClient {
    /// Creates a new [`WebullDataClient`] instance.
    ///
    /// # Errors
    ///
    /// Returns an error if credentials cannot be resolved or the HTTP client
    /// cannot be built.
    pub fn new(
        client_id: ClientId,
        config: WebullDataClientConfig,
        cache: CacheView,
    ) -> anyhow::Result<Self> {
        let credential = Credential::resolve(
            config
                .api_key
                .as_ref()
                .map(|key| key.expose_secret().to_owned()),
            config
                .api_secret
                .as_ref()
                .map(|secret| secret.expose_secret().to_owned()),
            config
                .access_token
                .as_ref()
                .map(|token| token.expose_secret().to_owned()),
        );

        anyhow::ensure!(
            !credential.api_key().is_empty() && !credential.api_secret().is_empty(),
            "Webull API key and secret are required (config or environment)"
        );

        let base_url = config
            .base_url_http
            .clone()
            .unwrap_or_else(|| WEBULL_HTTP_BASE_URL.to_string());
        let host = if config.base_url_http.is_some() {
            host_from_base_url(&base_url)
        } else {
            WEBULL_HTTP_HOST.to_string()
        };

        let http_config = WebullHttpConfig {
            base_url,
            host,
            proxy_url: config
                .proxy_url
                .as_ref()
                .map(|url| url.expose_secret().to_owned()),
            timeout_secs: config.timeout_secs,
            retry: RetryConfig {
                max_retries: config.max_retries,
                initial_delay_ms: config.retry_delay_initial_ms,
                max_delay_ms: config.retry_delay_max_ms,
                ..Default::default()
            },
        };
        let http = WebullHttpClient::new_with_config(credential, http_config)?;

        let state = PollState {
            http,
            sender: get_data_event_sender(),
            category: Ustr::from(&config.category),
            subs: std::sync::Arc::new(Mutex::new(Subscriptions::default())),
        };

        Ok(Self {
            client_id,
            config,
            cache,
            state,
            poll_tasks: TaskGroup::new(),
            is_connected: false,
        })
    }

    fn poll_loop(
        &self,
        cancellation: CancellationToken,
    ) -> impl std::future::Future<Output = ()> + Send + 'static {
        let state = self.state.clone();
        let interval = Duration::from_secs(self.config.poll_interval_secs.max(1));

        async move {
            loop {
                tokio::select! {
                    biased;
                    () = cancellation.cancelled() => break,
                    () = time::sleep(interval) => {}
                }

                if let Err(e) = state.poll_once().await {
                    log::error!("Webull poll failed: {e:#}");
                }
            }
        }
    }
}

fn now_ms() -> u64 {
    Timestamp::now().as_millisecond().max(0) as u64
}

fn price_from_str(value: &str, precision: u8) -> anyhow::Result<Price> {
    let decimal =
        Decimal::from_str(value).map_err(|e| anyhow::anyhow!("invalid price '{value}': {e}"))?;
    Price::from_decimal_dp(decimal, precision)
        .map_err(|e| anyhow::anyhow!("invalid price '{value}': {e}"))
}

fn quantity_from_str(value: &str, precision: u8) -> anyhow::Result<Quantity> {
    let decimal =
        Decimal::from_str(value).map_err(|e| anyhow::anyhow!("invalid volume '{value}': {e}"))?;
    Quantity::from_decimal_dp(decimal, precision)
        .map_err(|e| anyhow::anyhow!("invalid volume '{value}': {e}"))
}

fn aggressor_side_from_str(value: &str) -> AggressorSide {
    match value {
        "B" => AggressorSide::Buy,
        "S" => AggressorSide::Sell,
        _ => AggressorSide::NoAggressor,
    }
}

#[async_trait(?Send)]
impl DataClient for WebullDataClient {
    fn client_id(&self) -> ClientId {
        self.client_id
    }

    fn venue(&self) -> Option<Venue> {
        None // Webull is a data provider, not a venue
    }

    fn start(&mut self) -> anyhow::Result<()> {
        log::debug!("Starting {}", self.client_id);
        Ok(())
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        log::debug!("Stopping {}", self.client_id);
        self.poll_tasks.begin_shutdown();
        self.is_connected = false;
        Ok(())
    }

    fn reset(&mut self) -> anyhow::Result<()> {
        log::debug!("Resetting {}", self.client_id);
        self.poll_tasks.begin_shutdown();
        self.poll_tasks = TaskGroup::new();
        self.state.subs = std::sync::Arc::new(Mutex::new(Subscriptions::default()));
        self.is_connected = false;
        Ok(())
    }

    fn dispose(&mut self) -> anyhow::Result<()> {
        log::debug!("Disposing {}", self.client_id);
        self.poll_tasks.begin_shutdown();
        self.is_connected = false;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.is_connected
    }

    fn is_disconnected(&self) -> bool {
        !self.is_connected
    }

    async fn connect(&mut self) -> anyhow::Result<()> {
        if self.is_connected {
            return Ok(());
        }

        log::info!("Connecting {}", self.client_id);

        if !self.poll_tasks.is_open() {
            self.poll_tasks
                .start_generation()
                .context("failed to reopen Webull polling tasks")?;
        }

        let cancellation = self.poll_tasks.cancellation_token();
        self.poll_tasks
            .spawn_named("WebullDataClient.poll", self.poll_loop(cancellation))
            .context("failed to spawn Webull polling task")?;

        self.is_connected = true;
        Ok(())
    }

    async fn disconnect(&mut self) -> anyhow::Result<()> {
        if !self.is_connected {
            return Ok(());
        }

        log::info!("Disconnecting {}", self.client_id);
        self.poll_tasks.begin_shutdown();
        self.poll_tasks
            .finish_shutdown(Duration::from_secs(5), Duration::from_secs(5))
            .await
            .context("failed to stop Webull polling task")?;
        self.poll_tasks = TaskGroup::new();
        self.is_connected = false;
        Ok(())
    }

    fn subscribe_quotes(&mut self, cmd: SubscribeQuotes) -> anyhow::Result<()> {
        let spec = self.resolve_instrument(cmd.instrument_id)?;
        let mut subs = self.state.subs.lock();

        if !subs.quotes.contains(&spec) {
            subs.quotes.push(spec);
        }

        Ok(())
    }

    fn unsubscribe_quotes(&mut self, cmd: &UnsubscribeQuotes) -> anyhow::Result<()> {
        let mut subs = self.state.subs.lock();
        subs.quotes
            .retain(|spec| spec.instrument_id != cmd.instrument_id);
        Ok(())
    }

    fn subscribe_trades(&mut self, cmd: SubscribeTrades) -> anyhow::Result<()> {
        let spec = self.resolve_instrument(cmd.instrument_id)?;
        let mut subs = self.state.subs.lock();

        if !subs.trades.contains(&spec) {
            subs.trades.push(spec);
        }

        Ok(())
    }

    fn unsubscribe_trades(&mut self, cmd: &UnsubscribeTrades) -> anyhow::Result<()> {
        let mut subs = self.state.subs.lock();
        subs.trades
            .retain(|spec| spec.instrument_id != cmd.instrument_id);
        Ok(())
    }

    fn subscribe_bars(&mut self, cmd: SubscribeBars) -> anyhow::Result<()> {
        let Some(timespan) = timespan_for_bar_spec(
            cmd.bar_type.spec().step.get() as u32,
            cmd.bar_type.spec().aggregation,
        ) else {
            anyhow::bail!(
                "Webull has no native timespan for the bar specification {:?}",
                cmd.bar_type.spec()
            );
        };

        let (price_precision, size_precision) =
            self.resolve_precisions(cmd.bar_type.instrument_id())?;
        let spec = BarSpec {
            bar_type: cmd.bar_type,
            timespan,
            price_precision,
            size_precision,
        };
        let mut subs = self.state.subs.lock();

        if !subs.bars.contains(&spec) {
            subs.bars.push(spec);
        }

        Ok(())
    }

    fn unsubscribe_bars(&mut self, cmd: &UnsubscribeBars) -> anyhow::Result<()> {
        let mut subs = self.state.subs.lock();
        subs.bars.retain(|spec| spec.bar_type != cmd.bar_type);
        Ok(())
    }
}

impl WebullDataClient {
    fn resolve_precisions(&self, instrument_id: InstrumentId) -> anyhow::Result<(u8, u8)> {
        let instrument = self
            .cache
            .borrow()
            .instrument(&instrument_id)
            .cloned()
            .with_context(|| format!("{instrument_id} is not loaded in the cache"))?;

        Ok((instrument.price_precision(), instrument.size_precision()))
    }

    fn resolve_instrument(&self, instrument_id: InstrumentId) -> anyhow::Result<InstrumentSpec> {
        let (price_precision, size_precision) = self.resolve_precisions(instrument_id)?;

        Ok(InstrumentSpec {
            instrument_id,
            price_precision,
            size_precision,
        })
    }
}

#[cfg(test)]
mod tests {
    use nautilus_model::enums::BarAggregation;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(1, BarAggregation::Minute, Some(Timespan::M1))]
    #[case(5, BarAggregation::Minute, Some(Timespan::M5))]
    #[case(2, BarAggregation::Hour, Some(Timespan::M120))]
    #[case(1, BarAggregation::Day, Some(Timespan::Day))]
    #[case(3, BarAggregation::Minute, None)]
    #[case(1, BarAggregation::Second, None)]
    fn test_timespan_for_bar_spec(
        #[case] step: u32,
        #[case] aggregation: BarAggregation,
        #[case] expected: Option<Timespan>,
    ) {
        assert_eq!(timespan_for_bar_spec(step, aggregation), expected);
    }

    #[rstest]
    #[case("B", AggressorSide::Buy)]
    #[case("S", AggressorSide::Sell)]
    #[case("N", AggressorSide::NoAggressor)]
    #[case("L", AggressorSide::NoAggressor)]
    fn test_aggressor_side_from_str(#[case] value: &str, #[case] expected: AggressorSide) {
        assert_eq!(aggressor_side_from_str(value), expected);
    }
}
