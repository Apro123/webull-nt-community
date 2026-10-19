// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
//  You may not use this file except in compliance with the License.
//  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
// -------------------------------------------------------------------------------------------------

//! Wire-format enums for the Webull Market Data API.

use nautilus_model::{
    data::BarSpecification,
    enums::{BarAggregation, PriceType},
};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString};

/// Webull intraday timespans and their Nautilus bar aggregation.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    Display,
    EnumIter,
    EnumString,
)]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(
        module = "nautilus_trader.adapters.webull",
        eq,
        hash,
        frozen,
        from_py_object,
        rename_all = "SCREAMING_SNAKE_CASE"
    )
)]
#[cfg_attr(
    feature = "python",
    pyo3_stub_gen::derive::gen_stub_pyclass_enum(module = "nautilus_trader.adapters.webull")
)]
#[repr(u8)]
pub enum Timespan {
    /// One-minute bars.
    #[strum(to_string = "M1")]
    M1 = 0,
    /// Five-minute bars.
    #[strum(to_string = "M5")]
    M5 = 1,
    /// Fifteen-minute bars.
    #[strum(to_string = "M15")]
    M15 = 2,
    /// Thirty-minute bars.
    #[strum(to_string = "M30")]
    M30 = 3,
    /// One-hour bars.
    #[strum(to_string = "M60")]
    M60 = 4,
    /// Two-hour bars.
    #[strum(to_string = "M120")]
    M120 = 5,
    /// Four-hour bars.
    #[strum(to_string = "M240")]
    M240 = 6,
    /// Daily bars.
    #[strum(to_string = "D", serialize = "Day")]
    Day = 7,
    /// Weekly bars.
    #[strum(to_string = "W", serialize = "Week")]
    Week = 8,
    /// Monthly bars.
    #[strum(to_string = "M", serialize = "Month")]
    Month = 9,
}

impl Timespan {
    /// The Nautilus bar specification this timespan corresponds to
    /// (last-price aggregation).
    #[must_use]
    pub fn bar_specification(self) -> BarSpecification {
        match self {
            Self::M1 => BarSpecification::new(1, BarAggregation::Minute, PriceType::Last),
            Self::M5 => BarSpecification::new(5, BarAggregation::Minute, PriceType::Last),
            Self::M15 => BarSpecification::new(15, BarAggregation::Minute, PriceType::Last),
            Self::M30 => BarSpecification::new(30, BarAggregation::Minute, PriceType::Last),
            Self::M60 => BarSpecification::new(1, BarAggregation::Hour, PriceType::Last),
            Self::M120 => BarSpecification::new(2, BarAggregation::Hour, PriceType::Last),
            Self::M240 => BarSpecification::new(4, BarAggregation::Hour, PriceType::Last),
            Self::Day => BarSpecification::new(1, BarAggregation::Day, PriceType::Last),
            Self::Week => BarSpecification::new(1, BarAggregation::Week, PriceType::Last),
            Self::Month => BarSpecification::new(1, BarAggregation::Month, PriceType::Last),
        }
    }

    /// The timespan value accepted by the Webull API.
    #[must_use]
    pub const fn api_value(self) -> &'static str {
        match self {
            Self::M1 => "M1",
            Self::M5 => "M5",
            Self::M15 => "M15",
            Self::M30 => "M30",
            Self::M60 => "M60",
            Self::M120 => "M120",
            Self::M240 => "M240",
            Self::Day => "D",
            Self::Week => "W",
            Self::Month => "M",
        }
    }
}
#[cfg(test)]
mod tests {
    use nautilus_model::enums::{BarAggregation, PriceType};
    use rstest::rstest;

    use super::Timespan;

    #[rstest]
    fn test_timespan_api_values() {
        assert_eq!(Timespan::M1.api_value(), "M1");
        assert_eq!(Timespan::M5.api_value(), "M5");
        assert_eq!(Timespan::M60.api_value(), "M60");
        assert_eq!(Timespan::Day.api_value(), "D");
        assert_eq!(Timespan::Week.api_value(), "W");
        assert_eq!(Timespan::Month.api_value(), "M");
    }

    #[rstest]
    fn test_timespan_bar_specifications() {
        let spec = Timespan::M5.bar_specification();
        assert_eq!(spec.step.get(), 5);
        assert_eq!(spec.aggregation, BarAggregation::Minute);
        assert_eq!(spec.price_type, PriceType::Last);

        let spec = Timespan::M120.bar_specification();
        assert_eq!(spec.step.get(), 2);
        assert_eq!(spec.aggregation, BarAggregation::Hour);

        let spec = Timespan::Month.bar_specification();
        assert_eq!(spec.aggregation, BarAggregation::Month);
    }

    #[rstest]
    fn test_timespan_strum_round_trip() {
        for value in [
            "M1", "M5", "M15", "M30", "M60", "M120", "M240", "D", "W", "M",
        ] {
            let parsed: Timespan = value.parse().expect(value);
            assert_eq!(parsed.to_string(), value);
        }
    }
}
