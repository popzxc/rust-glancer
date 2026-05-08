from pathlib import Path


ROOT = Path(__file__).resolve().parent
GROUP_COUNT = 10
ITEMS_PER_GROUP = 32
WORKFLOW_COUNT = 120
STEPS_PER_WORKFLOW = 24


def main() -> None:
    write_workspace()
    write_items_crate()
    write_body_crate()
    write_app_crate()


def write_workspace() -> None:
    write(
        ROOT / "Cargo.toml",
        """[workspace]
members = ["crates/app", "crates/body", "crates/items"]
resolver = "3"
""",
    )


def write_items_crate() -> None:
    crate = ROOT / "crates/items"
    write(
        crate / "Cargo.toml",
        """[package]
name = "bench_items"
version = "0.1.0"
edition = "2024"
""",
    )

    modules = "\n".join(f"pub mod group_{group:02};" for group in range(GROUP_COUNT))
    calls = "\n".join(
        f"    seed = group_{group:02}::entry(seed);" for group in range(GROUP_COUNT)
    )
    write(
        crate / "src/lib.rs",
        f"""#![allow(dead_code)]

{modules}

pub trait FoldValue {{
    fn fold_value(&self, seed: i64) -> i64;
}}

pub fn combined_seed(mut seed: i64) -> i64 {{
{calls}
    seed
}}
""",
    )

    for group in range(GROUP_COUNT):
        write(crate / f"src/group_{group:02}.rs", group_source(group))


def group_source(group: int) -> str:
    sections = [
        "#![allow(dead_code)]\n\nuse crate::FoldValue;\n".rstrip(),
        f"pub const GROUP_ID: i64 = {group};",
    ]
    entry_lines = ["pub fn entry(mut seed: i64) -> i64 {"]

    for item in range(ITEMS_PER_GROUP):
        record = f"Record{group:02}_{item:02}"
        choice = f"Choice{group:02}_{item:02}"
        trait_name = f"GroupTrait{group:02}_{item:02}"
        method = f"compute_{group:02}_{item:02}"
        make = f"make_{group:02}_{item:02}"
        bias = group * 100 + item

        sections.append(
            f"""
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct {record} {{
    value: i64,
    marker: i64,
}}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum {choice} {{
    Unit,
    Tuple({record}),
    Struct {{ value: i64, marker: i64 }},
}}

pub trait {trait_name} {{
    fn {method}(&self, seed: i64) -> i64;
}}

impl {record} {{
    pub fn new(value: i64) -> Self {{
        Self {{
            value,
            marker: GROUP_ID + {bias},
        }}
    }}

    pub fn bump(self, delta: i64) -> Self {{
        Self {{
            value: self.value + delta + self.marker,
            marker: self.marker,
        }}
    }}

    pub fn combine(self, other: Self) -> Self {{
        Self {{
            value: self.value + other.value + self.marker,
            marker: self.marker + other.marker,
        }}
    }}

    pub fn value(&self) -> i64 {{
        self.value
    }}
}}

impl FoldValue for {record} {{
    fn fold_value(&self, seed: i64) -> i64 {{
        self.value + self.marker + seed
    }}
}}

impl {trait_name} for {record} {{
    fn {method}(&self, seed: i64) -> i64 {{
        self.fold_value(seed) + {bias}
    }}
}}

pub fn {make}(seed: i64) -> {record} {{
    {record}::new(seed + {bias})
}}
""".rstrip()
        )

        entry_lines.append(f"    let item_{item:02} = {make}(seed);")
        entry_lines.append(f"    seed = item_{item:02}.bump({item}).fold_value(seed);")
        entry_lines.append(f"    seed = item_{item:02}.{method}(seed);")

    entry_lines.append("    seed")
    entry_lines.append("}")
    sections.append("\n".join(entry_lines))

    return generated_header() + "\n\n".join(sections) + "\n"


def write_body_crate() -> None:
    crate = ROOT / "crates/body"
    write(
        crate / "Cargo.toml",
        """[package]
name = "bench_body"
version = "0.1.0"
edition = "2024"

[dependencies]
bench_items = { path = "../items" }
""",
    )

    calls = "\n".join(
        f"    seed = workflows::workflow_{workflow:03}(seed);"
        for workflow in range(WORKFLOW_COUNT)
    )
    write(
        crate / "src/lib.rs",
        f"""#![allow(dead_code)]

pub mod workflows;

pub fn run_all(mut seed: i64) -> i64 {{
{calls}
    seed
}}
""",
    )
    write(crate / "src/workflows.rs", workflows_source())


def workflows_source() -> str:
    imports = ["use bench_items::FoldValue;"]
    for group in range(GROUP_COUNT):
        names = []
        for item in range(ITEMS_PER_GROUP):
            names.extend(
                [
                    f"GroupTrait{group:02}_{item:02}",
                    f"Record{group:02}_{item:02}",
                    f"make_{group:02}_{item:02}",
                ]
            )
        imports.append(f"use bench_items::group_{group:02}::{{{', '.join(names)}}};")

    functions = []
    for workflow in range(WORKFLOW_COUNT):
        lines = [f"pub fn workflow_{workflow:03}(mut seed: i64) -> i64 {{"]
        for step in range(STEPS_PER_WORKFLOW):
            group = (workflow + step) % GROUP_COUNT
            item = (workflow * 7 + step * 3) % ITEMS_PER_GROUP
            record = f"Record{group:02}_{item:02}"
            make = f"make_{group:02}_{item:02}"
            method = f"compute_{group:02}_{item:02}"
            local = f"item_{step:02}"
            lines.append(f"    let {local}: {record} = {make}(seed);")
            lines.append(f"    seed = {local}.bump({step}).fold_value(seed);")
            lines.append(f"    seed = {local}.{method}(seed);")
        lines.append("    seed")
        lines.append("}")
        functions.append("\n".join(lines))

    return (
        generated_header()
        + "#![allow(unused_imports)]\n\n"
        + "\n".join(imports)
        + "\n\n"
        + "\n\n".join(functions)
        + "\n"
    )


def write_app_crate() -> None:
    crate = ROOT / "crates/app"
    write(
        crate / "Cargo.toml",
        """[package]
name = "bench_app"
version = "0.1.0"
edition = "2024"

[dependencies]
bench_body = { path = "../body" }
bench_items = { path = "../items" }
""",
    )
    write(
        crate / "src/lib.rs",
        """#![allow(dead_code)]

pub fn run(seed: i64) -> i64 {
    bench_body::run_all(seed) + bench_items::combined_seed(seed)
}
""",
    )
    write(
        crate / "src/main.rs",
        """fn main() {
    println!("{}", bench_app::run(1));
}
""",
    )


def generated_header() -> str:
    return "// Generated by generate.py. Do not edit by hand.\n"


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


if __name__ == "__main__":
    main()
