# Documentation contents

[Documentation contents](contents.md) is the index for Mornington's
documentation set.

## Product definition and design

- [Roadmap](roadmap.md) sequences GIST delivery slices, dependency gates, and
  deferred scope.

- [Terms of reference](terms-of-reference.md) defines users, goals, scope,
  acceptance criteria, assumptions, and open product questions.
- [Technical design](mornington-design.md) specifies the proposed application,
  behavioural storage contracts, identity, thread map, and local hosting.
- [ADR 001: Behavioural storage ports](adr-001-behavioural-storage-ports.md)
  records the RouchDB direction and the boundary for future adapters.

## Project guides

- [User guide](users-guide.md) explains how to use the generated project and
  its public build and test commands.
- [Developer guide](developers-guide.md) explains the local workflow and
  implementation tooling for contributors.
- [Repository layout](repository-layout.md) explains the generated project's
  top-level files, directories, and ownership boundaries.
- [Documentation style guide](documentation-style-guide.md) defines the
  spelling, structure, Markdown, Architecture Decision Record (ADR), Request
  for Comments (RFC), and roadmap conventions used by this documentation set.
- [Polonius borrow-checker policy](polonius.md) records the nightly compiler
  contract, borrow-centric design rules, and audit tags used by this project.

## Rust reference material

- [Reliable testing in Rust via dependency injection](reliable-testing-in-rust-via-dependency-injection.md)
  explains how to keep tests deterministic by injecting environment, clock,
  filesystem, and other external dependencies.
- [Rust doctest Don't Repeat Yourself guide](rust-doctest-dry-guide.md)
  explains how to write maintainable, executable Rust documentation examples.
- [Rust testing with `rstest` fixtures](rust-testing-with-rstest-fixtures.md)
  explains fixture-based, parameterized, and asynchronous testing with `rstest`.

## Engineering practice

- [Complexity antipatterns and refactoring strategies](complexity-antipatterns-and-refactoring-strategies.md)
  explains cognitive complexity, the bumpy-road antipattern, and refactoring
  approaches for maintainable code.
- [Scripting standards](scripting-standards.md) explains the preferred Python
  scripting stack, command execution patterns, and test expectations for helper
  scripts.
