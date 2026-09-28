# Capacity Study For The Nogood Database

> **Scope.** This is an experiment note, not a description of current
> behavior. It measures how `Config::nogood_capacity` affects
> `--nogood` search on the canonical benchmark cases, on a size ladder of
> `B3/S23` c/4 ships, and on four larger cases that plain search can finish
> but that motivated this study. The code remains the source of truth;
> `docs/sat-ideas.md` describes the mechanisms.

The study was prompted by the concern that the automatic capacity
`max(2048, 4 * world_size)` (`NogoodDb::with_world_size`) may be fitted to
the small canonical cases and may not extrapolate to larger searches. The
main questions were:

1. Is the database actually saturating on the benchmark cases, or does the
   capacity only add maintenance cost there?
2. Does the best `capacity / world_size` ratio drift with the problem size?
3. Do the larger cases change the picture, given that `--nogood` cannot
   finish them within short budgets?

## Setup

- Machine: Intel i9-12900KS, 24 threads (8 P-cores with SMT, 8 E-cores),
  31 GiB RAM, Linux 7.1.13, release build.
- Commit `79f3c4a` plus a diagnostic-only change in two files:
  `World::world_size`, `World::nogood_capacity`, `World::nogood_entries`,
  and the matching JSON fields in the non-TUI output (`tui/src/main.rs`).
  No search logic changed.
- Measurement: every non-TUI run used `--no-tui --format json --step N`, so
  each progress interval prints one JSON line with the cumulative
  `elapsed_secs`, `steps`, every `SearchStats` counter, and every
  `NogoodStats` counter, including `bucket_updates`, `capped_queries`, and
  the histograms. Times in the tables are the program's internal
  `elapsed_secs`, not external wall time.
- Stop conditions: first solution (`c1`, `c2`, `c4`, `c5`, the ladder, the
  large cases), the 10th solution (`c6`, `--no-stop` with an early kill), and
  exhaustion (`c7`).
- Budgets: 300 s per run in the exploratory sweeps, 5400 s (ladder
  frontiers) and 9000 s (large cases) in the overnight runs. A killed run is
  a right-censored observation.
- Two timing tiers:
  - Step counts and all counters were collected with four concurrent runs,
    each pinned to a distinct P-core (`taskset -c 4,6,8,10`). Step counts
    are deterministic for a fixed seed and capacity; the concurrent wall
    times are inflated and are not used in the tables.
  - The "clean time" columns come from a sequential re-run pinned to a
    single P-core. As a sanity check, clean times reproduce the older
    canonical table (`3f63a21`): `c5` plain 4.29 s vs 4.31 s, `c7` `--nogood`
    auto 3.61 s vs 3.53 s, `c1` `--nogood` auto 3.11 s vs 3.03 s.

The canonical sweep used the capacity grid
`{256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536}` plus
`{0.5, 1, 2, 4, 8} * world_size` plus the automatic value, and `--backjump`
as the "no database" endpoint. The ladder used a subset
`{256, 1024, 4096, auto}` (plus more in the exploratory sweep).

## Case Definitions

The case labels are mnemonic, not part of the API. `c1`-`c7` follow the
order of the canonical benchmark table in
[`docs/sat-ideas.md`](sat-ideas.md); the rule is passed with `-r RULE`, the
positional arguments are `W H P`, and the other flags keep their usual
meaning. `world_size` in the tables is
`(W + 2 * radius) * (H + 2 * radius) * P`, including the padding ring, which
is also the basis of the automatic capacity. `c3` does not appear in this
study: it is the Generations case `3457/357/5`, and `--nogood` is rejected
for rules with more than two states by `Config::check()`.

