from pathlib import Path


ROOT = Path(__file__).resolve().parent
GROUP_COUNT = 18
ITEMS_PER_GROUP = 28
FACADE_MODULES = 18
APP_MODULES = 12


def main() -> None:
    write_workspace()
    write_base_crate()
    write_facade_crate()
    write_app_crate()


def write_workspace() -> None:
    write(
        ROOT / "Cargo.toml",
        """[workspace]
members = ["crates/app", "crates/base", "crates/facade"]
resolver = "3"
""",
    )


def write_base_crate() -> None:
    crate = ROOT / "crates/base"
    write(
        crate / "Cargo.toml",
        """[package]
name = "bench_def_base"
version = "0.1.0"
edition = "2024"
""",
    )

    modules = "\n".join(f"pub mod group_{group:02};" for group in range(GROUP_COUNT))
    prelude = "\n".join(f"pub use crate::group_{group:02}::*;" for group in range(GROUP_COUNT))
    write(
        crate / "src/lib.rs",
        generated_header()
        + f"""#![allow(dead_code)]
#![allow(ambiguous_glob_reexports)]
#![allow(non_camel_case_types)]

{modules}

pub mod prelude {{
{indent(prelude)}
}}
""",
    )

    for group in range(GROUP_COUNT):
        write(crate / f"src/group_{group:02}.rs", base_group_source(group))


def base_group_source(group: int) -> str:
    sections = [generated_header(), f"pub const GROUP_{group:02}: usize = {group};"]

    for item in range(ITEMS_PER_GROUP):
        sections.append(
            f"""#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaseType_{group:02}_{item:02} {{
    pub value: usize,
}}

pub trait BaseTrait_{group:02}_{item:02} {{
    fn base_value(&self) -> usize;
}}

impl BaseTrait_{group:02}_{item:02} for BaseType_{group:02}_{item:02} {{
    fn base_value(&self) -> usize {{
        self.value + GROUP_{group:02} + {item}
    }}
}}

pub fn make_base_{group:02}_{item:02}(value: usize) -> BaseType_{group:02}_{item:02} {{
    BaseType_{group:02}_{item:02} {{ value }}
}}"""
        )

    nested_exports = "\n".join(
        f"    pub use super::{{BaseTrait_{group:02}_{item:02} as NestedTrait_{item:02}, BaseType_{group:02}_{item:02} as NestedType_{item:02}}};"
        for item in range(ITEMS_PER_GROUP)
    )
    sections.append(f"pub mod nested {{\n{nested_exports}\n}}")

    return "\n\n".join(sections) + "\n"


def write_facade_crate() -> None:
    crate = ROOT / "crates/facade"
    write(
        crate / "Cargo.toml",
        """[package]
name = "bench_def_facade"
version = "0.1.0"
edition = "2024"

[dependencies]
bench_def_base = { path = "../base" }
""",
    )

    modules = "\n".join(f"pub mod layer_{layer:02};" for layer in range(FACADE_MODULES))
    exports = "\n".join(f"pub use super::layer_{layer:02}::*;" for layer in range(FACADE_MODULES))
    write(
        crate / "src/lib.rs",
        generated_header()
        + f"""#![allow(dead_code)]
#![allow(ambiguous_glob_reexports)]
#![allow(non_camel_case_types)]
#![allow(unused_imports)]

pub use bench_def_base::prelude::*;

{modules}

pub mod all {{
{indent(exports)}
}}
""",
    )

    for layer in range(FACADE_MODULES):
        write(crate / f"src/layer_{layer:02}.rs", facade_layer_source(layer))


def facade_layer_source(layer: int) -> str:
    sections = [generated_header(), "#![allow(unused_imports)]"]
    group = layer % GROUP_COUNT
    next_group = (layer + 1) % GROUP_COUNT

    sections.append(f"pub use bench_def_base::group_{group:02}::*;")
    sections.append(f"pub use bench_def_base::group_{next_group:02}::nested::*;")

    for item in range(ITEMS_PER_GROUP):
        sections.append(
            f"""pub use bench_def_base::group_{group:02}::{{
    BaseTrait_{group:02}_{item:02} as LocalTrait_{item:02},
    BaseType_{group:02}_{item:02} as LocalType_{item:02},
    make_base_{group:02}_{item:02} as local_make_{item:02},
}};

pub type FacadeAlias_{layer:02}_{item:02} = LocalType_{item:02};

pub fn facade_make_{layer:02}_{item:02}(value: usize) -> FacadeAlias_{layer:02}_{item:02} {{
    local_make_{item:02}(value)
}}"""
        )

    sections.append(
        f"""pub mod nested_{layer:02} {{
    pub use super::*;
    pub use bench_def_base::group_{next_group:02}::*;
}}"""
    )

    return "\n\n".join(sections) + "\n"


def write_app_crate() -> None:
    crate = ROOT / "crates/app"
    write(
        crate / "Cargo.toml",
        """[package]
name = "bench_def_app"
version = "0.1.0"
edition = "2024"

[dependencies]
bench_def_base = { path = "../base" }
bench_def_facade = { path = "../facade" }
""",
    )

    modules = "\n".join(f"pub mod use_site_{site:02};" for site in range(APP_MODULES))
    calls = "\n".join(f"    value += use_site_{site:02}::run_{site:02}(value);" for site in range(APP_MODULES))
    write(
        crate / "src/lib.rs",
        generated_header()
        + f"""#![allow(dead_code)]
#![allow(ambiguous_glob_imports)]
#![allow(non_camel_case_types)]
#![allow(unused_imports)]

{modules}

pub fn run_all(mut value: usize) -> usize {{
{calls}
    value
}}
""",
    )

    for site in range(APP_MODULES):
        write(crate / f"src/use_site_{site:02}.rs", app_use_site_source(site))


def app_use_site_source(site: int) -> str:
    layer = site % FACADE_MODULES
    group = site % GROUP_COUNT
    imports = [
        "use bench_def_facade::all::*;",
        f"use bench_def_base::group_{group:02}::nested::*;",
    ]

    statements = []
    for item in range(ITEMS_PER_GROUP):
        statements.extend(
            [
                f"    let local_{item:02}: FacadeAlias_{layer:02}_{item:02} = facade_make_{layer:02}_{item:02}(value + {item});",
                f"    value += bench_def_facade::layer_{layer:02}::LocalTrait_{item:02}::base_value(&local_{item:02});",
            ]
        )

    return (
        generated_header()
        + "#![allow(unused_imports)]\n\n"
        + "\n".join(imports)
        + f"""\n\npub fn run_{site:02}(mut value: usize) -> usize {{
{chr(10).join(statements)}
    value
}}
"""
    )


def indent(text: str) -> str:
    return "\n".join(f"    {line}" if line else "" for line in text.splitlines())


def generated_header() -> str:
    return "// Generated by generate.py. Do not edit by hand.\n"


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


if __name__ == "__main__":
    main()
