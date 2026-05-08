# synthetic_body_heavy

This benchmark fixture is generated Rust code built to stress body IR resolution more
than parsing or item collection.

The workspace has three crates:

- `bench_items`: many records, enums, traits, inherent impls, and trait impls.
- `bench_body`: generated workflow functions with many local bindings, method calls,
  trait method calls, and repeated type propagation.
- `bench_app`: a tiny binary/library crate that depends on both generated crates.

Regenerate it from this directory with:

```sh
python3 generate.py
cargo generate-lockfile
```

The generated source files intentionally favor repeatable compiler-shaped workloads over
idiomatic application code.
