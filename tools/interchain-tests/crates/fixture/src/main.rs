use std::{env, fmt, process, str::FromStr};

const HELP: &str = "Usage: nomic-harness-fixture --mode <MODE>\n\nModes:\n  healthy\n  delayed-readiness\n  transient-failure\n  permanent-failure\n  crash-after-readiness\n  hang\n";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Healthy,
    DelayedReadiness,
    TransientFailure,
    PermanentFailure,
    CrashAfterReadiness,
    Hang,
}

impl FromStr for Mode {
    type Err = ParseModeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "healthy" => Ok(Self::Healthy),
            "delayed-readiness" => Ok(Self::DelayedReadiness),
            "transient-failure" => Ok(Self::TransientFailure),
            "permanent-failure" => Ok(Self::PermanentFailure),
            "crash-after-readiness" => Ok(Self::CrashAfterReadiness),
            "hang" => Ok(Self::Hang),
            _ => Err(ParseModeError(value.to_owned())),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ParseModeError(String);

impl fmt::Display for ParseModeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown fixture mode: {}", self.0)
    }
}

#[derive(Debug, Eq, PartialEq)]
enum Command {
    Help,
    Run(Mode),
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut args = args.into_iter();
    match (args.next().as_deref(), args.next()) {
        (Some("--help" | "-h"), None) => Ok(Command::Help),
        (Some("--mode"), Some(mode)) if args.next().is_none() => mode
            .parse()
            .map(Command::Run)
            .map_err(|error: ParseModeError| error.to_string()),
        _ => Err("expected `--help` or `--mode <MODE>`".to_owned()),
    }
}

fn main() {
    match parse_args(env::args().skip(1)) {
        Ok(Command::Help) => print!("{HELP}"),
        Ok(Command::Run(_)) => {}
        Err(error) => {
            eprintln!("error: {error}\n\n{HELP}");
            process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_args, Command, Mode};

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn parses_each_typed_mode() {
        let cases = [
            ("healthy", Mode::Healthy),
            ("delayed-readiness", Mode::DelayedReadiness),
            ("transient-failure", Mode::TransientFailure),
            ("permanent-failure", Mode::PermanentFailure),
            ("crash-after-readiness", Mode::CrashAfterReadiness),
            ("hang", Mode::Hang),
        ];

        for (name, expected) in cases {
            assert_eq!(
                parse_args(args(&["--mode", name])),
                Ok(Command::Run(expected))
            );
        }
    }

    #[test]
    fn rejects_unknown_mode() {
        assert_eq!(
            parse_args(args(&["--mode", "unknown"])),
            Err("unknown fixture mode: unknown".to_owned())
        );
    }

    #[test]
    fn parses_help_without_accepting_extra_arguments() {
        assert_eq!(parse_args(args(&["--help"])), Ok(Command::Help));
        assert!(parse_args(args(&["--help", "extra"])).is_err());
    }
}
