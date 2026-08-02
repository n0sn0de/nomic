//! Bounded declarative topology without runtime networking details.

use nomic_harness_protocol::CanonicalId;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
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
        topology.startup_order()?;
        Ok(topology)
    }
    pub fn id(&self) -> &CanonicalId {
        &self.id
    }
    pub fn nodes(&self) -> &[CanonicalId] {
        &self.nodes
    }

    /// Returns a deterministic order produced by a lexical Kahn walk.
    pub fn startup_order(&self) -> Result<Vec<&CanonicalId>, TopologyError> {
        let mut incoming = self
            .nodes
            .iter()
            .map(|node| (node, 0_usize))
            .collect::<BTreeMap<_, _>>();
        let mut outgoing = self
            .nodes
            .iter()
            .map(|node| (node, Vec::new()))
            .collect::<BTreeMap<_, _>>();
        for link in &self.links {
            *incoming
                .get_mut(&link.to)
                .ok_or(TopologyError::InvalidLink)? += 1;
            outgoing
                .get_mut(&link.from)
                .ok_or(TopologyError::InvalidLink)?
                .push(&link.to);
        }
        let mut ready = incoming
            .iter()
            .filter_map(|(node, count)| (*count == 0).then_some(*node))
            .collect::<BTreeSet<_>>();
        let mut order = Vec::with_capacity(self.nodes.len());
        while let Some(node) = ready.pop_first() {
            order.push(node);
            for successor in outgoing.get(node).ok_or(TopologyError::InvalidLink)? {
                let count = incoming
                    .get_mut(successor)
                    .ok_or(TopologyError::InvalidLink)?;
                *count -= 1;
                if *count == 0 {
                    ready.insert(successor);
                }
            }
        }
        if order.len() != self.nodes.len() {
            return Err(TopologyError::Cycle);
        }
        Ok(order)
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
    Cycle,
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
