use crate::contract::{
    BindingRegistry, ContractProfile, ScenarioContract, ScenarioOutcome, ScenarioStatus,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::Read;
use std::path::Path;

const MAX_MANIFEST_BYTES: usize = 1_048_576;
const MAX_SCENARIOS: usize = 64;
const MAX_STRING_BYTES: usize = 256;
const MAX_LIST_ITEMS: usize = 32;
const MAX_DEADLINE_MS: u64 = 86_400_000;
const MAX_BUDGET_BYTES: u64 = 1 << 40;

#[derive(Debug)]
pub struct ManifestError(String);

impl Display for ManifestError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ManifestError {}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioManifest {
    pub schema_version: String,
    pub profile: ContractProfile,
    pub scenarios: Vec<ScenarioContract>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct InventoryCounts {
    pub enabled: usize,
    pub blocked: usize,
    pub pass: usize,
    pub fail: usize,
}

impl InventoryCounts {
    pub fn from_manifest(manifest: &ScenarioManifest) -> Self {
        let mut counts = Self::default();
        for scenario in &manifest.scenarios {
            match scenario.status {
                ScenarioStatus::Enabled => counts.enabled += 1,
                ScenarioStatus::Blocked => counts.blocked += 1,
            }
        }
        counts
    }

    pub fn from_outcomes(outcomes: impl IntoIterator<Item = ScenarioOutcome>) -> Self {
        let mut counts = Self::default();
        for outcome in outcomes {
            match outcome {
                ScenarioOutcome::Pass => counts.pass += 1,
                ScenarioOutcome::Fail => counts.fail += 1,
                ScenarioOutcome::Blocked => counts.blocked += 1,
            }
        }
        counts
    }
}

pub fn load_manifest(path: impl AsRef<Path>) -> Result<ScenarioManifest, ManifestError> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|error| {
        ManifestError(format!("cannot read manifest {}: {error}", path.display()))
    })?;
    let mut bytes = Vec::new();
    file.take((MAX_MANIFEST_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            ManifestError(format!("cannot read manifest {}: {error}", path.display()))
        })?;
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(ManifestError(format!(
            "manifest exceeds {MAX_MANIFEST_BYTES} bytes"
        )));
    }
    let input = String::from_utf8(bytes)
        .map_err(|error| ManifestError(format!("invalid manifest UTF-8: {error}")))?;
    parse_manifest(&input)
}

pub fn parse_manifest(input: &str) -> Result<ScenarioManifest, ManifestError> {
    if input.len() > MAX_MANIFEST_BYTES {
        return Err(ManifestError(format!(
            "manifest exceeds {MAX_MANIFEST_BYTES} bytes"
        )));
    }
    let manifest: ScenarioManifest = toml::from_str(input)
        .map_err(|error| ManifestError(format!("invalid manifest: {error}")))?;
    manifest.validate()?;
    Ok(manifest)
}

impl ScenarioManifest {
    pub fn validate(&self) -> Result<(), ManifestError> {
        bounded_string("schema_version", &self.schema_version)?;
        if self.schema_version != "1" {
            return Err(ManifestError(format!(
                "unsupported schema_version {}",
                self.schema_version
            )));
        }
        if self.scenarios.is_empty() || self.scenarios.len() > MAX_SCENARIOS {
            return Err(ManifestError(format!(
                "scenario count must be between 1 and {MAX_SCENARIOS}"
            )));
        }
        let mut ids = BTreeSet::new();
        for scenario in &self.scenarios {
            scenario.validate()?;
            if !ids.insert(&scenario.id) {
                return Err(ManifestError(format!(
                    "duplicate scenario id {}",
                    scenario.id
                )));
            }
        }
        Ok(())
    }

    pub fn validate_h0_inventory(&self) -> Result<(), ManifestError> {
        self.validate()?;
        if self.scenarios.len() != 10 {
            return Err(ManifestError(
                "H0 inventory must contain HAR-001 through HAR-010 exactly once".into(),
            ));
        }
        for number in 1..=10 {
            let id = format!("HAR-{number:03}");
            let scenario = self
                .scenarios
                .iter()
                .find(|scenario| scenario.id == id)
                .ok_or_else(|| ManifestError(format!("H0 inventory missing {id}")))?;
            let required = if matches!(number, 3 | 4) {
                ScenarioStatus::Blocked
            } else {
                ScenarioStatus::Enabled
            };
            if scenario.status != required {
                return Err(ManifestError(format!(
                    "illegal H0 status for {id}: expected {}",
                    match required {
                        ScenarioStatus::Blocked => "blocked",
                        ScenarioStatus::Enabled => "enabled",
                    }
                )));
            }
        }
        Ok(())
    }

