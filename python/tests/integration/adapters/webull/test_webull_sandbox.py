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
Live smoke tests against the Webull sandbox (test) market data endpoints.

These exercise the snapshot, ticks, and historical bars paths against
`api.sandbox.webull.com` (the documented test-environment host; see
<https://developer.webull.com/apis/docs/sdk>). The sandbox is shared and can
be intermittently unavailable, so every call is retried up to three times
with exponential backoff.

The tests are skipped unless sandbox app credentials are present in the
environment, so runs with all adapter variables stripped (for example
`make pre-flight`) never depend on them.

"""

import asyncio
import os
import time

import pytest

from nautilus_trader.adapters.webull import WebullHistoricalClient


SANDBOX_BASE_URL = "https://api.sandbox.webull.com"
CATEGORY = "US_STOCK"
SYMBOL = "AAPL"

MAX_ATTEMPTS = 3
BASE_DELAY_S = 1.0

pytestmark = pytest.mark.skipif(
    not os.environ.get("WEBULL_API_KEY") or not os.environ.get("WEBULL_API_SECRET"),
    reason="requires WEBULL_API_KEY/WEBULL_API_SECRET sandbox app credentials",
)


def _client() -> WebullHistoricalClient:
    return WebullHistoricalClient(
        api_key=os.environ["WEBULL_API_KEY"],
        api_secret=os.environ["WEBULL_API_SECRET"],
        access_token=os.environ.get("WEBULL_ACCESS_TOKEN"),
        base_url=SANDBOX_BASE_URL,
    )


async def _call_with_retry(awaitable_factory):
    """
    Await the factory, retrying up to three times with exponential backoff.
    """
    # The client surfaces every request failure (network, rate limit, API
    # error) as a ValueError, so any failure is retried rather than sorted
    # by type. Delays grow 1s, 2s across the three attempts.
    for attempt in range(MAX_ATTEMPTS):
        try:
            return await awaitable_factory()
        except ValueError:
            if attempt == MAX_ATTEMPTS - 1:
                raise
            await asyncio.sleep(BASE_DELAY_S * 2**attempt)

    raise AssertionError("unreachable")


@pytest.mark.asyncio
async def test_sandbox_snapshot() -> None:
    """
    Test a snapshot request completes against the sandbox.
    """
    client = _client()
    snapshots = await _call_with_retry(lambda: client.get_snapshot([SYMBOL], CATEGORY))
    assert isinstance(snapshots, list)


@pytest.mark.asyncio
async def test_sandbox_ticks() -> None:
    """
    Test a ticks request completes against the sandbox.
    """
    client = _client()
    end_ms = int(time.time() * 1000)
    ticks = await _call_with_retry(lambda: client.get_ticks(SYMBOL, CATEGORY, end_ms, count=10))
    assert isinstance(ticks, list)


@pytest.mark.asyncio
async def test_sandbox_history_bars() -> None:
    """
    Test a paged historical bars fetch completes against the sandbox.
    """
    client = _client()
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - 2 * 24 * 60 * 60 * 1000  # Two days back
    bars = await _call_with_retry(
        lambda: client.get_history_bars(SYMBOL, CATEGORY, "M5", start_ms, end_ms),
    )
    assert isinstance(bars, list)
