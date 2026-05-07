# synthetic_def_map_heavy

This benchmark fixture is generated Rust code built to stress def-map import and scope
resolution.

It uses a small workspace with a base crate, a facade crate, and an app crate. The generated
modules contain dense re-export trees, glob imports, aliases, nested modules, and cross-crate
paths. Bodies are tiny; the important work is module/import graph construction.

Regenerate it from this directory with:

```sh
python3 generate.py
cargo generate-lockfile
```
