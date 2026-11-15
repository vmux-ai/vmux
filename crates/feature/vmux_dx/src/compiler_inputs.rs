use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use quote::ToTokens;
use serde::Deserialize;
use syn::visit::{self, Visit};
use syn::{Expr, Item, Pat, Stmt, Type, UseTree};

static CURRENT: OnceLock<CompilerInputs> = OnceLock::new();

#[derive(Default)]
pub(super) struct CompilerInputs {
    files: BTreeMap<PathBuf, String>,
    aliases: BTreeMap<String, String>,
    bindings: BTreeSet<String>,
    included: BTreeSet<String>,
    harness: String,
    constructors: BTreeSet<String>,
    methods: BTreeMap<String, BTreeSet<usize>>,
    parameters: BTreeMap<String, usize>,
    temporaries: BTreeSet<String>,
    written: BTreeMap<String, BTreeSet<usize>>,
    executed: BTreeSet<String>,
    modules: BTreeSet<PathBuf>,
    file_base: PathBuf,
    module_base: PathBuf,
    attribute_base: PathBuf,
}

impl CompilerInputs {
    pub(super) fn current() -> &'static Self {
        CURRENT.get_or_init(|| {
            let directory = std::env::current_dir().expect("source-audit directory");
            let root = directory
                .ancestors()
                .find(|path| path.join("Cargo.toml").is_file() && path.join("crates").is_dir())
                .expect("source-audit workspace");
            Self::read(root).expect("verified compiler inputs")
        })
    }

    pub(super) fn owner(&self, path: &Path) -> Option<&str> {
        let Ok(path) = path.canonicalize() else {
            return None;
        };
        self.files.get(&path).map(String::as_str)
    }

    pub(super) fn read(root: &Path) -> Result<Self, String> {
        let mut paths = Vec::new();
        Self::paths(&root.join("crates"), &mut paths)?;
        for directory in ["examples", "patches", "website", "scripts"] {
            let directory = root.join(directory);
            if directory.is_dir() {
                Self::paths(&directory, &mut paths)?;
            }
        }
        let mut inputs = Self::default();
        for path in &paths {
            if path.file_name().is_none_or(|name| name != "ecs-policy.ron") {
                continue;
            }
            let source = Self::regular(path)?;
            let fragment: Fragment = ron::from_str(&source).map_err(|error| error.to_string())?;
            for record in &fragment.boundaries {
                if record.interface != "Rust compiler input data" {
                    continue;
                }
                let (path, owner) = inputs.verify(root, record, &fragment)?;
                if inputs.files.insert(path, owner).is_some() {
                    return Err("Duplicate compiler input record".into());
                }
            }
        }
        inputs.refuse_modules(&paths)?;
        Ok(inputs)
    }

    fn paths(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in std::fs::read_dir(directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_dir()
            {
                if matches!(
                    path.file_name().and_then(|name| name.to_str()),
                    Some("target" | ".git" | ".worktrees" | ".build-tools" | "node_modules")
                ) {
                    continue;
                }
                Self::paths(&path, paths)?;
            } else if path.extension().is_some_and(|extension| extension == "rs")
                || path
                    .file_name()
                    .is_some_and(|name| name == "ecs-policy.ron" || name == "Cargo.toml")
            {
                paths.push(path);
            }
        }
        Ok(())
    }

    fn regular(path: &Path) -> Result<String, String> {
        let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
        if !metadata.is_file() || metadata.is_symlink() {
            return Err(format!(
                "Compiler input must be a regular file: {}",
                path.display()
            ));
        }
        std::fs::read_to_string(path).map_err(|error| error.to_string())
    }

    fn verify(
        &self,
        root: &Path,
        record: &Record,
        fragment: &Fragment,
    ) -> Result<(PathBuf, String), String> {
        let owner = record
            .owner
            .as_ref()
            .ok_or("Compiler input has no harness owner")?;
        let contract = fragment
            .boundaries
            .iter()
            .find(|record| &record.key == owner)
            .ok_or("Compiler input owner is not reviewed in its fragment")?;
        if contract.interface != "Rust compiler callback and Cargo regression interface" {
            return Err("Compiler input owner is not a compiler harness".into());
        }
        let (source, name) = owner
            .split_once("||impl:")
            .ok_or("Compiler input owner must be its harness implementation")?;
        let source_path = root.join(source);
        let workspace = root.canonicalize().map_err(|error| error.to_string())?;
        if !source_path
            .canonicalize()
            .map_err(|error| error.to_string())?
            .starts_with(&workspace)
        {
            return Err("Compiler harness is outside the workspace".into());
        }
        let source = Self::regular(&source_path)?;
        let file = syn::parse_file(&source).map_err(|error| error.to_string())?;
        let proof = Self::harness(&file, name, &contract.fingerprint)?;
        let (input, kind) = record
            .key
            .split_once("||")
            .ok_or("Invalid compiler input key")?;
        if kind != "script" || !input.ends_with(".rs") {
            return Err("Compiler input must identify an ordinary Rust file".into());
        }
        let path = root.join(input);
        let bytes = Self::regular(&path)?;
        if Self::fingerprint(&bytes) != record.fingerprint {
            return Err(format!("Compiler input fingerprint changed: {input}"));
        }
        let path = path.canonicalize().map_err(|error| error.to_string())?;
        if !path.starts_with(&workspace) {
            return Err("Compiler input is outside the workspace".into());
        }
        let parent = source_path
            .parent()
            .ok_or("Compiler harness has no directory")?;
        for included in proof.included {
            let included = parent
                .join(included)
                .canonicalize()
                .map_err(|error| error.to_string())?;
            if included == path {
                return Ok((path, owner.clone()));
            }
        }
        Err(format!(
            "Compiler input is not passed to its Cargo harness: {input}"
        ))
    }

    fn harness(file: &syn::File, name: &str, fingerprint: &str) -> Result<Self, String> {
        let mut proof = Self {
            harness: name.into(),
            ..Self::default()
        };
        for item in &file.items {
            if let Item::Use(item) = item {
                proof.imports(&item.tree, "");
            }
            if let Item::Mod(item) = item
                && matches!(item.ident.to_string().as_str(), "std" | "tempfile")
            {
                return Err("Compiler harness shadows an external interface".into());
            }
            if let Item::Macro(item) = item
                && item
                    .ident
                    .as_ref()
                    .is_some_and(|name| name == "include_str")
            {
                return Err("Compiler harness shadows source inclusion".into());
            }
        }
        let structure = file
            .items
            .iter()
            .find_map(|item| match item {
                Item::Struct(value) if value.ident == name => Some(value),
                _ => None,
            })
            .ok_or("Compiler harness owner is not a struct")?;
        for field in &structure.fields {
            let Type::Path(path) = &field.ty else {
                continue;
            };
            if proof.external(&path.path) == "tempfile::TempDir"
                && let Some(name) = &field.ident
            {
                proof.temporaries.insert(name.to_string());
            }
        }
        if !Self::testing(&structure.attrs) || proof.temporaries.is_empty() {
            return Err("Compiler harness must own a test-only temporary project".into());
        }
        let implementation = file
            .items
            .iter()
            .find_map(|item| match item {
                Item::Impl(value) if value.self_ty.to_token_stream().to_string() == name => {
                    Some(value)
                }
                _ => None,
            })
            .ok_or("Compiler harness implementation is missing")?;
        if !Self::testing(&implementation.attrs)
            || Self::fingerprint(&implementation.to_token_stream().to_string()) != fingerprint
        {
            return Err("Compiler harness contract is stale".into());
        }
        proof.visit_item_impl(implementation);
        if proof.methods.is_empty() {
            return Err("Compiler harness must compile its written source through Cargo".into());
        }
        for item in &file.items {
            if let Item::Fn(function) = item
                && function
                    .attrs
                    .iter()
                    .any(|attribute| attribute.path().is_ident("test"))
            {
                proof.bindings.clear();
                if Self::enabled(&function.attrs, false) {
                    proof.visit_item_fn(function);
                }
            }
        }
        Ok(proof)
    }

    fn testing(attributes: &[syn::Attribute]) -> bool {
        attributes.iter().any(|attribute| {
            attribute.path().is_ident("cfg")
                && matches!(&attribute.meta, syn::Meta::List(list) if list.tokens.to_string() == "test")
        })
    }

    fn imports(&mut self, tree: &UseTree, prefix: &str) {
        match tree {
            UseTree::Path(value) => {
                self.imports(&value.tree, &format!("{prefix}{}::", value.ident))
            }
            UseTree::Name(value) => {
                self.aliases
                    .insert(value.ident.to_string(), format!("{prefix}{}", value.ident));
            }
            UseTree::Rename(value) => {
                self.aliases
                    .insert(value.rename.to_string(), format!("{prefix}{}", value.ident));
            }
            UseTree::Group(value) => {
                for item in &value.items {
                    self.imports(item, prefix);
                }
            }
            UseTree::Glob(_) => {}
        }
    }

    fn external(&self, path: &syn::Path) -> String {
        let name = path.to_token_stream().to_string().replace(' ', "");
        let (root, suffix) = name.split_once("::").unwrap_or((&name, ""));
        match self.aliases.get(root) {
            Some(alias) if !suffix.is_empty() => format!("{alias}::{suffix}"),
            Some(alias) => alias.clone(),
            None => name,
        }
    }

    fn fingerprint(source: &str) -> String {
        let mut hash = 0xcbf29ce484222325u64;
        for byte in source.bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
        format!("{hash:016x}")
    }

    fn refuse_modules(&self, paths: &[PathBuf]) -> Result<(), String> {
        if self.files.is_empty() {
            return Ok(());
        }
        for path in paths {
            if path.file_name().is_some_and(|name| name == "Cargo.toml") {
                self.refuse_targets(path)?;
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "rs")
                || self.owner(path).is_some()
            {
                continue;
            }
            let source = Self::regular(path)?;
            let file = syn::parse_file(&source).map_err(|error| error.to_string())?;
            let directory = path.parent().unwrap().to_path_buf();
            let mut references = Self {
                file_base: directory.clone(),
                attribute_base: directory.clone(),
                module_base: if matches!(
                    path.file_stem().and_then(|name| name.to_str()),
                    Some("lib" | "main" | "mod")
                ) {
                    directory
                } else {
                    directory.join(path.file_stem().unwrap())
                },
                ..Self::default()
            };
            references.visit_file(&file);
            for target in references.modules {
                if self.owner(&target).is_some() {
                    return Err(format!(
                        "Compiler input is also a Rust module: {}",
                        target.display()
                    ));
                }
            }
        }
        Ok(())
    }

    fn temporary(&self, expression: &Expr) -> Option<String> {
        match expression {
            Expr::MethodCall(call) => self.temporary(&call.receiver),
            Expr::Field(field) if matches!(field.base.as_ref(), Expr::Path(value) if value.path.is_ident("self")) =>
            {
                let syn::Member::Named(name) = &field.member else {
                    return None;
                };
                self.temporaries
                    .contains(&name.to_string())
                    .then(|| name.to_string())
            }
            Expr::Reference(value) => self.temporary(&value.expr),
            Expr::Paren(value) => self.temporary(&value.expr),
            _ => None,
        }
    }

    fn command(&self, expression: &Expr) -> Option<(bool, Option<String>)> {
        match expression {
            Expr::Call(call)
                if matches!(call.func.as_ref(), Expr::Path(value) if self.external(&value.path) == "std::process::Command::new")
                    && matches!(call.args.first(), Some(Expr::Lit(value)) if matches!(&value.lit, syn::Lit::Str(value) if value.value() == "cargo")) =>
            {
                Some((false, None))
            }
            Expr::MethodCall(call) => {
                let (mut dylint, mut directory) = self.command(&call.receiver)?;
                if call.method == "args"
                    && let Some(Expr::Array(arguments)) = call.args.first()
                {
                    dylint |= arguments.elems.iter().any(|argument| matches!(argument, Expr::Lit(value) if matches!(&value.lit, syn::Lit::Str(value) if value.value() == "dylint")));
                }
                if call.method == "current_dir"
                    && let Some(argument) = call.args.first()
                {
                    directory = self.temporary(argument);
                }
                Some((dylint, directory))
            }
            Expr::Paren(value) => self.command(&value.expr),
            _ => None,
        }
    }

    fn enabled(attributes: &[syn::Attribute], unknown: bool) -> bool {
        for attribute in attributes {
            if attribute.path().is_ident("ignore") {
                return false;
            }
            if attribute.path().is_ident("cfg")
                && let syn::Meta::List(list) = &attribute.meta
            {
                let Ok(condition) = syn::parse2::<Expr>(list.tokens.clone()) else {
                    return false;
                };
                if !Self::condition(&condition).unwrap_or(unknown) {
                    return false;
                }
            }
        }
        true
    }

    fn condition(expression: &Expr) -> Option<bool> {
        match expression {
            Expr::Path(value) if value.path.is_ident("test") => Some(true),
            Expr::Call(call) => {
                let Expr::Path(function) = call.func.as_ref() else {
                    return None;
                };
                if function.path.is_ident("not") && call.args.len() == 1 {
                    return Some(!Self::condition(call.args.first()?)?);
                }
                let all = function.path.is_ident("all");
                if !all && !function.path.is_ident("any") {
                    return None;
                }
                let mut unknown = false;
                for argument in &call.args {
                    match Self::condition(argument) {
                        Some(value) if value != all => return Some(!all),
                        Some(_) => {}
                        None => unknown = true,
                    }
                }
                if unknown { None } else { Some(all) }
            }
            _ => None,
        }
    }

    fn refuse_targets(&self, manifest: &Path) -> Result<(), String> {
        let source = Self::regular(manifest)?;
        let metadata: toml::Value = toml::from_str(&source).map_err(|error| error.to_string())?;
        let directory = manifest.parent().ok_or("Cargo manifest has no directory")?;
        if let Some(path) = metadata
            .get("lib")
            .and_then(|value| value.get("path"))
            .and_then(toml::Value::as_str)
            && self.owner(&directory.join(path)).is_some()
        {
            return Err("Compiler input is also a Cargo library target".into());
        }
        if let Some(binaries) = metadata.get("bin").and_then(toml::Value::as_array) {
            for binary in binaries {
                if let Some(path) = binary.get("path").and_then(toml::Value::as_str)
                    && self.owner(&directory.join(path)).is_some()
                {
                    return Err("Compiler input is also a Cargo binary target".into());
                }
            }
        }
        Ok(())
    }
}

