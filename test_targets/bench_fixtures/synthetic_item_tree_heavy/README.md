# synthetic_item_tree_heavy

This benchmark fixture is generated Rust code built to stress item-tree lowering.

It contains many files with structs, enums, traits, impl blocks, associated items,
type aliases, consts, statics, and signatures. Function bodies are deliberately tiny,
so the useful signal is mostly item collection and lowering rather than body analysis.

Regenerate it from this directory with:

```sh
python3 generate.py
cargo generate-lockfile
```
