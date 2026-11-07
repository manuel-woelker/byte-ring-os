# DR-000: Record decisions in the repository

- Status: Accepted
- Date: 2026-11-07

## Decision

Record significant technical and product decisions as Markdown files in
`docs/decisions/`. Each record explains the decision, its rationale and context,
its consequences, and the alternatives considered with reasons for rejecting
them.

Always include the decision title in the filename, using the format
`DR-NNN Decision title.md`. Use sequential identifiers starting with `DR-000`,
followed by `DR-001`, `DR-002`, and so on. Capitalize the first letter of the
title and use spaces between words, not dashes, for example
`DR-001 Use a custom reflection mechanism.md`. The document title includes the
same identifier and descriptive decision title. Assign the next unused number
when adding a record.

Every record must contain:

- **Status and date:** Whether the decision is Proposed, Accepted, Rejected,
  or Superseded, and the date it reached that status.
- **Decision:** The concrete choice and its scope. State what we will do.
- **Rationale:** Why this choice meets the needs and constraints better than
  the alternatives. Include the tradeoffs that drove the choice.
- **Context:** The problem, relevant requirements, constraints, and assumptions
  known when the decision was made.
- **Consequences:** Expected benefits, costs, limitations, risks, and follow-up
  work resulting from the choice.
- **Considered alternatives:** Plausible alternatives, including keeping the
  current approach when relevant, and a specific reason each was rejected.

Create a record when a choice materially affects architecture, public APIs,
project conventions, or product behavior, especially when reversing it would
be costly or its reasoning would otherwise be easy to lose. Routine changes
do not require a decision record.

Review records alongside the implementation through the normal repository
review process. A proposal becomes Accepted when the choice is agreed upon;
writing a proposal alone does not establish acceptance. Link relevant records
from implementation discussions and related documentation.

Keep accepted records as historical accounts. Correct typos and clarify wording
without changing their meaning. When a decision changes, create a new record,
mark the earlier one Superseded, update its status date, and link the records
in both directions. Do not renumber or reuse identifiers, including those of
rejected proposals.

## Rationale

Keeping decisions beside the code makes their reasoning available during
development and review. A consistent format makes choices and tradeoffs easy
to find without requiring a separate documentation system.

Explicit alternatives help future contributors distinguish a deliberate
tradeoff from an oversight. Preserving superseded records explains how and why
the project evolved.

## Context

The project is establishing its architecture and development conventions.
Decisions currently emerge through implementation and discussion, but those
conversations are not a durable or easily discoverable reference for future
contributors.

We need a lightweight way to retain reasoning, assumptions, and rejected
options without documenting every implementation detail or introducing new
tooling. This record establishes that convention and follows its own format.

## Consequences

- Contributors can find decisions and their reasoning in version control.
- Decisions can be reviewed with the changes they guide.
- Authors must spend time documenting meaningful choices and alternatives.
- Records capture assumptions at a point in time; readers must check their
  status and linked successors before treating them as current guidance.
- Sequential identifiers may conflict across concurrent branches. Resolve
  collisions before merging and update affected links.
- No generator or validation tool is required. Consistency depends on review.

## Considered alternatives

### Leave decisions in conversations and commit messages

Rejected because the rationale becomes scattered and difficult to discover.
These sources remain useful supporting references, but do not reliably capture
context, consequences, or rejected alternatives in one place.

### Keep decisions in an external wiki or document service

Rejected because it adds a separate location and access dependency, and makes
it harder to review decisions alongside code or recover their historical state.

### Use only comments and general project documentation

Rejected because those documents primarily describe current behavior. They
provide no consistent place to explain rejected options or preserve the
reasoning behind superseded choices.

### Require a record for every change

Rejected because routine edits would create administrative overhead and obscure
the significant decisions this collection is intended to preserve.

### Adopt a dedicated decision-management tool

Rejected because Markdown and repository review meet the current needs with
less setup and maintenance. Reconsider tooling if the volume of records makes
navigation or consistency difficult.
