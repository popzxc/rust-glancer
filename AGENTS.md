## Use `impl` blocks for scoping where it makes sense

When adding functions that operate on structs/enums, prefer adding them as methods rather than pure functions.
Even if function is not explicitly related to a struct/enum, but it only exists as a helper for it, prefer adding it as a static method -- it helps with logical grouping. Pure functions should be relatively rare, and they typically represent either big chunks of isolated business logic, or shared general-purpose helpers.
Bad:
```
fn build_item(val_a: u8, val_b: u16) -> Item {
    let item_rank = item_rank(val_a, val_b);
    Item { item_rank }
}
fn item_rank(val_a: u8, val_b: u16) -> u16 { .. } 
```
Good:
```
impl Item {
    fn build(val_a: u8, val_b: u16) -> Self { 
        let item_rank = Self::item_rank(val_a, val_b);
        Self { item_rank }
    }
    fn rank(val_a: u8, val_b: u16) -> u16 { .. }
}
```

## Avoid single-use helpers

Instead of introducing single-use helpers, prefer embedding functionality as a block with comment.
Bad (if only used once):
```
fn collapse_whitespace(text: String) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
```
Good:
```
// Ensure that all the whitespaces are normal " ".
text = text.split_whitespace().collect::<Vec<_>>().join(" ")
```

## Paths

For `cargo_metadata` items, always use fully qualified paths.

This project defines a lot of similarly looking names, so include the module path when you refer to something,
e.g. `def_map::Package` instead of `Package`.

## State of project

This software is heavily WIP, we don't care about backward compatibility.
It is not yet in production, so we must optimize for the code quality right now rather than legacy compatibility.
