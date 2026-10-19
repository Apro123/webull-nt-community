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

//! Python bindings from [PyO3](https://pyo3.rs).
//!
//! The module is exposed as `nautilus_trader._libnautilus.webull`.

pub mod config;
pub mod factories;
pub mod historical;

use nautilus_common::factories::{ClientConfig, DataClientFactory};
use nautilus_core::python::{to_pyruntime_err, to_pyvalue_err};
use nautilus_system::get_global_pyo3_registry;
use pyo3::prelude::*;

use crate::{
    common::{
        consts::{WEBULL, WEBULL_CLIENT_ID},
        enums::Timespan,
    },
    config::WebullDataClientConfig,
    factories::WebullDataClientFactory,
};

#[expect(clippy::needless_pass_by_value)]
fn extract_webull_data_factory(
    py: Python<'_>,
    factory: Py<PyAny>,
) -> PyResult<Box<dyn DataClientFactory>> {
    match factory.extract::<WebullDataClientFactory>(py) {
        Ok(f) => Ok(Box::new(f)),
        Err(e) => Err(to_pyvalue_err(format!(
            "Failed to extract WebullDataClientFactory: {e}"
        ))),
    }
}

#[expect(clippy::needless_pass_by_value)]
fn extract_webull_data_config(
    py: Python<'_>,
    config: Py<PyAny>,
) -> PyResult<Box<dyn ClientConfig>> {
    match config.extract::<WebullDataClientConfig>(py) {
        Ok(c) => Ok(Box::new(c)),
        Err(e) => Err(to_pyvalue_err(format!(
            "Failed to extract WebullDataClientConfig: {e}"
        ))),
    }
}

/// Webull Python module.
///
/// The module is exposed as `nautilus_trader._libnautilus.webull`.
///
/// # Errors
///
/// Returns an error if registering any module components fails.
#[pymodule]
pub fn webull(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    use self::historical::WebullHistoricalClient;

    m.add("WEBULL", WEBULL)?;
    m.add("WEBULL_CLIENT_ID", *WEBULL_CLIENT_ID)?;
    m.add_class::<Timespan>()?;
    m.add_class::<WebullDataClientConfig>()?;
    m.add_class::<WebullDataClientFactory>()?;
    m.add_class::<WebullHistoricalClient>()?;

    let registry = get_global_pyo3_registry();

    if let Err(e) =
        registry.register_factory_extractor(WEBULL.to_string(), extract_webull_data_factory)
    {
        return Err(to_pyruntime_err(format!(
            "Failed to register Webull data factory extractor: {e}"
        )));
    }

    if let Err(e) = registry.register_config_extractor(
        "WebullDataClientConfig".to_string(),
        extract_webull_data_config,
    ) {
        return Err(to_pyruntime_err(format!(
            "Failed to register Webull data config extractor: {e}"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use nautilus_model::identifiers::ClientId;
    use pyo3::prelude::*;
    use rstest::rstest;

    #[rstest]
    fn test_module_registration() {
        Python::initialize();
        Python::attach(|py| {
            let module = pyo3::wrap_pymodule!(super::webull)(py);
            let module = module.bind(py);

            assert_eq!(module.name().unwrap(), "webull");
            assert_eq!(
                module
                    .getattr("WEBULL")
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "WEBULL",
            );
            assert_eq!(
                module
                    .getattr("WEBULL_CLIENT_ID")
                    .unwrap()
                    .extract::<ClientId>()
                    .unwrap(),
                ClientId::from("WEBULL"),
            );
            assert!(module.getattr("WebullDataClientConfig").is_ok());
            assert!(module.getattr("WebullDataClientFactory").is_ok());
            assert!(module.getattr("WebullHistoricalClient").is_ok());
            assert!(module.getattr("Timespan").is_ok());
        });
    }
}
