use rg_parse::TargetId;

/// Stable identifier of one module inside a target map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModuleId(pub usize);

/// Stable identifier of one local definition inside a target map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalDefId(pub usize);

/// Stable identifier of one impl block inside a target map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalImplId(pub usize);

/// Stable identifier of one lowered import inside a target map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImportId(pub usize);

/// Stable identifier of one analyzed package inside a project analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PackageSlot(pub usize);

/// Stable reference to one target across the whole project analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TargetRef {
    pub package: PackageSlot,
    pub target: TargetId,
}

/// Stable reference to one module across the whole project analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModuleRef {
    pub target: TargetRef,
    pub module: ModuleId,
}

/// Stable reference to one local definition across the whole project analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalDefRef {
    pub target: TargetRef,
    pub local_def: LocalDefId,
}

/// Stable reference to one impl block across the whole project analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalImplRef {
    pub target: TargetRef,
    pub local_impl: LocalImplId,
}

/// Stable reference to one import across the whole project analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImportRef {
    pub target: TargetRef,
    pub import: ImportId,
}

/// Namespace-resolved target-level definition reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DefId {
    Module(ModuleRef),
    Local(LocalDefRef),
}
