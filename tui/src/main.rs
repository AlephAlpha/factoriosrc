mod app;
mod args;
mod event;
mod tui;
mod ui;

use crate::{args::Args, tui::Tui};
use color_eyre::Result;
use crossterm::tty::IsTty;
use factoriosrc_lib::{Status, WorldAllocator};
use std::io::stdout;

/// Run the program without the TUI interface.
fn run_no_tui<'a>(args: Args, allocator: &'a mut WorldAllocator<'a>) -> Result<()> {
    let mut world = allocator.new_world(args.config)?;

    while matches!(world.status(), Status::NotStarted | Status::Running) {
        world.search(args.step);
        println!("{}", world.rle(0));
    }

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse_and_validate();
    let mut allocator = WorldAllocator::new();

    let stdout = stdout();

    if args.no_tui || !stdout.is_tty() {
        return run_no_tui(args, &mut allocator);
    }

    let mut tui = Tui::new(args, &mut allocator)?;
    tui.run()?;

    Ok(())
}
