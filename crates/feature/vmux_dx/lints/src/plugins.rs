use rustc_errors::DiagDecorator;
use rustc_hir::def::{DefKind, Res};
use rustc_hir::def_id::DefId;
use rustc_hir::{HirId, Item, ItemKind, UseKind};
use rustc_lint::LateContext;
use rustc_middle::ty::{Ty, TyKind};
use rustc_span::Span;

use crate::VMUX_PLUGIN_NAME;

impl crate::VmuxExportedSystem<'_> {
    pub(super) fn collect_plugins(&mut self, cx: &LateContext<'_>) {
        if !self.owned {
            return;
        }
        for package in cx.tcx.crates(()).iter().copied() {
            if cx.tcx.crate_name(package).as_str() != "bevy_app" {
                continue;
            }
            for interface in cx.tcx.traits(package) {
                if cx.tcx.item_name(*interface).as_str() != "Plugin" {
                    continue;
                }
                for implementation in cx.tcx.all_impls(*interface) {
                    let ty = cx
                        .tcx
                        .type_of(implementation)
                        .instantiate_identity()
                        .skip_norm_wip();
                    if let TyKind::Adt(owner, _) = ty.kind() {
                        self.plugin_types.insert(owner.did());
                    }
                }
            }
        }
    }

    pub(super) fn check_plugin_item(&self, cx: &LateContext<'_>, item: &Item<'_>) {
        if !self.owned {
            return;
        }
        match item.kind {
            ItemKind::Struct(..)
            | ItemKind::Enum(..)
            | ItemKind::Union(..)
            | ItemKind::TyAlias(..) => {
                let ty = cx
                    .tcx
                    .type_of(item.owner_id.def_id)
                    .instantiate_identity()
                    .skip_norm_wip();
                let name = cx.tcx.item_name(item.owner_id.def_id.to_def_id());
                self.plugin_name(cx, ty, item.hir_id(), item.span, name.as_str());
            }
            ItemKind::Use(path, UseKind::Single(name)) => {
                if let Some(Res::Def(_, target)) = path.res.type_ns {
                    self.imported_plugin(cx, item, target, name.name.as_str());
                }
            }
            ItemKind::Use(path, UseKind::Glob) => {
                if let Some(Res::Def(DefKind::Mod, module)) = path.res.type_ns {
                    self.glob_plugins(cx, item, module);
                }
            }
            _ => {}
        }
    }

    fn imported_plugin(&self, cx: &LateContext<'_>, item: &Item<'_>, target: DefId, name: &str) {
        if !matches!(
            cx.tcx.def_kind(target),
            DefKind::Struct | DefKind::Enum | DefKind::Union | DefKind::TyAlias
        ) {
            return;
        }
        let ty = cx
            .tcx
            .type_of(target)
            .instantiate_identity()
            .skip_norm_wip();
        self.plugin_name(cx, ty, item.hir_id(), item.span, name);
    }

    fn glob_plugins(&self, cx: &LateContext<'_>, item: &Item<'_>, module: DefId) {
        let children = match module.as_local() {
            Some(module) => cx.tcx.module_children_local(module),
            None => cx.tcx.module_children(module),
        };
        let scope = cx.tcx.parent(item.owner_id.def_id.to_def_id());
        for child in children {
            if !child.vis.is_accessible_from(scope, cx.tcx) {
                continue;
            }
            if let Res::Def(_, target) = child.res {
                self.imported_plugin(cx, item, target, child.ident.name.as_str());
            }
        }
    }

    fn plugin_name(&self, cx: &LateContext<'_>, ty: Ty<'_>, node: HirId, span: Span, name: &str) {
        if name != "Plugin" && name.ends_with("Plugin") {
            return;
        }
        let TyKind::Adt(owner, _) = ty.kind() else {
            return;
        };
        if !self.plugin_types.contains(&owner.did()) {
            return;
        }
        cx.tcx.emit_node_span_lint(
            VMUX_PLUGIN_NAME,
            node,
            span,
            DiagDecorator(|diagnostic| {
                diagnostic.primary_message(format!(
                    "Bevy plugin `{name}` must use a descriptive name ending in `Plugin`"
                ));
            }),
        );
    }
}