| label | arguments after `-r RULE` | rule | stop condition | role |
|---|---|---|---|---|
| c1 | `26 8 4 -y 1 -n a` | `B3/S23` | first solution | canonical, Life c/4 ship |
| c2 | `64 64 1 -n a` | `B3/S23` | first solution | canonical, period-1 oscillator |
| c4 | `50 10 4 -x 2 -s D2- -n a` | `R3,C2,S2,B3,N+` | first solution | canonical, factorio-like totalistic rule |
| c5 | `30 9 4 -x 1 -n a` | `B2n3/S23-q` | first solution | canonical, non-totalistic Life-like rule |
| c6 | `20 20 2 -n r --seed 1 --no-stop` | `B3/S23` | 10th solution | canonical, random branching |
| c7 | `7 7 2 -n a --no-stop` | `B3/S23` | exhaustion (9537 solutions) | canonical, small complete search |
| L1 | `100 17 3 -x 2 -s D2- -n a` | `R3,C2,S2,B3,N+` | first solution | larger case |
| L2 | `100 10 4 -x 1 -n a` | `B3/S23` | first solution | larger case |
| L3 | `31 8 5 -y 1 -n a` | `B3/S23` | first solution | larger case |
| L4 | `100 18 5 -x 1 -s D2- -n a` | `B3/S23` | first solution | larger case |
| A`W` | `W 8 4 -y 1 -n a` | `B3/S23` | first solution | size-ladder rung, `W` in 26..80 |

The A-rungs use the same rule, orientation, and height as `c1`, so the only
ladder parameter is the width. The rungs that the automatic capacity cannot
finish within 300 s (`A41`, `A46`) are revisited in the overnight runs.

## Saturation At The Automatic Capacity

The first surprise is that the automatic database is *not* oversized on the
small cases: it saturates and churns heavily almost everywhere. `c2`
(64x64, period 1) is the only case with zero reductions.

| case | world_size | cap | cap/size | observed | steps/s | reductions | evicted (unused) | relearned | bucket_upd/step | capped |
|---|---:|---:|---:|---|---:|---:|---:|---:|---:|---:|
| c1 | 1120 | 4480 | 4.00 | 442k (solved) | 122k | 82 | 184k (96k) | 1262 | 3453 | . |
| c2 | 4356 | 17424 | 4.00 | 2007 (solved) | 496k | 0 | 0 (0) | 0 | 1 | . |
| c4 | 3584 | 14336 | 4.00 | 4.50M in 238 s | 19k | 173 | 1.24M (503k) | 16559 | 15k | 0.97 |
| c5 | 1408 | 5632 | 4.00 | 2.06M (solved) | 70k | 315 | 887k (480k) | 6763 | 4343 | 0.70 |
| c6 | 968 | 3872 | 4.00 | 61k in 1 s | 62k | 11 | 21k (12k) | 357 | 3527 | 0.75 |
| c7 | 162 | 2048 | 12.64 | 653k (nosolution) | 181k | 214 | 219k (139k) | 21298 | 2524 | 0.96 |
| L1 | 7314 | 29256 | 4.00 | 3.10M in 300 s | 10k | 51 | 746k (275k) | 2323 | 36k | . |
| L2 | 4896 | 19584 | 4.00 | 12.40M in 299 s | 42k | 538 | 5.27M (2.76M) | 35273 | 16k | . |
| L3 | 1650 | 6600 | 4.00 | 33.00M in 300 s | 110k | 4145 | 13.68M (6.75M) | 112399 | 6308 | . |
| L4 | 10200 | 40800 | 4.00 | 10.00M in 297 s | 34k | 140 | 2.86M (899k) | 33122 | 30k | . |

Two consequences:

- The eviction policy is active on every nontrivial case, so the capacity
  formula is exercised even by the canonical workloads. The hypothesis that
  the benchmark "never fills the database" is false.
- `relearned_evicted` and `evicted_unused` show that a large part of the
  churn is speculative: e.g. `c7` re-learned 21k patterns in 653k steps, and
  `c5` evicted 480k never-used entries. A larger capacity retains more of
  them, but `capped_queries` shows why that does not translate one-for-one
  into pruning: at the largest capacities almost every completion query hits
  the `MAX_QUERY_CANDIDATES = 64` prefix bound (`c7`: 0.9975, `c4`: 0.9947).
  The query cap is a second, independent bound on how much a large database
  can help.

## Canonical Capacity Sweeps

