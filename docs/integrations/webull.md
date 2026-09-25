# Webull

:::warning
This Webull adapter is part of [webull-nt-community](https://github.com/Apro123/webull-nt-community),
an independent community project. It is not affiliated with, endorsed by, or supported by
Nautech Systems Pty Ltd, the official NautilusTrader project, or Webull.
:::

Webull provides US equities market data through its OpenAPI.

This fork integrates with the Webull historical market data endpoints.
The capabilities of this adapter include:

- `WebullHistoricalClient.get_history_bars` fetches historical bars through
  the venue's paged API and converts them to Nautilus `Bar` objects.
- `WebullHistoricalClient.get_snapshot` fetches live quote snapshots.
- `WebullHistoricalClient.get_ticks` fetches recent trade ticks.

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

## Bars

`get_history_bars` accepts the following timespans, mapped to Nautilus bar
aggregations:

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

## Limitations

- Data provider only; execution is not supported yet.
- The API allows up to 300 requests per minute; the client applies a
  per-minute quota and retries once on rate-limit responses.
- Historical bar depth and session availability are subject to the venue's
  data terms for the account.

## Contributing

:::info
For additional features or to contribute to the Webull adapter, open an issue or pull request on
the [webull-nt-community](https://github.com/Apro123/webull-nt-community) repository.
:::
