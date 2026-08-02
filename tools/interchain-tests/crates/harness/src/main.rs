use nomic_bridge_harness::contract::h0_binding_registry;
use nomic_bridge_harness::manifest::{load_manifest, InventoryCounts};
use serde::Serialize;
use std::env;
use std::process::ExitCode;

#[derive(Serialize)]
struct Listing<'a> {
    schema_version: &'a str,
    enabled: usize,
    blocked: usize,
    pass: usize,
    fail: usize,
    scenarios: Vec<ListingScenario<'a>>,
}

#[derive(Serialize)]
struct ListingScenario<'a> {
    id: &'a str,
    status: nomic_bridge_harness::contract::ScenarioStatus,
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<String, String> {
    let (command, manifest_path, json) = parse_args(&args)?;
    let manifest = load_manifest(manifest_path).map_err(|error| error.to_string())?;
    manifest
        .validate_declared_profile(&h0_binding_registry())
        .map_err(|error| error.to_string())?;
    manifest
        .validate_h0_inventory()
        .map_err(|error| error.to_string())?;
    match command {
        Command::Validate if !json => Ok(format!(
            "valid scenario manifest: schema_version={} scenarios={}",
            manifest.schema_version,
            manifest.scenarios.len()
        )),
        Command::List if json => {
            let counts = InventoryCounts::from_manifest(&manifest);
            let listing = Listing {
                schema_version: &manifest.schema_version,
                enabled: counts.enabled,
                blocked: counts.blocked,
                pass: 0,
                fail: 0,
                scenarios: manifest
                    .scenarios
                    .iter()
                    .map(|scenario| ListingScenario {
                        id: &scenario.id,
                        status: scenario.status,
                    })
                    .collect(),
            };
            serde_json::to_string_pretty(&listing).map_err(|error| error.to_string())
        }
        _ => Err(usage()),
    }
}

#[derive(Clone, Copy)]
enum Command {
    Validate,
    List,
}

fn parse_args(args: &[String]) -> Result<(Command, &str, bool), String> {
    let command = match args.first().map(String::as_str) {
        Some("contract") if args.get(1).map(String::as_str) == Some("validate") => {
            Command::Validate
        }
        Some("scenario") if args.get(1).map(String::as_str) == Some("list") => Command::List,
        _ => return Err(usage()),
    };
    let manifest_index = args
        .iter()
        .position(|argument| argument == "--manifest")
        .ok_or_else(usage)?;
    let manifest = args.get(manifest_index + 1).ok_or_else(usage)?;
    let json = args.iter().any(|argument| argument == "--json");
    let expected_len = if json { 5 } else { 4 };
    if args.len() != expected_len || (matches!(command, Command::Validate) && json) {
        return Err(usage());
    }
    Ok((command, manifest, json))
}

fn usage() -> String {
    "usage: nomic-bridge-harness contract validate --manifest <path>\n       nomic-bridge-harness scenario list --manifest <path> --json".into()
}
