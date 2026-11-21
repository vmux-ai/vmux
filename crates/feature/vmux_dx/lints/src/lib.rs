#![feature(rustc_private)]

extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_lint;
extern crate rustc_middle;
extern crate rustc_session;

use rustc_errors::DiagDecorator;
use rustc_hir::def::Res;
use rustc_hir::def_id::{DefId, LOCAL_CRATE};
use rustc_hir::{Body, Expr, ExprKind, HirId, Node, PatKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::ty::{Ty, TyKind, Visibility};
use std::collections::HashSet;

mod aliases;
mod naming;
mod ownership;
#[cfg(test)]
mod registrations;

dylint_linting::dylint_library!();

rustc_session::declare_lint! {
    pub VMUX_EXPORTED_SYSTEM,
    Deny,
    "registered Vmux systems must remain private to their defining module"
}

rustc_session::declare_lint! {
    pub VMUX_ECS_OWNERSHIP,
    Allow,
    "Vmux state must have a resolved registered ECS owner"
}

rustc_session::declare_lint! {
    pub VMUX_SYSTEM_NAME,
    Deny,
    "registered Vmux systems must use short operation names within their module"
}

#[unsafe(no_mangle)]
pub fn register_lints(_: &rustc_session::Session, store: &mut rustc_lint::LintStore) {
    store.register_lints(&[VMUX_EXPORTED_SYSTEM, VMUX_SYSTEM_NAME, VMUX_ECS_OWNERSHIP]);
    store.register_late_lint_pass(Box::new(|_| Box::new(VmuxExportedSystem::default())));
}

rustc_session::impl_lint_pass!(VmuxExportedSystem<'_> => [VMUX_EXPORTED_SYSTEM, VMUX_SYSTEM_NAME, VMUX_ECS_OWNERSHIP]);

#[derive(Default)]
struct VmuxExportedSystem<'tcx> {
    named_systems: HashSet<DefId>,
    ownership: ownership::Ownership<'tcx>,
    reported: HashSet<DefId>,
    configurations: HashSet<HirId>,
}

impl<'tcx> LateLintPass<'tcx> for VmuxExportedSystem<'tcx> {
    fn check_crate(&mut self, cx: &LateContext<'tcx>) {
        self.ownership.traits(cx);
    }

    fn check_crate_post(&mut self, cx: &LateContext<'tcx>) {
        self.ownership.finish(cx);
    }

    fn check_expr(&mut self, cx: &LateContext<'tcx>, expression: &'tcx Expr<'tcx>) {
        if !Self::applies(cx) {
            return;
        }
        self.ownership.call(cx, expression);
        self.register(cx, expression);
    }

    fn check_body(&mut self, cx: &LateContext<'tcx>, body: &Body<'tcx>) {
        if !Self::applies(cx) {
            return;
        }
        let method = body.value.hir_id.owner.def_id.to_def_id();
        let Some(interface) = Self::trait_owner(cx, method) else {
            return;
        };
        if cx.tcx.crate_name(interface.krate).as_str() != "bevy_ecs"
            || cx.tcx.item_name(interface).as_str() != "Component"
            || !matches!(
                cx.tcx.item_name(method).as_str(),
                "on_add" | "on_insert" | "on_discard" | "on_remove" | "on_despawn"
            )
        {
            return;
        }
        self.configuration(cx, body.value);
    }
}

impl<'tcx> VmuxExportedSystem<'tcx> {
    fn applies(cx: &LateContext<'_>) -> bool {
        let package = std::env::var("CARGO_PKG_NAME")
            .unwrap_or_else(|_| cx.tcx.crate_name(LOCAL_CRATE).as_str().to_owned());
        package.starts_with("vmux_")
    }

