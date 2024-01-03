use clap::{error::ErrorKind, CommandFactory, Parser};
use factoriosrc_lib::Config;

/// A simple tool to search for patterns in Factorio cellular automata.
#[derive(Debug, Parser)]
pub struct Args {
    #[command(flatten)]
    pub config: Config,

    /// Number of steps between each display of the current partial result.
    ///
    /// If the TUI interface is disabled, the program will print the current partial result
    /// every `step` steps. If `step` is not specified, it will only print the final result.
    ///
    /// If the TUI interface is enabled, the program will display the current partial result
    /// every `step` steps. If `step` is not specified, it will default to 100000.
    #[arg(long)]
    pub step: Option<usize>,

    /// Whether to disable the TUI interface.
    ///
    /// WARNING: the search may take a very long time.
    #[arg(long)]
    pub no_tui: bool,

    /// Whether to increase the world size when the search fails.
    ///
    /// If the height is greater than the width, the width will increased by 1.
    /// Otherwise, the height will increased by 1.
    ///
    /// When the world size is increased, the search will be restarted, and the current search
    /// status will be lost.
    #[arg(long)]
    pub increase_world_size: bool,
}

impl Args {
    /// Parse and validate the command line arguments.
    pub fn parse_and_validate() -> Self {
        let args = Self::parse();

        if args.step == Some(0) {
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
