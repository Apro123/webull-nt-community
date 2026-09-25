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
Test webull historical client behavior.
"""

import pytest

from nautilus_trader.adapters.webull import WebullHistoricalClient


def test_client_constructor_masks_credentials() -> None:
    """
    Test client constructor masks credentials in the repr.
    """
    client = WebullHistoricalClient(
        api_key="test-key",
        api_secret="test-secret",
        access_token="test-token",
    )

    assert "test-key" not in repr(client)
    assert "test-secret" not in repr(client)
    assert "test-token" not in repr(client)


def test_get_history_bars_rejects_unknown_timespan() -> None:
    """
    Test get history bars rejects an unknown timespan.
    """
    client = WebullHistoricalClient(api_key="test-key", api_secret="test-secret")

    with pytest.raises(ValueError, match="invalid timespan"):
        client.get_history_bars("AAPL", "stock", "M7", 0, 1)