impl<'ast> Visit<'ast> for CompilerInputs {
    fn visit_impl_item_fn(&mut self, function: &'ast syn::ImplItemFn) {
        self.parameters.clear();
        self.written.clear();
        self.executed.clear();
        let mut position = 0;
        for input in &function.sig.inputs {
            if let syn::FnArg::Typed(input) = input {
                if let Pat::Ident(binding) = input.pat.as_ref() {
                    self.parameters.insert(binding.ident.to_string(), position);
                }
                position += 1;
            }
        }
        if !function
            .sig
            .inputs
            .iter()
            .any(|input| matches!(input, syn::FnArg::Receiver(_)))
            && matches!(&function.sig.output, syn::ReturnType::Type(_, value) if matches!(value.as_ref(), Type::Path(value) if value.path.is_ident("Self") || value.path.is_ident(&self.harness)))
        {
            self.constructors.insert(function.sig.ident.to_string());
        }
        visit::visit_impl_item_fn(self, function);
        for directory in &self.executed {
            if let Some(inputs) = self.written.get(directory) {
                self.methods
                    .entry(function.sig.ident.to_string())
                    .or_default()
                    .extend(inputs.iter().copied());
            }
        }
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if matches!(call.func.as_ref(), Expr::Path(value) if self.external(&value.path) == "std::fs::write")
            && let (Some(path), Some(Expr::Path(input))) =
                (call.args.first(), call.args.iter().nth(1))
            && let Some(name) = input.path.get_ident()
            && let Some(position) = self.parameters.get(&name.to_string())
            && let Some(directory) = self.temporary(path)
        {
            self.written.entry(directory).or_default().insert(*position);
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if matches!(call.method.to_string().as_str(), "output" | "status")
            && let Some((true, Some(directory))) = self.command(&call.receiver)
        {
            self.executed.insert(directory);
        }
        visit::visit_expr_method_call(self, call);
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        if let Pat::Ident(binding) = &local.pat {
            let name = binding.ident.to_string();
            self.bindings.remove(&name);
            if let Some(value) = &local.init
                && let Expr::Call(call) = value.expr.as_ref()
                && let Expr::Path(path) = call.func.as_ref()
            {
                let path = self.external(&path.path);
                if let Some((owner, method)) = path.rsplit_once("::")
                    && owner.rsplit("::").next() == Some(self.harness.as_str())
                    && self.constructors.contains(method)
                {
                    self.bindings.insert(name);
                }
            }
        }
        visit::visit_local(self, local);
    }

    fn visit_block(&mut self, block: &'ast syn::Block) {
        let bindings = self.bindings.clone();
        for statement in &block.stmts {
            self.visit_stmt(statement);
            if matches!(
                statement,
                Stmt::Expr(Expr::Return(_) | Expr::Break(_) | Expr::Continue(_), _)
            ) {
                break;
            }
        }
        self.bindings = bindings;
    }

    fn visit_expr_assign(&mut self, assignment: &'ast syn::ExprAssign) {
        if let Expr::Path(path) = assignment.left.as_ref()
            && let Some(name) = path.path.get_ident()
        {
            self.bindings.remove(&name.to_string());
        }
        visit::visit_expr_assign(self, assignment);
    }

    fn visit_expr_if(&mut self, expression: &'ast syn::ExprIf) {
        if self.harness.is_empty() {
            visit::visit_expr_if(self, expression);
        } else {
            self.bindings.clear();
        }
    }

    fn visit_expr_match(&mut self, expression: &'ast syn::ExprMatch) {
        if self.harness.is_empty() {
            visit::visit_expr_match(self, expression);
        } else {
            self.bindings.clear();
        }
    }

    fn visit_expr_for_loop(&mut self, expression: &'ast syn::ExprForLoop) {
        let (Pat::Tuple(pattern), Expr::Array(inputs)) =
            (expression.pat.as_ref(), expression.expr.as_ref())
        else {
            visit::visit_expr_for_loop(self, expression);
            return;
        };
        let mut invalid = BTreeSet::new();
        for statement in &expression.body.stmts {
            match statement {
                Stmt::Local(local) => {
                    if let Pat::Ident(binding) = &local.pat {
                        invalid.insert(binding.ident.to_string());
                    }
                }
                Stmt::Expr(Expr::Assign(value), _) => {
                    if let Expr::Path(value) = value.left.as_ref()
                        && let Some(name) = value.path.get_ident()
                    {
                        invalid.insert(name.to_string());
                    }
                }
                Stmt::Expr(Expr::Return(_) | Expr::Break(_) | Expr::Continue(_), _) => break,
                Stmt::Expr(Expr::MethodCall(call), _) => {
                    let Expr::Path(receiver) = call.receiver.as_ref() else {
                        continue;
                    };
                    let Some(receiver) = receiver.path.get_ident() else {
                        continue;
                    };
                    if !self.bindings.contains(&receiver.to_string())
                        || invalid.contains(&receiver.to_string())
                    {
                        continue;
                    }
                    let Some(positions) = self.methods.get(&call.method.to_string()) else {
                        continue;
                    };
                    for position in positions {
                        let Some(Expr::Path(argument)) = call.args.iter().nth(*position) else {
                            continue;
                        };
                        let Some(argument) = argument.path.get_ident() else {
                            continue;
                        };
                        if invalid.contains(&argument.to_string()) {
                            continue;
                        }
                        let Some(index) = pattern.elems.iter().position(|value| matches!(value, Pat::Ident(binding) if binding.ident == *argument)) else { continue; };
                        for input in &inputs.elems {
                            if let Expr::Tuple(tuple) = input
                                && let Some(Expr::Macro(value)) = tuple.elems.iter().nth(index)
                                && value.mac.path.is_ident("include_str")
                                && let Ok(path) =
                                    syn::parse2::<syn::LitStr>(value.mac.tokens.clone())
                            {
                                self.included.insert(path.value());
                            }
                        }
                    }
                }
                _ => {
                    self.bindings.clear();
                }
            }
        }
        visit::visit_expr_for_loop(self, expression);
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if !Self::enabled(&item.attrs, true) {
            return;
        }
        let mut override_path = None;
        for attribute in &item.attrs {
            if attribute.path().is_ident("path")
                && let syn::Meta::NameValue(value) = &attribute.meta
                && let Expr::Lit(value) = &value.value
                && let syn::Lit::Str(path) = &value.lit
            {
                override_path = Some(path.value());
            }
        }
        if let Some((_, items)) = &item.content {
            let previous = self.module_base.clone();
            let attribute = self.attribute_base.clone();
            self.module_base = match override_path {
                Some(path) => self.attribute_base.join(path),
                None => self.module_base.join(item.ident.to_string()),
            };
            self.attribute_base = self.module_base.clone();
            for item in items {
                self.visit_item(item);
            }
            self.module_base = previous;
            self.attribute_base = attribute;
        } else if let Some(path) = override_path {
            self.modules.insert(self.attribute_base.join(path));
        } else {
            self.modules
                .insert(self.module_base.join(format!("{}.rs", item.ident)));
            self.modules
                .insert(self.module_base.join(item.ident.to_string()).join("mod.rs"));
        }
    }

    fn visit_macro(&mut self, value: &'ast syn::Macro) {
        if value.path.is_ident("include")
            && let Ok(path) = syn::parse2::<syn::LitStr>(value.tokens.clone())
        {
            self.modules.insert(self.file_base.join(path.value()));
        }
        visit::visit_macro(self, value);
    }
}

#[derive(Deserialize)]
struct Fragment {
    #[serde(default)]
    boundaries: Vec<Record>,
}

#[derive(Deserialize)]
struct Record {
    key: String,
    fingerprint: String,
    interface: String,
    #[serde(default)]
    owner: Option<String>,
}
