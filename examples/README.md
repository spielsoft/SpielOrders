# Demo Package

`Demo Workshop.splord` is a usable SpielOrder Package. Its people, organizations,
Vendors, products, catalog references, events, and documents are invented.
Contact details use placeholder `.example` domains. It contains no real
purchasing or personal data.

Copy the entire directory, including its hidden `.spielorders` directories, to a
disposable location before opening it. Keep the checked-in Package as the reusable
starting point. With the standalone CLI on PATH, from the repository root:

```sh
cp -R "examples/Demo Workshop.splord" "/tmp/SpielOrders walkthrough.splord"
spielorders index rebuild --root "/tmp/SpielOrders walkthrough.splord"
spielorders validate --root "/tmp/SpielOrders walkthrough.splord"
```

The Package Index is derived and ignored by Git; rebuild it after every fresh
checkout or copy, as described in [Restoring a Package](../README.md#restoring-a-package).
