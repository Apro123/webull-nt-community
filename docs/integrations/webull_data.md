# Webull market data

How the Webull data provider works, and how to keep repeated backtests from
re-fetching the same data. Complements the [Webull integration guide](webull.md).

## What the client provides

`WebullHistoricalClient` (from `nautilus_trader.adapters.webull`) exposes:

- `get_history_bars(symbol, category, timespan, start_ms, end_ms,
  trading_sessions=None, venue="WEBULL", price_precision=2)` returns Nautilus `Bar`
  objects, oldest first.
- `get_snapshot(symbols, category)` returns a list of dicts with the venue's
  string-typed fields and millisecond epoch times.
- `get_ticks(symbol, category, end_ms, count=200)` returns a list of dicts, newest
  first.

All three are coroutines (`await`).

## Fetch semantics

- `category` is the venue product category, e.g. `"US_STOCK"` (not `"stock"`).
- `timespan` is one of `M1 M5 M15 M30 M60 M120 M240 D W M`, mapped to Nautilus
  bar aggregations (e.g. 5-minute becomes `5-MINUTE-LAST-EXTERNAL`).
- `start_ms` / `end_ms` are epoch **milliseconds**.
- The bars endpoint returns up to 200 bars per request, **newest first**, and
  ignores the requested start time. The client pages backwards with
  `end_time` until the start is reached or the data wall is hit, dedupes page
  boundaries, and returns the bars globally oldest first. A five-day M5 fetch
  takes 2 requests.
- `trading_sessions` filters sessions, e.g. `"PRE,RTH,ATH"`; omit for the
  venue default.
- The venue reports prices and volumes as strings; the client normalizes them
  to `Price` (at the given precision) and `Quantity`.
- The rate limit is 300 requests/minute; the client applies a per-minute
  quota and retries once on 429.
- History depth and session availability are subject to the venue's data
  terms; for some accounts M5 bars lag real time by a few days.

## Multiple strategies, one fetch

No cache is needed within a process: the fetched `bars` are plain in-memory
objects. Either run every strategy on one engine:

```python
engine.add_strategy(strategy_a)
engine.add_strategy(strategy_b)
engine.run()
```

or re-feed the same list into cloned or fresh engines for independent runs.

## Caching across runs: the Parquet data catalog

The Nautilus Parquet catalog *is* the cache; no Redis and no custom layer
are needed. Redis in Nautilus is the in-memory catalog for live data and does
not apply to historical fetches.

Store the fetched data once:

```python
from nautilus_trader.persistence import ParquetDataCatalog

catalog = ParquetDataCatalog("./catalog")
catalog.write_instruments([instrument])
catalog.write_bars(bars, start=bars[0].ts_event, end=bars[-1].ts_event)
```

Later backtests read the catalog instead of calling the API:

```python
from nautilus_trader.config import BacktestDataConfig

data = [
    BacktestDataConfig(
        data_type="Bar",
        catalog_path="./catalog",
        instrument_id=instrument.id,
    ),
]
```

### Check-then-fetch

The catalog and the Webull client are unaware of each other, so the "fetch
only what is missing" glue is yours to write. The catalog provides the
primitives (`get_missing_intervals_for_request`, `query_bars`):

```python
bar_type = "AAPL.WEBULL-5-MINUTE-LAST-EXTERNAL"
start_ns, end_ns = ...  # UnixNanos

missing = catalog.get_missing_intervals_for_request(
    start_ns,
    end_ns,
    "bars",
    bar_type,
)

if missing:
    fresh = []
    for m_start, m_end in missing:
        fresh.extend(
            await client.get_history_bars(  # venue takes ms
                "AAPL",
                "US_STOCK",
                "M5",
                start_ms=m_start // 1_000_000,
                end_ms=m_end // 1_000_000,
            ),
        )
    catalog.write_bars(
        fresh,
        start=fresh[0].ts_event,
        end=fresh[-1].ts_event,
    )

bars = catalog.query_bars(
    [bar_type],
    where_clause=f"ts_event >= {start_ns} AND ts_event < {end_ns}",
)
```

Five gotchas:

- **Units**: the catalog uses **nanoseconds**, the venue client uses
  **milliseconds**.
- **Disjoint writes**: `write_bars` rejects overlapping ranges by default, so
  write only the fetched gap, not the union.
- **Write ranges**: `write_bars` names files from `start`/`end`, so always
  pass the fetched data's `ts_event` range. Omitting them creates zero-width
  files that gap detection cannot see, and the same bars get re-downloaded.
- **Query scope**: `query_bars` matches identifiers against the stored bar
  types, so query with the full bar type string when more than one timeframe
  is stored; an instrument-level query mixes all of them.
- **Range queries**: the `start`/`end` parameters filter on `ts_init`
  (when the record was created). Webull bars set `ts_init` to `ts_event`
  (the replay convention), so `start`/`end` range them directly; a
  `where_clause` on `ts_event` works too.

### Daily fetches, multi-day backtests

`scripts/webull_bulk_download.py` downloads a date range into the
catalog; re-running it fetches only the missing intervals, so the data stays
current without re-downloading:

```bash
python/.venv/bin/python scripts/webull_bulk_download.py \
    --symbol AAPL --timespan M1 --start 2025-09-08 --end 2026-09-04
```

For 1-minute bars, each day is small enough to download in one request:

- Regular hours are 78 M1 bars per day, under the 200-bar page limit, so each
  day is a single API request with no paging.
- With `trading_sessions="PRE,RTH,ATH"` a day is about 192 bars, still one
  request.
- At the 300 requests/minute limit, a full year downloads in well under a
  minute; a year of M1 bars is about 19,500 bars, which is small for the
  backtest engine.

Each day's `write_bars` lands in its own Parquet file. A multi-day backtest
reads only the files for the requested range, merges them in time order, and
the engine clock skips overnight and weekend gaps without special handling.

Practical notes:

- Fetch the timeframe you backtest at. If the strategy trades M5, fetch M5
  directly instead of downloading M1 and resampling; each timespan is its
  own bar type in the catalog, so both can coexist.
- Fix the session filter up front. Mixing RTH-only days with extended-hours
  days changes bar counts and gap lengths, not market structure.
- Walk backwards day by day until a day returns no (or a short) page to find
  the data wall; the newest day or two may lag real time.

Steady state: the first run pays the API; every later run, and every
additional strategy, pays nothing.

## Simple backtest with downloaded data

The section between the PASTE markers at the top of
`scripts/webull_bulk_download.py` is self-contained. Copy it to the top of
any backtest script and call it before loading data. The first run downloads
what is missing; every later run is a no-op:

```python
import asyncio

# Pasted from scripts/webull_bulk_download.py, between the PASTE markers
asyncio.run(
    ensure_webull_bars(
        symbol="AAPL",
        timespan="M5",
        start="2025-09-08",
        end="2026-09-04",
    )
)

from nautilus_trader.persistence import ParquetDataCatalog

catalog = ParquetDataCatalog("./catalog")
# Query by full bar type: instrument-level queries mix all stored timeframes.
bars = catalog.query_bars(["AAPL.WEBULL-5-MINUTE-LAST-EXTERNAL"])
```

Then pass `bars` to the engine as usual (`engine.add_data(bars)`, or
`BacktestDataConfig` for node-based runs).
`examples/backtest/equity_ema_cross_aapl_webull.py` has the full wiring with
the tutorial EMACross strategy.