`c2` gets no detailed table because every capacity solves it in exactly
2007 steps, and `c4` gets none because no tested capacity finds a solution
within 300 s; both still appear in the saturation, summary, and large-case
tables. Except for the random-branching `c6`, every tested capacity found
the same first solution on cases that stop at the first solution, and every
`c7` capacity enumerated exactly the same 9537 solutions.

### `c1`: `B3/S23 26 8 4 -y 1 -n a` (world_size 1120, first solution)

| config | cap | cap/size | steps | clean time (s) | steps/s | reductions |
|---|---:|---:|---:|---:|---:|---:|
| plain | . | . | 6.63M | 1.281 | 5.17M | . |
| backjump | . | . | 13.61M | 43.135 | 315k | . |
| 256 | 256 | 0.23 | 649k | 3.284 | 198k | 2080 |
| 512 | 512 | 0.46 | 565k | 2.980 | 189k | 914 |
| 1024 | 1024 | 0.91 | 523k | 2.910 | 180k | 426 |
| 2048 | 2048 | 1.83 | 486k | 2.951 | 165k | 198 |
| 4096 | 4096 | 3.66 | 459k | 3.155 | 146k | 94 |
| 8192 | 8192 | 7.31 | 416k | 3.420 | 122k | 42 |
| 16384 | 16384 | 14.63 | 389k | 4.066 | 96k | 19 |
| 32768 | 32768 | 29.26 | 369k | 5.256 | 70k | 8 |
| 65536 | 65536 | 58.51 | 359k | 7.218 | 50k | 3 |
| auto | 4480 | 4.00 | 442k | 3.114 | 142k | 82 |

Steps fall monotonically with capacity, but with diminishing returns, while
per-step cost grows roughly with the database size. The time optimum is at
cap 1024 (ratio 0.91, 2.91 s); the automatic cap is 7% slower (3.11 s), and
a 64x larger database is 2.5x slower. `--nogood` itself is 2.3x slower than
plain on this case even at the best capacity.

### `c5`: `B2n3/S23-q 30 9 4 -x 1 -n a` (world_size 1408, first solution)

| config | cap | cap/size | steps | clean time (s) | steps/s | reductions | capped |
|---|---:|---:|---:|---:|---:|---:|---:|
| plain | . | . | 17.23M | 4.291 | 4.01M | . | . |
| backjump | . | . | >31M | (killed) | . | . | . |
| 256 | 256 | 0.18 | 2.99M | 17.385 | 172k | 9806 | 0.04 |
| 512 | 512 | 0.36 | 2.68M | 16.338 | 164k | 4436 | 0.13 |
| 1024 | 1024 | 0.73 | 2.47M | 15.806 | 156k | 2053 | 0.40 |
| 1408 | 1408 | 1.00 | 2.35M | 15.420 | 152k | 1426 | 0.46 |
| 2048 | 2048 | 1.45 | 2.28M | 15.472 | 147k | 952 | 0.58 |
| 4096 | 4096 | 2.91 | 2.12M | 16.168 | 131k | 444 | 0.70 |
| 8192 | 8192 | 5.82 | 2.00M | 18.055 | 111k | 210 | 0.71 |
| 16384 | 16384 | 11.64 | 1.87M | 21.581 | 87k | 98 | 0.73 |
| 32768 | 32768 | 23.27 | 1.77M | 28.361 | 62k | 46 | 0.74 |
| 65536 | 65536 | 46.55 | 1.66M | 37.286 | 45k | 21 | 0.76 |
| auto | 5632 | 4.00 | 2.06M | 16.980 | 121k | 315 | 0.70 |

The optimum is cap 1408 (ratio 1.0, 15.42 s); the automatic cap is 10%
slower and `--nogood` stays 3.6x slower than plain even at the optimum.
`capped` rises monotonically with capacity, so the extra retention buys
less and less pruning.

### `c6`: `B3/S23 20 20 2 -n r --seed 1 --no-stop` (world_size 968, 10th solution)

