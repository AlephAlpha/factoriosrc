//! A small benchmark driver for the SAT-solver-inspired search experiments.
//!
//! Runs a search with a uniform time limit and prints one JSON line per run
//! (or per world size, for exhaust-and-grow workflows). The results are
//! recorded in the consolidated benchmark section of `docs/sat-ideas.md`.
//!
//! Examples:
//!
//! - First solution:
//!   `cargo run --release -p factoriosrc-lib --example bench -- B3/S23 26 8 4 -y 1 -n a`
//! - Enumerate all solutions:
//!   `cargo run --release -p factoriosrc-lib --example bench -- B3/S23 5 5 2 --enumerate`
//! - Exhaust and grow:
//!   `cargo run --release -p factoriosrc-lib --example bench -- B3/S23 4 4 1 --enumerate --grow 8`
//!
//! The timing starts after the world (and its rule table) is built, so it
//! measures the search itself.

use factoriosrc_lib::{Config, NewState, NogoodStats, Status, Symmetry, World};
use std::time::{Duration, Instant};

const DEFAULT_TIME_LIMIT: f64 = 240.0;

struct BenchArgs {
    rule: String,
    width: u32,
    height: u32,
    period: u32,
    dx: i32,
    dy: i32,
    symmetry: Option<Symmetry>,
    new_state: NewState,
    seed: Option<u64>,
    backjump: bool,
    nogood: bool,
    nogood_translate: bool,
    phase_saving: bool,
    lookahead: bool,
    enumerate: bool,
    grow: u32,
    time_limit: f64,
}

impl Default for BenchArgs {
    fn default() -> Self {
        Self {
            rule: String::new(),
            width: 0,
            height: 0,
            period: 1,
            dx: 0,
            dy: 0,
            symmetry: None,
            new_state: NewState::Dead,
            seed: None,
            backjump: false,
            nogood: false,
            nogood_translate: false,
            phase_saving: false,
            lookahead: false,
            enumerate: false,
            grow: 0,
            time_limit: DEFAULT_TIME_LIMIT,
        }
    }
}

fn usage() -> ! {
    eprintln!(
        "usage: bench [FLAGS] <RULE> <WIDTH> <HEIGHT> <PERIOD>

flags:
  -x, --dx <N>                horizontal translation (default 0)
  -y, --dy <N>                vertical translation (default 0)
  -s, --symmetry <SYM>        symmetry, e.g. C1, D2-, D2| (default C1)
  -n, --new-state <a|d|r>     new-state strategy (default d)
      --seed <N>              random seed (used with -n r)
      --backjump              enable conflict analysis + backjumping
      --nogood                enable the nogood database
      --nogood-translate      enable translated templates
      --phase-saving          enable phase saving
      --lookahead             enable lookahead probing
      --enumerate             continue after each solution until exhaustion
      --grow <N>              after exhaustion, grow the world (at most N times;
                              implies --enumerate)
      --time-limit <SECS>     give up after this many seconds (default {DEFAULT_TIME_LIMIT})"
    );
    std::process::exit(2);
}

fn next_value(args: &[String], i: &mut usize) -> String {
    match args.get(*i) {
        Some(v) => {
            *i += 1;
            v.clone()
        }
        None => usage(),
    }
}

fn parse_args() -> BenchArgs {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut a = BenchArgs::default();
    let mut positional = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].clone();
        i += 1;
        match arg.as_str() {
            "-x" | "--dx" => {
                a.dx = next_value(&args, &mut i)
                    .parse()
                    .unwrap_or_else(|_| usage())
            }
            "-y" | "--dy" => {
                a.dy = next_value(&args, &mut i)
                    .parse()
                    .unwrap_or_else(|_| usage())
            }
            "-s" | "--symmetry" => {
                let v = next_value(&args, &mut i);
                a.symmetry = Some(v.parse().unwrap_or_else(|_| usage()));
            }
            "-n" | "--new-state" => {
                a.new_state = match next_value(&args, &mut i).as_str() {
                    "a" | "alive" => NewState::Alive,
                    "d" | "dead" => NewState::Dead,
                    "r" | "random" => NewState::Random,
                    _ => usage(),
                };
            }
            "--seed" => {
                a.seed = Some(
                    next_value(&args, &mut i)
                        .parse()
                        .unwrap_or_else(|_| usage()),
                )
            }
            "--backjump" => a.backjump = true,
            "--nogood" => a.nogood = true,
            "--nogood-translate" => a.nogood_translate = true,
            "--phase-saving" => a.phase_saving = true,
            "--lookahead" => a.lookahead = true,
            "--enumerate" => a.enumerate = true,
            "--grow" => {
                a.grow = next_value(&args, &mut i)
                    .parse()
                    .unwrap_or_else(|_| usage())
            }
            "--time-limit" => {
                a.time_limit = next_value(&args, &mut i)
                    .parse()
                    .unwrap_or_else(|_| usage())
            }
            _ => positional.push(arg),
        }
    }
    if positional.len() != 4 {
        usage();
    }
    a.rule = positional[0].clone();
    a.width = positional[1].parse().unwrap_or_else(|_| usage());
    a.height = positional[2].parse().unwrap_or_else(|_| usage());
    a.period = positional[3].parse().unwrap_or_else(|_| usage());
    a
}

