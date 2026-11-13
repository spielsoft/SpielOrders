# Desktop acceptance

How a desktop milestone is shown to work. These requirements constrain how correctness is
established, not the choice of Swift implementation.

## One pass on the final build

Acceptance runs once, after partner review's findings are settled. Build the Validation
app, install a stable copy, and work through the milestone brief's acceptance checks by
operating it. When a check fails, fix it, rebuild, and repeat the checks that fix could
affect. Acceptance is reached when every check holds on the build being accepted.

A check that was not operated is reported as unverified. Compilation, a component fake,
or a fabricated delegate never stands in for an observation.

The record is the verdict, in the accepting commit's message: what the builder operated,
what the owner operated, and what remains unverified and why. Keep no log of intermediate
builds, executables, or observations. The owner then uses the app, and their sign-off
accepts the milestone.

## Driving the app

Drive the built app through its accessibility interface, in the background, and take
screenshots to check what it looks like. This is the ordinary route: it needs no idle
machine and no window agreed with the owner, so use it while building as well as at
acceptance. The owner working in other apps at the same time does not make such a run
inconclusive.

Every interactive control carries a stable accessible name, and an identifier where
names repeat, in the same commit that introduces the control. That is product surface,
used by VoiceOver and by any automation client, not code that serves tests. Address
controls by name or identifier, not by their position in the accessibility tree, which
shifts whenever the view changes. Identify table rows through their named cell content;
leave the native row containers unchanged.

A few checks need the real pointer, keyboard, or foreground: drag gestures such as
column reordering and resizing, the Dock, and anything XCUITest runs. Batch those, ask
the owner once for a short window, and run them together. A foreground run that the
owner interrupted is inconclusive: rerun it. Never start a logout, restart, or shutdown
without the owner.

XCUITest scenarios rerun at no cost but take the foreground and need Automation Mode,
which the owner enables once with
`sudo automationmodetool enable-automationmode-without-authentication`; until then its
compiled scenarios establish nothing. A test runner's forced termination is not a user
Close or Quit.

Compare presentation with the built artifact at the sizes it ships with: usable window
size, dense tables, the final row, inspector layout. Check focus and commands in the real
app: nested sheets keep parent drafts, Save and Escape target the active editor, and Undo
works at the intended draft scope.

## Thin presentation

Exercise real calls through the [library interface](../../crates/spielorders/include/spielorders.h)
as well as view tests. Money, numeric parsing, status and date behavior, reusable values,
and Vendor resolution come from the library, and original-read guards reach it.

Selection follows stable identity, not a display name that another entity may reuse. A
cancelled, refused, or stale operation keeps the editor's input. Presentation follows
result and entry codes, not message text. A committed result with warnings stays
committed, and the UI does not invite a duplicate retry. An older response never replaces
the current draft or query.

## Application lifecycle

Test the installed application: window Close, menu and keyboard Quit, and Dock Quit.
Closing an active Package returns its window to the welcome view, and Close from the
welcome view quits. Dirty cancellation keeps a usable draft, approval settles owned workers, and a
Close or Quit at startup is not swallowed. Follow
[AppKit's termination protocol](https://developer.apple.com/documentation/appkit/nsapplication/terminate(_:))
when work or a draft defers the decision.

The app logs each Close or Quit request with whether work is active. To show leaving
with work in flight, read that receipt for the process under test; an action that merely
preceded the exit does not show it. A debugger may hold the disposable app at a real call
to reach such a moment. It never replaces code or results, and it is detached afterwards.

## Search and effects

An empty index result, an unavailable provider, denied access, and a query failure are
different outcomes. Stopping keeps the results already gathered. Swift supplies settled
hits and provider facts to the library, which alone decides eligibility, grouping, and
ordering, as [Effect and adapter boundaries](../architecture/boundaries.md#native-services)
defines. A failed Trash handoff leaves the removed Order recoverable. Finder selection
needs a native observation.

## Safety

Use the [conversion guide](first-collection-conversion.md) for isolated archive testing.
Acceptance uses the Validation identity built by the [gate](../../scripts/gate.sh),
and XCUITest uses the development identity. Both keep their sandbox container apart from
production. A custom UserDefaults suite or a changed HOME does not isolate AppKit's own
window state.

Never launch the production identity for acceptance, reset its container, overwrite the
owner's preferences, or copy test metadata into the real archive. Run a stable installed
copy of the Validation bundle and never rebuild a running bundle in place. Produce real
failures only by changing permissions on disposable fixtures, and restore them.
