# synthetic_parse_heavy

This benchmark fixture is generated Rust code built to stress parser throughput.

It favors large token volume in type aliases, attributes, doc comments, macro
definitions, macro invocations, and simple declarations. Function-like items are
avoided so the target stays focused on parse/token/syntax measurements rather than
body IR resolution.

Regenerate it from this directory with:

```sh
python3 generate.py
cargo generate-lockfile
```
