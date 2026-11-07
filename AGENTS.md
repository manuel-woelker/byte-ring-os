# Repository guidance

## Code style

Follow [the code-style guide](<docs/Code style.md>). Prioritize readability,
good documentation, and simple designs. `lib.rs` and `mod.rs` must contain only
module declarations; put code and other definitions in named modules.

## Verification

Always run `./n check` after completing each unit of work, including fixes and
follow-up changes. Resolve failures caused by your changes before reporting the
work complete. If an existing issue prevents the checks from passing, report
the failing task and the blocker explicitly.

Run development tools through `./t` so they use the pinned toolchain.

## Commits

Commit after every completed unit of work, including fixes and follow-up
changes, once the required checks pass. Use a descriptive commit message and
include only changes belonging to that unit. Do not include unrelated user
changes or commit work with failing checks; report any blocker explicitly.

## Architecture and planning

Read [architecture](docs/Architecture.md) before changing kernel boundaries.
Keep architecture documentation accurate about implemented and proposed
behavior. Follow [planning conventions](docs/plans/README.md) for multi-step
work and [DR-000](<docs/decisions/DR-000 Record decisions in the repository.md>)
for significant choices. Keep portable logic testable on the host and unsafe
hardware code isolated. Never treat VM validation as sufficient authority or
assume shared memory stays unchanged after validation.

Crate-level attributes such as `#![no_std]` are permitted in crate roots; all
other definitions still belong in named modules. The freestanding binary uses
`main.rs` for its entry and panic handler.
