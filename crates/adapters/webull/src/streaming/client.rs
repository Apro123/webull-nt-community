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

//! MQTT streaming client for the Webull Data Streaming API.

use std::{fmt::Debug, time::Duration};

use nautilus_core::UUID4;
use rumqttc::{AsyncClient, Event, EventLoop, Incoming, MqttOptions, Transport};

use crate::{
    common::{
        credential::Credential,
        urls::{
            STREAMING_SUBSCRIBE_PATH, STREAMING_UNSUBSCRIBE_PATH, WEBULL_MQTT_HOST,
            WEBULL_MQTT_PORT,
        },
    },
    http::{
        client::{WebullHttpClient, WebullHttpConfig},
        error::Error as HttpError,
        models::{WebullDepthBook, WebullStreamSubscribeRequest, WebullStreamUnsubscribeRequest},
    },
    streaming::parse::{WebullStreamSnapshot, WebullStreamTick, parse_stream_topic_payload},
};

/// Default MQTT password; the venue accepts any non-empty value.
const DEFAULT_MQTT_PASSWORD: &str = "nautilus";

/// Errors returned by the Webull streaming client.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Underlying signed HTTP control-plane failure.
    #[error(transparent)]
    Http(#[from] HttpError),

    /// MQTT client or event-loop failure.
    #[error("MQTT error: {0}")]
    Mqtt(#[from] rumqttc::ClientError),

    /// MQTT connection / protocol failure from the event loop.
    #[error("MQTT connection error: {0}")]
    MqttConnection(#[source] Box<rumqttc::ConnectionError>),

    /// Failed to encode a subscribe/unsubscribe request body.
    #[error("Failed to serialize streaming request: {0}")]
    Serialize(#[from] serde_json::Error),

    /// Failed to decode a streaming payload.
    #[error(transparent)]
    Parse(#[from] anyhow::Error),
}

/// Result alias for the streaming client.
pub type Result<T> = std::result::Result<T, Error>;

/// A decoded Webull streaming payload.
#[derive(Debug, Clone, PartialEq)]
pub enum WebullStreamMessage {
    /// Full depth quote (`quote` topic).
    Quote(WebullDepthBook),
    /// Session snapshot (`snapshot` topic).
    Snapshot(WebullStreamSnapshot),
    /// Trade tick (`tick` topic).
    Tick(WebullStreamTick),
    /// Server notice (`notice` topic, JSON).
    Notice(serde_json::Value),
    /// Keepalive / echo (`echo` topic, empty payload).
    Echo,
}

/// MQTT market data streaming client for Webull.
///
/// Owns both the rumqttc [`AsyncClient`] and its [`EventLoop`] so callers can
/// poll for connection progress and publishes without spawning a background
/// task.
pub struct WebullStreamingClient {
    session_id: String,
    http: WebullHttpClient,
    mqtt: AsyncClient,
    eventloop: EventLoop,
}

impl Debug for WebullStreamingClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(stringify!(WebullStreamingClient))
            .field("session_id", &self.session_id)
            .field("http", &self.http)
            .finish_non_exhaustive()
    }
}

impl WebullStreamingClient {
    /// Creates a streaming client against the production MQTT broker.
    ///
    /// Generates a UUID session id used as the MQTT `ClientId`, and builds a
    /// signed [`WebullHttpClient`] for subscribe/unsubscribe control requests.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn new(credential: Credential) -> Result<Self> {
        Self::new_with_endpoints(credential, None, None, None)
    }

    /// Creates a streaming client with optional MQTT and HTTP overrides.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn new_with_endpoints(
        credential: Credential,
        mqtt_host: Option<&str>,
        mqtt_port: Option<u16>,
        http_config: Option<WebullHttpConfig>,
    ) -> Result<Self> {
        let http = match http_config {
            Some(config) => WebullHttpClient::new_with_config(credential, config)?,
            None => WebullHttpClient::new(credential)?,
        };
        Self::from_http_client(http, mqtt_host, mqtt_port)
    }

    /// Creates a streaming client that reuses an existing HTTP client.
    ///
    /// # Errors
    ///
    /// This constructor currently cannot fail; it returns [`Result`] for API
    /// symmetry with the other constructors.
    pub fn from_http_client(
        http: WebullHttpClient,
        mqtt_host: Option<&str>,
        mqtt_port: Option<u16>,
    ) -> Result<Self> {
        let credential = http.credential().clone();
        let session_id = UUID4::new().as_str().replace('-', "");
        let host = mqtt_host.unwrap_or(WEBULL_MQTT_HOST);
        let port = mqtt_port.unwrap_or(WEBULL_MQTT_PORT);

        let mut options = MqttOptions::new(session_id.clone(), host, port);
        options.set_keep_alive(Duration::from_secs(30));
        options.set_credentials(credential.api_key(), DEFAULT_MQTT_PASSWORD);
        options.set_transport(Transport::tls_with_default_config());

        let (mqtt, eventloop) = AsyncClient::new(options, 64);

        Ok(Self {
            session_id,
            http,
            mqtt,
            eventloop,
        })
    }

    /// Returns the MQTT session id / ClientId.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Returns a reference to the underlying HTTP client.
    #[must_use]
    pub const fn http(&self) -> &WebullHttpClient {
        &self.http
    }

    /// Returns a reference to the rumqttc async client.
    #[must_use]
    pub const fn mqtt(&self) -> &AsyncClient {
        &self.mqtt
    }

    /// Returns a mutable reference to the owned MQTT event loop.
    pub fn eventloop_mut(&mut self) -> &mut EventLoop {
        &mut self.eventloop
    }

    /// Subscribes to streaming data via the signed HTTP control plane.
    ///
    /// Call after the MQTT connection is established (typically after the first
    /// successful [`Self::poll`] ConnAck). `grab` defaults to `"true"` so the
    /// venue pushes an initial snapshot when available.
    ///
    /// # Errors
    ///
    /// Returns an error if the request cannot be serialized or the HTTP call fails.
    pub async fn subscribe(
        &self,
        symbols: Vec<String>,
        category: impl Into<String>,
        sub_types: Vec<String>,
        depth: Option<u32>,
        overnight_required: Option<bool>,
    ) -> Result<()> {
        let request = WebullStreamSubscribeRequest {
            session_id: self.session_id.clone(),
            symbols,
            category: category.into(),
            sub_types,
            grab: "true".to_string(),
            depth,
            overnight_required,
        };
        let body = serde_json::to_string(&request)?;
        self.http
            .signed_post(STREAMING_SUBSCRIBE_PATH, &body)
            .await?;
        Ok(())
    }

    /// Unsubscribes from streaming data via the signed HTTP control plane.
    ///
    /// # Errors
    ///
    /// Returns an error if the request cannot be serialized or the HTTP call fails.
    pub async fn unsubscribe(
        &self,
        symbols: Vec<String>,
        category: impl Into<String>,
        sub_types: Vec<String>,
        unsubscribe_all: Option<bool>,
    ) -> Result<()> {
        let request = WebullStreamUnsubscribeRequest {
            session_id: self.session_id.clone(),
            symbols,
            category: category.into(),
            sub_types,
            unsubscribe_all,
        };
        let body = serde_json::to_string(&request)?;
        self.http
            .signed_post(STREAMING_UNSUBSCRIBE_PATH, &body)
            .await?;
        Ok(())
    }

    /// Polls the MQTT event loop once.
    ///
    /// Returns `Ok(Some(message))` when an inbound publish is decoded, `Ok(None)`
    /// for other MQTT events (ConnAck, PingResp, outbound acks, etc.).
    ///
    /// # Errors
    ///
    /// Returns an error if the event loop fails or a publish payload cannot be parsed.
    pub async fn poll(&mut self) -> Result<Option<WebullStreamMessage>> {
        let event = self
            .eventloop
            .poll()
            .await
            .map_err(|e| Error::MqttConnection(Box::new(e)))?;

        match event {
            Event::Incoming(Incoming::Publish(publish)) => {
                let message = parse_stream_topic_payload(&publish.topic, &publish.payload)?;
                Ok(Some(message))
            }
            _ => Ok(None),
        }
    }
}

impl From<rumqttc::ConnectionError> for Error {
    fn from(value: rumqttc::ConnectionError) -> Self {
        Self::MqttConnection(Box::new(value))
    }
}