| config | cap | cap/size | steps to 10th | clean time (s) | steps/s | reductions | capped |
|---|---:|---:|---:|---:|---:|---:|---:|
| plain | . | . | 42.93M | 6.337 | 6.78M | . | . |
| 256 | 256 | 0.26 | 2.29M | 11.714 | 196k | 6638 | 0.00 |
| 512 | 512 | 0.53 | 715k | 4.277 | 167k | 1104 | 0.13 |
| 1024 | 1024 | 1.06 | 939k | 6.134 | 153k | 733 | 0.51 |
| 1936 | 1936 | 2.00 | 696k | . | . | 295 | 0.76 |
| 2048 | 2048 | 2.12 | 26k | 0.169 | 156k | 10 | 0.93 |
| 3872 | 3872 | 4.00 | 58k | 0.457 | 127k | 12 | 0.75 |
| 8192 | 8192 | 8.46 | 75k | 0.771 | 97k | 6 | 0.95 |
| 32768 | 32768 | 33.85 | 54k | 0.701 | 76k | 0 | 1.00 |
| 65536 | 65536 | 67.70 | 54k | 0.699 | 76k | 0 | 1.00 |
| auto | 3872 | 4.00 | 58k | 0.457 | 127k | 12 | 0.86 |

This case uses random branching, so different eviction decisions lead to
different search paths and the time curve is not smooth (cap 2048 is 2.7x
faster than the automatic cap here, while cap 1024 is worse than both).
It is the only case where the automatic formula clearly loses on time, and
the result should be treated as seed-specific rather than a trend.

### `c7`: `B3/S23 7 7 2 -n a --no-stop` (world_size 162, exhaustion)

| config | cap | cap/size | steps | clean time (s) | steps/s | reductions | capped |
|---|---:|---:|---:|---:|---:|---:|---:|
| plain | . | . | 3.54M | 0.691 | 5.12M | . | . |
| backjump | . | . | 2.50M | 2.489 | 1.00M | . | . |
| 256 | 256 | 1.58 | 1.26M | 3.797 | 332k | 3894 | 0.08 |
| 512 | 512 | 3.16 | 1.06M | 3.701 | 287k | 1602 | 0.48 |
| 1024 | 1024 | 6.32 | 858k | 3.575 | 240k | 618 | 0.83 |
| 1296 | 1296 | 8.00 | 785k | 3.548 | 221k | 435 | 0.89 |
| 2048 | 2048 | 12.64 | 653k | 3.606 | 181k | 214 | 0.96 |
| 4096 | 4096 | 25.28 | 476k | 3.872 | 123k | 66 | 0.98 |
| 8192 | 8192 | 50.57 | 358k | 4.596 | 78k | 18 | 0.99 |
| 65536 | 65536 | 404.54 | 271k | 9.692 | 28k | 0 | 1.00 |
| auto | 2048 | 12.64 | 653k | 3.606 | 181k | 214 | 0.96 |

Here the floor dominates: the automatic 2048 is within 2% of the best
measured capacity (1296), and the whole range 512-4096 is within 5%.
All capacities still enumerate exactly 9537 solutions, which confirms that
the capacity only changes pruning, not the solution set.

### Summary of the canonical sweeps

| case | world_size | best cap | best cap/size | best time | auto cap/size | auto/best time | plain time |
|---|---:|---:|---:|---:|---:|---:|---:|
| c1 | 1120 | 1024 | 0.91 | 2.910 | 4.00 | 1.07 | 1.281 |
| c5 | 1408 | 1408 | 1.00 | 15.420 | 4.00 | 1.10 | 4.291 |
| c6 | 968 | 2048 | 2.12 | 0.169 | 4.00 | 2.70 | 6.337 |
| c7 | 162 | 1296 | 8.00 | 3.548 | 12.64 | 1.02 | 0.691 |
| c2 | 4356 | (steps identical at every cap) | . | . | 4.00 | 1.00 | >300 |

## Size Ladder