fn build_config(a: &BenchArgs) -> Config {
    let mut config = Config::new(&a.rule, a.width, a.height, a.period).with_new_state(a.new_state);
    config = config.with_translations(a.dx, a.dy);
    if let Some(symmetry) = a.symmetry {
        config = config.with_symmetry(symmetry);
    }
    if let Some(seed) = a.seed {
        config = config.with_seed(seed);
    }
    if a.phase_saving {
        config = config.with_phase_saving();
    }
    if a.lookahead {
        config = config.with_lookahead();
    }
    if a.backjump {
        config = config.with_backjump();
    }
    if a.nogood {
        config = config.with_nogood();
    }
    if a.nogood_translate {
        config = config.with_nogood_translate();
    }
    config
}

fn flags_label(a: &BenchArgs) -> String {
    let mut flags = Vec::new();
    if a.backjump {
        flags.push("backjump");
    }
    if a.nogood {
        flags.push("nogood");
    }
    if a.nogood_translate {
        flags.push("nogood-translate");
    }
    if a.phase_saving {
        flags.push("phase-saving");
    }
    if a.lookahead {
        flags.push("lookahead");
    }
    if a.enumerate {
        flags.push("enumerate");
    }
    if a.grow > 0 {
        flags.push("grow");
    }
    if flags.is_empty() {
        "default".to_string()
    } else {
        flags.join("+")
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push('?'),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[allow(clippy::too_many_arguments)]
fn print_phase(
    a: &BenchArgs,
    width: u32,
    height: u32,
    status: &str,
    secs: f64,
    steps: u64,
    solutions: u64,
    first: Option<&(String, usize)>,
    stats: Option<NogoodStats>,
) {
    let (rle, pop) = match first {
        Some((rle, pop)) => (json_str(rle), pop.to_string()),
        None => ("null".to_string(), "null".to_string()),
    };
    let stats = match stats {
        Some(s) => format!(
            ",\"stats\":{{\"learned\":{},\"hits\":{},\"fired\":{},\"evicted\":{},\"reductions\":{},\"free\":{},\"edge\":{},\"axis\":{},\"local\":{},\"templates\":{}}}",
            s.learned,
            s.hits,
            s.fired,
            s.evicted,
            s.reductions,
            s.free,
            s.edge_pinned,
            s.axis_pinned,
            s.local,
            s.templates,
        ),
        None => String::new(),
    };
    println!(
        "{{\"rule\":{},\"size\":\"{width}x{height}\",\"period\":{},\"flags\":{},\"dx\":{},\"dy\":{},\"status\":{},\"secs\":{secs:.3},\"steps\":{steps},\"solutions\":{solutions},\"population\":{},\"rle\":{}{}}}",
        json_str(&a.rule),
        a.period,
        json_str(&flags_label(a)),
        a.dx,
        a.dy,
        json_str(status),
        pop,
        rle,
        stats,
    );
}

fn main() {
    let args = parse_args();
    let enumerate = args.enumerate || args.grow > 0;
    let config = build_config(&args);
    let mut world = match World::new(config) {
        Ok(world) => world,
        Err(e) => {
            println!(
                "{{\"rule\":{},\"flags\":{},\"status\":\"Rejected\",\"error\":{}}}",
                json_str(&args.rule),
                json_str(&flags_label(&args)),
                json_str(&e.to_string()),
            );
            return;
        }
    };

    let limit = Duration::from_secs_f64(args.time_limit);
    let mut grow_left = args.grow;
    loop {
        let phase_start = Instant::now();
        let mut steps: u64 = 0;
        let mut solutions: u64 = 0;
        let mut first: Option<(String, usize)> = None;
        let mut timed_out = false;
        loop {
            let status = world.search(1);
            steps += 1;
            if steps.is_multiple_of(1024) && phase_start.elapsed() >= limit {
                timed_out = true;
                break;
            }
            match status {
                Status::Solved => {
                    solutions += 1;
                    if first.is_none() {
                        first = Some((world.rle(0, true), world.population(0)));
                    }
                    if !enumerate {
                        break;
                    }
                }
                Status::NoSolution => break,
                Status::Running | Status::NotStarted => {}
            }
        }
        let secs = phase_start.elapsed().as_secs_f64();
        let status = if timed_out {
            "Timeout"
        } else {
            match world.status() {
                Status::NoSolution => "NoSolution",
                Status::Solved => "Solved",
                _ => "Running",
            }
        };
        let (width, height) = (world.config().width, world.config().height);
        print_phase(
            &args,
            width,
            height,
            status,
            secs,
            steps,
            solutions,
            first.as_ref(),
            world.nogood_stats().copied(),
        );
        if timed_out || world.status() != Status::NoSolution || grow_left == 0 {
            break;
        }
        world.increase_world_size();
        grow_left -= 1;
    }
}
