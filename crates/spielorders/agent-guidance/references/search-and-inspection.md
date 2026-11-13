# Search and inspection

```sh
spielorders package show --root "$package"
spielorders package list orders --root "$package"
spielorders package list vendors --root "$package"
spielorders package values orderer --root "$package"
spielorders search orders "laser" --root "$package"
spielorders search vendors "optics" --root "$package"
spielorders search supporting-material "quote" --root "$package"
```

`package show` is the complete canonical Package inspection, not an agent-only snapshot.
Reusable-value kinds are `orderer`, `ordering_institution`, `project`, and `order_group`.
`package values order_group` lists every group name across Projects, a name used under
several counted once; `search orders` with the Project's name shows its Orders' groups.
`order show` targets an Order directory; `vendor show` takes a Vendor Name and Package.

Native record search matches managed facts. Supporting Material search uses the OS
index and groups eligible files by recognized healthy Order; it is not a recursive
content inventory. An empty completed result, unavailable provider, denied access,
failure, and partial result have different meanings. Read the reported state rather
than treating absence from the index as proof that no supporting file exists.