The `c1` family (`B3/S23 W 8 4 -y 1 -n a`) was extended from `W = 26` to
`W = 80`. A rung is included in the sweep when the automatic capacity can
still find a solution within 300 s. Difficulty is not monotonic in width:
`W = 56` is much easier than `W = 46`, and `W = 41, 46, 51` time out while
`W = 56, 64, 80` finish. The completable rungs are:

| rung | world_size | plain steps | auto cap | auto cap/size | auto time | best explicit cap | best cap/size | best time | auto/best time |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| A26 = c1 | 1120 | 6.63M | 4480 | 4.00 | 3.114 | 1024 | 0.91 | 2.910 | 1.07 |
| A31 | 1320 | 15.75M | 5280 | 4.00 | 4.462 | 1024 | 0.78 | 4.325 | 1.03 |
| A36 | 1520 | 365.09M | 6080 | 4.00 | 117.685 | 4096 | 2.69 | 118.193 | 1.00 |
| A56 | 2320 | 16.08M | 9280 | 4.00 | 14.039 | 4096 | 1.77 | 13.130 | 1.07 |
| A64 | 2640 | 478.79M | 10560 | 4.00 | 248.824 | 4096 | 1.55 | 254.443 | 0.98 |
| A80 | 3280 | 14.70M | 13120 | 4.00 | 27.736 | 4096 | 1.25 | 25.170 | 1.10 |

(Only `{256, 1024, 4096, auto}` were re-timed sequentially; the exploratory
sweep also tried `0.5x` and `2x` sizes and larger absolute caps, and no
non-auto cap was better than the ones shown by more than noise.)

Within the window where `--nogood` can finish at all, the best absolute
capacity grows only slowly, from 1k entries at `world_size` 1120-1320 to 4k
at 1520-3280, while the best ratio does not grow (0.9, 0.8, 2.7, 1.8, 1.6,
1.3 in order of increasing size). The data therefore gives no support for
the `4 * world_size` ratio as a scaling law: the optimum looks like a
slowly growing absolute working set, not a fixed fraction of the world. At
the same time the automatic ratio-4 capacity is within 0-10% of the best
measured time on five of the six rungs, so on this window the formula is
not catastrophic — just consistently on the over-sized side.

## Large Cases: 300 s Snapshots

None of the larger cases finishes under `--nogood` within 300 s (the
overnight section below extends some of these budgets). The snapshots show
why: steps per second drop by roughly two orders of magnitude relative to
plain, while the expected step reduction to a first solution is at best a
few tens of times.

| case | config | cap | T (s) | steps in T | steps/s | vs plain |
|---|---|---:|---:|---:|---:|---:|
| c4 | plain | . | 74.5 | 103.45M (solved) | 1.39M | 1.00 |
| c4 | backjump | . | 299 | 13.00M | 43k | 0.03 |
| c4 | auto | 14336 | 298 | 4.50M | 15k | 0.01 |
| L1 | plain | . | 300 | 426.90M | 1.42M | 1.00 |
| L1 | auto | 29256 | 300 | 3.10M | 10k | 0.007 |
| L1 | guard | 29256 | 300 | 391.10M | 1.30M | 0.92 |
| L2 | plain | . | 300 | 1440.30M | 4.80M | 1.00 |
| L2 | auto | 19584 | 299 | 12.40M | 42k | 0.009 |
| L2 | guard | 19584 | 300 | 1247.60M | 4.16M | 0.87 |
| L3 | plain | . | 300 | 1638.50M | 5.46M | 1.00 |
| L3 | auto | 6600 | 300 | 33.00M | 110k | 0.02 |
| L3 | guard | 6600 | 300 | 1404.00M | 4.68M | 0.86 |
| L4 | plain | . | 300 | 934.30M | 3.11M | 1.00 |
| L4 | auto | 40800 | 297 | 10.00M | 34k | 0.011 |
| L4 | guard | 40800 | 300 | 818.40M | 2.73M | 0.88 |

`--nogood-guard` recovers 86-92% of the plain throughput: it suspends the
machinery for most of the run and falls back to chronological backtracking,
so it is effectively "plain plus a little shallow learning". `--backjump`
without a database is already 23x slower than plain on `c4`, so most of the
loss is conflict analysis and backjumping, not the database itself; the
database adds another ~3x at the automatic capacity. The overnight runs
revisit these ordering questions with longer budgets.

