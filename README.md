# webull-nt-community

> [!IMPORTANT]
> This is an independent community project. It is not affiliated with, endorsed by, or supported
> by Nautech Systems Pty Ltd or the official NautilusTrader project, and it is not an official
> Webull product. Webull and NautilusTrader are trademarks of their respective owners.

A public fork of [nautilus_trader](https://github.com/nautechsystems/nautilus_trader) (based on
release `2.0.0rc7` (Rust crates `0.66.0`)) that adds a **Webull market data adapter**. Everything else is unmodified
upstream code. For the engine itself - architecture, installation, tutorials, security policy -
read the [official repository](https://github.com/nautechsystems/nautilus_trader) and the
[documentation](https://nautilustrader.io/docs/); they are not duplicated here.

## Webull status

Feature universe: [Market Data API](https://developer.webull.com/apis/docs/market-data-api/overview),
[Trade API](https://developer.webull.com/apis/docs/trade-api/overview). Scope is **US market data** (stocks/ETFs plus Non-Display options, futures,
crypto, and event-contract REST/stream surfaces). Live polling remains stock/ETF
oriented; Trade API is still out of scope.

### Done

| Area                                  | Notes                                                                                                                                         |
| :------------------------------------ | :-------------------------------------------------------------------------------------------------------------------------------------------- |
| Signed REST client                    | HMAC-SHA1 per the Webull spec; app key/secret, optional 2FA access token; 60 req/min self-throttle, retry with backoff honoring `Retry-After` |
| Snapshot quotes (`get_snapshot`)      | Real-time stock/ETF quotes as `QuoteTick` objects                                                                                             |
| Trade ticks (`get_ticks`)             | Recent trades as `TradeTick` objects, count-limited                                                                                           |
| Historical bars (`get_history_bars`)  | `M1`-`M240`, `D`, `W`, `M`; backwards paging with dedupe; `PRE,RTH,ATH` session filter                                                        |
| Live data client (`WebullDataClient`) | Polling `DataClient` for quotes/trades/bars, registered through `WebullDataClientFactory` and `WebullDataClientConfig`                        |
| Python bindings                       | `WebullHistoricalClient`, `WebullDataClientConfig`, `WebullDataClientFactory`, `Timespan`                                                     |
| Backtest example                      | [`examples/backtest/equity_ema_cross_aapl_webull.py`](examples/backtest/equity_ema_cross_aapl_webull.py)                                      |
| Instrument discovery                  | `WebullInstrumentProvider` loads stock/ETF and crypto profiles into the cache via `/trading/instruments/*/profiles/list`                      |
| Order book depth (REST)               | `get_stock_depths` / `get_futures_depths` / `get_event_depths` → Clear+Add `OrderBookDeltas` (TotalView / overnight entitlements as required) |
| Multi-asset REST snapshots/ticks      | Options, futures, crypto, and event-contract Non-Display paths via `get_snapshots_for_category` / `get_ticks_for_category`                    |
| Market data streaming (MQTT)          | `WebullStreamingClient` over MQTT 3.1.1 with HTTP subscribe/unsubscribe control plane and protobuf parsers                                    |
| Catalog bulk download                 | [`scripts/webull_bulk_download.py`](scripts/webull_bulk_download.py), incremental gap fill into a Parquet catalog                             |
| Tests                                 | Rust unit + mock-server integration tests; Python unit tests; live sandbox smoke tests (skipped without credentials)                          |
| Documentation                         | [Adapter guide](docs/integrations/webull.md), [data guide](docs/integrations/webull_data.md)                                                  |

### Needs to be done

| Area                                            | Status      | Notes                                                                                |
| :---------------------------------------------- | :---------- | :----------------------------------------------------------------------------------- |
| Options/futures/crypto live DataClient wiring   | Partial     | REST/stream clients exist; polling `WebullDataClient` is still US stock/ETF oriented |
| Event-contract first-class instruments          | Not started | Depth/tick REST + stream topics exist; no `InstrumentProvider` mapping yet           |
| Trade API (orders, positions, balances, events) | Not started | Out of scope for this fork's current phase                                           |

## Getting started

Build the fork from source the same way as upstream (see the upstream
[developer guide](https://nautilustrader.io/docs/latest/developer_guide/installation)), then
provide credentials through the environment only - never commit them; a git-ignored `.env` file
is a convenient place to keep them:

```bash
export WEBULL_API_KEY=...
export WEBULL_API_SECRET=...
export WEBULL_ACCESS_TOKEN=...   # only if the account has 2FA enforced
python examples/backtest/equity_ema_cross_aapl_webull.py
```

Sandbox (test-environment) smoke tests:

```bash
WEBULL_API_KEY=... WEBULL_API_SECRET=... \
uv run --project python pytest python/tests/integration/adapters/webull -v
```

## Cleaning Rust build caches

Building and testing this workspace can leave tens of gigabytes under `target/`,
mostly Rust incremental compilation caches (`target/*/incremental`, especially
the custom `nextest` profile). Those caches are safe to delete; compiled deps are
kept, so the next build is cheaper than a full `cargo clean`.

```bash
bash scripts/clean-rust-incremental.bash          # dry-run: list sizes only
bash scripts/clean-rust-incremental.bash --apply  # delete incremental/ dirs
```

For a full wipe of `target/` (and other build dirs), use `make clean-builds`.

## License

LGPL-3.0-only, as upstream; see [LICENSE](LICENSE). The Webull adapter code in this fork is
distributed under the same license.
