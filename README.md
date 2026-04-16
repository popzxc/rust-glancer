# rust-glimpser

Incomplete-by-design LSP that trades completeness for speed and memory.
`rust-analyzer` is great, but it is just too heavy.

This project aims to get you 70% there, with most of low-hanging fruits supported,
but not more.

Do not use it if:
- You fully rely on LSP and won't be comfortable without one.
- You work on complex projects with heavy trait solving and bleeding edge features.

Use it if:
- `rust-analyzer` too slow/heavy for you
- You don't mind to grep stuff from time to time.

The core principle is that we don't try to do anything smart:
- no build script running
- no proc macro expanding
- no trait solving

We check the sources and try to get stuff we can easily collect, but not more.

## License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
