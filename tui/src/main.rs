#![warn(clippy::nursery)]

mod app;
mod args;
mod event;
mod layout;
mod tui;
mod ui;

use crate::{
    app::App,
    args::{Cli, Command, LoadArgs, NewArgs, OutputFormat},
    tui::Tui,
};
use color_eyre::Result;
use crossterm::tty::IsTty;
use factoriosrc_lib::{Status, World};
use std::{io::stdout, time::Instant};

/// Run a search without the TUI interface.
fn run_no_tui(
    world: &mut World,
    step: Option<usize>,
    format: OutputFormat,
    generation: i32,
) -> Result<()> {
    let start = Instant::now();

    while matches!(world.status(), Status::NotStarted | Status::Running) {
        world.search(step);

        match format {
            OutputFormat::Rle => {
                println!("{}", world.rle(generation, true));
            }
            OutputFormat::Json => {
                let rle = world.rle(generation, true);
                let output = serde_json::json!({
                    "status": world.status().to_string(),
                    "generation": generation,
                    "population": world.population(generation),
                    "elapsed_secs": start.elapsed().as_secs_f64(),
                    "cells_checked": world.cells_checked(),
                    "rle": rle,
                });
                println!("{output}");
            }
            OutputFormat::Human => {
                let elapsed = start.elapsed();
                let pop = world.population(generation);
                let cells = world.cells_checked();
                println!(
                    "Status: {:?} | Gen: {generation} | Pop: {pop} | Cells: {cells} | Time: {elapsed:.2?}",
                    world.status(),
                );
                println!("{}", world.rle(generation, true));
            }
        }

        if matches!(world.status(), Status::Solved | Status::NoSolution) {
            break;
        }
    }

    Ok(())
}

/// Run a new search without the TUI interface.
fn run_no_tui_new(args: NewArgs) -> Result<()> {
    let mut world = World::new(args.config)?;
    let step = args.step;
    let format = args.format;
    let generation = args.generation;

    run_no_tui(&mut world, step, format, generation)
}

/// Run a loaded search without the TUI interface.
fn run_no_tui_load(args: LoadArgs) -> Result<()> {
    let format = args.format;
    let generation = args.generation;
    let app = App::load(args)?;
    let mut world = app.world;
    let step = Some(app.step);

    run_no_tui(
        &mut world,
        step,
        format,
        generation.unwrap_or(app.generation),
    )
}

fn main() -> Result<()> {
    let args = Cli::parse_and_validate();
    let use_tui = stdout().is_tty();

    match args.command {
        Command::New(new_args) => {
            if new_args.no_tui || !use_tui {
                run_no_tui_new(new_args)?;
            } else {
                let mut tui = Tui::new(Command::New(new_args))?;
                tui.run()?;
            }
        }
        Command::Load(load_args) => {
            if load_args.no_tui || !use_tui {
                run_no_tui_load(load_args)?;
            } else {
                let mut tui = Tui::new(Command::Load(load_args))?;
                tui.run()?;
            }
        }
    }

    Ok(())
}
