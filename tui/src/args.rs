use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum, error::ErrorKind};
use factoriosrc_lib::{CellState, Config, KnownCell};
use std::path::PathBuf;

/// Output format for non-TUI mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum OutputFormat {
    /// RLE format (default).
    #[default]
    Rle,
    /// JSON format.
    Json,
    /// Human-readable summary.
    Human,
}

/// A simple tool to search for patterns in Factorio and other cellular automata.
#[derive(Debug, Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// Either start a new search or load a saved search.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Start a new search.
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

    /// Whether to disable the TUI interface.
    ///
    /// If the program is run in a non-interactive environment (e.g. when stdout is not a TTY),
    /// the TUI interface will be automatically disabled, and this flag will be ignored.
    ///
    /// WARNING: the search may take a very long time. It is not possible to pause the search
    /// or save the state of the search in non-TUI mode.
    #[arg(long)]
    pub no_tui: bool,

    /// A path to save the state of the search.
    ///
    /// If not specified, the state will not be saved.
    ///
    /// The state will be saved when quitting the application.
    #[arg(long)]
    pub save: Option<PathBuf>,

    /// Set a cell to a known state, specified as `x,y,t,state`.
    ///
    /// `state` is either `alive` or `dead`. This option can be repeated.
    ///
    /// Example: --known-cell 0,0,0,alive --known-cell 1,2,0,dead
    #[arg(short = 'k', long = "known-cell", value_name = "X,Y,T,STATE")]
    pub known_cells: Vec<String>,

    /// Path to a file containing known cells.
    ///
    /// Format: one cell per line `x,y,t,state`.
    #[arg(long, value_name = "PATH")]
    pub known_cells_file: Option<PathBuf>,

    /// Output format for non-TUI mode.
    #[arg(long, value_name = "FORMAT", default_value = "rle")]
    pub format: OutputFormat,

    /// Generation to display (only used in non-TUI mode).
    #[arg(short = 'g', long, value_name = "GEN", default_value_t = 0)]
    pub generation: i32,
}

/// Load a saved search.
#[derive(Debug, Args)]
pub struct LoadArgs {
    /// A path to load the state of the search.
    pub load: PathBuf,

    /// A path to save the state of the search.
    ///
    /// If not specified, it will default to the path of the loaded state.
    ///
    /// The state will be saved when quitting the application.
    #[arg(long)]
    pub save: Option<PathBuf>,

    /// Override the step size for the loaded search.
    #[arg(long)]
    pub step: Option<usize>,

    /// Override whether to continue searching after finding a solution.
    #[arg(long)]
    pub no_stop: Option<bool>,

    /// Override whether to increase the world size when the search fails.
    #[arg(long)]
    pub increase_world_size: Option<bool>,

    /// Whether to disable the TUI interface.
    ///
    /// If the program is run in a non-interactive environment (e.g. when stdout is not a TTY),
    /// the TUI interface will be automatically disabled, and this flag will be ignored.
    ///
    /// WARNING: the search may take a very long time. It is not possible to pause the search
    /// or save the state of the search in non-TUI mode.
    #[arg(long)]
    pub no_tui: bool,

    /// Output format for non-TUI mode.
    #[arg(long, value_name = "FORMAT", default_value = "rle")]
    pub format: OutputFormat,

    /// Generation to display (only used in non-TUI mode).
    #[arg(short = 'g', long, value_name = "GEN")]
    pub generation: Option<i32>,
}

