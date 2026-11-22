use std::collections::{HashMap, HashSet};

use rustc_errors::DiagDecorator;
use rustc_hir::def::DefKind;
use rustc_hir::def_id::DefId;
use rustc_hir::{Expr, ExprKind};
use rustc_lint::LateContext;
use rustc_middle::ty::{GenericArgsRef, Ty, TyKind};

use crate::VMUX_ECS_OWNERSHIP;

#[derive(Default)]
pub(super) struct Ownership<'tcx> {
    pub site: Option<DefId>,
    entrypoints: HashSet<DefId>,
    callers: HashMap<DefId, HashSet<DefId>>,
    systems: Vec<(DefId, GenericArgsRef<'tcx>, DefId)>,
    roles: HashMap<DefId, HashSet<&'static str>>,
    used: HashSet<DefId>,
    owners: HashMap<DefId, HashSet<DefId>>,
    reported: HashSet<(DefId, String)>,
}

impl<'tcx> Ownership<'tcx> {
    pub fn traits(&mut self, cx: &LateContext<'tcx>) {
        if let Some((entry, _)) = cx.tcx.entry_fn(()) {
            self.entrypoints.insert(entry);
        }
        for (interface, implementations) in cx.tcx.all_local_trait_impls(()) {
            let package = cx.tcx.crate_name(interface.krate);
            let name = cx.tcx.item_name(*interface);
            let role = match (package.as_str(), name.as_str()) {
                ("bevy_ecs", "Component") => "component",
                ("bevy_ecs", "Resource") => "resource",
                ("bevy_ecs", "Message" | "Event") => "message",
                ("bevy_ecs", "SystemParam") => "parameter",
                ("bevy_ecs", "QueryData") => "query",
                ("bevy_app", "Plugin") => "plugin",
                _ => continue,
            };
            for implementation in implementations {
                if matches!(role, "plugin" | "component") {
                    for method in cx
                        .tcx
                        .associated_items(implementation.to_def_id())
                        .in_definition_order()
                    {
                        if role == "plugin" && method.name().as_str() == "build"
                            || role == "component"
                                && matches!(
                                    method.name().as_str(),
                                    "on_add"
                                        | "on_insert"
                                        | "on_discard"
                                        | "on_remove"
                                        | "on_despawn"
                                )
                        {
                            self.entrypoints.insert(method.def_id);
                        }
                    }
                }
                let ty = cx.tcx.type_of(*implementation).instantiate_identity();
                if let TyKind::Adt(owner, _) = ty.skip_norm_wip().kind() {
                    self.roles.entry(owner.did()).or_default().insert(role);
                }
            }
        }
    }

    pub fn call(&mut self, cx: &LateContext<'tcx>, expression: &'tcx Expr<'tcx>) {
        let function = match expression.kind {
            ExprKind::Call(function, _) => {
                let TyKind::FnDef(function, _) = cx.typeck_results().expr_ty(function).kind()
                else {
                    return;
                };
                *function
            }
            ExprKind::MethodCall(..) => {
                let Some(function) = cx.typeck_results().type_dependent_def_id(expression.hir_id)
                else {
                    return;
                };
                function
            }
            _ => return,
        };
        if function.is_local() {
            self.callers
                .entry(function)
                .or_default()
                .insert(expression.hir_id.owner.def_id.to_def_id());
        }
    }

    pub fn system(
        &mut self,
        _: &LateContext<'tcx>,
        function: DefId,
        arguments: GenericArgsRef<'tcx>,
    ) {
        if function.is_local()
            && let Some(site) = self.site
        {
            self.systems.push((function, arguments, site));
        }
    }

    pub(super) fn has_role(&self, owner: DefId, role: &str) -> bool {
        self.roles
            .get(&owner)
            .is_some_and(|roles| roles.contains(role))
    }

