use crate::{
    body_ir::{BodyData, BodyId, BodyRef, ExprData, ExprId, ExprKind},
    def_map::TargetRef,
    item_tree::ParamKind,
    parse::{FileId, span::Span},
    semantic_ir::{FieldRef, FunctionData, FunctionRef, TypeDefRef},
};

use super::{
    Analysis,
    data::{CompletionItem, CompletionKind, CompletionTarget},
    ty::type_defs_from_body_ty,
};

pub(super) struct CompletionResolver<'a, 'project>(&'a Analysis<'project>);

impl<'a, 'project> CompletionResolver<'a, 'project> {
    pub(super) fn new(analysis: &'a Analysis<'project>) -> Self {
        Self(analysis)
    }

    pub(super) fn completions_at_dot(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Vec<CompletionItem> {
        let Some((body_ref, receiver)) = self.receiver_expr_at_dot(target, file_id, offset) else {
            return Vec::new();
        };
        let Some(receiver_data) = self
            .body_data(body_ref)
            .and_then(|body| body.expr(receiver))
        else {
            return Vec::new();
        };

        let mut completions = Vec::new();
        for ty in type_defs_from_body_ty(&receiver_data.ty) {
            self.push_type_completions(ty, &mut completions);
        }
        completions.sort_by(|left, right| {
            left.label
                .cmp(&right.label)
                .then(left.kind.cmp(&right.kind))
        });
        completions
    }

    fn receiver_expr_at_dot(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Option<(BodyRef, ExprId)> {
        let target_bodies = self.0.project.body_ir_db().target_bodies(target)?;
        let mut best = None::<(BodyRef, ExprId, u32)>;

        for (body_idx, body) in target_bodies.bodies().iter().enumerate() {
            if body.source.file_id != file_id || !body.source.span.contains(offset) {
                continue;
            }

            let body_ref = BodyRef {
                target,
                body: BodyId(body_idx),
            };
            for expr in &body.exprs {
                if expr.source.file_id != file_id || !offset_in_dot_expr(expr, body, offset) {
                    continue;
                }

                let Some(receiver) = receiver_expr(expr) else {
                    continue;
                };
                let len = expr.source.span.len();
                if best.is_none_or(|(_, _, best_len)| len < best_len) {
                    best = Some((body_ref, receiver, len));
                }
            }
        }

        best.map(|(body, receiver, _)| (body, receiver))
    }

    fn body_data(&self, body_ref: BodyRef) -> Option<&BodyData> {
        self.0.project.body_ir_db().body_data(body_ref)
    }

    fn push_type_completions(&self, ty: TypeDefRef, completions: &mut Vec<CompletionItem>) {
        for field in self.0.project.semantic_ir_db().fields_for_type(ty) {
            self.push_field_completion(field, completions);
        }

        for function in self
            .0
            .project
            .semantic_ir_db()
            .inherent_functions_for_type(ty)
        {
            self.push_function_completion(function, CompletionKind::InherentMethod, completions);
        }

        for function in self.0.project.semantic_ir_db().trait_functions_for_type(ty) {
            self.push_function_completion(function, CompletionKind::TraitMethod, completions);
        }
    }

    fn push_field_completion(&self, field: FieldRef, completions: &mut Vec<CompletionItem>) {
        let Some(data) = self.0.project.semantic_ir_db().field_data(field) else {
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
        let Some(data) = self.0.project.semantic_ir_db().function_data(function) else {
            return;
        };
        if !function_has_self_receiver(data) {
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

fn function_has_self_receiver(data: &FunctionData) -> bool {
    data.declaration
        .params
        .first()
        .is_some_and(|param| matches!(param.kind, ParamKind::SelfParam))
}

fn offset_in_dot_expr(expr: &ExprData, body: &BodyData, offset: u32) -> bool {
    let Some(receiver) = receiver_expr(expr) else {
        return false;
    };
    let Some(receiver_data) = body.expr(receiver) else {
        return false;
    };
    let Some(dot_span) = dot_span(expr) else {
        return false;
    };
    let completion_end = member_name_span(expr)
        .map(|span| span.text.end)
        .unwrap_or(expr.source.span.text.end);

    receiver_data.source.span.text.end <= dot_span.text.start
        && dot_span.text.end <= offset
        && offset <= completion_end
}

fn receiver_expr(expr: &ExprData) -> Option<ExprId> {
    match &expr.kind {
        ExprKind::MethodCall {
            receiver: Some(receiver),
            ..
        }
        | ExprKind::Field {
            base: Some(receiver),
            ..
        } => Some(*receiver),
        _ => None,
    }
}

fn member_name_span(expr: &ExprData) -> Option<Span> {
    match &expr.kind {
        ExprKind::MethodCall {
            method_name_span, ..
        } => *method_name_span,
        ExprKind::Field { field_span, .. } => *field_span,
        _ => None,
    }
}

fn dot_span(expr: &ExprData) -> Option<Span> {
    match &expr.kind {
        ExprKind::MethodCall { dot_span, .. } => *dot_span,
        ExprKind::Field { dot_span, .. } => *dot_span,
        _ => None,
    }
}
