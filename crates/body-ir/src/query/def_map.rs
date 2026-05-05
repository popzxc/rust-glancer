//! DefMap read surface required by Body IR resolution.

use rg_def_map::{DefMapDb, DefMapReadTxn, ModuleRef, Path, ResolvePathResult};
use rg_package_store::PackageStoreError;

/// Minimal def-map query surface used by body-resolution reads.
///
/// Build passes use the resident DB; analysis queries use read transactions. Keeping the resolver
/// generic over this tiny interface prevents query code from depending on resident storage.
pub(crate) trait DefMapQuery {
    fn resolve_path(
        &self,
        from: ModuleRef,
        path: &Path,
    ) -> Result<ResolvePathResult, PackageStoreError>;

    fn resolve_path_in_type_namespace(
        &self,
        from: ModuleRef,
        path: &Path,
    ) -> Result<ResolvePathResult, PackageStoreError>;
}

impl DefMapQuery for DefMapDb {
    fn resolve_path(
        &self,
        from: ModuleRef,
        path: &Path,
    ) -> Result<ResolvePathResult, PackageStoreError> {
        Ok(DefMapDb::resolve_path(self, from, path))
    }

    fn resolve_path_in_type_namespace(
        &self,
        from: ModuleRef,
        path: &Path,
    ) -> Result<ResolvePathResult, PackageStoreError> {
        Ok(DefMapDb::resolve_path_in_type_namespace(self, from, path))
    }
}

impl DefMapQuery for DefMapReadTxn<'_> {
    fn resolve_path(
        &self,
        from: ModuleRef,
        path: &Path,
    ) -> Result<ResolvePathResult, PackageStoreError> {
        DefMapReadTxn::resolve_path(self, from, path)
    }

    fn resolve_path_in_type_namespace(
        &self,
        from: ModuleRef,
        path: &Path,
    ) -> Result<ResolvePathResult, PackageStoreError> {
        DefMapReadTxn::resolve_path_in_type_namespace(self, from, path)
    }
}