    fn register(&mut self, cx: &LateContext<'tcx>, expression: &'tcx Expr<'tcx>) {
        let (method, arguments, qualified) = match expression.kind {
            ExprKind::MethodCall(_, _, arguments, _) => {
                let Some(method) = cx.typeck_results().type_dependent_def_id(expression.hir_id)
                else {
                    return;
                };
                (method, arguments, false)
            }
            ExprKind::Call(function, arguments) => {
                let TyKind::FnDef(method, _) = cx.typeck_results().expr_ty(function).kind() else {
                    return;
                };
                (*method, arguments, true)
            }
            _ => return,
        };
        let Some(index) = Self::registration_argument(cx, method, qualified) else {
            return;
        };
        if let Some(systems) = arguments.get(index) {
            self.ownership.site = Some(expression.hir_id.owner.def_id.to_def_id());
            self.configuration(cx, systems);
        }
    }

    fn registration_argument(
        cx: &LateContext<'_>,
        method: DefId,
        qualified: bool,
    ) -> Option<usize> {
        if let Some(interface) = Self::trait_owner(cx, method)
            && cx.tcx.crate_name(interface.krate).as_str() == "bevy_ecs"
            && cx.tcx.item_name(interface).as_str() == "RunSystemOnce"
            && matches!(
                cx.tcx.item_name(method).as_str(),
                "run_system_once" | "run_system_once_with"
            )
        {
            return Some(usize::from(qualified));
        }
        let Some(implementation) = cx.tcx.impl_of_assoc(method) else {
            return None;
        };
        let TyKind::Adt(owner, _) = cx
            .tcx
            .type_of(implementation)
            .instantiate_identity()
            .skip_norm_wip()
            .kind()
        else {
            return None;
        };
        let package = cx.tcx.crate_name(method.krate);
        let owner = cx.tcx.item_name(owner.did());
        let name = cx.tcx.item_name(method);
        match (package.as_str(), owner.as_str(), name.as_str()) {
            ("bevy_app", "App" | "SubApp", "add_systems") => Some(1 + usize::from(qualified)),
            ("bevy_app", "App" | "SubApp", "add_observer")
            | ("bevy_ecs", "World" | "Commands", "add_observer")
            | ("bevy_ecs", "EntityCommands" | "EntityWorldMut", "observe") => {
                Some(usize::from(qualified))
            }
            (
                "bevy_ecs",
                "ComponentHooks",
                "on_add" | "on_insert" | "on_discard" | "on_remove" | "on_despawn",
            ) => Some(usize::from(qualified)),
            ("bevy_ecs", "Observer", "new") if qualified => Some(0),
            _ => None,
        }
    }

    fn configuration(&mut self, cx: &LateContext<'tcx>, expression: &'tcx Expr<'tcx>) {
        if !self.configurations.insert(expression.hir_id) {
            return;
        }
        self.visit_configuration(cx, expression);
        self.configurations.remove(&expression.hir_id);
    }