## Overnight Runs

Two ladder frontiers (`A41`, `A46`, where the automatic capacity times out
in 300 s) and the two largest cases (`L1`, `L4`) were run with longer
budgets: 5400 s for the ladder frontiers and 9000 s for the large cases,
with capacities 2048, 8192, and automatic, plus the guard control on the
large cases.

### Ladder frontiers

`A41` and `A46` are rungs whose automatic capacity had timed out at 300 s.
With longer budgets every tested capacity solves them, so those timeouts
were budget artefacts, not a capacity failure. The times below come from the
8-way overnight batch; they are comparable to each other but not to the
clean sequential tables above.

| rung | world_size | config | cap | cap/size | steps | time (s) | steps/s |
|---|---:|---|---:|---:|---:|---:|---:|
| A41 | 1720 | nogood | 2048 | 1.19 | 59.75M | 559.9 | 107k |
| A41 | 1720 | nogood | 8192 | 4.76 | 46.21M | 592.2 | 78k |
| A41 | 1720 | nogood | auto | 6880 | 4.00 | 47.91M | 567.7 | 84k |
| A46 | 1920 | nogood | 2048 | 1.07 | 140.45M | 1388.0 | 101k |
| A46 | 1920 | nogood | 8192 | 4.27 | 113.81M | 1473.8 | 77k |
| A46 | 1920 | nogood | auto | 7680 | 4.00 | 114.97M | 1440.3 | 80k |

Within the batch, cap 2048 (ratio ~1.1) was the fastest by about 1-6%, and
the automatic capacity was within 4%. The missing entries in the
exploratory sweep were censored at 300 s for the same reason.

### Large cases

Plain reference completions, sequential and clean: `L1` 1.679G steps /
1168 s, `L2` 3.091G steps / 648 s, `L3` 4.559G steps / 838 s, `L4` 2.876G
steps / 921 s.

| case | world_size | config | cap | steps | time (s) | outcome |
|---|---:|---|---:|---:|---:|---|
| L1 | 7314 | plain | . | 1.679G | 1168 (clean) | solved |
| L1 | 7314 | guard | 29256 | 1.679G | 1295 | solved |
| L1 | 7314 | nogood | auto | 29256 | >140M | >8712 | censored |
| L1 | 7314 | nogood | 8192 | 8192 | >185M | >8976 | censored |
| L1 | 7314 | nogood | 2048 | 2048 | >200M | >8955 | censored |
| L4 | 10200 | plain | . | 2.876G | 921 (clean) | solved |
| L4 | 10200 | guard | 40800 | 2.843G | 1059 | solved |
| L4 | 10200 | nogood | auto | 40800 | 58.81M | 2708 | solved |
| L4 | 10200 | nogood | 8192 | 8192 | 144.53M | 4380 | solved |
| L4 | 10200 | nogood | 2048 | 2048 | >305M | >9000 | censored |

Two observations matter for the capacity question:

- On `L4` the automatic capacity was clearly the best fixed capacity
  tested. It solved in 45 min, cap 8192 in 73 min, and cap 2048 did not
  finish in 2.5 h. Retention paid off at this scale: the automatic database
  cut the search to 58.8M steps against 144.5M and >305M for the smaller
  caps, which more than offset its 1.5-1.6x lower throughput. This is the
  opposite of the small-case optima and shows that the optimal working set
  does grow once the search is deep enough.
- On `L1` none of the fixed-capacity configurations finished in 2.5 h,
  while plain finished in 19.5 min. The `L4` result therefore does not
  generalize into a recommendation: the automatic formula is defensible for
  the hardest completed case, but fixed-capacity `--nogood` is still 3x or
  more slower than plain on this class of cases.
- `--nogood-guard` reproduced the plain search almost exactly on both cases
  (`L1`: identical 1.679G steps; `L4`: 1.2% fewer steps) at 11-15% more
  time. On these cases the guard is plain plus a small overhead, not a
  speedup. It remains the only nogood-family configuration that both
  finishes and stays close to plain.

