use factoriosrc_lib::Status;
use std::time::Duration;

/// A single generation rendered in a UI-neutral format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationSnapshot {
    /// The generation index.
    pub generation: i32,
    /// The population on that generation.
    pub population: usize,
    /// The RLE text shown to the user.
    pub rle: String,
}

/// A search snapshot that frontends can render however they like.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchSnapshot {
    /// Search status.
    pub status: Status,
    /// Whether the search is currently running.
    pub running: bool,
    /// Time elapsed since the search started.
    pub elapsed: Duration,
    /// Renderable generations of the current world.
    pub generations: Vec<GenerationSnapshot>,
    /// A proxy metric for search progress.
    pub cells_checked: usize,
}

impl SearchSnapshot {
    /// Number of available generations.
    pub fn generation_count(&self) -> usize {
        self.generations.len()
    }

    /// Return the generation at the given index, clamped to the available range.
    pub fn generation(&self, generation: i32) -> Option<&GenerationSnapshot> {
        if self.generations.is_empty() {
            return None;
        }

        let index = generation.max(0) as usize;
        self.generations
            .get(index.min(self.generations.len().saturating_sub(1)))
    }

    /// Return the generation with the smallest population.
    pub fn smallest_population(&self) -> Option<&GenerationSnapshot> {
        self.generations
            .iter()
            .min_by_key(|generation| generation.population)
    }
}
