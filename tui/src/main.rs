mod app;
mod args;
mod event;
mod tui;
mod ui;

use crate::{args::Args, tui::Tui};
use color_eyre::Result;
use crossterm::tty::IsTty;
use factoriosrc_lib::{Status, World};
use std::io::stdout;

/// Run the program without the TUI interface.
fn run_no_tui(args: Args) -> Result<()> {
    let mut world = World::new(args.config)?;

    while matches!(world.status(), Status::NotStarted | Status::Running) {
        world.search(args.step);
        println!("{}", world.rle(0, true));
    }

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse_and_validate();

    let stdout = stdout();

    if args.no_tui || !stdout.is_tty() {
        return run_no_tui(args);
    }

    let mut tui = Tui::new(args)?;
    tui.run()?;

    Ok(())
}
