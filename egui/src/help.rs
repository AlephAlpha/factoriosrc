pub const CONFIG_PANEL_TOOLTIP: &str = "Core search configuration and runtime options.";
pub const COPY_RLE_TOOLTIP: &str = "Copy the visible RLE text.";
pub const CONFIG_TOGGLE_TOOLTIP: &str =
    "Show or hide the configuration sidebar while viewing results.";
pub const DETAILS_TOGGLE_TOOLTIP: &str = "Show or hide the details sidebar.";
pub const HELP_TOOLTIP: &str = "Open the help window.";
pub const LIVE_VIEW_TOOLTIP: &str = "Switch the workspace back to the live search snapshot.";
pub const GENERATION_TOOLTIP: &str =
    "Select which generation is shown for the current result source.";
pub const HISTORY_TOGGLE_TOOLTIP: &str = "Show or hide the stored-solution history sidebar.";
pub const LIVE_HISTORY_TOOLTIP: &str = "Return to the live search snapshot.";
pub const HISTORY_ENTRY_TOOLTIP: &str = "View this stored solution in the workspace.";
pub const KNOWN_CELLS_EDIT_TOOLTIP: &str =
    "Open the known-cells editor for per-generation cell constraints.";
pub const KNOWN_CELLS_CLEAR_GEN_TOOLTIP: &str = "Remove known cells from the current generation.";
pub const KNOWN_CELLS_CLEAR_ALL_TOOLTIP: &str = "Remove all known cells from the editor.";
pub const KNOWN_CELLS_APPLY_TOOLTIP: &str =
    "Save the working known cells back to the configuration.";
pub const KNOWN_CELLS_CANCEL_TOOLTIP: &str = "Discard known-cells edits and close the editor.";
pub const STATUS_TOOLTIP: &str = "Current search status.";
pub const SOLUTIONS_TOOLTIP: &str = "The number of stored solutions found so far.";
pub const POPULATION_TOOLTIP: &str = "Population on the currently displayed generation.";
pub const ELAPSED_TOOLTIP: &str = "Elapsed search time.";
pub const CHECKED_TOOLTIP: &str = "The number of state assignments made by the search so far.";

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

pub const WORKSPACE_ACTIONS: [(&str, &str); 6] = [
    ("Copy RLE", COPY_RLE_TOOLTIP),
    ("Config", CONFIG_TOGGLE_TOOLTIP),
    ("Details", DETAILS_TOGGLE_TOOLTIP),
    ("History", HISTORY_TOGGLE_TOOLTIP),
    ("Live", LIVE_VIEW_TOOLTIP),
    ("Generation", GENERATION_TOOLTIP),
];

pub const KNOWN_CELLS_ACTIONS: [(&str, &str); 5] = [
    ("Edit known cells", KNOWN_CELLS_EDIT_TOOLTIP),
    ("Clear Gen", KNOWN_CELLS_CLEAR_GEN_TOOLTIP),
    ("Clear All", KNOWN_CELLS_CLEAR_ALL_TOOLTIP),
    ("Apply", KNOWN_CELLS_APPLY_TOOLTIP),
    ("Cancel", KNOWN_CELLS_CANCEL_TOOLTIP),
];

pub const CONFIG_NOTES: [(&str, &str); 4] = [
    (
        "Short help",
        "Hover config labels for concise field summaries.",
    ),
    (
        "Detailed help",
        "Use the field reference below for longer config descriptions sourced from factoriosrc-lib.",
    ),
    (
        "Auto values",
        "Search order, diagonal width, and translation constraints still follow factoriosrc-lib rules.",
    ),
    (
        "Validation",
        "New validates the configuration before a search session is created.",
    ),
];
