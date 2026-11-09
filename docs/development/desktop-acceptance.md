# Desktop acceptance

These requirements carry useful validation lessons into the native desktop. They
constrain how to establish correctness, not the choice of Swift implementation.

## Thin presentation

Exercise real calls through the [library interface](../../crates/spielorders/include/spielorders.h)
as well as view tests. Verify that original-read
guards reach core; money, numeric parsing, status/date behavior, reusable values, and
Vendor resolution are not recomputed in the UI.

Preserve stable identity when external edits rename a selected Vendor. Resending an
unchanged display name can accidentally select a different Vendor if that name is reused.
Use only intended changes and the original evidence. A cancelled, refused, or stale
operation retains the editor's input; presentation follows result and entry codes,
not an interpretation of message text.

A new Order's first Save publishes its complete draft, including Items. A refusal keeps
all draft data. Editing preserves numeric precision; formatted monetary text is not a
replacement for the stored value. A committed result with refresh warnings remains
committed; the UI must not encourage a duplicate retry. Older previews or search
responses cannot replace the current draft or query.
Observe the Order README editor's editable committed-with-warnings state natively.

## Native interaction

Drive the exact built app through real UI actions using computer-use automation or
XCUITest; a test runner's forced termination is not evidence of user Close or Quit.
Keep an unavailable driver explicitly unverified rather than treating compilation as a
successful interaction run. XCUITest cannot enable automation mode on the owner's
machine, so its compiled scenarios establish no run. Add feature scenarios as the
corresponding surfaces become available, including sheets, scrolling, and native
handoffs. A component fake or fabricated
delegate is not native evidence.

Record automation permissions and unlocked-session prerequisites. An unavailable
automation route remains unverified; permission setup does not imply every system
interaction is automatable. Keep owner-operated system checks distinct.

Run foreground UI automation only during an owner-approved exclusive desktop window.
While the owner works, use headless checks. Concurrent owner interaction makes a UI
run inconclusive; rerun it in the coordinated window rather than treating it as a
product failure or pass.

Check focus and commands with the real app: nested sheets retain parent drafts, Save and
Escape target the active editor, and Item deletion cannot remove an Order. Native Undo
must operate at the intended draft scope.

Compare concrete presentation requirements with the built artifact, including usable
window size, dense tables, actual viewport bounds, the final row, and inspector layout.
Do not prove scrolling with a viewport the shipped view never uses.

## Application lifecycle

Test the installed application path, not a callback attached to a fabricated delegate.
Exercise window Close, menu/keyboard Quit, and Dock Quit; dirty cancellation retains a
usable draft, approval settles owned workers, and startup-time Close or Quit is not
silently swallowed.

An in-flight exit observation establishes that work is active when AppKit receives the
termination request, not merely before the test clicks Close or Quit. The app logs each
termination request with whether work is active; that receipt is validation evidence any
observer may read, not a test hook. Correlate the tested process's receipt with that
action. Missing receipts or work that
settles before receipt leave the route unverified, not passed.

Use native termination semantics. Cancelling an OS termination request is a refusal to
log out, not a general-purpose asynchronous pause. Follow
[AppKit's termination protocol](https://developer.apple.com/documentation/appkit/nsapplication/terminate(_:))
when work or draft confirmation defers a decision.

Clean logout is a sufficient representative owner-run session-termination check; do not
claim restart or shutdown was directly observed. Never initiate disruptive session
tests without explicit owner coordination.

## Search and effects

An empty index result, unavailable provider, denied access, and query failure are
different outcomes. Exercise real cancellation and process settlement, including a
slow provider that remains active until completion or Stop under
[Effect and adapter boundaries](../architecture/boundaries.md#native-services).

Verify that Swift's native query supplies settled hits and provider facts to the
library rather than reproducing eligibility, grouping, or ordering. Verify native
Trash after a successful recoverable removal, including a failed Trash handoff that
leaves the removed Order accessible.

Preserve complete results already gathered when stopping. Where a provider uses a
subprocess, a child or descendant holding a pipe open must not prevent bounded return.
Stale paths receive actionable
feedback; actual Finder selection requires native observation.

## Evidence and safety

Use the [conversion guide](first-collection-conversion.md) for isolated archive testing.
Use the separate validation application identity built by the
[native checks](../../scripts/native-checks.sh) for Release acceptance, and the
development identity for XCUITest. Both isolate the sandbox container from production.
A custom UserDefaults suite isolates application-owned keys, not AppKit's panel or
window state; changing HOME does not establish isolation either. Never launch the
production identity for acceptance, reset its container, overwrite owner preferences,
or copy test metadata into the production archive.

For direct native acceptance, run a stable installed copy of the Validation bundle,
outside a worker-private temporary build tree. Never rebuild the running bundle in
place. Gate builds run separately without replacing the observed executable; system
presentation checks use the normal installed-artifact path.

Keep acceptance concise and current. Separate automated proof, direct native observation,
and owner reports. Tie native claims to the built executable; a changed route needs fresh
evidence. Keep only evidence necessary to establish the current acceptance verdict and
durable lessons, not transient log locations or repeated command transcripts.
To observe an in-flight route, a debugger may hold the disposable app at a real call;
record those observations as debugger-assisted, never replace code or results, and detach
afterwards. Produce real failures by changing permissions on disposable fixtures only,
and restore them. Native observations name the source revision and the hash of the
executable actually operated. Release builds are not byte-reproducible: equal source revisions alone do
not establish that two executables are the same observed build.
