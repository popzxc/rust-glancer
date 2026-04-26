use rg_def_map::TargetRef;
use rg_parse::FileId;
use rg_semantic_ir::{FieldRef, FunctionRef, TypeDefRef};

use super::{
    Analysis,
    data::{CompletionItem, CompletionKind, CompletionTarget},
    ty::type_defs_from_body_ty,
};

pub(super) struct CompletionResolver<'a, 'db>(&'a Analysis<'db>);

impl<'a, 'db> CompletionResolver<'a, 'db> {
    pub(super) fn new(analysis: &'a Analysis<'db>) -> Self {
        Self(analysis)
    }

    pub(super) fn completions_at_dot(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Vec<CompletionItem> {
        let Some(receiver_ty) = self.0.body_ir.receiver_ty_at_dot(target, file_id, offset) else {
            return Vec::new();
        };

        let mut completions = Vec::new();
        for ty in type_defs_from_body_ty(receiver_ty) {
            self.push_type_completions(ty, &mut completions);
        }
        completions.sort_by(|left, right| {
            left.label
                .cmp(&right.label)
                .then(left.kind.cmp(&right.kind))
        });
        completions
    }

    fn push_type_completions(&self, ty: TypeDefRef, completions: &mut Vec<CompletionItem>) {
        for field in self.0.semantic_ir.fields_for_type(ty) {
            self.push_field_completion(field, completions);
        }

        for function in self.0.semantic_ir.inherent_functions_for_type(ty) {
            self.push_function_completion(function, CompletionKind::InherentMethod, completions);
        }

        for function in self.0.semantic_ir.trait_functions_for_type(ty) {
            self.push_function_completion(function, CompletionKind::TraitMethod, completions);
        }
    }

    fn push_field_completion(&self, field: FieldRef, completions: &mut Vec<CompletionItem>) {
        let Some(data) = self.0.semantic_ir.field_data(field) else {
            return;
        };
        let Some(key) = data.field.key.as_ref() else {
            return;
        };
        let target = CompletionTarget::Field(field);
        if completions
            .iter()
            .any(|completion| completion.target == target)
        {
            return;
        }

        completions.push(CompletionItem {
            label: key.to_string(),
            kind: CompletionKind::Field,
            target,
        });
    }

    fn push_function_completion(
        &self,
        function: FunctionRef,
        kind: CompletionKind,
        completions: &mut Vec<CompletionItem>,
    ) {
        let Some(data) = self.0.semantic_ir.function_data(function) else {
            return;
        };
        if !data.has_self_receiver() {
            return;
        }
        if completions
            .iter()
            .any(|completion| completion.target == CompletionTarget::Function(function))
        {
            return;
        }

        completions.push(CompletionItem {
            label: data.name.clone(),
            kind,
            target: CompletionTarget::Function(function),
        });
    }
}
