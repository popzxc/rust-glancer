//! Builds the retained phase databases for a fresh project snapshot.

use anyhow::Context as _;

use rg_body_ir::{BodyIrBuildPolicy, BodyIrDb};
use rg_def_map::DefMapDb;
use rg_item_tree::ItemTreeDb;
use rg_parse::ParseDb;
use rg_semantic_ir::SemanticIrDb;
use rg_text::PackageNameInterners;
use rg_workspace::WorkspaceMetadata;

use crate::profile::BuildProfiler;

pub(super) struct BuiltPhases {
    pub(super) names: PackageNameInterners,
    pub(super) parse: ParseDb,
    pub(super) def_map: DefMapDb,
    pub(super) semantic_ir: SemanticIrDb,
    pub(super) body_ir: BodyIrDb,
}

pub(super) fn build(
    workspace: &WorkspaceMetadata,
    body_ir_policy: BodyIrBuildPolicy,
    profiler: &mut BuildProfiler,
) -> anyhow::Result<BuiltPhases> {
    let mut parse = ParseDb::build(workspace).context("while attempting to build parse db")?;
    let mut names = PackageNameInterners::new(parse.package_count());
    let process_memory = profiler.sample_process_memory();
    let parse_bytes = profiler.measure(&parse);
    profiler.record("after parse", parse_bytes, parse_bytes, process_memory);

    let item_tree = ItemTreeDb::build_with_interners(&mut parse, &mut names)
        .context("while attempting to build item tree db")?;
    let process_memory = profiler.sample_process_memory();
    let names_bytes = profiler.measure(&names);
    let parse_bytes = profiler.measure(&parse);
    let item_tree_bytes = profiler.measure(&item_tree);
    profiler.record(
        "after item-tree",
        item_tree_bytes,
        profiler.sum_retained(&[names_bytes, parse_bytes, item_tree_bytes]),
        process_memory,
    );

    let def_map = DefMapDb::builder(workspace, &parse, &item_tree)
        .name_interners(&mut names)
        .build()
        .context("while attempting to build def map db")?;
    let process_memory = profiler.sample_process_memory();
    let names_bytes = profiler.measure(&names);
    let def_map_bytes = profiler.measure(&def_map);
    profiler.record(
        "after def-map",
        def_map_bytes,
        profiler.sum_retained(&[names_bytes, parse_bytes, item_tree_bytes, def_map_bytes]),
        process_memory,
    );

    let semantic_ir = SemanticIrDb::builder(&item_tree, &def_map)
        .build()
        .context("while attempting to build semantic ir db")?;
    let process_memory = profiler.sample_process_memory();
    let names_bytes = profiler.measure(&names);
    let semantic_ir_bytes = profiler.measure(&semantic_ir);
    profiler.record(
        "after semantic-ir",
        semantic_ir_bytes,
        profiler.sum_retained(&[
            names_bytes,
            parse_bytes,
            item_tree_bytes,
            def_map_bytes,
            semantic_ir_bytes,
        ]),
        process_memory,
    );

    // ItemTree is a lowering input, not retained project state. Dropping it here makes the
    // following process-only checkpoint useful for separating transient build pressure from final
    // retained memory.
    drop(item_tree);
    let process_memory = profiler.sample_process_memory();
    let names_bytes = profiler.measure(&names);
    profiler.record(
        "after item-tree drop",
        None,
        profiler.sum_retained(&[names_bytes, parse_bytes, def_map_bytes, semantic_ir_bytes]),
        process_memory,
    );

    let body_ir = BodyIrDb::builder(&parse, &def_map, &semantic_ir)
        .name_interners(&mut names)
        .policy(body_ir_policy)
        .build()
        .context("while attempting to build body ir db")?;
    let process_memory = profiler.sample_process_memory();
    let names_bytes = profiler.measure(&names);
    let body_ir_bytes = profiler.measure(&body_ir);
    profiler.record(
        "after body-ir",
        body_ir_bytes,
        profiler.sum_retained(&[
            names_bytes,
            parse_bytes,
            def_map_bytes,
            semantic_ir_bytes,
            body_ir_bytes,
        ]),
        process_memory,
    );

    parse.evict_syntax_trees();
    parse.shrink_to_fit();
    let process_memory = profiler.sample_process_memory();
    names.shrink_to_fit();
    let names_bytes = profiler.measure(&names);
    let parse_bytes = profiler.measure(&parse);
    profiler.record(
        "after parse syntax eviction",
        parse_bytes,
        profiler.sum_retained(&[
            names_bytes,
            parse_bytes,
            def_map_bytes,
            semantic_ir_bytes,
            body_ir_bytes,
        ]),
        process_memory,
    );

    Ok(BuiltPhases {
        names,
        parse,
        def_map,
        semantic_ir,
        body_ir,
    })
}
