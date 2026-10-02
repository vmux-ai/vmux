use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use syn::visit::{self, Visit};
use syn::{
    Attribute, Block, Expr, ExprMethodCall, FnArg, ImplItem, Item, ItemFn, Stmt, Type, TypePath,
    UseTree, Visibility,
};

const GENERIC_MODULES: &[&str] = &[
    "bin", "host", "lib", "main", "plugin", "runtime", "src", "test", "tests", "ui",
];

#[test]
fn registered_systems_and_observers_use_short_names_from_their_module_context() {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates dir");
    let mut violations = Vec::new();

    walk(crates_dir, &mut |path, source| {
        let Ok(file) = syn::parse_file(source) else {
            return;
        };
        let mut functions = FunctionNames::default();
        functions.visit_file(&file);
        let mut systems = RegisteredSystems::default();
        systems.visit_file(&file);

        for name in functions.0.intersection(&systems.0) {
            if let Some(reason) = violation(path, name) {
                violations.push(format!("{}: {name}: {reason}", path.display()));
            }
        }
    });

    assert!(
        violations.is_empty(),
        "registered systems and observers must use short operation names within their owning module:\n{}",
        violations.join("\n")
    );
}

#[test]
fn registered_systems_and_observers_are_private_to_their_plugin_module() {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates dir");
    let mut violations = Vec::new();

    walk(crates_dir, &mut |path, source| {
        let Ok(file) = syn::parse_file(source) else {
            return;
        };
        audit_private_systems(path, &file.items, "crate", &mut violations);
    });

    assert!(
        violations.is_empty(),
        "registered systems and observers must be private to the module whose plugin schedules them:\n{}",
        violations.join("\n")
    );
}

fn audit_private_systems(path: &Path, items: &[Item], scope: &str, violations: &mut Vec<String>) {
    let public = items
        .iter()
        .filter_map(|item| {
            let Item::Fn(function) = item else {
                return None;
            };
            (!matches!(function.vis, Visibility::Inherited)).then(|| function.sig.ident.to_string())
        })
        .collect::<BTreeSet<_>>();
    let mut systems = RegisteredSystems::default();
    for item in items {
        if !matches!(item, Item::Mod(_)) {
            systems.visit_item(item);
        }
    }
    for name in public.intersection(&systems.0) {
        violations.push(format!("{} ({scope}): {name}", path.display()));
    }

    for item in items {
        let Item::Mod(module) = item else {
            continue;
        };
        let Some((_, items)) = &module.content else {
            continue;
        };
        let nested = format!("{scope}::{}", module.ident);
        audit_private_systems(path, items, &nested, violations);
    }
}

#[test]
fn modules_follow_the_workspace_physical_layout() {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates dir");
    let mut violations = Vec::new();
    audit_module_layout(crates_dir, &mut violations);

    assert!(
        violations.is_empty(),
        "modules must avoid mod.rs, generic collection names, and one-child directory splits:\n{}",
        violations.join("\n")
    );
}

fn audit_module_layout(dir: &Path, violations: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some("target") {
                continue;
            }
            let sibling = path.with_extension("rs");
            if sibling.is_file() {
                let children = std::fs::read_dir(&path)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter(|entry| {
                        entry.path().extension().and_then(|value| value.to_str()) == Some("rs")
                    })
                    .count();
                if children == 1 {
                    violations.push(format!("{}: one-child module directory", path.display()));
                }
            }
            if path.file_name().and_then(|name| name.to_str()) == Some("systems") {
                violations.push(format!("{}: generic systems directory", path.display()));
            }
            audit_module_layout(&path, violations);
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name == "mod.rs"
            || matches!(
                name,
                "helper.rs"
                    | "helpers.rs"
                    | "page_state.rs"
                    | "tools.rs"
                    | "util.rs"
                    | "utils.rs"
                    | "view.rs"
            )
        {
            violations.push(path.display().to_string());
        }
        if name == "native_page.rs" && !path.ends_with("vmux_macro/src/native_page.rs") {
            violations.push(path.display().to_string());
        }
    }
}

#[derive(Default)]
struct FunctionNames(BTreeSet<String>);

impl<'ast> Visit<'ast> for FunctionNames {
    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        self.0.insert(item.sig.ident.to_string());
        visit::visit_item_fn(self, item);
    }
}

#[derive(Default)]
struct RegisteredSystems(BTreeSet<String>);

