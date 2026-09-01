# SpielOrders agent instructions

## Dependency and toolchain currency

- Keep every toolchain, runtime, library, and other dependency on the newest
  available minor and patch release within its currently selected major version.
  Apply those updates by default without waiting for separate approval.
- Ask the user before adopting a new major version because that may change public
  contracts, compatibility, or migration requirements.
- Never use “do not update” as the silent fallback. If an in-major update is blocked,
  report the blocker and the newest version that could be verified instead.

## Subagents for review

- The project owner has authorized subagents for review work in this repository. A
  project-local skill that specifies parallel sub-agents — the `code-review` skill's two
  independent Standards and Spec axes are the case that matters — is a standing request
  from the owner. Run it in its designed parallel form.
- Do not downgrade a skill's designed mechanism to a single-reader pass on the strength
  of a general "only use subagents when asked" default. That default is satisfied here.
- This also covers the validating session's own review and verification work, not only
  the milestone gate.
- If a mechanism a skill depends on is genuinely unavailable, that blocks the work and
  is reported as a blocker. It is never silently substituted with a weaker form.
- Spawn the two axes as two sub-agent calls in one message and run them in the
  foreground. Do not end your turn while either is outstanding. A sub-agent's children
  do not survive its turn ending, so backgrounding the axes and ending the turn to wait
  for them kills them unreported: the review never runs and nothing says so.
- Write findings down as they are produced, not only when the review concludes. Findings
  held solely in a reviewing agent's context are lost with that agent.

**Why this is written down.** M3 closed with a milestone-wide review that found `0`
blocking findings on its Standards axis. An external adversarial audit of the same range
then found `9`, including paths that can corrupt canonical Package state. The gate had
run both axes as one reader because it read the general default as forbidding
sub-agents, and disclosed the deviation without treating it as blocking. Per-ticket
review had already been retired in exchange for that milestone review, so the one
remaining safeguard ran at a fraction of its designed strength and nobody knew until an
outside reader looked.

**Why the mechanics are written down too.** Closing M3 cost six worker sessions for four
tickets. Four of them lost their review to the turn-ending fault above — one lost a
completed Spec axis whose findings were never recorded anywhere and had to be run again.
The axes were being invoked correctly in substance and still produced nothing, which is
the same shape of failure as the single-reader pass: a safeguard that appears to have run.
See [Driving agent work in one shared worktree](docs/development/agent-execution.md).
