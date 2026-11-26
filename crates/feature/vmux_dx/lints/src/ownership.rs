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
    world_uses: Vec<(Ty<'tcx>, DefId, bool)>,
    roles: HashMap<DefId, HashSet<&'static str>>,
    used: HashSet<DefId>,
    type_refs: HashMap<DefId, HashSet<DefId>>,
    io_adapters: HashSet<DefId>,
    ui_callers: HashSet<DefId>,
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
                ("bevy_ecs", "SystemSet" | "ScheduleLabel") => "system_set",
                ("bevy_ecs", "Bundle") => "bundle",
                ("bevy_ecs", "Command") => "command",
                ("bevy_ecs", "Relationship" | "RelationshipTarget") => "relationship",
                ("bevy_app", "Plugin") => "plugin",
                ("bevy_app", "PluginGroup") => "plugin_group",
                ("bevy_reflect", "FromType") => "from_type",
                ("bevy_reflect", "Reflect") => "reflect",
                ("vmux_job", "Job") => "job",
                ("vmux_ecs" | "vmux_store", "Schema") => "schema",
                ("vmux_page", "Page" | "PageComponent") => "page",
                ("vmux_feature", "FeatureManifestSource") => "manifest",
                ("vmux_api", "UiState" | "UiEvent" | "Contract" | "UiStateProjection") => {
                    "contract"
                }
                ("inventory", "Collect") => "inventory",
                ("dioxus_core", "Properties") => "props",
                ("dioxus_router" | "dioxus_router_core", "Routable") => "routable",
                ("serde" | "serde_core", "Serialize" | "Deserialize")
                | ("rkyv", "Archive" | "Serialize" | "Deserialize") => "serde",
                ("core" | "std", "Error") => "error",
                ("core" | "std", "Display") => "display",
                ("core" | "std", "Drop") => "drop",
                ("syn", "Visit" | "VisitMut")
                | ("yaml_rust" | "yaml_rust2", "MarkedEventReceiver")
                | ("vte", "Perform") => "parser",
                _ => continue,
            };
            for implementation in implementations {
                if matches!(role, "plugin" | "component" | "command") {
                    for method in cx
                        .tcx
                        .associated_items(implementation.to_def_id())
                        .in_definition_order()
                    {
                        if role == "plugin" && method.name().as_str() == "build"
                            || role == "command" && method.name().as_str() == "apply"
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
                if let TyKind::Adt(owner, _) = ty.skip_norm_wip().kind()
                    && owner.did().is_local()
                {
                    self.roles.entry(owner.did()).or_default().insert(role);
                }
            }
        }
        for implementations in cx.tcx.all_local_trait_impls(()).values() {
            for implementation in implementations {
                let ty = cx
                    .tcx
                    .type_of(*implementation)
                    .instantiate_identity()
                    .skip_norm_wip();
                if let TyKind::Adt(owner, _) = ty.kind()
                    && !owner.did().is_local()
                    && cx.tcx.crate_name(owner.did().krate).as_str() == "bevy_app"
                    && cx.tcx.item_name(owner.did()).as_str() == "App"
                {
                    for method in cx
                        .tcx
                        .associated_items(implementation.to_def_id())
                        .in_definition_order()
                    {
                        self.entrypoints.insert(method.def_id);
                    }
                }
            }
        }
    }

    pub fn call(&mut self, cx: &LateContext<'tcx>, expression: &'tcx Expr<'tcx>) {
        let caller = expression.hir_id.owner.def_id.to_def_id();
        for argument in cx.typeck_results().expr_ty(expression).walk() {
            if let Some(ty) = argument.as_type()
                && let TyKind::Adt(owner, _) = ty.kind()
                && owner.did().is_local()
            {
                self.type_refs
                    .entry(owner.did())
                    .or_default()
                    .insert(caller);
            }
        }
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
        self.record_bevy_call(cx, expression, function, caller);
        if function.is_local() {
            self.callers.entry(function).or_default().insert(caller);
        } else {
            let krate = cx.tcx.crate_name(function.krate);
            let krate_str = krate.as_str();
            if Self::is_external_io_crate(krate_str)
                || Self::is_external_io_call(cx, function, krate_str)
            {
                self.record_io_caller(cx, caller);
            } else if krate_str == "dioxus" || krate_str.starts_with("dioxus_") {
                self.ui_callers.insert(caller);
            }
        }
    }

    fn record_bevy_call(
        &mut self,
        cx: &LateContext<'tcx>,
        expression: &'tcx Expr<'tcx>,
        function: DefId,
        caller: DefId,
    ) {
        if function.is_local() {
            return;
        }
        if let ExprKind::MethodCall(_, _, arguments, _) = expression.kind {
            let krate = cx.tcx.crate_name(function.krate);
            let name = cx.tcx.item_name(function);
            match (krate.as_str(), name.as_str()) {
                ("bevy_ecs", "spawn" | "insert" | "try_insert") => {
                    if let Some(bundle) = arguments.first() {
                        self.bundles
                            .push((cx.typeck_results().expr_ty(bundle), caller));
                    }
                }
                (
                    "bevy_app" | "bevy_ecs",
                    "init_resource"
                    | "init_non_send"
                    | "add_message"
                    | "add_event"
                    | "resource"
                    | "resource_mut"
                    | "get_resource"
                    | "get_resource_mut"
                    | "get_resource_or_init"
                    | "get_resource_or_insert_with"
                    | "contains_resource"
                    | "remove_resource"
                    | "non_send_resource"
                    | "non_send_resource_mut",
                ) => {
                    for ty in cx.typeck_results().node_args(expression.hir_id).types() {
                        self.world_uses.push((ty, caller, false));
                    }
                }
                (
                    "bevy_app" | "bevy_ecs",
                    "insert_resource" | "insert_non_send" | "trigger" | "write_message",
                ) => {
                    if let Some(arg) = arguments.first() {
                        self.world_uses
                            .push((cx.typeck_results().expr_ty(arg), caller, false));
                    }
                }
                _ => {}
            }
        } else if let ExprKind::Call(..) = expression.kind
            && cx.tcx.crate_name(function.krate).as_str() == "bevy_ecs"
            && cx.tcx.item_name(function).as_str() == "new"
            && cx
                .tcx
                .def_path(function)
                .to_string_no_crate_verbose()
                .contains("SystemState")
            && let TyKind::Adt(_, args) = cx.typeck_results().expr_ty(expression).kind()
        {
            for ty in args.types() {
                self.world_uses.push((ty, caller, true));
            }
        }
    }

    fn record_io_caller(&mut self, cx: &LateContext<'tcx>, caller: DefId) {
        self.io_adapters.insert(caller);
        if let Some(implementation) = cx.tcx.impl_of_assoc(caller) {
            let ty = cx
                .tcx
                .type_of(implementation)
                .instantiate_identity()
                .skip_norm_wip();
            if let TyKind::Adt(owner, _) = ty.kind()
                && owner.did().is_local()
            {
                self.io_adapters.insert(owner.did());
            }
        }
    }

    fn is_external_io_call(cx: &LateContext<'tcx>, function: DefId, krate: &str) -> bool {
        if matches!(krate, "tokio_util" | "async_lock") {
            return true;
        }
        if !matches!(krate, "std" | "tokio") {
            return false;
        }
        let path = cx.tcx.def_path(function).to_string_no_crate_verbose();
        [
            "::fs::",
            "::process::",
            "::net::",
            "::io::",
            "::env::",
            "::os::",
            "::thread::",
            "::sync::mpsc::",
            "::sync::",
            "::runtime::",
            "::task::",
            "::time::",
            "::signal::",
            "::path::Path::canonicalize",
            "::path::Path::metadata",
            "::path::Path::symlink_metadata",
            "::path::Path::exists",
            "::path::Path::is_file",
            "::path::Path::is_dir",
            "::path::Path::read_dir",
            "::path::Path::read_link",
        ]
        .iter()
        .any(|seg| path.contains(seg))
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
                        "Res" | "ResMut" | "NonSend" | "NonSendMut" | "MessageReader"
                        | "MessageWriter" | "MessageMutator" | "RemovedComponents" | "On" => {
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
                if package.as_str() == "core" && name.as_str() == "Option" {
                    for data in arguments.types() {
                        self.parameter(cx, data, function, visiting);
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
                            "component" | "resource" | "bundle" | "query" | "parameter"
                        )
                    })
                    .then_some(*id)
            })
            .collect();
        let dto_roles = self
            .roles
            .iter()
            .filter_map(|(id, roles)| {
                let is_dto = roles
                    .iter()
                    .any(|role| matches!(*role, "contract" | "serde" | "error" | "reflect"));
                is_dto.then_some(*id)
            })
            .collect();
        let mut graph = ValueGraph {
            root: owner.did(),
            root_is_message: self.has_role(owner.did(), "message")
                && !self.has_role(owner.did(), "component")
                && !self.has_role(owner.did(), "resource"),
            canonical,
            dtos: dto_roles,
            io_adapters: self.io_adapters.clone(),
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
            || cx.tcx.crate_name(rustc_hir::def_id::LOCAL_CRATE).as_str() == "build_script_build"
            || std::env::var("CARGO_CRATE_NAME").as_deref() == Ok("build_script_build")
        {
            return;
        }
        self.propagate_io_adapters(cx);
        if cx.tcx.entry_fn(()).is_none() {
            for local in cx.tcx.hir_crate_items(()).definitions() {
                let def = local.to_def_id();
                if matches!(cx.tcx.def_kind(def), DefKind::Fn | DefKind::AssocFn)
                    && cx.tcx.visibility(def).is_public()
                {
                    self.entrypoints.insert(def);
                }
            }
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
        for (ty, caller, is_param) in std::mem::take(&mut self.world_uses) {
            if self.reachable(caller, &mut HashSet::new()) {
                if is_param {
                    self.parameter(cx, ty, caller, &mut HashSet::new());
                } else {
                    self.root(cx, ty);
                }
            }
        }
        if cx.tcx.entry_fn(()).is_none() {
            self.register_exported_ecs(cx);
        }
        let helper_owned = self.collect_helper_owned(cx);
        for local in cx.tcx.hir_crate_items(()).definitions() {
            let definition = local.to_def_id();
            if !matches!(
                cx.tcx.def_kind(definition),
                DefKind::Struct | DefKind::Enum | DefKind::Union
            ) || cx.tcx.def_span(definition).from_expansion()
                || self.is_exempt_declaration(cx, definition, &helper_owned)
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

    fn is_exempt_declaration(
        &self,
        cx: &LateContext<'tcx>,
        definition: DefId,
        helper_owned: &HashSet<DefId>,
    ) -> bool {
        self.has_role(definition, "plugin")
            || self.has_role(definition, "relationship")
            || self.stateless_unit_role(cx, definition, "plugin_group")
            || self.has_role(definition, "from_type")
            || self.has_role(definition, "reflect") && !self.is_ecs_role(definition)
            || cx.tcx.entry_fn(()).is_none()
                && cx.tcx.visibility(definition).is_public()
                && !self.is_ecs_role(definition)
                && !self.has_role(definition, "system_set")
            || self.stateless_system_set(cx, definition)
            || self.stateless_unit_role(cx, definition, "manifest")
            || self.stateless_unit_role(cx, definition, "schema")
            || self.stateless_unit_role(cx, definition, "page")
            || self.concrete_job_struct(cx, definition)
            || self.has_role(definition, "command")
                && matches!(cx.tcx.def_kind(definition), DefKind::Struct)
            || self.has_role(definition, "serde")
            || self.has_role(definition, "error")
            || self.has_role(definition, "display")
                && cx.tcx.item_name(definition).as_str().ends_with("Error")
            || self.has_role(definition, "parser")
                && matches!(cx.tcx.def_kind(definition), DefKind::Struct)
            || self.has_role(definition, "drop")
                && !self.is_ecs_role(definition)
                && matches!(cx.tcx.def_kind(definition), DefKind::Struct)
            || self.has_role(definition, "contract")
            || self.stateless_unit_adapter(cx, definition)
            || self.unit_enum(cx, definition)
            || self.stack_local_or_copy_value(cx, definition)
            || self.native_io_adapter(cx, definition)
            || self.io_adapters.contains(&definition) && !self.is_ecs_role(definition)
            || helper_owned.contains(&definition)
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
    }

    fn propagate_io_adapters(&mut self, cx: &LateContext<'tcx>) {
        let mut queue: Vec<DefId> = self.io_adapters.iter().copied().collect();
        while let Some(callee) = queue.pop() {
            let Some(callers) = self.callers.get(&callee).cloned() else {
                continue;
            };
            for caller in callers {
                if self.io_adapters.insert(caller) {
                    queue.push(caller);
                }
                if let Some(implementation) = cx.tcx.impl_of_assoc(caller) {
                    let ty = cx
                        .tcx
                        .type_of(implementation)
                        .instantiate_identity()
                        .skip_norm_wip();
                    if let TyKind::Adt(owner, _) = ty.kind()
                        && owner.did().is_local()
                        && self.io_adapters.insert(owner.did())
                    {
                        queue.push(owner.did());
                    }
                }
            }
        }
    }

    fn unit_enum(&self, cx: &LateContext<'tcx>, definition: DefId) -> bool {
        if self.is_ecs_role(definition) || self.has_role(definition, "system_set") {
            return false;
        }
        let ty = cx
            .tcx
            .type_of(definition)
            .instantiate_identity()
            .skip_norm_wip();
        matches!(ty.kind(), TyKind::Adt(owner, _) if owner.is_enum() && owner.variants().iter().all(|v| v.fields.is_empty()))
    }

    fn is_ecs_role(&self, definition: DefId) -> bool {
        self.has_role(definition, "component")
            || self.has_role(definition, "resource")
            || self.has_role(definition, "message")
            || self.has_role(definition, "bundle")
            || self.has_role(definition, "query")
            || self.has_role(definition, "parameter")
    }

    fn collect_helper_owned(&self, cx: &LateContext<'tcx>) -> HashSet<DefId> {
        let mut roots = HashSet::new();
        for local in cx.tcx.hir_crate_items(()).definitions() {
            let def = local.to_def_id();
            if matches!(cx.tcx.def_kind(def), DefKind::Struct | DefKind::Enum) {
                let is_adapter = self.io_adapters.contains(&def)
                    || self.native_io_adapter(cx, def)
                    || self.stack_local_or_copy_value(cx, def)
                    || self.has_role(def, "drop") && !self.is_ecs_role(def)
                    || self.has_role(def, "error")
                    || self.has_role(def, "command")
                    || self.has_role(def, "job")
                    || self.has_role(def, "parser")
                    || self.has_role(def, "serde")
                    || self.has_role(def, "contract")
                    || self.has_role(def, "props")
                    || self.has_role(def, "routable")
                    || self.has_role(def, "from_type");
                if is_adapter {
                    roots.insert(def);
                }
            }
        }
        let mut owned = HashSet::new();
        let mut queue: Vec<DefId> = roots.iter().copied().collect();
        let mut callers_to_types: HashMap<DefId, Vec<DefId>> = HashMap::new();
        for (adt, callers) in &self.type_refs {
            for caller in callers {
                callers_to_types.entry(*caller).or_default().push(*adt);
                if let Some(implementation) = cx.tcx.impl_of_assoc(*caller) {
                    let ty = cx
                        .tcx
                        .type_of(implementation)
                        .instantiate_identity()
                        .skip_norm_wip();
                    if let TyKind::Adt(owner, _) = ty.kind()
                        && owner.did().is_local()
                    {
                        callers_to_types.entry(owner.did()).or_default().push(*adt);
                    }
                }
            }
        }
        for ui_fn in &self.ui_callers {
            queue.push(*ui_fn);
        }
        for io_fn in &self.io_adapters {
            queue.push(*io_fn);
        }
        while let Some(source) = queue.pop() {
            if let Some(targets) = callers_to_types.get(&source) {
                for target in targets {
                    if !self.is_ecs_role(*target)
                        && !self.has_role(*target, "plugin")
                        && !self.has_role(*target, "system_set")
                        && matches!(cx.tcx.def_kind(*target), DefKind::Struct | DefKind::Enum)
                        && owned.insert(*target)
                    {
                        queue.push(*target);
                    }
                }
            }
            if matches!(cx.tcx.def_kind(source), DefKind::Struct | DefKind::Enum) {
                let ty = cx
                    .tcx
                    .type_of(source)
                    .instantiate_identity()
                    .skip_norm_wip();
                if let TyKind::Adt(adt, args) = ty.kind() {
                    for field in adt.all_fields() {
                        for arg in field.ty(cx.tcx, args).skip_norm_wip().walk() {
                            if let Some(inner) = arg.as_type()
                                && let TyKind::Adt(target, _) = inner.kind()
                                && target.did().is_local()
                                && !self.is_ecs_role(target.did())
                                && !self.has_role(target.did(), "plugin")
                                && !self.has_role(target.did(), "system_set")
                                && owned.insert(target.did())
                            {
                                queue.push(target.did());
                            }
                        }
                    }
                }
            }
        }
        owned
    }

    fn register_exported_ecs(&mut self, cx: &LateContext<'tcx>) {
        let exported: Vec<DefId> = cx
            .tcx
            .hir_crate_items(())
            .definitions()
            .map(|local| local.to_def_id())
            .filter(|definition| {
                matches!(
                    cx.tcx.def_kind(*definition),
                    DefKind::Struct | DefKind::Enum | DefKind::Union
                ) && cx.tcx.visibility(*definition).is_public()
            })
            .collect();
        for definition in exported {
            if self.has_role(definition, "component")
                || self.has_role(definition, "resource")
                || self.has_role(definition, "message")
            {
                let ty = cx
                    .tcx
                    .type_of(definition)
                    .instantiate_identity()
                    .skip_norm_wip();
                self.root(cx, ty);
            } else if self.has_role(definition, "bundle")
                || self.has_role(definition, "query")
                || self.has_role(definition, "parameter")
            {
                self.used.insert(definition);
            }
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
            || !matches!(cx.tcx.def_kind(definition), DefKind::Struct | DefKind::Enum)
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

    fn native_io_adapter(&self, cx: &LateContext<'tcx>, definition: DefId) -> bool {
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
        let TyKind::Adt(owner, arguments) = ty.kind() else {
            return false;
        };
        if !owner.all_fields().any(|field| {
            Self::has_native_io_handle(cx, field.ty(cx.tcx, arguments).skip_norm_wip())
        }) {
            return false;
        }
        if cx.tcx.visibility(definition).is_public() {
            return true;
        }
        let Some(callers) = self.type_refs.get(&definition) else {
            return false;
        };
        callers
            .iter()
            .any(|caller| self.reachable(*caller, &mut HashSet::new()))
    }

    fn has_native_io_handle(cx: &LateContext<'tcx>, ty: Ty<'tcx>) -> bool {
        for argument in ty.walk() {
            let Some(ty) = argument.as_type() else {
                continue;
            };
            if let TyKind::Dynamic(predicates, _) = ty.kind()
                && ValueGraph::is_external_io_dynamic(cx, predicates)
            {
                return true;
            }
            let TyKind::Adt(owner, _) = ty.kind() else {
                continue;
            };
            if owner.did().is_local() {
                continue;
            }
            let krate = cx.tcx.crate_name(owner.did().krate);
            let name = cx.tcx.item_name(owner.did());
            if Self::is_external_io_crate(krate.as_str())
                || krate.as_str() == "tokio" && Self::is_tokio_io_handle(name.as_str())
                || krate.as_str() == "tokio_util" && name.as_str() == "CancellationToken"
                || krate.as_str() == "std" && Self::is_std_io_handle(name.as_str())
            {
                return true;
            }
        }
        false
    }

    fn is_external_io_crate(krate: &str) -> bool {
        Self::is_native_runtime_crate(krate) || Self::is_codec_or_utility_crate(krate)
    }

    fn is_native_runtime_crate(krate: &str) -> bool {
        [
            "crossbeam_channel",
            "async_channel",
            "vmux_channel",
            "vmux_http",
            "vmux_job",
            "vmux_quic",
            "vmux_runner",
            "vmux_watch",
            "async_io",
            "async_lock",
            "socket2",
            "libc",
            "nix",
            "rustix",
            "signal_hook",
            "flume",
            "quinn",
            "rustls",
            "rcgen",
            "rusqlite",
            "reqwest",
            "hyper",
            "tokio_tungstenite",
            "tungstenite",
            "taffy",
            "syntect",
            "portable_pty",
            "alacritty_terminal",
            "vte",
            "objc2",
            "objc2_app_kit",
            "objc2_foundation",
            "objc2_core_foundation",
            "objc2_av_foundation",
            "objc2_screen_capture_kit",
            "objc2_user_notifications",
            "objc2_ui_kit",
            "objc2_core_media",
            "objc2_core_video",
            "core_foundation",
            "core_graphics",
            "core_video",
            "security_framework",
            "block2",
            "dispatch2",
            "tempfile",
            "notify",
            "arboard",
            "cef",
            "muda",
            "winit",
            "wry",
            "sysinfo",
            "polling",
            "web_sys",
            "js_sys",
            "wasm_bindgen",
        ]
        .contains(&krate)
    }

    fn is_codec_or_utility_crate(krate: &str) -> bool {
        [
            "ring",
            "ed25519_dalek",
            "aes_gcm",
            "argon2",
            "hkdf",
            "zip",
            "tar",
            "flate2",
            "lsp_types",
            "ropey",
            "git2",
            "tree_sitter",
            "pulldown_cmark",
            "ureq",
            "zeroize",
            "image",
            "fontdue",
            "agent_client_protocol",
            "url",
            "ron",
            "serde_json",
            "toml",
            "clap",
            "regex",
            "globset",
            "ignore",
            "walkdir",
            "dirs",
            "sha2",
            "hex",
            "base64",
            "uuid",
            "time",
            "chrono",
            "fluent",
            "fluent_bundle",
            "unic_langid",
            "semver",
            "percent_encoding",
            "http",
            "axum",
            "tower",
        ]
        .contains(&krate)
    }

    fn is_tokio_io_handle(name: &str) -> bool {
        [
            "Runtime",
            "Handle",
            "JoinHandle",
            "AbortHandle",
            "JoinSet",
            "Sender",
            "Receiver",
            "UnboundedSender",
            "UnboundedReceiver",
            "Notify",
            "Semaphore",
            "Mutex",
            "RwLock",
            "OwnedSemaphorePermit",
            "TcpStream",
            "TcpListener",
            "UdpSocket",
            "UnixStream",
            "UnixListener",
            "Child",
            "ChildStdin",
            "ChildStdout",
            "ChildStderr",
            "Command",
            "BufReader",
            "BufWriter",
            "ReadHalf",
            "WriteHalf",
            "File",
            "Interval",
        ]
        .contains(&name)
    }

    fn is_std_io_handle(name: &str) -> bool {
        [
            "File",
            "ReadDir",
            "DirEntry",
            "FileType",
            "Metadata",
            "BufReader",
            "BufWriter",
            "Stdin",
            "Stdout",
            "Stderr",
            "PipeReader",
            "PipeWriter",
            "Error",
            "Child",
            "ChildStdin",
            "ChildStdout",
            "ChildStderr",
            "Command",
            "TcpStream",
            "TcpListener",
            "UdpSocket",
            "UnixStream",
            "UnixListener",
            "UnixDatagram",
            "OwnedFd",
            "RawFd",
            "Sender",
            "Receiver",
            "SyncSender",
            "JoinHandle",
        ]
        .contains(&name)
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
    root_is_message: bool,
    canonical: HashSet<DefId>,
    dtos: HashSet<DefId>,
    io_adapters: HashSet<DefId>,
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
            if let TyKind::Adt(owner, _) = ty.kind()
                && self.dtos.contains(&owner.did())
            {
                return true;
            }
            self.issues
                .push((location, "canonical ECS state retains a recursive model"));
            return false;
        }
        let valid = self.inspect(cx, ty, retained, location);
        self.visiting.remove(&ty);
        valid
    }

    fn is_external_io_dynamic(
        cx: &LateContext<'tcx>,
        predicates: &rustc_middle::ty::List<
            rustc_middle::ty::Binder<'tcx, rustc_middle::ty::ExistentialPredicate<'tcx>>,
        >,
    ) -> bool {
        predicates.principal_def_id().is_none_or(|trait_id| {
            if trait_id.is_local() {
                return false;
            }
            let krate = cx.tcx.crate_name(trait_id.krate);
            let path = cx.tcx.def_path_str(trait_id);
            Ownership::is_external_io_crate(krate.as_str())
                || path.starts_with("std::io::")
                || path.starts_with("tokio::io::")
                || path.starts_with("core::any::")
        })
    }

    fn inspect(
        &mut self,
        cx: &LateContext<'tcx>,
        ty: Ty<'tcx>,
        retained: bool,
        location: DefId,
    ) -> bool {
        match ty.kind() {
            TyKind::Dynamic(predicates, _) => {
                if Self::is_external_io_dynamic(cx, predicates) {
                    return true;
                }
                self.issues
                    .push((location, "canonical ECS state hides a dynamic engine"));
                return false;
            }
            TyKind::FnPtr(..) | TyKind::FnDef(..) => {
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
                return self.inspect_adt(cx, ty, *owner, arguments, retained, location);
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
                if ty.walk().any(|arg| {
                    arg.as_type()
                        .is_some_and(|t| matches!(t.kind(), TyKind::Param(..)))
                }) {
                    return true;
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

    fn inspect_adt(
        &mut self,
        cx: &LateContext<'tcx>,
        ty: Ty<'tcx>,
        owner: rustc_middle::ty::AdtDef<'tcx>,
        arguments: GenericArgsRef<'tcx>,
        retained: bool,
        location: DefId,
    ) -> bool {
        if !owner.did().is_local() {
            return self.external(cx, ty, retained, location);
        }
        if location != self.root && self.io_adapters.contains(&owner.did()) {
            self.values.insert(owner.did());
            return true;
        }
        let is_copy = cx.tcx.type_is_copy_modulo_regions(cx.typing_env(), ty);
        let is_unit_enum = owner.is_enum() && owner.variants().iter().all(|v| v.fields.is_empty());
        let allowed_retained = self.dtos.contains(&owner.did()) || is_copy || is_unit_enum;
        let canonical_blocked =
            self.canonical.contains(&owner.did()) && !allowed_retained && !self.root_is_message;
        if location != self.root && (canonical_blocked || retained && !allowed_retained) {
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
        valid
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
        if Ownership::is_external_io_crate(package.as_str())
            || package.as_str() == "tokio" && Ownership::is_tokio_io_handle(name.as_str())
            || package.as_str() == "tokio_util" && name.as_str() == "CancellationToken"
            || package.as_str() == "std" && Ownership::is_std_io_handle(name.as_str())
            || package.as_str() == "bevy_tasks" && name.as_str() == "Task"
            || package.as_str() == "vmux_job" && name.as_str() == "BlockingTask"
            || package.as_str() == "vmux_git" && name.as_str() == "GitTask"
        {
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
