use clap::{error::ErrorKind, Args, CommandFactory, Parser, Subcommand};
use factoriosrc_lib::Config;
use std::path::PathBuf;

/// A simple tool to search for patterns in Factorio cellular automata.
#[derive(Debug, Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Whether to disable the TUI interface.
    ///
    /// WARNING: the search may take a very long time. It is not possible to pause the search
    /// or save the state of the search i
    #[arg(long)]
    pub no_tui: bool,
}

/// Either start a new search or load a saved search.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Start a new search.
    #[command(arg_required_else_help = true)]
    New(NewArgs),

    /// Load a saved search.
    Load(LoadArgs),
}

/// Start a new search.
#[derive(Debug, Args)]
pub struct NewArgs {
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

    /// Whether to increase the world size when the search fails.
    ///
    /// If the diagonal width exists and is smaller than the width, it will be increased by 1.
    /// Otherwise, if the height is greater than the width, the width will increased by 1.
    /// Otherwise, the height will increased by 1.
    ///
    /// If the configuration requires a square world, both the width and the height will be
    /// increased by 1.
    ///
    /// When the world size is increased, the search will be restarted, and the current search
    /// status will be lost.
    #[arg(long)]
    pub increase_world_size: bool,

    /// Do not stop the search when a solution is found.
    ///
    /// The search will continue until no more solutions exist, or paused by the user.
    #[arg(long)]
    pub no_stop: bool,

    /// A path to save the state of the search.
    ///
    /// If not specified, the state will not be saved.
    ///
    /// The state will be saved when quitting the application.
    #[arg(long)]
    pub save: Option<PathBuf>,
}

/// Load a saved search.
#[derive(Debug, Args)]
pub struct LoadArgs {
    /// A path to load the state of the search.
    pub load: PathBuf,

    /// A path to save the state of the search.
    ///
    /// If not specified, the state will not be saved.
    ///
    /// The state will be saved when quitting the application.
    #[arg(long)]
    pub save: Option<PathBuf>,
}

impl Cli {
    /// Parse and validate the command line arguments.
    pub fn parse_and_validate() -> Self {
        let args = Self::parse();
        let no_tui = args.no_tui;

        if let Command::New(args) = args.command {
            if args.step == Some(0) {
                Self::command()
                    .error(ErrorKind::ValueValidation, "step must be > 0")
                    .exit();
            }

            let args = match args.config.check() {
                Ok(config) => NewArgs { config, ..args },
                Err(e) => Self::command().error(ErrorKind::ValueValidation, e).exit(),
            };

            Self {
                command: Command::New(args),
                no_tui,
            }
        } else {
            args
        }
    }
}
