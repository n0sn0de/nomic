use nomic_harness_fixture::{
    read_stdin_config, CanaryInputs, Fixture, FixtureConfig, Mode, MAX_CANARY_BYTES,
};
use std::env;
use std::net::SocketAddr;
use std::process;
use std::time::Duration;

const NAMED_CANARY_ENV: &str = "NOMIC_FIXTURE_SYNTHETIC_CANARY";
const HELP: &str = "Usage: nomic-harness-fixture --listen <loopback>:0 --mode <MODE> --argv-canary <VALUE> --log-canary <VALUE>\nConfiguration: one bounded typed JSON object on stdin containing config_canary, component_id, run_id, network_id, and capabilities";

fn main() {
    if let Err(error) = run() {
        eprintln!("fixture error: {error}");
        process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.as_slice() == ["--internal-child", "hang"] {
        loop {
            std::thread::park();
        }
    }
    if args.as_slice() == ["--help"] {
        println!("{HELP}");
        return Ok(());
    }
    if args.len() != 8 || args.chunks_exact(2).any(|pair| !pair[0].starts_with("--")) {
        return Err("invalid command line; use --help".into());
    }
    let value = |flag: &str| {
        let matches = args
            .chunks_exact(2)
            .filter(|pair| pair[0] == flag)
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            Ok(matches[0][1].clone())
        } else {
            Err(format!("expected exactly one {flag}"))
        }
    };
    let listen_addr: SocketAddr = value("--listen")?.parse().map_err(|_| "invalid listener")?;
    let mode = parse_mode(&value("--mode")?)?;
    let argv = value("--argv-canary")?;
    let intentional_log = value("--log-canary")?;
    let named_env =
        env::var(NAMED_CANARY_ENV).map_err(|_| format!("missing {NAMED_CANARY_ENV}"))?;
    let stdin_config = read_stdin_config(std::io::stdin()).map_err(|error| error.to_string())?;
    for (name, canary) in [
        ("argv", &argv),
        ("named env", &named_env),
        ("config", &stdin_config.config_canary),
        ("log", &intentional_log),
    ] {
        if canary.len() > MAX_CANARY_BYTES {
            return Err(format!("{name} canary exceeds bound"));
        }
    }
    let fixture = Fixture::bind_with_event_writer(
        FixtureConfig {
            listen_addr,
            mode,
            canaries: CanaryInputs {
                argv,
                named_env,
                config: stdin_config.config_canary,
                intentional_log,
            },
            read_timeout: Duration::from_millis(500),
            write_timeout: Duration::from_millis(500),
            readiness: nomic_harness_fixture::ReadinessIdentity::ready(
                stdin_config.component_id,
                stdin_config.run_id,
                stdin_config.network_id,
                stdin_config.capabilities,
            )
            .map_err(|error| error.to_string())?,
        },
        std::io::stdout(),
    )
    .map_err(|error| error.to_string())?;
    fixture.serve_process().map_err(|error| error.to_string())
}

fn parse_mode(value: &str) -> Result<Mode, String> {
    match value {
        "healthy" => Ok(Mode::Healthy),
        "permanent-failure" => Ok(Mode::PermanentFailure),
        "panic-after-readiness" => Ok(Mode::PanicAfterReadiness),
        "crash-after-readiness" => Ok(Mode::CrashAfterReadiness),
        "hang" => Ok(Mode::Hang),
        "hang-with-descendant" => Ok(Mode::HangWithDescendant),
        _ if value.starts_with("delayed-readiness:") => value[18..]
            .parse()
            .map(|delay_ms| Mode::DelayedReadiness { delay_ms })
            .map_err(|_| "invalid delayed-readiness mode".into()),
        _ if value.starts_with("transient-failure:") => value[18..]
            .parse()
            .map(|failures| Mode::TransientFailure { failures })
            .map_err(|_| "invalid transient-failure mode".into()),
        _ => Err("unknown mode".into()),
    }
}
