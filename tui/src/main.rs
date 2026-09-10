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
    no_stop: bool,
) -> Result<()> {
    let start = Instant::now();

    loop {
        let status = world.search(step);
        let solved = status == Status::Solved;

        match format {
            OutputFormat::Rle => {
                if solved {
                    println!("{}", world.rle(generation, true));
                }
            }
            OutputFormat::Json => {
                let rle = solved.then(|| world.rle(generation, true));
                let nogood = world.nogood_stats().map(|stats| {
                    serde_json::json!({
                        "learned": stats.learned,
                        "hits": stats.hits,
                        "fired": stats.fired,
                        "evicted": stats.evicted,
                        "reductions": stats.reductions,
                        "queries": stats.queries,
                        "capped_queries": stats.capped_queries,
                        "literals_total": stats.literals_total,
                        "rejected_long": stats.rejected_long,
                    })
                });
                let output = serde_json::json!({
                    "status": status.to_string(),
                    "generation": generation,
                    "population": world.population(generation),
                    "elapsed_secs": start.elapsed().as_secs_f64(),
                    "cells_checked": world.cells_checked(),
                    "nogood": nogood,
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
                    status,
                );
                if solved {
                    println!("{}", world.rle(generation, true));
                }
            }
        }

        // Continue after a solution when `no_stop` is set, like the TUI loop;
        // calling `search` again on a solved world resumes the enumeration.
        if status == Status::NoSolution || (solved && !no_stop) {
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
    let no_stop = args.no_stop;

    run_no_tui(&mut world, step, format, generation, no_stop)
}

/// Run a loaded search without the TUI interface.
fn run_no_tui_load(args: LoadArgs) -> Result<()> {
    let format = args.format;
    let generation = args.generation;
    let app = App::load(args)?;
    let mut world = app.world;
    let step = Some(app.step);
    let no_stop = app.no_stop;

    run_no_tui(
        &mut world,
        step,
        format,
        generation.unwrap_or(app.generation),
        no_stop,
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