    pub fn validate_declared_profile(
        &self,
        registry: &BindingRegistry,
    ) -> Result<(), ManifestError> {
        self.validate()?;
        if &self.profile != registry.profile() {
            return Err(ManifestError(format!(
                "manifest profile {} does not match binding registry profile {}",
                self.profile.as_str(),
                registry.profile().as_str()
            )));
        }
        for scenario in &self.scenarios {
            if scenario.status == ScenarioStatus::Blocked {
                continue;
            }
            let binding = scenario
                .executable
                .as_ref()
                .expect("enabled scenarios have an executable after validation");
            let Some(registered) = registry.binding_for(&scenario.id) else {
                return Err(ManifestError(format!(
                    "{}: no executable binding registered",
                    scenario.id
                )));
            };
            if binding != registered {
                return Err(ManifestError(format!(
                    "{}: executable binding {binding} does not match registered binding {registered}",
                    scenario.id
                )));
            }
        }
        Ok(())
    }
}

impl ScenarioContract {
    fn validate(&self) -> Result<(), ManifestError> {
        let valid_id = self.id.strip_prefix("HAR-").is_some_and(|number| {
            number.len() == 3 && number.bytes().all(|byte| byte.is_ascii_digit()) && number != "000"
        });
        if !valid_id {
            return Err(ManifestError(format!("invalid scenario id {}", self.id)));
        }
        for (name, value) in [
            ("case_id", self.case_id.as_str()),
            ("spec_version", self.spec_version.as_str()),
            ("release_requirement", self.release_requirement.as_str()),
            ("phase", self.phase.as_str()),
            ("topology", self.topology.as_str()),
            ("semantic_seed_domain", self.semantic_seed_domain.as_str()),
            ("ci_lane", self.ci_lane.as_str()),
        ] {
            bounded_string(name, value).map_err(|error| error.with_id(&self.id))?;
        }
        for (name, values) in [
            ("capabilities", &self.capabilities),
            ("fixtures", &self.fixtures),
            ("steps", &self.steps),
            ("barriers", &self.barriers),
            ("terminal_predicates", &self.terminal_predicates),
            ("unchanged_projections", &self.unchanged_projections),
            ("oracles", &self.oracles),
            ("invariants", &self.invariants),
            ("evidence", &self.evidence),
        ] {
            bounded_list(name, values).map_err(|error| error.with_id(&self.id))?;
        }
        for (name, value, maximum) in [
            (
                "operation_deadline_ms",
                self.operation_deadline_ms,
                MAX_DEADLINE_MS,
            ),
            (
                "overall_deadline_ms",
                self.overall_deadline_ms,
                MAX_DEADLINE_MS,
            ),
            (
                "memory_budget_bytes",
                self.memory_budget_bytes,
                MAX_BUDGET_BYTES,
            ),
            (
                "artifact_budget_bytes",
                self.artifact_budget_bytes,
                MAX_BUDGET_BYTES,
            ),
        ] {
            if value == 0 || value > maximum {
                return Err(ManifestError(format!(
                    "{}: {name} must be between 1 and {maximum}",
                    self.id
                )));
            }
        }
        if self.operation_deadline_ms > self.overall_deadline_ms {
            return Err(ManifestError(format!(
                "{}: operation_deadline_ms exceeds overall_deadline_ms",
                self.id
            )));
        }
        if self.status == ScenarioStatus::Enabled {
            let Some(executable) = &self.executable else {
                return Err(ManifestError(format!(
                    "{}: enabled scenario requires executable",
                    self.id
                )));
            };
            bounded_string("executable", executable.as_str())
                .map_err(|error| error.with_id(&self.id))?;
        } else if self.executable.is_some() {
            return Err(ManifestError(format!(
                "{}: blocked scenario must not have executable",
                self.id
            )));
        }
        Ok(())
    }
}

impl ManifestError {
    fn with_id(self, id: &str) -> Self {
        Self(format!("{id}: {}", self.0))
    }
}

fn bounded_string(name: &str, value: &str) -> Result<(), ManifestError> {
    if value.trim().is_empty() {
        return Err(ManifestError(format!("{name} must not be empty")));
    }
    if value.len() > MAX_STRING_BYTES {
        return Err(ManifestError(format!(
            "{name} exceeds {MAX_STRING_BYTES} bytes"
        )));
    }
    Ok(())
}

fn bounded_list(name: &str, values: &[String]) -> Result<(), ManifestError> {
    if values.is_empty() {
        return Err(ManifestError(format!("{name} must not be empty")));
    }
    if values.len() > MAX_LIST_ITEMS {
        return Err(ManifestError(format!(
            "{name} exceeds {MAX_LIST_ITEMS} items"
        )));
    }
    for value in values {
        bounded_string(name, value)?;
    }
    Ok(())
}
