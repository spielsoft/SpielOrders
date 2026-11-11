# Diagnostics and repair

```sh
spielorders validate --root "$package"
spielorders order validate --root "$directory"
spielorders repair --root "$package"
spielorders order repair --root "$directory"
spielorders index status --root "$package"
spielorders index rebuild --root "$package" --dry-run
```

Validation reports integrity without fixing anything. Package and Order repair each
diagnose their own scope. The result names cases, offered actions, required facts, and
the command to apply one action. Supply that case/action and the explicit resolution;
there is no inferred repair or Fix All. Complete-record replacement needs complete JSON,
not guessed partial salvage. A stale-evidence refusal preserves the current files.

Ordinary mutations refuse damaged or unsupported state; use the returned diagnostics
to select the response. Index status is read-only; rebuild explicitly refreshes the
derived index when the canonical Package permits it. Rebuild is not canonical repair.
Metadata upgrade becomes applicable only when a later format is available; this build
does not invent an upgrade command for a format it cannot upgrade.
