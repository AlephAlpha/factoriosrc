use clap::{error::ErrorKind, CommandFactory, Parser};
use factoriosrc_lib::Config;

/// A simple tool to search for patterns in Factorio cellular automata.
#[derive(Debug, Parser)]
pub struct Args {
    #[command(flatten)]
    pub config: Config,

    /// Number of steps between each display of the current partial result.
    #[arg(long, default_value = "100000")]
    pub step: usize,

    /// Whether to disable the TUI interface.
    ///
    /// If the TUI interface is disabled, the program will print all the partial results to
    /// stdout.
    ///
    /// WARNING: the search may take a very long time, and the output may be very large.
    #[arg(long)]
    pub no_tui: bool,
}

impl Args {
    /// Parse and validate the command line arguments.
    pub fn parse_and_validate() -> Self {
        let args = Self::parse();

        if args.step == 0 {
            Self::command()
                .error(ErrorKind::ValueValidation, "step must be > 0")
                .exit();
        }

        match args.config.check() {
            Ok(config) => Self { config, ..args },
            Err(e) => Self::command().error(ErrorKind::ValueValidation, e).exit(),
        }
    }
}
