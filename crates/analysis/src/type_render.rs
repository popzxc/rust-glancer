//! Compact type labels for UI surfaces.
//!
//! This renderer intentionally favors short, recognizable names over fully-qualified debug output.
//! The analysis layer already returns stable IDs; inlay hints and future hovers need labels that
//! are useful while reading code.

use rg_body_ir::{BodyGenericArg, BodyLocalNominalTy, BodyNominalTy, BodyTy};
use rg_semantic_ir::TypeDefId;

use super::Analysis;

pub(super) struct TypeRenderer<'a, 'db>(&'a Analysis<'db>);

impl<'a, 'db> TypeRenderer<'a, 'db> {
    pub(super) fn new(analysis: &'a Analysis<'db>) -> Self {
        Self(analysis)
    }

    pub(super) fn render(&self, ty: &BodyTy) -> Option<String> {
        match ty {
            BodyTy::Unit => Some("()".to_string()),
            BodyTy::Never => Some("!".to_string()),
            BodyTy::Syntax(ty) => Some(ty.to_string()),
            BodyTy::Reference(inner) => self.render(inner).map(|inner| format!("&{inner}")),
            BodyTy::LocalNominal(types) => {
                self.render_joined(types.iter().filter_map(|ty| self.render_local_nominal(ty)))
            }
            BodyTy::Nominal(types) | BodyTy::SelfTy(types) => {
                self.render_joined(types.iter().filter_map(|ty| self.render_nominal(ty)))
            }
            BodyTy::Unknown => None,
        }
    }

    fn render_joined(&self, labels: impl Iterator<Item = String>) -> Option<String> {
        let mut labels = labels.collect::<Vec<_>>();
        labels.sort();
        (!labels.is_empty()).then(|| labels.join(" | "))
    }

    fn render_local_nominal(&self, ty: &BodyLocalNominalTy) -> Option<String> {
        let body = self.0.body_ir.body_data(ty.item.body)?;
        let item = body.local_item(ty.item.item)?;
        Some(format!(
            "{}{}",
            item.name,
            self.render_generic_args(&ty.args)
        ))
    }

    fn render_nominal(&self, ty: &BodyNominalTy) -> Option<String> {
        let target_ir = self.0.semantic_ir.target_ir(ty.def.target)?;
        let name = match ty.def.id {
            TypeDefId::Struct(id) => target_ir.items().struct_data(id)?.name.as_str(),
            TypeDefId::Enum(id) => target_ir.items().enum_data(id)?.name.as_str(),
            TypeDefId::Union(id) => target_ir.items().union_data(id)?.name.as_str(),
        };

        Some(format!("{name}{}", self.render_generic_args(&ty.args)))
    }

    fn render_generic_args(&self, args: &[BodyGenericArg]) -> String {
        if args.is_empty() {
            return String::new();
        }

        format!(
            "<{}>",
            args.iter()
                .map(|arg| self.render_generic_arg(arg))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    fn render_generic_arg(&self, arg: &BodyGenericArg) -> String {
        match arg {
            BodyGenericArg::Type(ty) => self.render(ty).unwrap_or_else(|| "_".to_string()),
            BodyGenericArg::Lifetime(lifetime) => lifetime.clone(),
            BodyGenericArg::Const(value) => value.clone(),
            BodyGenericArg::AssocType { name, ty } => match ty {
                Some(ty) => format!(
                    "{name} = {}",
                    self.render(ty).unwrap_or_else(|| "_".to_string())
                ),
                None => name.clone(),
            },
            BodyGenericArg::Unsupported(text) => text.clone(),
        }
    }
}
