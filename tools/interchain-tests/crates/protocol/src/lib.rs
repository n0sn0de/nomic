//! Shared, bounded wire types for harness lifecycle probes.

use serde::{de, Deserialize, Deserializer, Serialize};
use std::fmt;

pub const READINESS_SCHEMA: &str = "readiness-v1";
pub const STARTUP_SCHEMA: &str = "startup-v1";
pub const MAX_ID_BYTES: usize = 64;
pub const MAX_CAPABILITIES: usize = 16;
pub const MAX_ENDPOINT_BYTES: usize = 64;
pub const MAX_STARTUP_EVENT_BYTES: usize = 512;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct CanonicalId(String);

impl CanonicalId {
    pub fn new(value: impl Into<String>) -> Result<Self, ProtocolError> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_ID_BYTES || !is_canonical(&value) {
            return Err(ProtocolError::InvalidIdentifier);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for CanonicalId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

fn is_canonical(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.first().is_some_and(u8::is_ascii_lowercase)
        && bytes
            .last()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && bytes.iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'.' | b'_')
        })
        && !bytes.windows(2).any(|pair| {
            matches!(pair[0], b'-' | b'.' | b'_') && matches!(pair[1], b'-' | b'.' | b'_')
        })
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessState {
    Ready,
    NotReady,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReadinessIdentity {
    pub schema: String,
    pub component_id: CanonicalId,
    pub run_id: CanonicalId,
    pub network_id: CanonicalId,
    pub state: ReadinessState,
    pub capabilities: Vec<CanonicalId>,
}

impl ReadinessIdentity {
    pub fn ready(
        component_id: CanonicalId,
        run_id: CanonicalId,
        network_id: CanonicalId,
        capabilities: impl IntoIterator<Item = CanonicalId>,
    ) -> Result<Self, ProtocolError> {
        Self::new(
            READINESS_SCHEMA.into(),
            component_id,
            run_id,
            network_id,
            ReadinessState::Ready,
            capabilities,
        )
    }

    pub fn not_ready(
        component_id: CanonicalId,
        run_id: CanonicalId,
        network_id: CanonicalId,
        capabilities: impl IntoIterator<Item = CanonicalId>,
    ) -> Result<Self, ProtocolError> {
        Self::new(
            READINESS_SCHEMA.into(),
            component_id,
            run_id,
            network_id,
            ReadinessState::NotReady,
            capabilities,
        )
    }

    pub fn new(
        schema: String,
        component_id: CanonicalId,
        run_id: CanonicalId,
        network_id: CanonicalId,
        state: ReadinessState,
        capabilities: impl IntoIterator<Item = CanonicalId>,
    ) -> Result<Self, ProtocolError> {
        let mut capabilities = capabilities.into_iter().collect::<Vec<_>>();
        capabilities.sort();
        if capabilities.len() > MAX_CAPABILITIES
            || capabilities.windows(2).any(|pair| pair[0] == pair[1])
        {
            return Err(ProtocolError::InvalidCapabilities);
        }
        Ok(Self {
            schema,
            component_id,
            run_id,
            network_id,
            state,
            capabilities,
        })
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema.len() > MAX_ID_BYTES {
            return Err(ProtocolError::InvalidSchema);
        }
        if self.capabilities.len() > MAX_CAPABILITIES
            || self.capabilities.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(ProtocolError::InvalidCapabilities);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum StartupEvent {
    Listening {
        schema: String,
        endpoint: String,
        component_id: CanonicalId,
        run_id: CanonicalId,
        network_id: CanonicalId,
    },
}

impl StartupEvent {
    pub fn listening(
        endpoint: impl Into<String>,
        component_id: CanonicalId,
        run_id: CanonicalId,
        network_id: CanonicalId,
    ) -> Result<Self, ProtocolError> {
        let event = Self::Listening {
            schema: STARTUP_SCHEMA.into(),
            endpoint: endpoint.into(),
            component_id,
            run_id,
            network_id,
        };
        event.validate()?;
        Ok(event)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        let Self::Listening {
            schema, endpoint, ..
        } = self;
        if schema != STARTUP_SCHEMA {
            return Err(ProtocolError::InvalidSchema);
        }
        if endpoint.is_empty() || endpoint.len() > MAX_ENDPOINT_BYTES {
            return Err(ProtocolError::InvalidEndpoint);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    Transient,
    Permanent,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Readiness,
    Echo { payload: String },
    Stop,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Response {
    Readiness { report: ReadinessIdentity },
    Echo { payload: String },
    Failure { class: FailureClass, remaining: u16 },
    Stopped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    InvalidIdentifier,
    InvalidCapabilities,
    InvalidSchema,
    InvalidEndpoint,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidIdentifier => "invalid canonical identifier",
            Self::InvalidCapabilities => "invalid capability list",
            Self::InvalidSchema => "invalid schema",
            Self::InvalidEndpoint => "invalid endpoint",
        })
    }
}
impl std::error::Error for ProtocolError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: &str) -> CanonicalId {
        CanonicalId::new(value).unwrap()
    }

    #[test]
    fn startup_event_is_versioned_bounded_and_closed() {
        let event =
            StartupEvent::listening("127.0.0.1:0", id("fixture"), id("run-a"), id("testnet"))
                .unwrap();
        let encoded = serde_json::to_string(&event).unwrap();
        assert!(encoded.len() <= MAX_STARTUP_EVENT_BYTES);
        assert!(serde_json::from_str::<StartupEvent>(&encoded)
            .unwrap()
            .validate()
            .is_ok());
        assert!(StartupEvent::listening(
            "x".repeat(MAX_ENDPOINT_BYTES + 1),
            id("fixture"),
            id("run-a"),
            id("testnet")
        )
        .is_err());
        let unknown = encoded.replacen("}", ",\"unknown\":true}", 1);
        assert!(serde_json::from_str::<StartupEvent>(&unknown).is_err());
    }
}
