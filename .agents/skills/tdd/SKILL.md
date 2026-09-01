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

## Anti-Pattern: Horizontal Slices

**DO NOT write all tests first, then all implementation.** This is "horizontal slicing" - treating RED as "write all tests" and GREEN as "write all code."

This produces **crap tests**:

- Tests written in bulk test _imagined_ behavior, not _actual_ behavior
- You end up testing the _shape_ of things (data structures, function signatures) rather than user-facing behavior
- Tests become insensitive to real changes - they pass when behavior breaks, fail when behavior is fine
- You outrun your headlights, committing to test structure before understanding the implementation

**Correct approach**: Vertical slices via tracer bullets. One test → one implementation → repeat. Each test responds to what you learned from the previous cycle. Because you just wrote the code, you know exactly what behavior matters and how to verify it.

```
WRONG (horizontal):
  RED:   test1, test2, test3, test4, test5
  GREEN: impl1, impl2, impl3, impl4, impl5

RIGHT (vertical):
  RED→GREEN: test1→impl1
  RED→GREEN: test2→impl2
  RED→GREEN: test3→impl3
  ...
```

## Workflow

### 1. Planning

Before writing any code:

- [ ] Establish the approved public interface and prioritized behaviors from the
      ticket/spec and its linked decisions
- [ ] Identify opportunities for [deep modules](deep-modules.md) (small interface, deep implementation)
- [ ] Design interfaces for [testability](interface-design.md)
- [ ] List the behaviors to test (not implementation steps)
- [ ] For each proposed test, state the running-system failure it would catch
- [ ] Ask the user only if a material interface or product choice is still open

An approved implementation ticket is planning approval. Do not repeat its design
interview in a fresh agent. Read additional decisions only when the selected
behavior is ambiguous.

**You can't test everything.** Follow the approved ticket's priorities and focus
testing effort on critical paths and complex logic, not every possible edge case.

### 2. Tracer Bullet

Write ONE test that confirms ONE thing about the system:

```
RED:   Write test for first behavior → test fails
GREEN: Write minimal code to pass → test passes
```

This is your tracer bullet - proves the path works end-to-end.

### 3. Incremental Loop

For each remaining behavior:

```
RED:   Write next test → fails
GREEN: Minimal code to pass → passes
```

Rules:

- One test at a time
- Only enough code to pass current test
- Don't anticipate future tests
- Keep tests focused on observable behavior

### 4. Hand Off to Code Review

After all planned behavior is GREEN, stop the TDD loop and hand the complete ticket
diff to `/code-review`, run in its parallel sub-agent form. Refactoring and structural
cleanup belong to that explicit review stage rather than being mixed into red-green
implementation.

- [ ] Run the focused tests for the behavior under review
- [ ] Let `/implement` run the affected repository gate once after the ticket is stable
- [ ] Invoke `/code-review` against the ticket's fixed starting point, parallel axes
- [ ] Address accepted correctness, specification, and maintainability findings
- [ ] Rerun focused and full tests after review-driven changes

Do not skip remediation merely because review is a separate stage. The ticket is
complete only after review findings are resolved or explicitly dispositioned.

## Checklist Per Cycle

```
[ ] Test describes behavior, not implementation
[ ] Test uses public interface only
[ ] Test would survive internal refactor
[ ] Test would fail for the real bug, not merely for a changed implementation shape
[ ] Code is minimal for this test
[ ] No speculative features added
```
