use std::collections::HashSet;

use rustc_errors::DiagDecorator;
use rustc_hir::def::DefKind;
use rustc_hir::def_id::DefId;
use rustc_hir::{Expr, ExprKind};
use rustc_infer::infer::TyCtxtInferExt;
use rustc_lint::LateContext;
use rustc_middle::ty::{Ty, TyKind, TypingEnv, Visibility};
use rustc_trait_selection::traits::type_known_to_meet_bound_modulo_regions;

use crate::VMUX_ECS_RUNTIME;

impl crate::VmuxExportedSystem<'_> {
    pub(super) fn collect_runtime(&mut self, cx: &LateContext<'_>) {
        if !self.owned {
            return;
        }
        for implementations in cx.tcx.all_local_trait_impls(()).values() {
            for implementation in implementations {
                self.trait_implementations
                    .insert(implementation.to_def_id());
            }
        }
        for package in cx.tcx.crates(()).iter().copied() {
            if cx.tcx.crate_name(package).as_str() != "bevy_ecs" {
                continue;
            }
            for interface in cx.tcx.traits(package) {
                if cx.tcx.item_name(*interface).as_str() != "SystemParam" {
                    continue;
                }
                self.parameter_trait = Some(*interface);
                return;
            }
        }
    }

    pub(super) fn runtime_reference(&mut self, cx: &LateContext<'_>, expression: &Expr<'_>) {
        if !matches!(expression.kind, ExprKind::Path(_)) {
            return;
        }
        let TyKind::FnDef(function, _) = cx.typeck_results().expr_ty(expression).kind() else {
            return;
        };
        if !function.is_local() {
            return;
        }
        let caller = cx
            .tcx
            .hir_enclosing_body_owner(expression.hir_id)
            .to_def_id();
        if caller != *function {
            self.helper_callers
                .entry(*function)
                .or_default()
                .insert(caller);
        }
    }

    pub(super) fn check_runtime(&self, cx: &LateContext<'_>) {
        for local in cx.tcx.hir_crate_items(()).definitions() {
            let function = local.to_def_id();
            if !matches!(cx.tcx.def_kind(function), DefKind::Fn | DefKind::AssocFn)
                || cx.tcx.def_span(function).from_expansion()
                || self.systems.contains(&function)
                || cx.tcx.trait_of_assoc(function).is_some()
            {
                continue;
            }
            let signature = cx
                .tcx
                .fn_sig(function)
                .instantiate_identity()
                .skip_norm_wip();
            let signature = cx.tcx.instantiate_bound_regions_with_erased(signature);
            if !signature
                .inputs()
                .iter()
                .any(|ty| self.runtime_type(cx, function, *ty, &mut HashSet::new()))
            {
                continue;
            }
            let reason = if let Some(implementation) = cx.tcx.impl_of_assoc(function) {
                if self.trait_implementations.contains(&implementation)
                    || self.parameter_owner(cx, function)
                {
                    continue;
                }
                "inherent methods with ECS runtime inputs must belong to a SystemParam"
            } else {
                if self.private_runtime_function(cx, function)
                    && (self.runtime_helper_owner(cx, function, &mut HashSet::new())
                        || self.runtime_callback_owner(cx, function))
                {
                    continue;
                }
                "ECS runtime helpers must be private and have one local registered owner"
            };
            cx.tcx.emit_node_span_lint(
                VMUX_ECS_RUNTIME,
                cx.tcx.local_def_id_to_hir_id(local),
                cx.tcx.def_span(function),
                DiagDecorator(|diagnostic| {
                    diagnostic
                        .primary_message(format!("`{}`: {reason}", cx.tcx.item_name(function)));
                }),
            );
        }
    }

    pub(super) fn runtime_field_reference(&mut self, cx: &LateContext<'_>, expression: &Expr<'_>) {
        if let ExprKind::Call(callee, arguments) = expression.kind {
            if let ExprKind::Field(base, _) = callee.kind {
                let ty = cx.typeck_results().expr_ty_adjusted(base).peel_refs();
                if let TyKind::Adt(owner, _) = ty.kind()
                    && owner.is_struct()
                    && self.ownership.has_role(owner.did(), "component")
                {
                    let field = owner.non_enum_variant().fields
                        [cx.typeck_results().field_index(callee.hir_id)]
                    .did;
                    let caller = cx
                        .tcx
                        .hir_enclosing_body_owner(expression.hir_id)
                        .to_def_id();
                    self.runtime_field_callers
                        .entry(field)
                        .or_default()
                        .insert(caller);
                }
            }
            if let TyKind::FnDef(function, _) = cx.typeck_results().expr_ty(callee).kind()
                && matches!(cx.tcx.def_kind(*function), DefKind::Ctor(..))
                && let TyKind::Adt(owner, _) = cx.typeck_results().expr_ty(expression).kind()
                && owner.is_struct()
                && self.ownership.has_role(owner.did(), "component")
            {
                for (field, argument) in owner.non_enum_variant().fields.iter().zip(arguments) {
                    if let TyKind::FnDef(function, _) = cx.typeck_results().expr_ty(argument).kind()
                        && function.is_local()
                    {
                        self.runtime_fields
                            .entry(field.did)
                            .or_default()
                            .insert(*function);
                    }
                }
            }
        }
        if let ExprKind::Struct(_, fields, _) = expression.kind
            && let TyKind::Adt(owner, _) = cx.typeck_results().expr_ty(expression).kind()
            && owner.is_struct()
            && self.ownership.has_role(owner.did(), "component")
        {
            for field in fields {
                if let TyKind::FnDef(function, _) = cx.typeck_results().expr_ty(field.expr).kind()
                    && function.is_local()
                    && let Some(definition) = owner
                        .non_enum_variant()
                        .fields
                        .iter()
                        .find(|definition| definition.name == field.ident.name)
                {
                    self.runtime_fields
                        .entry(definition.did)
                        .or_default()
                        .insert(*function);
                }
            }
        }
    }

    fn runtime_callback_owner(&self, cx: &LateContext<'_>, function: DefId) -> bool {
        let mut owners = HashSet::new();
        for (field, callbacks) in &self.runtime_fields {
            if !callbacks.contains(&function) {
                continue;
            }
            let Some(callers) = self.runtime_field_callers.get(field) else {
                continue;
            };
            for caller in callers {
                if !self.runtime_helper_owner(cx, *caller, &mut HashSet::new()) {
                    return false;
                }
                owners.insert(*caller);
            }
        }
        owners.len() == 1
    }

    fn runtime_type<'tcx>(
        &self,
        cx: &LateContext<'tcx>,
        context: DefId,
        ty: Ty<'tcx>,
        visiting: &mut HashSet<Ty<'tcx>>,
    ) -> bool {
        if !visiting.insert(ty) {
            return false;
        }
        match ty.kind() {
            TyKind::Ref(_, value, _) => {
                self.system_parameter(cx, context, ty)
                    || self.runtime_type(cx, context, *value, visiting)
            }
            TyKind::RawPtr(value, _) => self.runtime_type(cx, context, *value, visiting),
            TyKind::Tuple(values) => values
                .iter()
                .any(|value| self.runtime_type(cx, context, value, visiting)),
            TyKind::Adt(owner, arguments) => {
                if cx.tcx.crate_name(owner.did().krate).as_str() == "core"
                    && cx.tcx.item_name(owner.did()).as_str() == "PhantomData"
                {
                    return false;
                }
                if self.system_parameter(cx, context, ty) {
                    return true;
                }
                if cx.tcx.crate_name(owner.did().krate).as_str() == "bevy_ecs"
                    && matches!(
                        cx.tcx.item_name(owner.did()).as_str(),
                        "EntityCommands" | "World"
                    )
                {
                    return true;
                }
                arguments
                    .types()
                    .any(|value| self.runtime_type(cx, context, value, visiting))
            }
            TyKind::Array(value, _) | TyKind::Slice(value) => {
                self.runtime_type(cx, context, *value, visiting)
            }
            TyKind::Param(..) | TyKind::Alias(..) => self.system_parameter(cx, context, ty),
            _ => false,
        }
    }

    fn parameter_owner(&self, cx: &LateContext<'_>, function: DefId) -> bool {
        let Some(implementation) = cx.tcx.impl_of_assoc(function) else {
            return false;
        };
        let ty = cx
            .tcx
            .type_of(implementation)
            .instantiate_identity()
            .skip_norm_wip();
        self.system_parameter(cx, implementation, ty)
    }

    fn system_parameter<'tcx>(&self, cx: &LateContext<'tcx>, context: DefId, ty: Ty<'tcx>) -> bool {
        let Some(interface) = self.parameter_trait else {
            return false;
        };
        let (inference, environment) = cx
            .tcx
            .infer_ctxt()
            .build_with_typing_env(TypingEnv::post_analysis(cx.tcx, context));
        type_known_to_meet_bound_modulo_regions(
            &inference,
            environment,
            cx.tcx.erase_and_anonymize_regions(ty),
            interface,
        )
    }

    fn private_runtime_function(&self, cx: &LateContext<'_>, function: DefId) -> bool {
        let Visibility::Restricted(scope) = cx.tcx.visibility(function) else {
            return false;
        };
        scope
            == cx
                .tcx
                .parent_module_from_def_id(function.expect_local())
                .to_mod_id()
    }

    fn runtime_helper_owner(
        &self,
        cx: &LateContext<'_>,
        function: DefId,
        visiting: &mut HashSet<DefId>,
    ) -> bool {
        if self.systems.contains(&function) || self.parameter_owner(cx, function) {
            return true;
        }
        if !visiting.insert(function) {
            return false;
        }
        let Some(callers) = self.helper_callers.get(&function) else {
            return false;
        };
        let mut owners = HashSet::new();
        for caller in callers {
            let mut caller = *caller;
            while cx.tcx.def_kind(caller) == DefKind::Closure && !self.systems.contains(&caller) {
                caller = cx.tcx.parent(caller);
            }
            owners.insert(caller);
        }
        if owners.len() != 1 {
            return false;
        }
        let caller = *owners.iter().next().unwrap();
        if cx.tcx.parent_module_from_def_id(caller.expect_local())
            != cx.tcx.parent_module_from_def_id(function.expect_local())
        {
            return false;
        }
        self.runtime_helper_owner(cx, caller, visiting)
    }
}
