use crate::{
    Project,
    item_tree::{FileTree, Package as ItemTreePackage, TargetRoot},
    parse::{FileId, Package, Target},
};

pub(crate) fn sorted_packages(project: &Project) -> Vec<(usize, &Package)> {
    let mut packages = project
        .parse_db()
        .packages()
        .iter()
        .enumerate()
        .collect::<Vec<_>>();
    packages.sort_by(|left, right| left.1.package_name().cmp(right.1.package_name()));
    packages
}

pub(crate) fn sorted_targets(package: &Package) -> Vec<&Target> {
    let mut targets = package.targets().iter().collect::<Vec<_>>();
    targets.sort_by(|left, right| {
        (
            left.kind.sort_order(),
            left.name.as_str(),
            left.src_path.as_path(),
        )
            .cmp(&(
                right.kind.sort_order(),
                right.name.as_str(),
                right.src_path.as_path(),
            ))
    });
    targets
}

pub(crate) fn sorted_item_tree_target_roots<'a>(
    package: &Package,
    item_trees: &'a ItemTreePackage,
) -> Vec<&'a TargetRoot> {
    let mut target_roots = item_trees.target_roots().iter().collect::<Vec<_>>();
    target_roots.sort_by(|left, right| {
        let left_target = package
            .target(left.target)
            .expect("parsed target should exist while sorting item-tree target roots");
        let right_target = package
            .target(right.target)
            .expect("parsed target should exist while sorting item-tree target roots");

        (
            left_target.kind.sort_order(),
            left_target.name.as_str(),
            left_target.src_path.as_path(),
        )
            .cmp(&(
                right_target.kind.sort_order(),
                right_target.name.as_str(),
                right_target.src_path.as_path(),
            ))
    });
    target_roots
}

pub(crate) fn sorted_item_tree_files<'a>(
    package: &Package,
    item_trees: &'a ItemTreePackage,
) -> Vec<&'a FileTree> {
    let mut files = item_trees.files().collect::<Vec<_>>();
    files.sort_by(|left, right| {
        let left_path = package
            .file_path(left.file)
            .expect("item-tree file should exist while sorting");
        let right_path = package
            .file_path(right.file)
            .expect("item-tree file should exist while sorting");
        left_path.cmp(right_path)
    });
    files
}

pub(crate) fn file_label(package: &Package, file_id: FileId) -> String {
    package
        .file_path(file_id)
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("<unknown>")
        .to_string()
}
