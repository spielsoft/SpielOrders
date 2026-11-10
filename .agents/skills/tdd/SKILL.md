---
name: tdd
description: Test-driven development with red-green vertical slices. Use when the user wants to build features or fix bugs test-first, wants integration tests, or asks for red-green development.
metadata:
  uuid: "e4a7f683-dd98-420e-bd58-3e8dc0f9f4d8"
---

# Test-Driven Development

## Philosophy

**Core principle**: Tests should verify behavior through public interfaces, not implementation details. Code can change entirely; tests shouldn't.

**Good tests** are integration-style: they exercise real code paths through public APIs. They describe _what_ the system does, not _how_ it does it. A good test reads like a specification - "user can checkout with valid cart" tells you exactly what capability exists. These tests survive refactors because they don't care about internal structure.

**Bad tests** are coupled to implementation. They mock internal collaborators, test private methods, or verify through external means (like querying a database directly instead of using the interface). The warning sign: your test breaks when you refactor, but behavior hasn't changed. If you rename an internal function and tests fail, those tests were testing implementation, not behavior.

**Behavior first is mandatory**: before writing or accepting a test, identify the real defect it would catch in the running system. If a test only proves that a particular helper was called, a module was imported in a sequence, or mocks received messages in a chosen order, it is not sufficient unless that choreography is itself the public contract. Prefer a smaller behavioral reproduction over a larger mock-based interface test whenever the behavior can be exercised.

See [tests.md](tests.md) for examples and [mocking.md](mocking.md) for mocking guidelines.

## Vertical slices

Build one behavior at a time, end to end: a test for it, the code that makes it pass, then
the next behavior. Each test then responds to what the last cycle taught you. Writing
every test first and all the code afterwards tests imagined behavior: the shape of data
structures and signatures, not what the system does.

How strictly you alternate is your call. What matters is that each test was seen to fail
for the right reason before the code made it pass, because that RED run is the only
evidence the test can fail.

## Deciding what to test

- Take the public interface and the prioritized behaviors from the approved spec and its
  decisions. An approved spec is planning approval; ask only if an interface or product
  choice is still open.
- Prefer [deep modules](deep-modules.md) and [interfaces designed for testability](interface-design.md).
- For each test, be able to state the running-system failure it would catch.
- Test the acceptance criteria and the failure paths a user can reach. Add a test for an
  edge case nobody named only when you can state the realistic input that reaches it. A
  few good tests beat many exhaustive ones.
- A test of an exclusion or a refusal exercises every entry the surface offers to reach
  it: each option placement a command line allows, each route to the same failure. A test
  that passes from one placement has certified one placement.

## When the behavior is green

Run the full checks, then hand the complete diff to review as the repository's
`AGENTS.md` describes. Restructuring belongs after green, with the tests holding
behavior still. Use [refactoring](refactoring.md) for that pass.

## What a good test looks like

- It describes behavior, not implementation.
- It uses a public interface only.
- It would survive an internal refactor.
- It would fail for the real bug, not merely for a changed implementation shape.
