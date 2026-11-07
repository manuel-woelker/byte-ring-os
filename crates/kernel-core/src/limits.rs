//! Explicit resource bounds for the first kernel interaction experiments.

/// Maximum instructions in a default version-one VM program.
///
/// This is an initial experimental budget, not a stable ABI guarantee.
pub const DEFAULT_INSTRUCTION_BUDGET: u32 = 1_024;
