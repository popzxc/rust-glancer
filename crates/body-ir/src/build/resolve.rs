//! Resolves lowered Body IR while a build mutator has privileged package access.

use rg_def_map::{DefMapReadTxn, PackageSlot, TargetRef};
use rg_package_store::PackageStoreError;
use rg_parse::TargetId;
use rg_semantic_ir::SemanticIrReadTxn;

use crate::{
    body::TargetBodiesStatus,
    db::BodyIrDbMutator,
    ids::{BodyId, BodyRef},
    resolution::{BodyResolver, SemanticResolutionIndex},
};

pub(super) fn resolve_bodies(
    db: &mut BodyIrDbMutator<'_>,
    def_map: &DefMapReadTxn<'_>,
    semantic_ir: &SemanticIrReadTxn<'_>,
) {
    let packages = (0..db.package_count()).map(PackageSlot).collect::<Vec<_>>();
    resolve_bodies_for_packages(db, def_map, semantic_ir, &packages)
        .expect("resident body resolution should not fail");
}

pub(super) fn resolve_bodies_for_packages(
    db: &mut BodyIrDbMutator<'_>,
    def_map: &DefMapReadTxn<'_>,
    semantic_ir: &SemanticIrReadTxn<'_>,
    packages: &[PackageSlot],
) -> Result<(), PackageStoreError> {
    let semantic_index = SemanticResolutionIndex::build(semantic_ir)?;

    // Resolution is a mutation pass over already-lowered bodies. Skipped targets intentionally
    // keep their body stores empty so dependency body internals stay cheap by default.
    for package_slot in packages {
        let Some(package) = db.package_mut(*package_slot) else {
            continue;
        };

        for (target_idx, target) in package.targets_mut().iter_mut().enumerate() {
            if matches!(target.status(), TargetBodiesStatus::Skipped) {
                continue;
            }

            let target_ref = TargetRef {
                package: *package_slot,
                target: TargetId(target_idx),
            };

            for (body_idx, body) in target.bodies_mut().iter_mut().enumerate() {
                BodyResolver::new(
                    def_map,
                    semantic_ir,
                    &semantic_index,
                    BodyRef {
                        target: target_ref,
                        body: BodyId(body_idx),
                    },
                    body,
                )
                .resolve()?;
            }
        }
    }

    Ok(())
}