## Findings

1. **The automatic capacity is exercised, not speculative.** Every
   nontrivial case reaches `capacity` and reduces the database, often many
   times per thousand steps. The `E2`-style worry that the formula is
   untested by the benchmark is not supported: it is tested, and the
   canonical cases are already in the eviction regime.
2. **The optimal capacity is a mild function of search depth, not of world
   size.** For the small and medium completed searches (`world_size`
   162-3280, up to hundreds of millions of plain steps), the best absolute
   capacity stays around 1k-4k entries and the best ratio is 0.8-2.7, flat
   or falling; the automatic ratio-4 capacity is within 0-10% of the best
   time there. But the largest completed case (`L4`, `world_size` 10200)
   reverses the direction: the automatic ratio-4 capacity beat ratio-0.8 by
   1.6x in time and ratio-0.2 did not finish at all. The data therefore does
   not support "`4 * world_size` is fitted to small cases and too large for
   large ones"; if anything the formula is on the conservative side, and the
   floor (2048) is well chosen for the small exhaustive case `c7`.
3. **Fixed-capacity `--nogood` is still not competitive on the large
   cases.** `--backjump` alone is already 23x slower than plain on `c4`, and
   adding the database costs more. On `L4` the automatic capacity finished
   2.9x slower than plain; on `L1` no tested fixed capacity finished in 2.5 h
   while plain took 19.5 min. `capped_queries` saturates (0.96-1.00) as the
   database grows, so the completion-query bound limits how much a larger
   capacity can buy.
4. **`--nogood-guard` is a safe fallback, not a speedup.** It restores
   86-92% of plain throughput in bounded snapshots, and on `L1`/`L4` it
   reproduced the plain search almost step-for-step at 11-15% more time.
   This supports treating the guard as the layer that decides *whether* to
   learn, with capacity as a secondary budget.
5. **Memory grows with capacity.** `max_rss` rises roughly linearly with the
   capacity (`c4`: 6.6 MB at 256 entries to 58.7 MB at 65536). The automatic
   capacity for the largest case is 40800 entries, which is tens of MB
   today, but the trend matters for even larger worlds.

## Threats To Validity

- The study is single-machine and mostly single-run. Step counts are
  deterministic, but wall times need the sequential pass; only a subset of
  capacities was re-timed sequentially on the ladder.
- The overnight times come from an 8-way concurrent batch, so they compare
  configurations within the batch but are inflated relative to the clean
  sequential plain reference. The step counts are unaffected.
- Timeouts right-censor the slowest configurations. A censored configuration
  could in principle have the best time-to-solution with a longer budget; the
  overnight runs probe a few such points.
- Capacity interacts with at least two other constants:
  `MAX_QUERY_CANDIDATES` (the `capped` column) and `MAX_NOGOOD_LITERALS`.
  A different query bound could change the optimal capacity.
- On the large cases only capacities 2048, 8192, and the automatic value
  were tried. On `L4` the automatic value was the best of those three; a
  still larger capacity was not ruled out there.
- `c6` is seed-dependent and non-monotonic; it should not be used alone to
  justify a capacity change.

## Reproduction

The canonical protocol is: build the release binary, then for each case run

```sh
target/release/factoriosrc-tui new --no-tui --format json --step N \
    -r RULE W H P ... [--nogood [--nogood-capacity CAP]] [--nogood-guard]
```

parse the last JSON line (or the last line before a `timeout` kill), and
record `elapsed_secs`, `steps`, `world_size`, `nogood_capacity`, and the
`nogood` counters. The step interval must be small enough that a killed run
produces at least one line; 1,000 for the small enumeration cases and
100,000-5,000,000 for the large ones. Step counts are deterministic for a
fixed seed and capacity; wall times should be taken from a sequential run
with the process pinned to one core.

The exploratory driver scripts were kept outside the repository; the case
lists, capacity grids, budgets, stop conditions, and command shape above are
the reproducible specification.