    fn visit_configuration(&mut self, cx: &LateContext<'tcx>, expression: &'tcx Expr<'tcx>) {
        self.system_type(cx, cx.typeck_results().expr_ty(expression));
        match expression.kind {
            ExprKind::Tup(values) => {
                for value in values {
                    self.configuration(cx, value);
                }
            }
            ExprKind::MethodCall(_, receiver, arguments, _) => {
                let Some(method) = cx.typeck_results().type_dependent_def_id(expression.hir_id)
                else {
                    return;
                };
                if Self::configuration_method(cx, method) {
                    self.configuration(cx, receiver);
                    if matches!(
                        cx.tcx.item_name(method).as_str(),
                        "pipe" | "run_if" | "distributive_run_if"
                    ) {
                        for system in arguments {
                            self.configuration(cx, system);
                        }
                    }
                }
            }
            ExprKind::Path(ref path) => {
                if let Res::Local(binding) = cx.qpath_res(path, expression.hir_id)
                    && let Node::Pat(pattern) = cx.tcx.hir_node(binding)
                    && matches!(pattern.kind, PatKind::Binding(_, _, _, None))
                    && let Some((_, Node::LetStmt(statement))) =
                        cx.tcx.hir_parent_iter(binding).next()
                {
                    let assignments =
                        aliases::Assignments::of(cx, expression, binding, statement.init);
                    for value in assignments.values {
                        self.configuration(cx, value);
                    }
                }
            }
            ExprKind::Call(function, arguments) => {
                let TyKind::FnDef(method, _) = cx.typeck_results().expr_ty(function).kind() else {
                    return;
                };
                if Self::configuration_method(cx, *method) {
                    if let Some(system) = arguments.first() {
                        self.configuration(cx, system);
                    }
                    if matches!(
                        cx.tcx.item_name(*method).as_str(),
                        "pipe" | "run_if" | "distributive_run_if"
                    ) && let Some(system) = arguments.get(1)
                    {
                        self.configuration(cx, system);
                    }
                }
            }
            ExprKind::Cast(value, _) | ExprKind::DropTemps(value) => {
                self.configuration(cx, value);
            }
            ExprKind::Block(block, _) => {
                if let Some(value) = block.expr {
                    self.configuration(cx, value);
                }
            }
            ExprKind::If(_, then, otherwise) => {
                self.configuration(cx, then);
                if let Some(otherwise) = otherwise {
                    self.configuration(cx, otherwise);
                }
            }
            ExprKind::Match(_, arms, _) => {
                for arm in arms {
                    self.configuration(cx, arm.body);
                }
            }
            ExprKind::Loop(..) => {
                for value in aliases::Assignments::exits(expression) {
                    self.configuration(cx, value);
                }
            }
            _ => {}
        }
    }

    fn configuration_method(cx: &LateContext<'_>, method: DefId) -> bool {
        let parent = cx.tcx.parent(method);
        if cx.tcx.crate_name(method.krate).as_str() == "core"
            && cx.tcx.item_name(parent).as_str() == "Some"
            && cx.tcx.item_name(cx.tcx.parent(parent)).as_str() == "Option"
        {
            return true;
        }
        let Some(trait_id) = Self::trait_owner(cx, method) else {
            return false;
        };
        cx.tcx.crate_name(trait_id.krate).as_str() == "bevy_ecs"
            && matches!(
                cx.tcx.item_name(trait_id).as_str(),
                "IntoScheduleConfigs" | "IntoSystem"
            )
    }

    fn trait_owner(cx: &LateContext<'_>, method: DefId) -> Option<DefId> {
        if let Some(owner) = cx.tcx.trait_of_assoc(method) {
            return Some(owner);
        }
        let method = cx.tcx.trait_item_of(method)?;
        cx.tcx.trait_of_assoc(method)
    }

    fn system_type(&mut self, cx: &LateContext<'tcx>, ty: Ty<'tcx>) {
        match ty.kind() {
            TyKind::FnDef(function, arguments) => {
                self.ownership
                    .system(cx, *function, arguments.skip_binder());
                self.system(cx, *function);
            }
            TyKind::Tuple(values) => {
                for value in values.iter() {
                    self.system_type(cx, value);
                }
            }
            _ => {}
        }
    }

    fn system(&mut self, cx: &LateContext<'_>, function: DefId) {
        let Some(local) = function.as_local() else {
            return;
        };
        self.check_name(cx, function);
        let Visibility::Restricted(scope) = cx.tcx.visibility(function) else {
            self.report(cx, function);
            return;
        };
        if scope != cx.tcx.parent_module_from_def_id(local).to_mod_id() {
            self.report(cx, function);
        }
    }

    fn report(&mut self, cx: &LateContext<'_>, function: DefId) {
        if !self.reported.insert(function) {
            return;
        }
        cx.tcx.emit_node_span_lint(
            VMUX_EXPORTED_SYSTEM,
            cx.tcx.local_def_id_to_hir_id(function.expect_local()),
            cx.tcx.def_span(function),
            DiagDecorator(|diagnostic| {
                diagnostic.primary_message(format!(
                    "registered system `{}` must be private to its defining module",
                    cx.tcx.item_name(function)
                ));
                diagnostic.help(
                    "register the system inside its owning plugin and remove the wider visibility",
                );
            }),
        );
    }
}