impl<'ast> Visit<'ast> for RegisteredSystems {
    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        match call.method.to_string().as_str() {
            "add_systems" => {
                if let Some(systems) = call.args.iter().nth(1) {
                    collect_paths(systems, &mut self.0);
                }
            }
            "add_observer" | "run_system_once" => {
                if let Some(system) = call.args.first() {
                    collect_paths(system, &mut self.0);
                }
            }
            _ => {}
        }
        visit::visit_expr_method_call(self, call);
    }
}

fn collect_paths(expression: &Expr, names: &mut BTreeSet<String>) {
    match expression {
        Expr::Array(array) => {
            for element in &array.elems {
                collect_paths(element, names);
            }
        }
        Expr::Call(call) => collect_paths(&call.func, names),
        Expr::Group(group) => collect_paths(&group.expr, names),
        Expr::MethodCall(call) => {
            collect_paths(&call.receiver, names);
            if matches!(
                call.method.to_string().as_str(),
                "run_if" | "distributive_run_if"
            ) {
                for argument in &call.args {
                    collect_paths(argument, names);
                }
            }
        }
        Expr::Paren(paren) => collect_paths(&paren.expr, names),
        Expr::Path(path) => {
            if let Some(segment) = path.path.segments.last() {
                names.insert(segment.ident.to_string());
            }
        }
        Expr::Reference(reference) => collect_paths(&reference.expr, names),
        Expr::Tuple(tuple) => {
            for element in &tuple.elems {
                collect_paths(element, names);
            }
        }
        _ => {}
    }
}

fn violation(path: &Path, name: &str) -> Option<String> {
    if name.starts_with("on_") {
        return Some("remove `on_`".to_string());
    }
    let operation = name;
    let segments = operation.split('_').collect::<Vec<_>>();
    for redundant in ["feature", "plugin", "system"] {
        if segments.contains(&redundant) {
            return Some(format!("remove `{redundant}`"));
        }
    }

    for context in module_context(path) {
        let context = context.split('_').collect::<Vec<_>>();
        if operation != context.join("_")
            && context.len() <= segments.len()
            && segments
                .windows(context.len())
                .any(|window| window == context)
        {
            let context = context.join("_");
            return Some(format!("remove repeated `{context}` context"));
        }
    }
    None
}

fn module_context(path: &Path) -> BTreeSet<String> {
    let mut context = BTreeSet::new();
    let components = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();

    if let Some(crate_name) = components
        .iter()
        .rev()
        .find(|name| name.starts_with("vmux_"))
    {
        context.insert(crate_name.trim_start_matches("vmux_").to_string());
    }
    if let Some(src) = components.iter().position(|component| *component == "src") {
        for component in components.iter().skip(src + 1) {
            let module = component.trim_end_matches(".rs");
            if !GENERIC_MODULES.contains(&module) {
                context.insert(module.to_string());
            }
        }
    }
    context
}

fn walk(dir: &Path, visit: &mut dyn FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some("target") {
                continue;
            }
            walk(&path, visit);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs")
            && let Ok(source) = std::fs::read_to_string(&path)
        {
            visit(&path, &source);
        }
    }
}

#[test]
fn system_name_policy_detects_framework_and_module_repetition() {
    let path = Path::new("crates/feature/vmux_bookmark/src/tool.rs");

    assert_eq!(
        violation(path, "bookmark_pin").as_deref(),
        Some("remove repeated `bookmark` context")
    );
    assert_eq!(
        violation(path, "on_bookmark_pin").as_deref(),
        Some("remove `on_`")
    );
    assert_eq!(
        violation(path, "tool_pin").as_deref(),
        Some("remove repeated `tool` context")
    );
    assert_eq!(
        violation(
            Path::new("crates/feature/vmux_editor/src/host/explorer/search.rs"),
            "queue_global_search_requests",
        )
        .as_deref(),
        Some("remove repeated `search` context")
    );
    assert_eq!(
        violation(path, "pin_system").as_deref(),
        Some("remove `system`")
    );
    assert!(violation(path, "pin").is_none());
    assert!(
        violation(
            Path::new("crates/feature/vmux_history/src/prune.rs"),
            "prune"
        )
        .is_none()
    );
}

#[test]
fn files_do_not_mix_imported_and_qualified_paths_for_one_type() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("workspace root");
    let mut violations = Vec::new();

    for root in [workspace.join("crates"), workspace.join("website")] {
        walk(&root, &mut |path, source| {
            let file = match syn::parse_file(source) {
                Ok(file) => file,
                Err(error) => {
                    violations.push(format!("{}: {error}", path.display()));
                    return;
                }
            };
            audit_imports(path, &file.items, "crate", &mut violations);
        });
    }

    assert!(
        violations.is_empty(),
        "use either an import or a qualified path for one type within a file:\n{}",
        violations.join("\n")
    );
}

