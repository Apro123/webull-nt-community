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
Example of equity ema cross on AAPL bars cached in the Nautilus data catalog.

The section between the PASTE markers is copied from scripts/webull_bulk_download.py:
the first run downloads the last five days of AAPL 5-minute bars into ./catalog, later
runs read the catalog without credentials. See docs/integrations/webull_data.md for
details.

"""

# ruff: noqa: E402, I001  # PASTE block below carries its own imports mid-file

import sys
from decimal import Decimal

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

from nautilus_trader.backtest import BacktestEngine
from nautilus_trader.config import BacktestEngineConfig
from nautilus_trader.config import RiskEngineConfig
from nautilus_trader.execution import PerContractFeeModel
from nautilus_trader.model import AccountType
from nautilus_trader.model import BarType
from nautilus_trader.model import Money
from nautilus_trader.model import OmsType
from nautilus_trader.model import TraderId

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "docs" / "tutorials"))

from ema_cross import EMACross
from ema_cross import EMACrossConfig


SYMBOL = "AAPL"
VENUE = Venue("WEBULL")
USD = Currency.from_str("USD")
BAR_TYPE = BarType.from_str(f"{SYMBOL}.WEBULL-5-MINUTE-LAST-EXTERNAL")


if __name__ == "__main__":
    end = datetime.now(UTC)
    start = end - timedelta(days=5)
    asyncio.run(
        ensure_webull_bars(
            symbol=SYMBOL,
            timespan="M5",
            start=start.strftime("%Y-%m-%d"),
            end=end.strftime("%Y-%m-%d"),
        ),
    )

    catalog = ParquetDataCatalog("./catalog")
    bars = catalog.query_bars(
        [str(BAR_TYPE)],
        start=dt_to_unix_nanos(start),
        end=dt_to_unix_nanos(end),
    )
    print(f"Backtesting {len(bars)} cached M5 bars")

    instrument = make_webull_instrument(SYMBOL)

    engine = BacktestEngine(
        BacktestEngineConfig(
            trader_id=TraderId.from_str("BACKTESTER-001"),
            risk_engine=RiskEngineConfig(bypass=True),
        ),
    )
    engine.add_venue(
        venue=VENUE,
        oms_type=OmsType.NETTING,
        account_type=AccountType.MARGIN,
        base_currency=USD,
        fee_model=PerContractFeeModel(Money(1, USD)),
        starting_balances=[Money(100_000, USD)],
    )
    engine.add_instrument(instrument)
    engine.add_data(bars)

    strategy = EMACross(
        EMACrossConfig(
            instrument_id=instrument.id,
            bar_type=BAR_TYPE,
            trade_size=Decimal(10),
        ),
    )
    engine.add_strategy(strategy)
    engine.run()

    print(engine.generate_account_report(VENUE))
    print(engine.generate_order_fills_report())
    print(engine.generate_positions_report())

    engine.dispose()
