pub const SEARCH_ACTIONS: [(&str, &str); 7] = [
    (
        "New",
        "Validate the current config and create a search session.",
    ),
    ("Load", "Resume a saved session from disk."),
    ("Start/Resume", "Run the current session."),
    ("Next", "Continue after a pause or a solution."),
    ("Pause", "Stop after the current step batch."),
    ("Save", "Write the current search state to disk."),
    ("Stop", "Discard the session and return to setup mode."),
];

pub const RESULT_NOTES: [(&str, &str); 4] = [
    (
        "RLE view",
        "Shows the selected generation with '.' for dead, 'o' for alive, and '?' for unknown.",
    ),
    (
        "Copy RLE",
        "Copies the visible generation so it can be pasted into Golly or another Life tool.",
    ),
    (
        "Generation",
        "The slider selects which generation is shown in the result view.",
    ),
    (
        "Metrics",
        "Population, solutions, elapsed time, and cells checked update from search snapshots.",
    ),
];

pub const CONFIG_NOTES: [(&str, &str); 4] = [
    (
        "Field docs",
        "Hover a field label to see the lib-level description.",
    ),
    (
        "Validation",
        "New validates the configuration before a search session is created.",
    ),
    (
        "Auto values",
        "Search order, diagonal width, and translation constraints still follow factoriosrc-lib rules.",
    ),
    (
        "Known cells",
        "Open the known-cells editor from the Known Cells section to pin alive and dead cells per generation.",
    ),
];
