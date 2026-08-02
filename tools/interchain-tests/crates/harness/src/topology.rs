//! Bounded declarative topology without runtime networking details.

use nomic_harness_protocol::CanonicalId;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fmt;

const MAX_TOPOLOGY_BYTES: usize = 8 * 1024;
const MAX_NODES: usize = 16;
const MAX_LINKS: usize = 32;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Topology {
    schema_version: String,
    id: CanonicalId,
    nodes: Vec<CanonicalId>,
    links: Vec<Link>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Link {
    from: CanonicalId,
    to: CanonicalId,
}

impl Topology {
    pub fn parse(source: &str) -> Result<Self, TopologyError> {
        if source.len() > MAX_TOPOLOGY_BYTES {
            return Err(TopologyError::Oversized);
        }
        let topology: Self = toml::from_str(source).map_err(TopologyError::Parse)?;
        if topology.schema_version != "1" {
            return Err(TopologyError::Schema);
        }
        if topology.nodes.is_empty()
            || topology.nodes.len() > MAX_NODES
            || topology.links.len() > MAX_LINKS
        {
            return Err(TopologyError::Bounds);
        }
        let nodes = topology.nodes.iter().collect::<BTreeSet<_>>();
        if nodes.len() != topology.nodes.len() {
            return Err(TopologyError::DuplicateNode);
        }
        let mut links = BTreeSet::new();
        for link in &topology.links {
            if !nodes.contains(&link.from)
                || !nodes.contains(&link.to)
                || !links.insert((&link.from, &link.to))
            {
                return Err(TopologyError::InvalidLink);
            }
        }
        Ok(topology)
    }
    pub fn id(&self) -> &CanonicalId {
        &self.id
    }
    pub fn nodes(&self) -> &[CanonicalId] {
        &self.nodes
    }
}

#[derive(Debug)]
pub enum TopologyError {
    Oversized,
    Parse(toml::de::Error),
    Schema,
    Bounds,
    DuplicateNode,
    InvalidLink,
}
impl fmt::Display for TopologyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "topology parse failed: {e}"),
            other => write!(f, "invalid topology: {other:?}"),
        }
    }
}
impl std::error::Error for TopologyError {}
