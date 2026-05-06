use std::{
    cell::Cell,
    path::{Path, PathBuf},
};

use divan::{
    Bencher, black_box, black_box_drop,
    counter::{BytesCount, ItemsCount},
};
use rg_body_ir::{BodyIrBuildPolicy, BodyIrDb};
use rg_def_map::DefMapDb;
use rg_item_tree::ItemTreeDb;
use rg_parse::ParseDb;
use rg_semantic_ir::SemanticIrDb;
use rg_text::NameInterner;
use rg_workspace::WorkspaceMetadata;

fn main() {
    divan::main();
}

struct BenchFixture {
    workspace: WorkspaceMetadata,
    parse: ParseDb,
    item_tree: ItemTreeDb,
    names_after_item_tree: NameInterner,
    names_after_semantic_ir: NameInterner,
    def_map: DefMapDb,
    semantic_ir: SemanticIrDb,
    source_files: usize,
    source_bytes: u64,
    item_tree_items: usize,
    def_map_imports: usize,
    semantic_items: usize,
    body_expressions: usize,
}

impl BenchFixture {
    fn load() -> Self {
        let workspace = WorkspaceMetadata::from_manifest_path(bench_manifest_path())
            .expect("bench target Cargo metadata should load");
        let mut parse = ParseDb::build(&workspace).expect("bench target parse db should build");
        let source_files = count_source_files(&parse);
        let source_bytes = count_source_bytes(&parse);

        let mut names = NameInterner::new();
        let item_tree = ItemTreeDb::build_with_interner(&mut parse, &mut names)
            .expect("bench target item tree should build");
        let item_tree_items = count_item_tree_items(&workspace, &item_tree);
        let names_after_item_tree = names.clone();

        let def_map = DefMapDb::builder(&workspace, &parse, &item_tree)
            .name_interner(&mut names)
            .build()
            .expect("bench target def map should build");
        let def_map_imports = def_map.stats().import_count;

        let semantic_ir = SemanticIrDb::builder(&item_tree, &def_map)
            .build()
            .expect("bench target semantic IR should build");
        let semantic_stats = semantic_ir.stats();
        let semantic_items = semantic_stats.struct_count
            + semantic_stats.union_count
            + semantic_stats.enum_count
            + semantic_stats.trait_count
            + semantic_stats.impl_count
            + semantic_stats.function_count
            + semantic_stats.type_alias_count
            + semantic_stats.const_count
            + semantic_stats.static_count;
        let names_after_semantic_ir = names.clone();

        let body_ir = BodyIrDb::builder(&parse, &def_map, &semantic_ir)
            .name_interner(&mut names)
            .policy(BodyIrBuildPolicy::workspace_packages())
            .build()
            .expect("bench target body IR should build");
        let body_expressions = body_ir.stats().expression_count;

        Self {
            workspace,
            parse,
            item_tree,
            names_after_item_tree,
            names_after_semantic_ir,
            def_map,
            semantic_ir,
            source_files,
            source_bytes,
            item_tree_items,
            def_map_imports,
            semantic_items,
            body_expressions,
        }
    }
}

#[divan::bench(sample_count = 10, sample_size = 1)]
fn parse_db(bencher: Bencher<'_, '_>) {
    let fixture = fixture();
    bencher
        .counter(BytesCount::from(fixture.source_bytes))
        .counter(ItemsCount::new(fixture.source_files))
        .bench_local(|| {
            let parse =
                ParseDb::build(black_box(&fixture.workspace)).expect("parse db should build");
            black_box_drop(parse);
        });
}

#[divan::bench(sample_count = 10, sample_size = 1)]
fn item_tree_db(bencher: Bencher<'_, '_>) {
    let fixture = fixture();
    bencher
        .counter(ItemsCount::new(fixture.item_tree_items))
        .with_inputs(|| (fixture.parse.clone(), NameInterner::new()))
        .bench_local_values(|(mut parse, mut names)| {
            let item_tree = ItemTreeDb::build_with_interner(&mut parse, &mut names)
                .expect("item tree should build");
            black_box_drop(item_tree);
        });
}

#[divan::bench(sample_count = 10, sample_size = 1)]
fn def_map_db(bencher: Bencher<'_, '_>) {
    let fixture = fixture();
    bencher
        .counter(ItemsCount::new(fixture.def_map_imports))
        .with_inputs(|| {
            (
                fixture.parse.clone(),
                fixture.item_tree.clone(),
                fixture.names_after_item_tree.clone(),
            )
        })
        .bench_local_values(|(parse, item_tree, mut names)| {
            let def_map = DefMapDb::builder(&fixture.workspace, &parse, &item_tree)
                .name_interner(&mut names)
                .build()
                .expect("def map should build");
            black_box_drop(def_map);
        });
}

#[divan::bench(sample_count = 10, sample_size = 1)]
fn semantic_ir_db(bencher: Bencher<'_, '_>) {
    let fixture = fixture();
    bencher
        .counter(ItemsCount::new(fixture.semantic_items))
        .with_inputs(|| (fixture.item_tree.clone(), fixture.def_map.clone()))
        .bench_local_values(|(item_tree, def_map)| {
            let semantic_ir = SemanticIrDb::builder(&item_tree, &def_map)
                .build()
                .expect("semantic IR should build");
            black_box_drop(semantic_ir);
        });
}

#[divan::bench(sample_count = 10, sample_size = 1)]
fn body_ir_db(bencher: Bencher<'_, '_>) {
    let fixture = fixture();
    bencher
        .counter(ItemsCount::new(fixture.body_expressions))
        .with_inputs(|| {
            (
                fixture.parse.clone(),
                fixture.names_after_semantic_ir.clone(),
            )
        })
        .bench_local_values(|(parse, mut names)| {
            let body_ir = BodyIrDb::builder(&parse, &fixture.def_map, &fixture.semantic_ir)
                .name_interner(&mut names)
                .policy(BodyIrBuildPolicy::workspace_packages())
                .build()
                .expect("body IR should build");
            black_box_drop(body_ir);
        });
}

fn fixture() -> &'static BenchFixture {
    thread_local! {
        static FIXTURE: Cell<Option<&'static BenchFixture>> = const { Cell::new(None) };
    }

    FIXTURE.with(|fixture| {
        if let Some(cached) = fixture.get() {
            return cached;
        }

        let loaded = Box::leak(Box::new(BenchFixture::load()));
        fixture.set(Some(loaded));
        loaded
    })
}

fn bench_manifest_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_targets/bench_target/Cargo.toml")
}

fn count_source_files(parse: &ParseDb) -> usize {
    parse
        .packages()
        .iter()
        .map(|package| package.parsed_files().count())
        .sum()
}

fn count_source_bytes(parse: &ParseDb) -> u64 {
    parse
        .packages()
        .iter()
        .flat_map(|package| package.parsed_files())
        .filter_map(|file| std::fs::metadata(file.path()).ok())
        .map(|metadata| metadata.len())
        .sum()
}

fn count_item_tree_items(workspace: &WorkspaceMetadata, item_tree: &ItemTreeDb) -> usize {
    (0..workspace.packages().len())
        .filter_map(|package| item_tree.package(package))
        .flat_map(|package| package.files())
        .map(|file| file.items.len())
        .sum()
}
