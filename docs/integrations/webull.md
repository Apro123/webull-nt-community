# Webull

:::warning
This Webull adapter is part of [webull-nt-community](https://github.com/Apro123/webull-nt-community),
an independent community project. It is not affiliated with, endorsed by, or supported by
Nautech Systems Pty Ltd, the official NautilusTrader project, or Webull.
:::

Webull provides US equities market data through its OpenAPI.

This fork integrates with the Webull historical and snapshot market data
endpoints. The capabilities of this adapter include:

- `WebullHistoricalClient.get_history_bars` fetches historical bars through
  the venue's paged API and converts them to Nautilus `Bar` objects.
- `WebullHistoricalClient.get_snapshot` fetches live quote snapshots as
  `QuoteTick` objects.
- `WebullHistoricalClient.get_ticks` fetches recent trades as `TradeTick`
  objects.
- `WebullDataClient` is a polling `DataClient` (registered through
  `WebullDataClientFactory` and `WebullDataClientConfig`) that delivers
  quotes, trades, and bars to a live engine from those same REST endpoints.

:::info
The adapter is a data provider. The Webull execution (trading) API is not
supported yet.
:::

## Overview

The adapter is implemented in Rust with Python bindings. It is compiled into
NautilusTrader, so it does not require a separate Webull client library
installation.

## Credentials

The client signs every request with the OpenAPI app key and app secret
(HMAC-SHA1). Accounts with two-factor authentication enabled must also
provide the 2FA access token, which the Webull SDK writes to
`conf/token.txt` (first line) next to the app credentials. When provided,
the adapter sends the token on every request.

## Live data

HTTP polling remains available via `WebullDataClient`. For push data, use
`WebullStreamingClient` (MQTT) with HTTP subscribe/unsubscribe. `WebullDataClient` polls the
snapshot, tick, and bar endpoints every `poll_interval_secs` (default 5) for
the instruments subscribed through the engine. Instruments must already be
loaded in the cache (the venue exposes no instrument discovery endpoint);
quote and trade subscriptions deliver ticks continuously, and bar
subscriptions deliver each completed bar of a supported timespan.

```python
from nautilus_trader.adapters.webull import (
    WebullDataClientConfig,
    WebullDataClientFactory,
)
from nautilus_trader.live.node import TradingNode

config = WebullDataClientConfig(poll_interval_secs=5)
node = TradingNode(...)
node.register_data_client_factory(WebullDataClientFactory, config)
```

## Bars

`get_history_bars(instrument_id, timespan, start_ms, end_ms,
trading_sessions=None, category="US_STOCK", price_precision=2)` accepts
the following timespans (as strings, or the `Timespan` enum's
`api_value()`), mapped to Nautilus bar aggregations:

| Timespan | Nautilus aggregation |
| :------- | :------------------- |
| `M1`     | `MINUTE`, step 1     |
| `M5`     | `MINUTE`, step 5     |
| `M15`    | `MINUTE`, step 15    |
| `M30`    | `MINUTE`, step 30    |
| `M60`    | `HOUR`, step 1       |
| `M120`   | `HOUR`, step 2       |
| `M240`   | `HOUR`, step 4       |
| `D`      | `DAY`                |
| `W`      | `WEEK`               |
| `M`      | `MONTH`              |

The bars endpoint returns up to 200 bars per request, newest first, and
ignores the requested start time. The client pages backwards from `end_ms`
until the start is reached or the data wall is hit, then returns the bars
oldest first with duplicate boundary bars removed.

Trading sessions can be filtered with `trading_sessions`, for example
`PRE,RTH,ATH` (omit for the venue default).

:::note
The venue reports prices and volumes as strings. The client normalizes them
to Nautilus `Price` at the given precision and `Quantity`.
:::

## Example

`examples/backtest/equity_ema_cross_aapl_webull.py` fetches five days of
AAPL 5-minute bars, backtests the tutorial `EMACross` strategy on them, and
prints the account, order fills, and positions reports.

Run it with the credentials in the environment:

```bash
export WEBULL_API_KEY=...
export WEBULL_API_SECRET=...
export WEBULL_ACCESS_TOKEN=...
python examples/backtest/equity_ema_cross_aapl_webull.py
```

## Sandbox

Webull publishes a test environment at `api.sandbox.webull.com`. Live smoke
tests against it are in
[`python/tests/integration/adapters/webull/test_webull_sandbox.py`](https://github.com/Apro123/webull-nt-community/blob/master/python/tests/integration/adapters/webull/test_webull_sandbox.py);
they skip unless sandbox app credentials are in the environment:

```bash
WEBULL_API_KEY=... WEBULL_API_SECRET=... \
uv run --project python pytest python/tests/integration/adapters/webull -v
```

Each call is retried up to three times with exponential backoff (the sandbox
is shared and intermittently unavailable). Unlike the official adapters, whose
credential-consuming smoke tests are Rust `#[ignore]` tests, these live in
Python so they exercise the same bindings users do.

## Limitations

- Data provider only; execution is not supported yet.
- Prices are rounded to the configured `price_precision` (default 2 in the
  example and scripts). Venue prices finer than that precision - such as
  extended-hours prints - are rounded; match the precision to the
  instrument.
- The Market Data API is limited to 60 requests per minute per app key
  (production tier); the client applies a per-minute quota and retries
  transient failures with exponential backoff, honoring the venue's
  `Retry-After` header (tunable through `WebullDataClientConfig`).
- Historical bar depth and session availability are subject to the venue's
  data terms for the account.

## Contributing

:::info
For additional features or to contribute to the Webull adapter, open an issue or pull request on
the [webull-nt-community](https://github.com/Apro123/webull-nt-community) repository.
:::
