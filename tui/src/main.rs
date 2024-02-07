mod app;
mod args;
mod event;
mod tui;
mod ui;

use crate::{
    app::App,
    args::{Cli, Command},
    tui::Tui,
};
use color_eyre::Result;
use crossterm::tty::IsTty;
use factoriosrc_lib::{Status, World};
use std::io::stdout;

/// Run the program without the TUI interface.
fn run_no_tui(args: Cli) -> Result<()> {
    let (mut world, step) = match args.command {
        Command::New(args) => (World::new(args.config)?, args.step),
        Command::Load(args) => {
            let app = App::load(args)?;
            (app.world, Some(app.step))
        }
    };

    while matches!(world.status(), Status::NotStarted | Status::Running) {
        world.search(step);
        println!("{}", world.rle(0, true));
    }

    Ok(())
}

fn main() -> Result<()> {
    let args = Cli::parse_and_validate();

    let stdout = stdout();

    if args.no_tui || !stdout.is_tty() {
        run_no_tui(args)?;
    } else {
        let mut tui = Tui::new(args)?;
        tui.run()?;
    }

    Ok(())
}