    fn reachable(&self, function: DefId, visiting: &mut HashSet<DefId>) -> bool {
        if self.entrypoints.contains(&function) {
            return true;
        }
        if !visiting.insert(function) {
            return false;
        }
        if let Some(callers) = self.callers.get(&function) {
            for caller in callers {
                if self.reachable(*caller, visiting) {
                    return true;
                }
            }
        }
        false
    }

    fn parameter(
        &mut self,
        cx: &LateContext<'tcx>,
        ty: Ty<'tcx>,
        function: DefId,
        visiting: &mut HashSet<Ty<'tcx>>,
    ) {
        if !visiting.insert(ty) {
            return;
        }
        match ty.kind() {
            TyKind::Tuple(values) => {
                for value in values.iter() {
                    self.parameter(cx, value, function, visiting);
                }
            }
            TyKind::Adt(owner, arguments) => {
                let package = cx.tcx.crate_name(owner.did().krate);
                let name = cx.tcx.item_name(owner.did());
                if package.as_str() == "bevy_ecs" {
                    match name.as_str() {
                        "Local" => self.report(
                            cx,
                            function,
                            "registered system retains Local state without a canonical ECS owner",
                        ),
                        "Query" | "Single" | "Populated" => {
                            if let Some(data) = arguments.types().next() {
                                self.data(cx, data, &mut HashSet::new());
                            }
                        }
                        "Res" | "ResMut" | "MessageReader" | "MessageWriter" | "On" => {
                            if let Some(data) = arguments.types().next() {
                                self.root(cx, data);
                            }
                        }
                        "ParamSet" => {
                            for data in arguments.types() {
                                self.parameter(cx, data, function, visiting);
                            }
                        }
                        _ => {}
                    }
                    return;
                }
                if self.has_role(owner.did(), "parameter") {
                    self.used.insert(owner.did());
                    for field in owner.all_fields() {
                        self.parameter(
                            cx,
                            field.ty(cx.tcx, arguments).skip_norm_wip(),
                            function,
                            visiting,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    fn data(&mut self, cx: &LateContext<'tcx>, ty: Ty<'tcx>, visiting: &mut HashSet<Ty<'tcx>>) {
        if !visiting.insert(ty) {
            return;
        }
        match ty.kind() {
            TyKind::Ref(_, value, _) => self.root(cx, *value),
            TyKind::Tuple(values) => {
                for value in values.iter() {
                    self.data(cx, value, visiting);
                }
            }
            TyKind::Adt(owner, arguments) => {
                let package = cx.tcx.crate_name(owner.did().krate);
                let name = cx.tcx.item_name(owner.did());
                if package.as_str() == "bevy_ecs" && matches!(name.as_str(), "Ref" | "Mut") {
                    if let Some(value) = arguments.types().next() {
                        self.root(cx, value);
                    }
                } else if self.has_role(owner.did(), "query") {
                    self.used.insert(owner.did());
                    for field in owner.all_fields() {
                        self.data(cx, field.ty(cx.tcx, arguments).skip_norm_wip(), visiting);
                    }
                } else if package.as_str() == "core" && name.as_str() == "Option" {
                    for value in arguments.types() {
                        self.data(cx, value, visiting);
                    }
                }
            }
            _ => {}
        }
    }

    fn root(&mut self, cx: &LateContext<'tcx>, ty: Ty<'tcx>) {
        let TyKind::Adt(owner, _) = ty.kind() else {
            return;
        };
        if !owner.did().is_local() {
            return;
        }
        if !self.has_role(owner.did(), "component")
            && !self.has_role(owner.did(), "resource")
            && !self.has_role(owner.did(), "message")
        {
            return;
        }
        self.used.insert(owner.did());
        let mut graph = ValueGraph {
            root: owner.did(),
            canonical: self.roles.keys().copied().collect(),
            values: HashSet::new(),
            visiting: HashSet::new(),
            issues: Vec::new(),
        };
        let valid = graph.check(cx, ty, false, owner.did());
        for (location, message) in graph.issues {
            self.report(cx, location, message);
        }
        if valid {
            for value in graph.values {
                self.owners.entry(value).or_default().insert(owner.did());
            }
        }
    }

    pub fn finish(&mut self, cx: &LateContext<'tcx>) {
        let package = std::env::var("CARGO_PKG_NAME").unwrap_or_else(|_| {
            cx.tcx
                .crate_name(rustc_hir::def_id::LOCAL_CRATE)
                .as_str()
                .to_owned()
        });
        if !package.starts_with("vmux_") {
            return;
        }
        for (function, arguments, site) in std::mem::take(&mut self.systems) {
            if !self.reachable(site, &mut HashSet::new()) {
                continue;
            }
            let signature = cx.tcx.fn_sig(function).instantiate(cx.tcx, arguments);
            for input in signature.skip_binder().inputs() {
                self.parameter(cx, *input, function, &mut HashSet::new());
            }
        }
        for local in cx.tcx.hir_crate_items(()).definitions() {
            let definition = local.to_def_id();
            if !matches!(
                cx.tcx.def_kind(definition),
                DefKind::Struct | DefKind::Enum | DefKind::Union
            ) || cx.tcx.def_span(definition).from_expansion()
                || self.stateless_plugin(cx, definition)
                || self.used.contains(&definition)
                || self
                    .owners
                    .get(&definition)
                    .is_some_and(|owners| owners.len() == 1)
            {
                continue;
            }
            self.report(
                cx,
                definition,
                "declaration has no unique resolved registered ECS owner",
            );
        }
    }

    fn stateless_plugin(&self, cx: &LateContext<'tcx>, definition: DefId) -> bool {
        if !self.has_role(definition, "plugin") {
            return false;
        }
        let ty = cx
            .tcx
            .type_of(definition)
            .instantiate_identity()
            .skip_norm_wip();
        let TyKind::Adt(owner, arguments) = ty.kind() else {
            return false;
        };
        for field in owner.all_fields() {
            let ty = field.ty(cx.tcx, arguments).skip_norm_wip();
            let TyKind::Adt(marker, _) = ty.kind() else {
                return false;
            };
            if cx.tcx.crate_name(marker.did().krate).as_str() != "core"
                || cx.tcx.item_name(marker.did()).as_str() != "PhantomData"
            {
                return false;
            }
        }
        true
    }

    fn report(&mut self, cx: &LateContext<'_>, definition: DefId, message: &'static str) {
        if !definition.is_local() || !self.reported.insert((definition, message.into())) {
            return;
        }
        cx.tcx.emit_node_span_lint(
            VMUX_ECS_OWNERSHIP,
            cx.tcx.local_def_id_to_hir_id(definition.expect_local()),
            cx.tcx.def_span(definition),
            DiagDecorator(|diagnostic| {
                diagnostic.primary_message(message);
                diagnostic.help("use canonical entities and registered typed systems; external adapters need their own proven contract");
            }),
        );
    }
}

struct ValueGraph<'tcx> {
    root: DefId,
    canonical: HashSet<DefId>,
    values: HashSet<DefId>,
    visiting: HashSet<Ty<'tcx>>,
    issues: Vec<(DefId, &'static str)>,
}

impl<'tcx> ValueGraph<'tcx> {
    fn check(
        &mut self,
        cx: &LateContext<'tcx>,
        ty: Ty<'tcx>,
        retained: bool,
        location: DefId,
    ) -> bool {
        if !self.visiting.insert(ty) {
            self.issues
                .push((location, "canonical ECS state retains a recursive model"));
            return false;
        }
        let valid = self.inspect(cx, ty, retained, location);
        self.visiting.remove(&ty);
        valid
    }

    fn inspect(
        &mut self,
        cx: &LateContext<'tcx>,
        ty: Ty<'tcx>,
        retained: bool,
        location: DefId,
    ) -> bool {
        match ty.kind() {
            TyKind::Dynamic(..) | TyKind::FnPtr(..) | TyKind::FnDef(..) => {
                self.issues
                    .push((location, "canonical ECS state hides a dynamic engine"));
                return false;
            }
            TyKind::Ref(_, value, _) | TyKind::RawPtr(value, _) | TyKind::Pat(value, _) => {
                return self.check(cx, *value, retained, location);
            }
            TyKind::Slice(value) | TyKind::Array(value, _) => {
                return self.check(cx, *value, true, location);
            }
            TyKind::Tuple(elements) => {
                let mut valid = true;
                for value in elements.iter() {
                    valid &= self.check(cx, value, retained, location);
                }
                return valid;
            }
            TyKind::Adt(owner, arguments) => {
                if owner.did().is_local() {
                    if location != self.root && (retained || self.canonical.contains(&owner.did()))
                    {
                        self.issues.push((
                            location,
                            "canonical ECS state embeds an independently owned model",
                        ));
                        return false;
                    }
                    self.values.insert(owner.did());
                    let mut valid = true;
                    for field in owner.all_fields() {
                        valid &= self.check(
                            cx,
                            field.ty(cx.tcx, arguments).skip_norm_wip(),
                            retained,
                            field.did,
                        );
                    }
                    return valid;
                }
                return self.external(cx, ty, retained, location);
            }
            TyKind::Alias(..) => {
                let result = cx.tcx.try_normalize_erasing_regions(
                    rustc_middle::ty::TypingEnv::post_analysis(cx.tcx, self.root),
                    rustc_middle::ty::Unnormalized::new_wip(ty),
                );
                if let Ok(resolved) = result
                    && resolved != ty
                {
                    return self.check(cx, resolved, retained, location);
                }
            }
            TyKind::Param(..) => {}
            TyKind::Bool
            | TyKind::Char
            | TyKind::Int(_)
            | TyKind::Uint(_)
            | TyKind::Float(_)
            | TyKind::Str
            | TyKind::Never => return true,
            _ => {}
        }
        self.issues.push((
            location,
            "canonical ECS state has an unresolved ownership type",
        ));
        false
    }

    fn external(
        &mut self,
        cx: &LateContext<'tcx>,
        ty: Ty<'tcx>,
        retained: bool,
        location: DefId,
    ) -> bool {
        let TyKind::Adt(owner, arguments) = ty.kind() else {
            return false;
        };
        if owner.is_box()
            && let Some(value) = arguments.types().next()
        {
            return self.check(cx, value, retained, location);
        }
        let package = cx.tcx.crate_name(owner.did().krate);
        let name = cx.tcx.item_name(owner.did());
        if package.as_str() == "core" && name.as_str() == "PhantomData" {
            return true;
        }
        if !Self::local(ty) {
            return true;
        }
        let collection = matches!(package.as_str(), "alloc" | "std" | "hashbrown")
            && matches!(
                name.as_str(),
                "Vec" | "VecDeque" | "HashMap" | "BTreeMap" | "HashSet" | "BTreeSet"
            );
        let mut valid = true;
        if collection {
            for value in arguments.types() {
                valid &= self.check(cx, value, true, location);
            }
        } else {
            for field in owner.all_fields() {
                valid &= self.check(
                    cx,
                    field.ty(cx.tcx, arguments).skip_norm_wip(),
                    retained,
                    location,
                );
            }
        }
        valid
    }

    fn local(ty: Ty<'tcx>) -> bool {
        for argument in ty.walk() {
            let Some(ty) = argument.as_type() else {
                continue;
            };
            match ty.kind() {
                TyKind::Adt(owner, _) if owner.did().is_local() => return true,
                TyKind::Adt(..)
                | TyKind::Ref(..)
                | TyKind::RawPtr(..)
                | TyKind::Pat(..)
                | TyKind::Array(..)
                | TyKind::Slice(..)
                | TyKind::Tuple(..)
                | TyKind::Bool
                | TyKind::Char
                | TyKind::Int(_)
                | TyKind::Uint(_)
                | TyKind::Float(_)
                | TyKind::Str
                | TyKind::Never => {}
                _ => return true,
            }
        }
        false
    }
}
