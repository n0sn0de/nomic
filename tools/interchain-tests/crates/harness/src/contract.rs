use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScenarioStatus {
    Blocked,
    Enabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionResult {
    Pass,
    Fail,
    MissingCapability,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ScenarioOutcome {
    #[serde(rename = "pass")]
    Pass,
    #[serde(rename = "fail")]
    Fail,
    #[serde(rename = "blocked")]
    Blocked,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ContractProfile(String);

impl ContractProfile {
    pub fn parse(value: &str) -> Result<Self, BindingError> {
        if !is_canonical_kebab(value) {
            return Err(BindingError(format!(
                "profile must be a canonical lowercase kebab-case identifier: {value:?}"
            )));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ContractProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ExecutableBinding(String);

impl ExecutableBinding {
    pub fn parse(value: &str) -> Result<Self, BindingError> {
        let Some((profile_and_version, name)) = value.split_once("::") else {
            return Err(BindingError::invalid_binding(value));
        };
        if value.matches("::").count() != 1 {
            return Err(BindingError::invalid_binding(value));
        }
        let Some((profile, version)) = profile_and_version.split_once("/v") else {
            return Err(BindingError::invalid_binding(value));
        };
        let canonical_version = !version.is_empty()
            && version.bytes().all(|byte| byte.is_ascii_digit())
            && version.as_bytes()[0] != b'0';
        if ContractProfile::parse(profile).is_err()
            || !canonical_version
            || !is_canonical_snake(name)
        {
            return Err(BindingError::invalid_binding(value));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ExecutableBinding {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ExecutableBinding {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug)]
pub struct BindingRegistry {
    profile: ContractProfile,
    bindings: BTreeMap<String, ExecutableBinding>,
}

impl BindingRegistry {
    pub fn new(
        profile: ContractProfile,
        bindings: impl IntoIterator<Item = (String, ExecutableBinding)>,
    ) -> Result<Self, BindingError> {
        let mut registrations = BTreeMap::new();
        let mut executable_bindings = BTreeSet::new();
        for (scenario_id, binding) in bindings {
            if scenario_id.trim().is_empty() {
                return Err(BindingError(
                    "registry scenario ID must not be blank".into(),
                ));
            }
            let binding_profile = binding
                .as_str()
                .split_once("/v")
                .expect("validated executable binding")
                .0;
            if binding_profile != profile.as_str() {
                return Err(BindingError(format!(
                    "binding {binding} does not belong to profile {}",
                    profile.as_str()
                )));
            }
            if registrations.contains_key(&scenario_id) {
                return Err(BindingError(format!(
                    "duplicate registry scenario ID {scenario_id}"
                )));
            }
            if !executable_bindings.insert(binding.clone()) {
                return Err(BindingError(format!(
                    "duplicate registry executable binding {binding}"
                )));
            }
            registrations.insert(scenario_id, binding);
        }
        Ok(Self {
            profile,
            bindings: registrations,
        })
    }

    pub fn profile(&self) -> &ContractProfile {
        &self.profile
    }

    pub fn binding_for(&self, scenario_id: &str) -> Option<&ExecutableBinding> {
        self.bindings.get(scenario_id)
    }

    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    pub fn bindings(&self) -> impl Iterator<Item = (&str, &ExecutableBinding)> {
        self.bindings
            .iter()
            .map(|(scenario_id, binding)| (scenario_id.as_str(), binding))
    }
}

pub fn h0_binding_registry() -> BindingRegistry {
    BindingRegistry::new(
        ContractProfile::parse("h0").expect("static H0 profile is valid"),
        [1, 2, 5, 6, 7, 8, 9, 10].map(|number| {
            (
                format!("HAR-{number:03}"),
                ExecutableBinding::parse(&format!("h0/v1::har_{number:03}"))
                    .expect("static H0 binding is valid"),
            )
        }),
    )
    .expect("static H0 registry is valid")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingError(String);

impl BindingError {
    fn invalid_binding(value: &str) -> Self {
        Self(format!(
            "executable binding must use canonical profile/vN::lower_snake_case syntax: {value:?}"
        ))
    }
}

impl Display for BindingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for BindingError {}

fn is_canonical_kebab(value: &str) -> bool {
    is_canonical_delimited(value, b'-')
}

fn is_canonical_snake(value: &str) -> bool {
    is_canonical_delimited(value, b'_')
}

fn is_canonical_delimited(value: &str, delimiter: u8) -> bool {
    !value.is_empty()
        && value.is_ascii()
        && value.as_bytes()[0].is_ascii_lowercase()
        && value.as_bytes()[value.len() - 1] != delimiter
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == delimiter)
        && !value
            .as_bytes()
            .windows(2)
            .any(|window| window == [delimiter, delimiter])
}

impl ScenarioOutcome {
    #[must_use]
    pub const fn from_result(status: ScenarioStatus, result: ExecutionResult) -> Self {
        match status {
            ScenarioStatus::Blocked => Self::Blocked,
            ScenarioStatus::Enabled => match result {
                ExecutionResult::Pass => Self::Pass,
                ExecutionResult::Fail | ExecutionResult::MissingCapability => Self::Fail,
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioContract {
    pub id: String,
    pub case_id: String,
    pub spec_version: String,
    pub status: ScenarioStatus,
    pub release_requirement: String,
    pub phase: String,
    pub topology: String,
    pub backend: Backend,
    pub fidelity: Fidelity,
    pub semantic_seed_domain: String,
    pub capabilities: Vec<String>,
    pub fixtures: Vec<String>,
    pub steps: Vec<String>,
    pub barriers: Vec<String>,
    pub terminal_predicates: Vec<String>,
    pub unchanged_projections: Vec<String>,
    pub oracles: Vec<String>,
    pub invariants: Vec<String>,
    pub evidence: Vec<String>,
    pub operation_deadline_ms: u64,
    pub overall_deadline_ms: u64,
    pub memory_budget_bytes: u64,
    pub artifact_budget_bytes: u64,
    pub ci_lane: String,
    pub executable: Option<ExecutableBinding>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Backend {
    Process,
    Container,
    ProcessAndContainer,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Fidelity {
    Synthetic,
    Integrated,
}