impl Cli {
    /// Parse and validate the command line arguments.
    pub fn parse_and_validate() -> Self {
        let mut args = Self::parse();

        match &mut args.command {
            Command::New(args) => {
                if args.step == Some(0) {
                    Self::command()
                        .error(ErrorKind::ValueValidation, "step must be > 0")
                        .exit();
                }

                // Parse --known-cell arguments.
                for s in &args.known_cells {
                    let parts: Vec<&str> = s.split(',').collect();
                    if parts.len() != 4 {
                        Self::command()
                            .error(
                                ErrorKind::ValueValidation,
                                format!(
                                    "invalid --known-cell format: '{s}'. \
                                     Expected 'x,y,t,state' where state is \
                                     'alive'/'dead' or '1'/'0'"
                                ),
                            )
                            .exit();
                    }
                    let (Ok(x), Ok(y), Ok(t)) = (
                        parts[0].parse::<u32>(),
                        parts[1].parse::<u32>(),
                        parts[2].parse::<u32>(),
                    ) else {
                        Self::command()
                            .error(
                                ErrorKind::ValueValidation,
                                format!(
                                    "invalid coordinates in '--known-cell {s}': \
                                         x, y, t must be non-negative integers"
                                ),
                            )
                            .exit();
                    };
                    let state = match parts[3] {
                        "dead" | "0" => CellState::Dead,
                        "alive" | "1" => CellState::Alive,
                        _ => {
                            Self::command()
                                .error(
                                    ErrorKind::ValueValidation,
                                    format!(
                                        "invalid state '{}' in '--known-cell {s}': \
                                         expected 'alive'/'dead' or '1'/'0'",
                                        parts[3]
                                    ),
                                )
                                .exit();
                        }
                    };
                    args.config.known_cells.push(KnownCell { x, y, t, state });
                }

                // Parse --known-cells-file.
                if let Some(path) = &args.known_cells_file {
                    let content = match std::fs::read_to_string(path) {
                        Ok(c) => c,
                        Err(e) => {
                            Self::command()
                                .error(
                                    ErrorKind::ValueValidation,
                                    format!("failed to read '{}': {e}", path.display()),
                                )
                                .exit();
                        }
                    };
                    for (line_num, line) in content.lines().enumerate() {
                        let line = line.trim();
                        if line.is_empty() || line.starts_with('#') {
                            continue;
                        }
                        let parts: Vec<&str> = line.split(',').collect();
                        if parts.len() != 4 {
                            Self::command()
                                .error(
                                    ErrorKind::ValueValidation,
                                    format!(
                                        "invalid format in '{}' line {}: '{line}'. \
                                         Expected 'x,y,t,state' where state is \
                                         'alive'/'dead' or '1'/'0'",
                                        path.display(),
                                        line_num + 1
                                    ),
                                )
                                .exit();
                        }
                        let (Ok(x), Ok(y), Ok(t)) = (
                            parts[0].parse::<u32>(),
                            parts[1].parse::<u32>(),
                            parts[2].parse::<u32>(),
                        ) else {
                            Self::command()
                                .error(
                                    ErrorKind::ValueValidation,
                                    format!(
                                        "invalid coordinates in '{}' line {}: \
                                             x, y, t must be non-negative integers",
                                        path.display(),
                                        line_num + 1
                                    ),
                                )
                                .exit();
                        };
                        let state = match parts[3] {
                            "dead" | "0" => CellState::Dead,
                            "alive" | "1" => CellState::Alive,
                            _ => {
                                Self::command()
                                    .error(
                                        ErrorKind::ValueValidation,
                                        format!(
                                            "invalid state '{}' in '{}' line {}: \
                                             expected 'alive'/'dead' or '1'/'0'",
                                            parts[3],
                                            path.display(),
                                            line_num + 1
                                        ),
                                    )
                                    .exit();
                            }
                        };
                        args.config.known_cells.push(KnownCell { x, y, t, state });
                    }
                }

                if args.generation < 0 {
                    Self::command()
                        .error(ErrorKind::ValueValidation, "generation must be >= 0")
                        .exit();
                }

                // Only run full config validation in non-TUI mode or when width/height are
                // explicitly provided. In TUI mode without dimensions the config form handles it.
                let use_tui = !args.no_tui;
                let has_dimensions = args.config.width > 0 && args.config.height > 0;
                if (!use_tui || has_dimensions)
                    && let Err(e) = args.config.check()
                {
                    Self::command().error(ErrorKind::ValueValidation, e).exit();
                }
            }
            Command::Load(args) => {
                args.save.get_or_insert_with(|| args.load.clone());
            }
        }

        args
    }
}
