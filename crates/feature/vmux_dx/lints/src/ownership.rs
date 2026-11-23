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
    bundles: Vec<(Ty<'tcx>, DefId)>,
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
                ("bevy_ecs", "Message" | "Event" | "EntityEvent") => "message",
                ("bevy_ecs", "SystemParam") => "parameter",
                ("bevy_ecs", "QueryData") => "query",
                ("bevy_ecs", "SystemSet") => "system_set",
                ("bevy_ecs", "Bundle") => "bundle",
                ("bevy_ecs", "Command") => "command",
                ("bevy_app", "Plugin") => "plugin",
                ("vmux_ecs", "Job") => "job",
                ("vmux_ecs", "Schema") => "schema",
                ("vmux_page", "Page" | "PageComponent") => "page",
                ("vmux_feature", "FeatureManifestSource") => "manifest",
                ("vmux_api", "UiState" | "UiEvent" | "Contract") => "contract",
                ("inventory", "Collect") => "inventory",
                ("dioxus_core", "Properties") => "props",
                ("dioxus_router" | "dioxus_router_core", "Routable") => "routable",
                ("serde" | "serde_core", "Serialize" | "Deserialize") => "serde",
                ("core" | "std", "Error") => "error",
                ("core" | "std", "Drop") => "drop",
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
        let caller = expression.hir_id.owner.def_id.to_def_id();
        if let ExprKind::MethodCall(_, _, arguments, _) = expression.kind
            && !function.is_local()
            && cx.tcx.crate_name(function.krate).as_str() == "bevy_ecs"
            && matches!(
                cx.tcx.item_name(function).as_str(),
                "spawn" | "insert" | "try_insert"
            )
            && let Some(bundle) = arguments.first()
        {
            self.bundles
                .push((cx.typeck_results().expr_ty(bundle), caller));
        }
        if function.is_local() {
            self.callers.entry(function).or_default().insert(caller);
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
                            let mut types = arguments.types();
                            if let Some(data) = types.next() {
                                self.data(cx, data, &mut HashSet::new());
                            }
                            if let Some(filter) = types.next() {
                                self.filter(cx, filter, &mut HashSet::new());
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
                } else if package.as_str() == "bevy_ecs" && name.as_str() == "Has" {
                    if let Some(value) = arguments.types().next() {
                        self.marker_filter(value);
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

    fn filter(&mut self, cx: &LateContext<'tcx>, ty: Ty<'tcx>, visiting: &mut HashSet<Ty<'tcx>>) {
        if !visiting.insert(ty) {
            return;
        }
        match ty.kind() {
            TyKind::Tuple(values) => {
                for value in values.iter() {
                    self.filter(cx, value, visiting);
                }
            }
            TyKind::Adt(owner, arguments) => {
                let package = cx.tcx.crate_name(owner.did().krate);
                let name = cx.tcx.item_name(owner.did());
                if package.as_str() == "bevy_ecs" {
                    match name.as_str() {
                        "With" | "Without" | "Added" | "Changed" => {
                            if let Some(value) = arguments.types().next() {
                                self.marker_filter(value);
                            }
                        }
                        "Or" => {
                            for value in arguments.types() {
                                self.filter(cx, value, visiting);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn marker_filter(&mut self, ty: Ty<'tcx>) {
        let TyKind::Adt(owner, _) = ty.kind() else {
            return;
        };
        if !owner.did().is_local() || !self.has_role(owner.did(), "component") {
            return;
        }
        if owner.is_struct() && owner.all_fields().next().is_none() {
            self.used.insert(owner.did());
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
        let canonical = self
            .roles
            .iter()
            .filter_map(|(id, roles)| {
                roles
                    .iter()
                    .any(|role| {
                        matches!(
                            *role,
                            "component" | "resource" | "message" | "bundle" | "query" | "parameter"
                        )
                    })
                    .then_some(*id)
            })
            .collect();
        let dto_roles = self
            .roles
            .iter()
            .filter_map(|(id, roles)| {
                let is_ecs = roles.iter().any(|role| {
                    matches!(
                        *role,
                        "component" | "resource" | "message" | "bundle" | "query" | "parameter"
                    )
                });
                let is_dto = roles
                    .iter()
                    .any(|role| matches!(*role, "contract" | "serde" | "error"));
                (!is_ecs && is_dto).then_some(*id)
            })
            .collect();
        let mut graph = ValueGraph {
            root: owner.did(),
            canonical,
            dtos: dto_roles,
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
        if cx.tcx.sess.opts.test
            || !package.starts_with("vmux_")
            || package == "vmux_macro"
            || std::env::var("CARGO_CRATE_NAME").as_deref() == Ok("build_script_build")
        {
            return;
        }
        for (function, arguments, site) in std::mem::take(&mut self.systems) {
            if !self.reachable(site, &mut HashSet::new()) {
                continue;
            }
            self.entrypoints.insert(function);
            let signature = cx.tcx.fn_sig(function).instantiate(cx.tcx, arguments);
            for input in signature.skip_binder().inputs() {
                self.parameter(cx, *input, function, &mut HashSet::new());
            }
        }
        for (bundle, caller) in std::mem::take(&mut self.bundles) {
            if self.reachable(caller, &mut HashSet::new()) {
                self.bundle_type(cx, bundle, &mut HashSet::new());
            }
        }
        for local in cx.tcx.hir_crate_items(()).definitions() {
            let definition = local.to_def_id();
            if !matches!(
                cx.tcx.def_kind(definition),
                DefKind::Struct | DefKind::Enum | DefKind::Union
            ) || cx.tcx.def_span(definition).from_expansion()
                || self.stateless_plugin(cx, definition)
                || self.stateless_system_set(cx, definition)
                || self.stateless_unit_role(cx, definition, "manifest")
                || self.stateless_unit_role(cx, definition, "schema")
                || self.stateless_unit_role(cx, definition, "page")
                || self.concrete_job_struct(cx, definition)
                || self.has_role(definition, "command")
                    && matches!(cx.tcx.def_kind(definition), DefKind::Struct)
                || self.has_role(definition, "serde")
                || self.has_role(definition, "error")
                || self.has_role(definition, "drop")
                    && !self.has_role(definition, "component")
                    && !self.has_role(definition, "resource")
                    && !self.has_role(definition, "message")
                    && !self.has_role(definition, "bundle")
                    && !self.has_role(definition, "query")
                    && !self.has_role(definition, "parameter")
                    && matches!(cx.tcx.def_kind(definition), DefKind::Struct)
                || self.has_role(definition, "contract")
                || self.stateless_unit_adapter(cx, definition)
                || self.stack_local_or_copy_value(cx, definition)
                || self.has_role(definition, "inventory")
                    && matches!(cx.tcx.def_kind(definition), DefKind::Struct)
                || self.has_role(definition, "props")
                    && matches!(cx.tcx.def_kind(definition), DefKind::Struct)
                || self.has_role(definition, "routable")
                    && matches!(cx.tcx.def_kind(definition), DefKind::Enum)
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

    fn bundle_type(
        &mut self,
        cx: &LateContext<'tcx>,
        ty: Ty<'tcx>,
        visiting: &mut HashSet<Ty<'tcx>>,
    ) {
        if !visiting.insert(ty) {
            return;
        }
        match ty.kind() {
            TyKind::Tuple(elements) => {
                for element in elements.iter() {
                    self.bundle_type(cx, element, visiting);
                }
            }
            TyKind::Adt(owner, arguments) => {
                if !owner.did().is_local() {
                    return;
                }
                if self.has_role(owner.did(), "component") {
                    self.root(cx, ty);
                    return;
                }
                if self.has_role(owner.did(), "bundle") {
                    self.used.insert(owner.did());
                    for field in owner.all_fields() {
                        self.bundle_type(cx, field.ty(cx.tcx, arguments).skip_norm_wip(), visiting);
                    }
                }
            }
            _ => {}
        }
    }

    fn stateless_system_set(&self, cx: &LateContext<'tcx>, definition: DefId) -> bool {
        if !self.has_role(definition, "system_set") {
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
        if owner.is_enum() {
            return owner
                .variants()
                .iter()
                .all(|variant| variant.fields.is_empty());
        }
        for field in owner.all_fields() {
            let ty = field.ty(cx.tcx, arguments).skip_norm_wip();
            let TyKind::Adt(marker, _) = ty.kind() else {
                return false;
            };
            let package = cx.tcx.crate_name(marker.did().krate);
            let name = cx.tcx.item_name(marker.did());
            if package.as_str() != "core" || !matches!(name.as_str(), "PhantomData" | "TypeId") {
                return false;
            }
        }
        true
    }

    fn stack_local_or_copy_value(&self, cx: &LateContext<'tcx>, definition: DefId) -> bool {
        if self.roles.contains_key(&definition)
            || !matches!(cx.tcx.def_kind(definition), DefKind::Struct)
        {
            return false;
        }
        let ty = cx
            .tcx
            .type_of(definition)
            .instantiate_identity()
            .skip_norm_wip();
        if cx.tcx.type_is_copy_modulo_regions(cx.typing_env(), ty) {
            return true;
        }
        let generics = cx.tcx.generics_of(definition);
        if generics.own_counts().lifetimes == 0 {
            return false;
        }
        let TyKind::Adt(owner, arguments) = ty.kind() else {
            return false;
        };
        owner
            .all_fields()
            .any(|field| Self::has_direct_borrow(cx, field.ty(cx.tcx, arguments).skip_norm_wip()))
    }

    fn has_direct_borrow(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> bool {
        match ty.kind() {
            TyKind::Ref(..) => true,
            TyKind::Tuple(elements) => elements
                .iter()
                .any(|element| Self::has_direct_borrow(cx, element)),
            TyKind::Adt(owner, arguments)
                if cx.tcx.crate_name(owner.did().krate).as_str() == "core"
                    && cx.tcx.item_name(owner.did()).as_str() == "Option" =>
            {
                arguments
                    .types()
                    .any(|inner| Self::has_direct_borrow(cx, inner))
            }
            _ => false,
        }
    }

    fn stateless_unit_adapter(&self, cx: &LateContext<'tcx>, definition: DefId) -> bool {
        if self.roles.contains_key(&definition) {
            return false;
        }
        let ty = cx
            .tcx
            .type_of(definition)
            .instantiate_identity()
            .skip_norm_wip();
        matches!(ty.kind(), TyKind::Adt(owner, _) if owner.is_struct() && owner.all_fields().next().is_none())
    }

    fn stateless_unit_role(&self, cx: &LateContext<'tcx>, definition: DefId, role: &str) -> bool {
        if !self.has_role(definition, role) {
            return false;
        }
        let ty = cx
            .tcx
            .type_of(definition)
            .instantiate_identity()
            .skip_norm_wip();
        matches!(ty.kind(), TyKind::Adt(owner, _) if owner.is_struct() && owner.all_fields().next().is_none())
    }

    fn concrete_job_struct(&self, cx: &LateContext<'tcx>, definition: DefId) -> bool {
        self.has_role(definition, "job") && matches!(cx.tcx.def_kind(definition), DefKind::Struct)
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
    dtos: HashSet<DefId>,
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
                    let is_copy = cx.tcx.type_is_copy_modulo_regions(cx.typing_env(), ty);
                    let allowed_retained = self.dtos.contains(&owner.did()) || is_copy;
                    if location != self.root
                        && (self.canonical.contains(&owner.did()) || retained && !allowed_retained)
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
                            retained && !allowed_retained,
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
            TyKind::Param(..) => return true,
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
