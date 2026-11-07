# Code style

Write code that is easy to read, explain, and change. Prefer a simple design
with clear behavior over a clever design with hidden assumptions.
Code is read and modified repeatedly; reducing the effort to understand it
usually matters more than saving a few lines when writing it.

## Readability

- Use names that describe the domain and the purpose of a value or operation.
  Avoid abbreviations unless they are established vocabulary. Clear names let
  readers understand intent without reconstructing it from usage.
- Keep functions focused on one responsibility. Make control flow and ownership
  visible; use early returns when they simplify the main path.
  Focused functions make dependencies, side effects, and failure paths easier
  to inspect and test.
- Prefer straightforward loops and matches when chained expressions obscure
  the steps of an operation. Use idiomatic Rust without making concision a goal.
  Readers should be able to follow execution order without unpacking several
  transformations mentally.
- Keep related behavior together. Split large modules by responsibility, rather
  than creating layers or tiny files with no independent purpose.
  Cohesion reduces navigation between files and keeps changes to one concern
  localized.
- Make invariants explicit in types and constructors where practical. Avoid
  duplicated state that can disagree and implicit ordering dependencies.
  Enforced invariants prevent invalid states; fewer representations mean fewer
  synchronization rules to remember.
- Follow rustfmt. Spend review attention on behavior and structure rather than
  manual formatting choices.
  A shared formatter produces consistent code and avoids style-only debates.

## Simple designs

- Start with the smallest correct solution that meets the current requirements.
  Add abstractions when they clarify a real responsibility or remove meaningful
  duplication.
  Each abstraction creates a concept to learn and maintain. Require a concrete
  benefit before adding that cost.
- Prefer concrete types, explicit data flow, and ordinary Rust definitions.
  Introduce traits, generics, macros, or registries when their benefit justifies
  the additional concepts callers and maintainers must understand.
  Explicit behavior is easier to trace, debug, and change than behavior hidden
  behind several dispatch or generation layers.
- Avoid speculative extensibility, unused configuration, and frameworks built
  around a single use case. Do not build capabilities that cannot be exercised.
  Unused flexibility adds branches and assumptions without evidence that they
  solve the eventual problem.
- Prefer safe Rust. Unsafe code requires a concrete need, documented safety
  invariants, and tests of the relevant boundaries.
  Safe Rust delegates memory and borrowing checks to the compiler. Unsafe code
  transfers those proof obligations to every future maintainer.
- Optimize where measurements or a clear workload justify it. Explain the
  tradeoff when an optimization makes code less readable.
  Measurements help ensure that added complexity improves the workload that
  matters rather than an imagined bottleneck.
- Keep error paths understandable. Use the shared error and result vocabulary,
  provide useful context, and avoid panics for expected input or runtime errors.
  Document unavoidable limits and intentional panic conditions.
  Expected failures should be recoverable and diagnosable. Explicit panic
  contracts let callers understand when normal error handling is insufficient.

## Module layout

`lib.rs` and `mod.rs` files contain only module declarations and required
crate-level attributes such as `#![no_std]`. Put actual
code and all other definitions in named modules.

These files provide a predictable map of the module hierarchy. Keeping
implementation out of them makes it easy to find the module responsible for
a feature and prevents crate roots from accumulating unrelated definitions.

```rust
pub mod arena;
pub mod error;
mod internal;
```

Do not put functions, structs, enums, traits, implementations, constants,
statics, type aliases, macro definitions, imports, re-exports, or test bodies
in `lib.rs` or `mod.rs`. Public modules expose their definitions through their
module paths. Keep tests beside the implementation in a named module or in
the crate's `tests/` directory.

Using public module paths makes a definition's location visible at its call
sites. This trades shorter imports for explicit organization and avoids a
second API surface maintained through re-exports.

Module names should describe the responsibility they contain. Avoid a generic
`utils` module when a domain-specific name makes the contents easier to find.
Specific names establish a boundary; a catch-all name makes unrelated additions
easy and their eventual organization harder.

## Documentation

- Document public APIs and meaningful internal contracts. Explain purpose,
  ownership, lifetimes, invariants, failure behavior, and limits when relevant.
  Callers need the contract to use an API correctly without reading its
  implementation. Internal contracts preserve assumptions during refactoring.
- Include small, executable examples for APIs whose usage is not obvious.
  Show error and borrowing behavior when those are part of the contract.
  Examples make the intended usage concrete, and executable examples catch
  documentation that no longer matches the API.
- Explain why a non-obvious choice exists. Do not add comments that simply
  repeat the code or narrate routine operations.
  Reasons survive changes to implementation details; redundant narration adds
  noise and another place that can become outdated.
- Keep documentation accurate when behavior changes. Distinguish implemented
  capabilities from possible future work.
  Incorrect documentation creates false expectations and can be more misleading
  than an explicit statement of a limitation.
- Record significant choices and rejected alternatives in
  [decision records](decisions/), following
  [DR-000](<decisions/DR-000 Record decisions in the repository.md>). Link to
  those records instead of duplicating their rationale in several places. A
  single historical account makes tradeoffs discoverable and avoids conflicting
  explanations as the design evolves.

## Verification

Test observable behavior and important edge cases: invalid inputs, error
propagation, ownership, stale handles, ordering, and boundaries as applicable.
Prefer tests that demonstrate the contract over tests that mirror each line
of the implementation. Use documentation tests for examples and compile-fail
tests when a compile-time restriction is an important API guarantee.

Behavior tests protect what callers rely on while allowing implementation
changes. Boundary and error tests catch cases that happy-path examples miss;
compile-fail tests verify guarantees that runtime tests cannot demonstrate.

Always run `./n check` after completing a unit of work. Fix failures introduced
by the change, and explicitly report existing blockers. Use `./t` to invoke
the pinned development tools and nextest to run ordinary Rust tests.

The repository check combines formatting, compilation, linting, and tests so
local success on one task does not hide failures elsewhere. Pinned tools keep
results reproducible and avoid mixing incompatible compiler artifacts. Nextest
provides the same ordinary test runner for local development and CI.