#[test]
fn imports_are_declared_at_module_top() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("workspace root");
    let mut violations = Vec::new();

    for root in [workspace.join("crates"), workspace.join("website")] {
        walk(&root, &mut |path, source| {
            let file = match syn::parse_file(source) {
                Ok(file) => file,
                Err(error) => {
                    violations.push(format!("{}: {error}", path.display()));
                    return;
                }
            };
            audit_import_placement(path, &file.items, "crate", &mut violations);
        });
    }

    assert!(
        violations.is_empty(),
        "imports must appear before every other item in their module and never inside a function:\n{}",
        violations.join("\n")
    );
}

#[test]
fn inherent_methods_do_not_own_ecs_runtime_parameters() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("workspace root");
    let mut violations = Vec::new();

    for root in [workspace.join("crates"), workspace.join("website")] {
        walk(&root, &mut |path, source| {
            if test_source(path)
                || path
                    .components()
                    .any(|part| part.as_os_str() == "vmux_native")
            {
                return;
            }
            let file = match syn::parse_file(source) {
                Ok(file) => file,
                Err(error) => {
                    violations.push(format!("{}: {error}", path.display()));
                    return;
                }
            };
            audit_inherent_ecs_methods(path, &file.items, "crate", &mut violations);
        });
    }

    assert!(
        violations.is_empty(),
        "ECS runtime mutation and lookup belong in systems or SystemParam methods:\n{}",
        violations.join("\n")
    );
}

fn audit_inherent_ecs_methods(
    path: &Path,
    items: &[Item],
    scope: &str,
    violations: &mut Vec<String>,
) {
    let system_params = items
        .iter()
        .filter_map(|item| {
            let Item::Struct(item) = item else {
                return None;
            };
            has_derive(&item.attrs, "SystemParam").then(|| item.ident.to_string())
        })
        .collect::<BTreeSet<_>>();

    for item in items {
        if let Item::Impl(item) = item
            && item.trait_.is_none()
            && let Type::Path(self_type) = item.self_ty.as_ref()
            && let Some(name) = self_type.path.segments.last()
            && !system_params.contains(&name.ident.to_string())
        {
            for member in &item.items {
                let ImplItem::Fn(method) = member else {
                    continue;
                };
                if cfg_test(&method.attrs) {
                    continue;
                }
                let mut runtime = EcsRuntimeType::default();
                for input in &method.sig.inputs {
                    let FnArg::Typed(input) = input else {
                        continue;
                    };
                    runtime.visit_type(&input.ty);
                }
                if runtime.found {
                    violations.push(format!(
                        "{}:{} ({scope}): {}::{}",
                        path.display(),
                        method.sig.fn_token.span.start().line,
                        name.ident,
                        method.sig.ident
                    ));
                }
            }
        }

        let Item::Mod(module) = item else {
            continue;
        };
        if cfg_test(&module.attrs) {
            continue;
        }
        let Some((_, nested)) = &module.content else {
            continue;
        };
        let nested_scope = format!("{scope}::{}", module.ident);
        audit_inherent_ecs_methods(path, nested, &nested_scope, violations);
    }
}

fn has_derive(attributes: &[Attribute], name: &str) -> bool {
    attributes.iter().any(|attribute| {
        if !attribute.path().is_ident("derive") {
            return false;
        }
        let mut found = false;
        let _ = attribute.parse_nested_meta(|meta| {
            if meta
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == name)
            {
                found = true;
            }
            Ok(())
        });
        found
    })
}

fn cfg_test(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        if !attribute.path().is_ident("cfg") {
            return false;
        }
        let syn::Meta::List(list) = &attribute.meta else {
            return false;
        };
        list.tokens
            .to_string()
            .split(|character: char| !character.is_alphanumeric() && character != '_')
            .any(|segment| segment == "test")
    })
}

fn test_source(path: &Path) -> bool {
    path.components().any(|part| part.as_os_str() == "tests")
        || path.file_stem().and_then(|name| name.to_str()) == Some("tests")
}

#[derive(Default)]
struct EcsRuntimeType {
    found: bool,
}

