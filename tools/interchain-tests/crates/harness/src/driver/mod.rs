//! Supervisor-owned process drivers and validated runtime endpoints.

pub mod process;

use nomic_harness_protocol::{CanonicalId, StartupEvent};
use std::fmt;
use std::net::{SocketAddr, TcpListener};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DynamicEndpoint(SocketAddr);

impl DynamicEndpoint {
    pub fn from_startup_event(
        event: &StartupEvent,
        component_id: &CanonicalId,
        run_id: &CanonicalId,
        network_id: &CanonicalId,
    ) -> Result<Self, EndpointError> {
        event.validate().map_err(|_| EndpointError::InvalidEvent)?;
        let StartupEvent::Listening {
            endpoint,
            component_id: actual_component,
            run_id: actual_run,
            network_id: actual_network,
            ..
        } = event;
        if actual_component != component_id || actual_run != run_id || actual_network != network_id
        {
            return Err(EndpointError::IdentityMismatch);
        }
        let address = endpoint.parse().map_err(|_| EndpointError::Malformed)?;
        Self::validate(address)
    }

    pub fn from_reserved_listener(listener: &TcpListener) -> Result<Self, EndpointError> {
        Self::validate(
            listener
                .local_addr()
                .map_err(|_| EndpointError::Malformed)?,
        )
    }

    fn validate(address: SocketAddr) -> Result<Self, EndpointError> {
        if !address.ip().is_loopback() {
            return Err(EndpointError::NonLoopback);
        }
        if address.port() == 0 {
            return Err(EndpointError::UnassignedPort);
        }
        Ok(Self(address))
    }

    pub fn socket_addr(self) -> SocketAddr {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndpointError {
    InvalidEvent,
    IdentityMismatch,
    Malformed,
    NonLoopback,
    UnassignedPort,
}
impl fmt::Display for EndpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid dynamic endpoint: {self:?}")
    }
}
impl std::error::Error for EndpointError {}
