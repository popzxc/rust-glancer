use rg_body_ir::{BodyLocalNominalTy, BodyNominalTy, ResolvedFieldRef, ResolvedFunctionRef};
use rg_def_map::TargetRef;
use rg_parse::FileId;

use super::{
    Analysis,
    data::{CompletionItem, CompletionKind, CompletionTarget},
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
        let Some(receiver) = self.0.body_ir.receiver_at_dot(target, file_id, offset) else {
            return Vec::new();
        };
        let Some(receiver_ty) = self.0.body_ir.receiver_ty(receiver) else {
            return Vec::new();
        };

        let mut completions = Vec::new();
        for ty in receiver_ty.local_nominals() {
            self.push_local_type_completions(ty, &mut completions);
        }
        for ty in receiver_ty.nominal_tys() {
            self.push_type_completions(ty, &mut completions);
        }
        completions.sort_by(|left, right| {
            left.label
                .cmp(&right.label)
                .then(left.kind.cmp(&right.kind))
        });
        completions
    }

    fn push_type_completions(&self, ty: &BodyNominalTy, completions: &mut Vec<CompletionItem>) {
        for field in self.0.semantic_ir.fields_for_type(ty.def) {
            self.push_field_completion(ResolvedFieldRef::Semantic(field), completions);
        }

        for function in self.0.semantic_ir.inherent_functions_for_type(ty.def) {
            if !self.0.body_ir.semantic_function_applies_to_receiver(
                self.0.def_map,
                self.0.semantic_ir,
                function,
                ty,
            ) {
                continue;
            }

            self.push_function_completion(
                ResolvedFunctionRef::Semantic(function),
                CompletionKind::InherentMethod,
                completions,
            );
        }

        for function in self.0.semantic_ir.trait_functions_for_type(ty.def) {
            self.push_function_completion(
                ResolvedFunctionRef::Semantic(function),
                CompletionKind::TraitMethod,
                completions,
            );
        }
    }

    fn push_local_type_completions(
        &self,
        ty: &BodyLocalNominalTy,
        completions: &mut Vec<CompletionItem>,
    ) {
        for field in self.0.body_ir.fields_for_local_type(ty.item) {
            self.push_field_completion(ResolvedFieldRef::BodyLocal(field), completions);
        }

        for function in self.0.body_ir.inherent_functions_for_local_type(ty.item) {
            if !self.0.body_ir.local_function_applies_to_receiver(
                self.0.def_map,
                self.0.semantic_ir,
                function,
                ty,
            ) {
                continue;
            }

            self.push_function_completion(
                ResolvedFunctionRef::BodyLocal(function),
                CompletionKind::InherentMethod,
                completions,
            );
        }
    }

    fn push_field_completion(
        &self,
        field: ResolvedFieldRef,
        completions: &mut Vec<CompletionItem>,
    ) {
        let Some(label) = self.field_completion_label(field) else {
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
            label,
            kind: CompletionKind::Field,
            target,
        });
    }

    fn field_completion_label(&self, field: ResolvedFieldRef) -> Option<String> {
        match field {
            ResolvedFieldRef::Semantic(field) => self
                .0
                .semantic_ir
                .field_data(field)?
                .field
                .key
                .as_ref()
                .map(ToString::to_string),
            ResolvedFieldRef::BodyLocal(field) => self
                .0
                .body_ir
                .local_field_data(field)?
                .field
                .key
                .as_ref()
                .map(ToString::to_string),
        }
    }

    fn push_function_completion(
        &self,
        function: ResolvedFunctionRef,
        kind: CompletionKind,
        completions: &mut Vec<CompletionItem>,
    ) {
        let Some(name) = self.function_completion_name(function) else {
            return;
        };
        if completions
            .iter()
            .any(|completion| completion.target == CompletionTarget::Function(function))
        {
            return;
        }

        completions.push(CompletionItem {
            label: name,
            kind,
            target: CompletionTarget::Function(function),
        });
    }

    fn function_completion_name(&self, function: ResolvedFunctionRef) -> Option<String> {
        match function {
            ResolvedFunctionRef::Semantic(function) => {
                let data = self.0.semantic_ir.function_data(function)?;
                data.has_self_receiver().then(|| data.name.clone())
            }
            ResolvedFunctionRef::BodyLocal(function) => {
                let data = self.0.body_ir.local_function_data(function)?;
                data.has_self_receiver().then(|| data.name.clone())
            }
        }
    }
}