impl<'ast> Visit<'ast> for EcsRuntimeType {
    fn visit_type_path(&mut self, path: &'ast TypePath) {
        self.found |= path.path.segments.iter().any(|segment| {
            matches!(
                segment.ident.to_string().as_str(),
                "Commands"
                    | "DeferredWorld"
                    | "EntityCommands"
                    | "EventReader"
                    | "EventWriter"
                    | "Local"
                    | "MessageReader"
                    | "MessageWriter"
                    | "NonSend"
                    | "NonSendMut"
                    | "ParamSet"
                    | "Query"
                    | "RemovedComponents"
                    | "Res"
                    | "ResMut"
                    | "Single"
                    | "World"
            )
        });
        visit::visit_type_path(self, path);
    }
}

fn audit_import_placement(path: &Path, items: &[Item], scope: &str, violations: &mut Vec<String>) {
    let mut body_started = false;
    for item in items {
        match item {
            Item::Use(import) => {
                if body_started {
                    violations.push(format!(
                        "{}:{} ({scope}): import follows another item",
                        path.display(),
                        import.use_token.span.start().line
                    ));
                }
            }
            Item::ExternCrate(_) => {}
            _ => body_started = true,
        }

        let Item::Mod(module) = item else {
            continue;
        };
        let Some((_, nested)) = &module.content else {
            continue;
        };
        let nested_scope = format!("{scope}::{}", module.ident);
        audit_import_placement(path, nested, &nested_scope, violations);
    }

    let mut block_imports = BlockImports {
        path,
        scope,
        violations,
    };
    for item in items {
        if !matches!(item, Item::Mod(_) | Item::Use(_)) {
            block_imports.visit_item(item);
        }
    }
}

struct BlockImports<'a> {
    path: &'a Path,
    scope: &'a str,
    violations: &'a mut Vec<String>,
}

impl<'ast> Visit<'ast> for BlockImports<'_> {
    fn visit_block(&mut self, block: &'ast Block) {
        for statement in &block.stmts {
            let Stmt::Item(Item::Use(import)) = statement else {
                continue;
            };
            self.violations.push(format!(
                "{}:{} ({}): import inside a function or block",
                self.path.display(),
                import.use_token.span.start().line,
                self.scope
            ));
        }
        visit::visit_block(self, block);
    }
}

fn audit_imports(path: &Path, items: &[Item], scope: &str, violations: &mut Vec<String>) {
    let mut imports = BTreeMap::<String, Vec<Vec<String>>>::new();
    for item in items {
        if let Item::Use(item) = item {
            collect_imports(Vec::new(), &item.tree, &mut imports);
        }
    }

    let mut paths = QualifiedPaths::default();
    for item in items {
        if !matches!(item, Item::Use(_) | Item::Mod(_)) {
            paths.visit_item(item);
        }
    }

    for (name, imported_paths) in imports {
        for imported in imported_paths {
            if paths
                .0
                .iter()
                .any(|qualified| qualified.starts_with(&imported))
            {
                violations.push(format!(
                    "{} ({scope}): `{name}` is both imported and written as `{}`",
                    path.display(),
                    imported.join("::")
                ));
            }
        }
    }

    for item in items {
        let Item::Mod(module) = item else {
            continue;
        };
        let Some((_, items)) = &module.content else {
            continue;
        };
        let nested = format!("{scope}::{}", module.ident);
        audit_imports(path, items, &nested, violations);
    }
}

fn collect_imports(
    mut prefix: Vec<String>,
    tree: &UseTree,
    imports: &mut BTreeMap<String, Vec<Vec<String>>>,
) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            collect_imports(prefix, &path.tree, imports);
        }
        UseTree::Name(name) => {
            let local = name.ident.to_string();
            if local == "self" {
                if let Some(local) = prefix.last() {
                    imports.entry(local.clone()).or_default().push(prefix);
                }
                return;
            }
            prefix.push(local.clone());
            imports.entry(local).or_default().push(prefix);
        }
        UseTree::Group(group) => {
            for item in &group.items {
                collect_imports(prefix.clone(), item, imports);
            }
        }
        UseTree::Glob(_) | UseTree::Rename(_) => {}
    }
}

#[derive(Default)]
struct QualifiedPaths(Vec<Vec<String>>);

impl<'ast> Visit<'ast> for QualifiedPaths {
    fn visit_item_mod(&mut self, _module: &'ast syn::ItemMod) {}

    fn visit_item_use(&mut self, _item: &'ast syn::ItemUse) {}

    fn visit_path(&mut self, path: &'ast syn::Path) {
        if path.segments.len() > 1 {
            self.0.push(
                path.segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect(),
            );
        }
        visit::visit_path(self, path);
    }
}
