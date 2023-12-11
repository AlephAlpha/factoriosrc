use clap::{error::ErrorKind, CommandFactory, Parser};
use factoriosrc_lib::{Config, Status, World};

#[derive(Debug, Parser)]
struct Args {
    #[command(flatten)]
    config: Config,

    /// Step size for showing intermediate results
    ///
    /// Prints the current result every `step` iterations.
    #[arg(long, default_value = "100000")]
    step: usize,
}

impl Args {
    fn new() -> Self {
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

fn main() {
    let args = Args::new();

    let mut world = World::new(args.config).unwrap();

    while matches!(world.get_status(), Status::NotStarted | Status::Running) {
        world.search(args.step);
        println!("{}", world.rle(0));
    }
}
