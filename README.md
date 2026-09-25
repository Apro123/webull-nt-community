# webull-nt-community

> [!IMPORTANT]
> This is an independent community project. It is not affiliated with, endorsed by, or supported
> by Nautech Systems Pty Ltd or the official NautilusTrader project, and it is not an official
> Webull product. Webull and NautilusTrader are trademarks of their respective owners.

A public fork of [nautilus_trader](https://github.com/nautechsystems/nautilus_trader) (based on
release `2.0.0rc5`) that adds a **Webull market data adapter**. Everything else is unmodified
upstream code. For the engine itself - architecture, installation, tutorials, security policy -
read the [official repository](https://github.com/nautechsystems/nautilus_trader) and the
[documentation](https://nautilustrader.io/docs/); they are not duplicated here.

## Webull status

Feature universe: [Market Data API](https://developer.webull.com/apis/docs/market-data-api/overview),
[Trade API](https://developer.webull.com/apis/docs/trade-api/overview). Scope so far is **US
stocks/ETFs market data only**.

### Done

| Area                                 | Notes                                                                                                                 |
| :----------------------------------- | :-------------------------------------------------------------------------------------------------------------------- |
| Signed REST client                   | HMAC-SHA1 per the Webull spec; app key/secret, optional 2FA access token; 60 req/min self-throttle with one 429 retry |
| Snapshot quotes (`get_snapshot`)     | Real-time stock/ETF quotes                                                                                            |
| Trade ticks (`get_ticks`)            | Recent trades, count-limited                                                                                          |
| Historical bars (`get_history_bars`) | `M1`-`M240`, `D`, `W`, `M`; backwards paging with dedupe; `PRE,RTH,ATH` session filter                                |
| Python bindings                      | `nautilus_trader.adapters.webull.WebullHistoricalClient`                                                              |
| Backtest example                     | [`examples/backtest/equity_ema_cross_aapl_webull.py`](examples/backtest/equity_ema_cross_aapl_webull.py)              |
| Catalog bulk download                | [`scripts/webull_bulk_download.py`](scripts/webull_bulk_download.py), incremental gap fill into a Parquet catalog     |
| Tests                                | Rust unit + mock-server integration tests; Python unit tests; live sandbox smoke tests (skipped without credentials)  |
| Documentation                        | [Adapter guide](docs/integrations/webull.md), [data guide](docs/integrations/webull_data.md)                          |

### Needs to be done

| Area                                            | Status      | Notes                                                               |
| :---------------------------------------------- | :---------- | :------------------------------------------------------------------ |
| Live adapter integration (`DataClient`/factory) | Not started | Currently a standalone fetch utility, not a registered live adapter |
| Market data streaming (MQTT/WebSocket)          | Not started | Real-time push instead of polling                                   |
| Options, futures, crypto, event contract data   | Not started | Categories beyond stocks/ETFs                                       |
| Order book depth (L2, overnight depth)          | Not started | Requires the venue's TotalView entitlements                         |
| Instrument discovery (`InstrumentProvider`)     | Not started | Bars need a caller-supplied venue/precision                         |
| Trade API (orders, positions, balances, events) | Not started | Out of scope for this fork's current phase                          |

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

## License

LGPL-3.0-only, as upstream; see [LICENSE](LICENSE). The Webull adapter code in this fork is
distributed under the same license.
