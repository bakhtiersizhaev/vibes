use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser, PartialEq)]
#[command(name = "vibes", about = "Rust daemon/runtime entrypoint for vibes")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand, PartialEq)]
pub(crate) enum Command {
    Init(InitArgs),
    Start(StartArgs),
    Status(StatusArgs),
    Stop(StopArgs),
    Setup(SetupArgs),
    Logs(LogsArgs),
}

#[derive(Debug, Args, PartialEq)]
pub(crate) struct InitArgs {
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub env: Option<String>,
}

#[derive(Debug, Args, PartialEq)]
pub(crate) struct StartArgs {
    #[arg(long)]
    pub token: Option<String>,
    #[arg(long)]
    pub admin: Option<i64>,
    #[arg(long)]
    pub env: Option<String>,
    #[arg(long)]
    pub restart: bool,
}

#[derive(Debug, Args, PartialEq)]
pub(crate) struct StatusArgs {}

#[derive(Debug, Args, PartialEq)]
pub(crate) struct StopArgs {
    #[arg(long)]
    pub force: bool,
    #[arg(long, default_value_t = 10.0)]
    pub timeout: f64,
}

#[derive(Debug, Args, PartialEq)]
pub(crate) struct SetupArgs {
    #[arg(long)]
    pub start: bool,
    #[arg(long)]
    pub restart: bool,
    #[arg(long)]
    pub env: Option<String>,
}

#[derive(Debug, Args, PartialEq)]
pub(crate) struct LogsArgs {
    #[arg(long)]
    pub follow: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_start_flags() {
        let cli = Cli::parse_from([
            "vibes",
            "start",
            "--token",
            "abc",
            "--admin",
            "123",
            "--restart",
            "--env",
            ".env.custom",
        ]);
        assert_eq!(
            cli.command,
            Some(Command::Start(StartArgs {
                token: Some("abc".to_owned()),
                admin: Some(123),
                env: Some(".env.custom".to_owned()),
                restart: true,
            }))
        );
    }

    #[test]
    fn parses_stop_defaults() {
        let cli = Cli::parse_from(["vibes", "stop"]);
        assert_eq!(
            cli.command,
            Some(Command::Stop(StopArgs {
                force: false,
                timeout: 10.0,
            }))
        );
    }

    #[test]
    fn parses_logs_follow() {
        let cli = Cli::parse_from(["vibes", "logs", "--follow"]);
        assert_eq!(cli.command, Some(Command::Logs(LogsArgs { follow: true })));
    }
}
