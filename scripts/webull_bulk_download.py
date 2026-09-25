#!/usr/bin/env python3
# -------------------------------------------------------------------------------------------------
#  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
#  https://nautechsystems.io
#
#  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
#  You may not use this file except in compliance with the License.
#  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
#
#  Unless required by applicable law or agreed to in writing, software
#  distributed under the License is distributed on an "AS IS" BASIS,
#  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
#  See the License for the specific language governing permissions and
#  limitations under the License.
# -------------------------------------------------------------------------------------------------
"""
Bulk download of Webull intraday bars into the Nautilus Parquet data catalog.

Fetches only the date ranges missing from the catalog, so re-running fills
gaps without re-downloading what is already stored.

Standalone:

    python/.venv/bin/python scripts/webull_bulk_download.py \
        --symbol AAPL --timespan M1 --start 2025-09-08 --end 2026-09-04

The section between the PASTE markers below is self-contained. Copy it into
any backtest script and call ``ensure_webull_bars`` before loading data, so
the script downloads what it needs and otherwise reads the catalog.

Requires the WEBULL_API_KEY, WEBULL_API_SECRET, and WEBULL_ACCESS_TOKEN
environment variables (see the "Webull" section of the README).

"""

# ruff: noqa: PLR0913, T201

import argparse

# === PASTE-ABLE HEADER START ===
import asyncio
import os
from datetime import UTC
from datetime import datetime
from datetime import timedelta
from pathlib import Path

from nautilus_trader.adapters.webull import WebullHistoricalClient
from nautilus_trader.core.datetime import dt_to_unix_nanos
from nautilus_trader.model import Currency
from nautilus_trader.model import Equity
from nautilus_trader.model import InstrumentId
from nautilus_trader.model import Price
from nautilus_trader.model import Symbol
from nautilus_trader.model import Venue
from nautilus_trader.persistence import ParquetDataCatalog


BAR_TYPE_SUFFIX = {
    "M1": "1-MINUTE",
    "M5": "5-MINUTE",
    "M15": "15-MINUTE",
    "M30": "30-MINUTE",
    "M60": "1-HOUR",
    "M120": "2-HOUR",
    "M240": "4-HOUR",
    "D": "1-DAY",
}


def ensure_catalog_dir(catalog_path: str) -> None:
    """
    Create the catalog root directory if it does not exist.
    """
    Path(catalog_path).mkdir(parents=True, exist_ok=True)


def make_webull_instrument(symbol: str) -> Equity:
    """
    Create the equity instrument for the symbol on the WEBULL venue.
    """
    return Equity(
        instrument_id=InstrumentId(Symbol(symbol), Venue("WEBULL")),
        raw_symbol=Symbol(symbol),
        currency=Currency.from_str("USD"),
        price_precision=2,
        price_increment=Price.from_str("0.01"),
        ts_event=0,
        ts_init=0,
    )


async def ensure_webull_bars(
    *,
    symbol: str,
    timespan: str,
    start: str,
    end: str,
    sessions: str = "PRE,RTH,ATH",
    catalog_path: str = "./catalog",
) -> int:
    """
    Download the bars missing from the catalog for [start, end].

    Both dates are YYYY-MM-DD (UTC days). Returns the number of new bars written; the
    catalog is left ready for reads.

    """
    symbol = symbol.upper()
    bar_type = f"{symbol}.WEBULL-{BAR_TYPE_SUFFIX[timespan]}-LAST-EXTERNAL"
    instrument = make_webull_instrument(symbol)

    ensure_catalog_dir(catalog_path)
    catalog = ParquetDataCatalog(catalog_path)
    catalog.write_instruments([instrument])

    start_ns = dt_to_unix_nanos(datetime.fromisoformat(start).replace(tzinfo=UTC))
    end_ns = dt_to_unix_nanos(
        (datetime.fromisoformat(end) + timedelta(days=1)).replace(tzinfo=UTC),
    )

    missing = catalog.get_missing_intervals_for_request(
        start_ns,
        end_ns,
        "bars",
        bar_type,
    )
    print(f"Catalog gaps for {bar_type}: {len(missing)} interval(s)")
    if not missing:
        return 0

    client = WebullHistoricalClient(
        api_key=os.environ["WEBULL_API_KEY"],
        api_secret=os.environ["WEBULL_API_SECRET"],
        access_token=os.environ["WEBULL_ACCESS_TOKEN"],
    )

    total = 0
    for gap_start, gap_end in missing:
        bars = await client.get_history_bars(
            symbol=symbol,
            category="US_STOCK",
            timespan=timespan,
            start_ms=gap_start // 1_000_000,
            # Venue end time is inclusive; keep the last bar out of the next gap.
            end_ms=gap_end // 1_000_000 - 1,
            trading_sessions=sessions,
        )
        # The venue start boundary is inclusive, and catalog gaps begin at
        # the stored data's end, so drop any boundary bar already stored.
        bars = [b for b in bars if b.ts_event > gap_start]
        if not bars:
            continue  # non-trading day, or beyond the venue data wall
        catalog.write_bars(bars, start=bars[0].ts_event, end=bars[-1].ts_event)
        total += len(bars)
        print(f"Wrote {len(bars)} bars ({bars[0].ts_event}..{bars[-1].ts_event})")

    return total


# === PASTE-ABLE HEADER END ===


def main() -> None:
    """
    Parse arguments and run the bulk download.
    """
    parser = argparse.ArgumentParser(
        description="Bulk download Webull intraday bars into the Parquet catalog.",
    )
    parser.add_argument("--symbol", default="AAPL")
    parser.add_argument("--timespan", default="M1", choices=sorted(BAR_TYPE_SUFFIX))
    parser.add_argument("--start", required=True, help="First day, YYYY-MM-DD (UTC)")
    parser.add_argument("--end", required=True, help="Last day, YYYY-MM-DD (UTC)")
    parser.add_argument("--sessions", default="PRE,RTH,ATH")
    parser.add_argument("--catalog", default="./catalog")
    args = parser.parse_args()
    asyncio.run(
        ensure_webull_bars(
            symbol=args.symbol,
            timespan=args.timespan,
            start=args.start,
            end=args.end,
            sessions=args.sessions,
            catalog_path=args.catalog,
        ),
    )


if __name__ == "__main__":
    main()
