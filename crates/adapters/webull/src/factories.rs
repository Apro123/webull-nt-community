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

//! Factory for creating Webull data clients.

use std::{any::Any, cell::RefCell, rc::Rc};

use nautilus_common::{
    cache::CacheView,
    clients::DataClient,
    clock::Clock,
    factories::{ClientConfig, DataClientFactory},
};
use nautilus_model::identifiers::ClientId;

use crate::{common::consts::WEBULL, config::WebullDataClientConfig, data::WebullDataClient};

impl ClientConfig for WebullDataClientConfig {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Factory for creating Webull data clients.
#[derive(Debug, Clone)]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "nautilus_trader.adapters.webull", from_py_object)
)]
#[cfg_attr(
    feature = "python",
    pyo3_stub_gen::derive::gen_stub_pyclass(module = "nautilus_trader.adapters.webull")
)]
pub struct WebullDataClientFactory;

impl WebullDataClientFactory {
    /// Creates a new [`WebullDataClientFactory`] instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for WebullDataClientFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl DataClientFactory for WebullDataClientFactory {
    fn create(
        &self,
        name: &str,
        config: &dyn ClientConfig,
        cache: CacheView,
        _clock: Rc<RefCell<dyn Clock>>,
    ) -> anyhow::Result<Box<dyn DataClient>> {
        let webull_config = config
            .as_any()
            .downcast_ref::<WebullDataClientConfig>()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Invalid config type for WebullDataClientFactory. \
                     Expected WebullDataClientConfig, was {config:?}",
                )
            })?
            .clone();

        let client_id = ClientId::from(name);
        let client = WebullDataClient::new(client_id, webull_config, cache)?;
        Ok(Box::new(client))
    }

    fn name(&self) -> &'static str {
        WEBULL
    }

    fn config_type(&self) -> &'static str {
        "WebullDataClientConfig"
    }
}

#[cfg(test)]
mod tests {
    use std::{any::Any, cell::RefCell, rc::Rc};

    use nautilus_common::{
        cache::Cache, clock::VirtualClock, factories::ClientConfig,
        live::runner::set_data_event_sender, messages::DataEvent,
    };
    use nautilus_core::string::secret::SecretString;
    use rstest::rstest;

    use super::*;

    #[derive(Debug)]
    struct UnrelatedConfig;

    impl ClientConfig for UnrelatedConfig {
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    fn test_context() -> (CacheView, Rc<RefCell<VirtualClock>>) {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel::<DataEvent>();
        set_data_event_sender(sender);

        (
            CacheView::new(Rc::new(RefCell::new(Cache::default()))),
            Rc::new(RefCell::new(VirtualClock::new())),
        )
    }

    #[rstest]
    fn test_factory_metadata() {
        let factory = WebullDataClientFactory::new();

        assert_eq!(factory.name(), WEBULL);
        assert_eq!(factory.config_type(), "WebullDataClientConfig");
    }

    #[rstest]
    fn test_create_with_wrong_config_type_is_an_error() {
        let factory = WebullDataClientFactory::new();
        let (cache, clock) = test_context();

        let result = factory.create("webull", &UnrelatedConfig, cache, clock);

        assert!(result.is_err(), "wrong config type must be rejected");
    }

    #[rstest]
    fn test_create_with_config_supplied_credentials() {
        let factory = WebullDataClientFactory::new();
        let (cache, clock) = test_context();
        let config = WebullDataClientConfig {
            api_key: Some(SecretString::from("test-key")),
            api_secret: Some(SecretString::from("test-secret")),
            ..Default::default()
        };

        let client = factory
            .create("test-webull", &config, cache, clock)
            .expect("creates client");

        assert_eq!(client.client_id(), ClientId::from("test-webull"));
        assert!(
            client.venue().is_none(),
            "data providers have no single venue"
        );
        assert!(client.is_disconnected());
    }
}
