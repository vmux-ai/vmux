use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use quote::ToTokens;
use syn::parse::Parser;
use syn::visit::{self, Visit};
use syn::{Expr, Item, Pat, Stmt, Type, UseTree};

static CURRENT: OnceLock<CompilerInputs> = OnceLock::new();

#[derive(Default)]
pub(super) struct CompilerInputs {
    workspace: PathBuf,
    files: BTreeMap<PathBuf, String>,
    aliases: BTreeMap<String, String>,
    bindings: BTreeSet<String>,
    included: BTreeSet<String>,
    harness: String,
    constructors: BTreeSet<String>,
    mutexes: BTreeSet<String>,
    methods: BTreeMap<String, BTreeMap<usize, BTreeSet<usize>>>,
    parameters: BTreeMap<String, usize>,
    temporaries: BTreeSet<String>,
    written: BTreeMap<(String, Vec<String>), Option<usize>>,
    projects: BTreeMap<String, toml::Value>,
    local_paths: BTreeMap<String, (String, Vec<String>)>,
    fixed_paths: BTreeMap<String, PathBuf>,
    source_paths: BTreeMap<String, PathBuf>,
    local_temporaries: BTreeSet<String>,
    source_directory: PathBuf,
    method: String,
    constructor: bool,
    uncertain: bool,
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
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.workspace.join(path)
        };
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
        let mut inputs = Self {
            workspace: root.canonicalize().map_err(|error| error.to_string())?,
            ..Self::default()
        };
        for path in &paths {
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let source = Self::regular(path)?;
            let Ok(file) = syn::parse_file(&source) else {
                continue;
            };
            inputs.discover(root, path, &file)?;
        }
        inputs.refuse_modules(&paths)?;
        Ok(inputs)
    }

    fn discover(&mut self, root: &Path, source: &Path, file: &syn::File) -> Result<(), String> {
        let workspace = root.canonicalize().map_err(|error| error.to_string())?;
        let source = source.canonicalize().map_err(|error| error.to_string())?;
        let relative = source
            .strip_prefix(&workspace)
            .map_err(|error| error.to_string())?;
        let parent = source.parent().ok_or("Compiler harness has no directory")?;
        for item in &file.items {
            let Item::Struct(structure) = item else {
                continue;
            };
            if !Self::testing(&structure.attrs) {
                continue;
            }
            let name = structure.ident.to_string();
            let Ok(proof) = Self::harness(file, &name, &source) else {
                continue;
            };
            let owner = format!("{}||impl:{name}", relative.display());
            for included in proof.included {
                let path = parent.join(included);
                Self::regular(&path)?;
                let path = path.canonicalize().map_err(|error| error.to_string())?;
                if !path.starts_with(&workspace) {
                    return Err("Compiler input is outside the workspace".into());
                }
                if path.extension().is_none_or(|extension| extension != "rs") {
                    return Err("Compiler input must be an ordinary Rust file".into());
                }
                if let Some(previous) = self.files.insert(path, owner.clone())
                    && previous != owner
                {
                    return Err("Compiler input has multiple harness owners".into());
                }
            }
        }
        Ok(())
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
                || path.file_name().is_some_and(|name| name == "Cargo.toml")
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

    fn harness(file: &syn::File, name: &str, source: &Path) -> Result<Self, String> {
        let mut proof = Self {
            harness: name.into(),
            source_directory: Self::cargo_directory(source)?,
            ..Self::default()
        };
        proof.interfaces(file)?;
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
        if !Self::testing(&implementation.attrs) {
            return Err("Compiler harness implementation must be test-only".into());
        }
        for item in &implementation.items {
            let syn::ImplItem::Fn(function) = item else {
                continue;
            };
            if Self::constructor(function, name) && Self::enabled(&function.attrs, false) {
                proof.project(function)?;
            }
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
                proof.uncertain = false;
                if Self::enabled(&function.attrs, false) {
                    proof.visit_item_fn(function);
                }
            }
        }
        Ok(proof)
    }

    fn interfaces(&mut self, file: &syn::File) -> Result<(), String> {
        for item in &file.items {
            if let Item::Use(item) = item {
                if Self::glob_import(&item.tree) {
                    return Err("Compiler harness has unresolved glob imports".into());
                }
                self.imports(&item.tree, "");
            }
            if let Item::Mod(item) = item
                && item
                    .attrs
                    .iter()
                    .any(|attribute| attribute.path().is_ident("macro_use"))
            {
                return Err("Compiler harness has unresolved textual macro imports".into());
            }
            if let Item::ExternCrate(item) = item
                && (item
                    .attrs
                    .iter()
                    .any(|attribute| attribute.path().is_ident("macro_use"))
                    || item.rename.as_ref().is_some_and(|(_, name)| {
                        matches!(name.to_string().as_str(), "std" | "tempfile")
                    }))
            {
                return Err("Compiler harness shadows an external macro or crate interface".into());
            }
            if let Item::Mod(item) = item
                && matches!(item.ident.to_string().as_str(), "std" | "tempfile")
            {
                return Err("Compiler harness shadows an external interface".into());
            }
            if let Item::Macro(item) = item
                && item.ident.as_ref().is_some_and(|name| {
                    matches!(name.to_string().as_str(), "include_str" | "format" | "env")
                })
            {
                return Err("Compiler harness shadows source inclusion".into());
            }
        }
        for name in ["include_str", "format", "env"] {
            if let Some(path) = self.aliases.get(name)
                && path != &format!("std::{name}")
            {
                return Err("Compiler harness shadows a builtin macro".into());
            }
        }
        if self.aliases.get("std").is_some_and(|path| path != "std") {
            return Err("Compiler harness shadows the standard crate".into());
        }
        for item in &file.items {
            let name = match item {
                Item::Struct(value) => &value.ident,
                Item::Enum(value) => &value.ident,
                Item::Union(value) => &value.ident,
                Item::Type(value) => &value.ident,
                Item::Trait(value) => &value.ident,
                _ => continue,
            };
            if self.aliases.contains_key(&name.to_string()) {
                return Err("Compiler harness shadows an imported interface".into());
            }
        }
        for item in &file.items {
            if let Item::Static(item) = item
                && Self::testing(&item.attrs)
                && self.unit_mutex(item)
            {
                self.mutexes.insert(item.ident.to_string());
            }
        }
        Ok(())
    }

    fn unit_mutex(&self, item: &syn::ItemStatic) -> bool {
        let Type::Path(value) = item.ty.as_ref() else {
            return false;
        };
        let mut path = value.path.clone();
        let Some(last) = path.segments.last_mut() else {
            return false;
        };
        let syn::PathArguments::AngleBracketed(arguments) = &last.arguments else {
            return false;
        };
        if arguments.args.len() != 1
            || !matches!(arguments.args.first(), Some(syn::GenericArgument::Type(Type::Tuple(value))) if value.elems.is_empty())
        {
            return false;
        }
        last.arguments = syn::PathArguments::None;
        if self.external(&path) != "std::sync::Mutex" {
            return false;
        }
        let Expr::Call(value) = item.expr.as_ref() else {
            return false;
        };
        let Expr::Path(function) = value.func.as_ref() else {
            return false;
        };
        self.external(&function.path) == "std::sync::Mutex::new"
            && value.args.len() == 1
            && matches!(value.args.first(), Some(Expr::Tuple(value)) if value.elems.is_empty())
    }

    fn synchronized(&self, expression: &Expr) -> bool {
        let Expr::Match(value) = expression else {
            return false;
        };
        let Expr::MethodCall(lock) = value.expr.as_ref() else {
            return false;
        };
        let Expr::Path(receiver) = lock.receiver.as_ref() else {
            return false;
        };
        let Some(name) = receiver.path.get_ident() else {
            return false;
        };
        if lock.method != "lock"
            || !lock.args.is_empty()
            || !self.mutexes.contains(&name.to_string())
            || value.arms.len() != 2
        {
            return false;
        }
        let mut seen = BTreeSet::new();
        for arm in &value.arms {
            if arm.guard.is_some() {
                return false;
            }
            let Pat::TupleStruct(pattern) = &arm.pat else {
                return false;
            };
            let Some(variant) = pattern.path.get_ident() else {
                return false;
            };
            if pattern.elems.len() != 1 || self.aliases.contains_key(&variant.to_string()) {
                return false;
            }
            let Some(Pat::Ident(binding)) = pattern.elems.first() else {
                return false;
            };
            if binding.by_ref.is_some() || binding.mutability.is_some() || binding.subpat.is_some()
            {
                return false;
            }
            match variant.to_string().as_str() {
                "Ok" if matches!(arm.body.as_ref(), Expr::Path(value) if value.path.is_ident(&binding.ident)) =>
                    {}
                "Err" if matches!(arm.body.as_ref(), Expr::MethodCall(value) if value.method == "into_inner" && value.args.is_empty() && matches!(value.receiver.as_ref(), Expr::Path(value) if value.path.is_ident(&binding.ident))) =>
                    {}
                _ => return false,
            }
            if !seen.insert(variant.to_string()) {
                return false;
            }
        }
        true
    }

    fn glob_import(tree: &UseTree) -> bool {
        match tree {
            UseTree::Path(value) => Self::glob_import(&value.tree),
            UseTree::Group(value) => {
                for item in &value.items {
                    if Self::glob_import(item) {
                        return true;
                    }
                }
                false
            }
            UseTree::Glob(_) => true,
            _ => false,
        }
    }

    fn cargo_directory(source: &Path) -> Result<PathBuf, String> {
        let directory = source
            .ancestors()
            .skip(1)
            .find(|path| path.join("Cargo.toml").is_file())
            .ok_or("Compiler harness has no Cargo manifest")?;
        Ok(directory.to_path_buf())
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
                self.import(value.ident.to_string(), format!("{prefix}{}", value.ident));
            }
            UseTree::Rename(value) => {
                self.import(value.rename.to_string(), format!("{prefix}{}", value.ident));
            }
            UseTree::Group(value) => {
                for item in &value.items {
                    self.imports(item, prefix);
                }
            }
            UseTree::Glob(_) => {}
        }
    }

    fn import(&mut self, name: String, path: String) {
        match self.aliases.entry(name) {
            Entry::Occupied(mut entry) => {
                entry.insert(String::new());
            }
            Entry::Vacant(entry) => {
                entry.insert(path);
            }
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

    fn constructor(function: &syn::ImplItemFn, owner: &str) -> bool {
        !function
            .sig
            .inputs
            .iter()
            .any(|input| matches!(input, syn::FnArg::Receiver(_)))
            && matches!(&function.sig.output, syn::ReturnType::Type(_, value) if matches!(value.as_ref(), Type::Path(value) if value.path.is_ident("Self") || value.path.is_ident(owner)))
    }

    fn project(&mut self, function: &syn::ImplItemFn) -> Result<(), String> {
        let previous = std::mem::take(&mut self.projects);
        let mutexes = self.mutexes.clone();
        self.constructor = true;
        self.uncertain = false;
        self.local_paths.clear();
        self.fixed_paths.clear();
        self.local_temporaries.clear();
        for statement in &function.block.stmts {
            if let Stmt::Local(local) = statement
                && let Some(init) = &local.init
                && init.diverge.is_none()
                && self.synchronized(&init.expr)
            {
                if let Pat::Ident(binding) = &local.pat {
                    self.mutexes.remove(&binding.ident.to_string());
                }
                continue;
            }
            self.visit_stmt(statement);
            if matches!(
                statement,
                Stmt::Expr(Expr::Return(_) | Expr::Break(_) | Expr::Continue(_), _)
            ) {
                self.unknown();
                break;
            }
        }
        self.constructor = false;
        self.mutexes = mutexes;
        if self.uncertain {
            return Err("Compiler project has unproven control flow".into());
        }
        let Some(Stmt::Expr(Expr::Struct(value), _)) = function.block.stmts.last() else {
            return Err("Compiler project constructor must return its owned fields".into());
        };
        if !value.path.is_ident("Self") && !value.path.is_ident(&self.harness) {
            return Err("Compiler project constructor returns a different owner".into());
        }
        let mut projects = previous;
        let mut owned = false;
        for field in &value.fields {
            let syn::Member::Named(name) = &field.member else {
                continue;
            };
            if let Some(path) = self.fixed(&field.expr)
                && let Some(previous) = self.source_paths.insert(name.to_string(), path.clone())
                && previous != path
            {
                return Err("Compiler project constructors use different source roots".into());
            }
            if !self.temporaries.contains(&name.to_string()) {
                continue;
            }
            let Expr::Path(path) = &field.expr else {
                return Err("Compiler project field must retain its created temporary".into());
            };
            let local = path
                .path
                .get_ident()
                .ok_or("Compiler project field is unresolved")?;
            let manifest = self
                .projects
                .get(&local.to_string())
                .ok_or("Compiler project has no proven manifest")?;
            if let Some(previous) = projects.insert(name.to_string(), manifest.clone())
                && previous != *manifest
            {
                return Err("Compiler project constructors use different targets".into());
            }
            owned = true;
        }
        self.projects = projects;
        if owned {
            self.constructors.insert(function.sig.ident.to_string());
        }
        self.local_paths.clear();
        self.fixed_paths.clear();
        self.local_temporaries.clear();
        Ok(())
    }

    fn relative(value: &str) -> Option<Vec<String>> {
        let mut parts = Vec::new();
        for part in Path::new(value).components() {
            match part {
                std::path::Component::Normal(value) => parts.push(value.to_str()?.to_owned()),
                std::path::Component::CurDir => {}
                _ => return None,
            }
        }
        Some(parts)
    }

    fn string(&self, expression: &Expr) -> Option<String> {
        match expression {
            Expr::Lit(value) => {
                let syn::Lit::Str(value) = &value.lit else {
                    return None;
                };
                let value = value.value();
                (!value.contains('\0')).then_some(value)
            }
            Expr::Path(value) => {
                let position = self.parameters.get(&value.path.get_ident()?.to_string())?;
                Some(format!("\0{position}\0"))
            }
            Expr::Macro(value) if value.mac.path.is_ident("format") => self.formatted(&value.mac),
            Expr::Reference(value) => self.string(&value.expr),
            Expr::Paren(value) => self.string(&value.expr),
            _ => None,
        }
    }

    fn formatted(&self, value: &syn::Macro) -> Option<String> {
        let arguments = syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated
            .parse2(value.tokens.clone())
            .ok()?;
        let Expr::Lit(first) = arguments.first()? else {
            return None;
        };
        let syn::Lit::Str(template) = &first.lit else {
            return None;
        };
        let template = template.value();
        if template.contains('\0') {
            return None;
        }
        let mut result = String::new();
        let mut chars = template.chars().peekable();
        let mut positional = 1;
        let mut consumed = BTreeSet::new();
        while let Some(character) = chars.next() {
            if character != '{' {
                if character == '}' && chars.next()? != '}' {
                    return None;
                }
                result.push(character);
                continue;
            }
            if chars.peek() == Some(&'{') {
                chars.next();
                result.push('{');
                continue;
            }
            let mut placeholder = String::new();
            loop {
                let character = chars.next()?;
                if character == '}' {
                    break;
                }
                placeholder.push(character);
            }
            if placeholder.is_empty() {
                result.push_str(&self.string(arguments.iter().nth(positional)?)?);
                consumed.insert(positional);
                positional += 1;
            } else if let Ok(index) = placeholder.parse::<usize>() {
                result.push_str(&self.string(arguments.iter().nth(index + 1)?)?);
                consumed.insert(index + 1);
            } else {
                let expression = syn::parse_str::<Expr>(&placeholder).ok()?;
                result.push_str(&self.string(&expression)?);
            }
        }
        if consumed.len() + 1 != arguments.len() {
            return None;
        }
        Some(result)
    }

    fn destination(&self, expression: &Expr) -> Option<(String, Vec<String>)> {
        match expression {
            Expr::MethodCall(call) if call.method == "path" && call.args.is_empty() => {
                let name = match call.receiver.as_ref() {
                    Expr::Field(field) if matches!(field.base.as_ref(), Expr::Path(value) if value.path.is_ident("self")) =>
                    {
                        let syn::Member::Named(name) = &field.member else {
                            return None;
                        };
                        self.temporaries
                            .contains(&name.to_string())
                            .then(|| name.to_string())?
                    }
                    Expr::Path(path) => {
                        let name = path.path.get_ident()?.to_string();
                        self.local_temporaries.contains(&name).then_some(name)?
                    }
                    _ => return None,
                };
                Some((name, Vec::new()))
            }
            Expr::MethodCall(call) if call.method == "join" && call.args.len() == 1 => {
                let (root, mut parts) = self.destination(&call.receiver)?;
                parts.extend(Self::relative(&self.string(call.args.first()?)?)?);
                Some((root, parts))
            }
            Expr::Path(path) => self
                .local_paths
                .get(&path.path.get_ident()?.to_string())
                .cloned(),
            Expr::Reference(value) => self.destination(&value.expr),
            Expr::Paren(value) => self.destination(&value.expr),
            _ => None,
        }
    }

    fn fixed(&self, expression: &Expr) -> Option<PathBuf> {
        match expression {
            Expr::Path(path) => self
                .fixed_paths
                .get(&path.path.get_ident()?.to_string())
                .cloned(),
            Expr::MethodCall(call) if call.method == "join" && call.args.len() == 1 => {
                let mut path = self.fixed(&call.receiver)?;
                let value = self.string(call.args.first()?)?;
                if value.contains('\0') {
                    return None;
                }
                for part in Self::relative(&value)? {
                    path.push(part);
                }
                Some(path)
            }
            Expr::Field(field) if matches!(field.base.as_ref(), Expr::Path(value) if value.path.is_ident("self")) =>
            {
                let syn::Member::Named(name) = &field.member else {
                    return None;
                };
                self.source_paths.get(&name.to_string()).cloned()
            }
            Expr::MethodCall(call)
                if call.method == "to_str" && call.args.is_empty() || Self::guarded(call) =>
            {
                self.fixed(&call.receiver)
            }
            Expr::Reference(value) => self.fixed(&value.expr),
            Expr::Paren(value) => self.fixed(&value.expr),
            _ => None,
        }
    }

    fn command<'a>(&self, expression: &'a Expr) -> Option<(String, Vec<&'a Expr>)> {
        match expression {
            Expr::Call(call)
                if matches!(call.func.as_ref(), Expr::Path(value) if self.external(&value.path) == "std::process::Command::new")
                    && matches!(call.args.first(), Some(Expr::Lit(value)) if matches!(&value.lit, syn::Lit::Str(value) if value.value() == "cargo")) =>
            {
                Some((String::new(), Vec::new()))
            }
            Expr::MethodCall(call) => {
                let (mut directory, mut arguments) = self.command(&call.receiver)?;
                match call.method.to_string().as_str() {
                    "args" => {
                        let Some(Expr::Array(values)) = call.args.first() else {
                            return None;
                        };
                        arguments.extend(values.elems.iter());
                    }
                    "arg" => arguments.push(call.args.first()?),
                    "current_dir" => {
                        let (root, parts) = self.destination(call.args.first()?)?;
                        if !parts.is_empty() {
                            return None;
                        }
                        directory = root;
                    }
                    _ => return None,
                }
                Some((directory, arguments))
            }
            Expr::Paren(value) => self.command(&value.expr),
            _ => None,
        }
    }

    fn compilation(&mut self, expression: &Expr) {
        if self.uncertain {
            return;
        }
        let Some((directory, arguments)) = self.command(expression) else {
            return;
        };
        let Some(separator) = arguments
            .iter()
            .position(|argument| self.string(argument).as_deref() == Some("--"))
        else {
            return;
        };
        if !self.dylint_arguments(&arguments[..separator]) {
            return;
        }
        let mut selected = None;
        let mut index = separator + 1;
        while index < arguments.len() {
            let Some(argument) = self.string(arguments[index]) else {
                return;
            };
            match argument.as_str() {
                "--example" | "--bin" => {
                    if selected.is_some() {
                        return;
                    }
                    index += 1;
                    let Some(name) = arguments.get(index).and_then(|value| self.string(value))
                    else {
                        return;
                    };
                    selected = Some((argument, name));
                }
                "--lib" => {
                    if selected.is_some() {
                        return;
                    }
                    selected = Some((argument, String::new()));
                }
                "--locked" | "--offline" | "--release" | "--message-format=json" => {}
                _ => return,
            }
            index += 1;
        }
        let Some((kind, name)) = selected else {
            return;
        };
        for path in self.target_paths(&directory, &kind, &name) {
            let key = (directory.clone(), path.clone());
            let Some(Some(position)) = self.written.get(&key) else {
                continue;
            };
            let mut parameters = BTreeSet::new();
            for part in &path {
                for (index, value) in part.split('\0').enumerate() {
                    if index % 2 == 1 {
                        let Ok(position) = value.parse::<usize>() else {
                            return;
                        };
                        parameters.insert(position);
                    }
                }
            }
            self.methods
                .entry(self.method.clone())
                .or_default()
                .entry(*position)
                .or_default()
                .extend(parameters);
        }
    }

    fn dylint_arguments(&self, arguments: &[&Expr]) -> bool {
        if arguments
            .first()
            .and_then(|value| self.string(value))
            .as_deref()
            != Some("dylint")
        {
            return false;
        }
        let mut index = 1;
        let mut library = false;
        while index < arguments.len() {
            if self.string(arguments[index]).as_deref() != Some("--path") || library {
                return false;
            }
            index += 1;
            if arguments
                .get(index)
                .and_then(|value| self.fixed(value))
                .as_deref()
                != Some(self.source_directory.as_path())
            {
                return false;
            }
            library = true;
            index += 1;
        }
        library
    }

    fn target_paths(&self, directory: &str, kind: &str, name: &str) -> Vec<Vec<String>> {
        let Some(manifest) = self.projects.get(directory) else {
            return Vec::new();
        };
        let group = match kind {
            "--example" => "example",
            "--bin" => "bin",
            "--lib" => "lib",
            _ => return Vec::new(),
        };
        if group == "lib" {
            let path = manifest
                .get("lib")
                .and_then(|value| value.get("path"))
                .and_then(toml::Value::as_str)
                .unwrap_or("src/lib.rs");
            return Self::relative(path).into_iter().collect();
        }
        if let Some(targets) = manifest.get(group).and_then(toml::Value::as_array) {
            if name.contains('\0') && !targets.is_empty() {
                return Vec::new();
            }
            for target in targets {
                if target.get("name").and_then(toml::Value::as_str) != Some(name) {
                    continue;
                }
                if let Some(path) = target.get("path").and_then(toml::Value::as_str) {
                    return Self::relative(path).into_iter().collect();
                }
            }
        }
        let automatic = if group == "example" {
            "autoexamples"
        } else {
            "autobins"
        };
        if manifest
            .get("package")
            .and_then(|value| value.get(automatic))
            .and_then(toml::Value::as_bool)
            == Some(false)
        {
            return Vec::new();
        }
        let parts = match Self::relative(name) {
            Some(parts) if parts.len() == 1 => parts,
            _ => return Vec::new(),
        };
        let prefix = if group == "example" {
            vec!["examples".into()]
        } else {
            vec!["src".into(), "bin".into()]
        };
        let mut file = prefix.clone();
        file.push(format!("{}.rs", parts[0]));
        let mut main = prefix;
        main.push(parts[0].clone());
        main.push("main.rs".into());
        vec![file, main]
    }

    fn filesystem(&mut self, call: &syn::ExprCall, successful: bool) -> bool {
        let Expr::Path(function) = call.func.as_ref() else {
            return false;
        };
        let operation = self.external(&function.path);
        if operation != "std::fs::write" && operation != "std::fs::copy" {
            return false;
        }
        let destination = if operation == "std::fs::copy" {
            call.args.iter().nth(1)
        } else {
            call.args.first()
        };
        let Some(destination) = destination.and_then(|value| self.destination(value)) else {
            self.unknown();
            return true;
        };
        if destination.1 == ["Cargo.toml"] {
            let source = if operation == "std::fs::copy" {
                call.args
                    .first()
                    .and_then(|value| self.fixed(value))
                    .and_then(|path| Self::regular(&path).ok())
            } else {
                call.args.iter().nth(1).and_then(|value| self.string(value))
            };
            let manifest = source
                .filter(|_| successful)
                .and_then(|source| toml::from_str::<toml::Value>(&source).ok());
            match manifest {
                Some(manifest)
                    if manifest
                        .get("package")
                        .and_then(|value| value.get("name"))
                        .and_then(toml::Value::as_str)
                        .is_some() =>
                {
                    self.projects.insert(destination.0, manifest);
                }
                _ => {
                    self.projects.remove(&destination.0);
                }
            }
            return true;
        }
        if self.constructor {
            return true;
        }
        let position = if successful && operation == "std::fs::write" {
            call.args.iter().nth(1).and_then(|value| match value {
                Expr::Path(path) => self
                    .parameters
                    .get(&path.path.get_ident()?.to_string())
                    .copied(),
                _ => None,
            })
        } else {
            None
        };
        self.written.insert(destination, position);
        true
    }

    fn guarded(call: &syn::ExprMethodCall) -> bool {
        match call.method.to_string().as_str() {
            "unwrap" => call.args.is_empty(),
            "expect" => {
                call.args.len() == 1
                    && matches!(call.args.first(), Some(Expr::Lit(value)) if matches!(value.lit, syn::Lit::Str(_)))
            }
            _ => false,
        }
    }

    fn unknown(&mut self) {
        self.written.clear();
        self.projects.clear();
        self.uncertain = true;
    }

    fn local_origin(&mut self, name: &str, expression: &Expr) {
        self.local_paths.remove(name);
        self.fixed_paths.remove(name);
        self.local_temporaries.remove(name);
        let origin = match expression {
            Expr::MethodCall(call) if Self::guarded(call) => call.receiver.as_ref(),
            _ => expression,
        };
        if self.constructor
            && let Expr::Call(call) = origin
            && let Expr::Path(function) = call.func.as_ref()
        {
            match self.external(&function.path).as_str() {
                "tempfile::tempdir" if !std::ptr::eq(origin, expression) => {
                    self.local_temporaries.insert(name.to_owned());
                    return;
                }
                "std::path::PathBuf::from" => {
                    if let Some(Expr::Macro(value)) = call.args.first()
                        && value.mac.path.is_ident("env")
                        && let Ok(value) = syn::parse2::<syn::LitStr>(value.mac.tokens.clone())
                        && value.value() == "CARGO_MANIFEST_DIR"
                    {
                        self.fixed_paths
                            .insert(name.to_owned(), self.source_directory.clone());
                        return;
                    }
                }
                _ => {}
            }
        }
        if let Some(path) = self.destination(expression) {
            self.local_paths.insert(name.to_owned(), path);
        }
        if let Some(path) = self.fixed(expression) {
            self.fixed_paths.insert(name.to_owned(), path);
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
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        self.unknown();
        if !self.method.is_empty() || self.constructor {
            return;
        }
        visit::visit_item_use(self, item);
    }

    fn visit_impl_item_fn(&mut self, function: &'ast syn::ImplItemFn) {
        if Self::constructor(function, &self.harness) || !Self::enabled(&function.attrs, false) {
            return;
        }
        let projects = self.projects.clone();
        self.uncertain = false;
        self.parameters.clear();
        self.written.clear();
        self.local_paths.clear();
        self.method = function.sig.ident.to_string();
        let mut position = 0;
        for input in &function.sig.inputs {
            if let syn::FnArg::Typed(input) = input {
                if let Pat::Ident(binding) = input.pat.as_ref() {
                    self.parameters.insert(binding.ident.to_string(), position);
                }
                position += 1;
            }
        }
        visit::visit_impl_item_fn(self, function);
        self.method.clear();
        self.parameters.clear();
        self.local_paths.clear();
        self.written.clear();
        self.projects = projects;
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if self.filesystem(call, false) {
            return;
        }
        if (!self.method.is_empty() || self.constructor)
            && !matches!(call.func.as_ref(), Expr::Path(value) if matches!(self.external(&value.path).as_str(), "std::process::Command::new" | "std::path::PathBuf::from" | "tempfile::tempdir" | "std::fs::create_dir" | "std::fs::create_dir_all"))
        {
            self.unknown();
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if Self::guarded(call)
            && let Expr::Call(operation) = call.receiver.as_ref()
            && self.filesystem(operation, true)
        {
            return;
        }
        let mechanical = Self::guarded(call)
            || (matches!(call.method.to_string().as_str(), "output" | "status")
                && call.args.is_empty()
                && self.command(&call.receiver).is_some())
            || self.command(&Expr::MethodCall(call.clone())).is_some()
            || self.destination(&Expr::MethodCall(call.clone())).is_some()
            || self.fixed(&Expr::MethodCall(call.clone())).is_some();
        if (!self.method.is_empty() || self.constructor) && !mechanical {
            self.unknown();
        }
        visit::visit_expr_method_call(self, call);
        if !self.method.is_empty()
            && call.args.is_empty()
            && matches!(call.method.to_string().as_str(), "output" | "status")
        {
            self.compilation(&call.receiver);
        }
    }

    fn visit_expr_try(&mut self, expression: &'ast syn::ExprTry) {
        if let Expr::Call(operation) = expression.expr.as_ref()
            && self.filesystem(operation, true)
        {
            return;
        }
        visit::visit_expr_try(self, expression);
    }

    fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {}

    fn visit_expr_async(&mut self, _: &'ast syn::ExprAsync) {}

    fn visit_expr_while(&mut self, expression: &'ast syn::ExprWhile) {
        if self.harness.is_empty() {
            visit::visit_expr_while(self, expression);
            return;
        }
        self.unknown();
    }

    fn visit_expr_loop(&mut self, expression: &'ast syn::ExprLoop) {
        if self.harness.is_empty() {
            visit::visit_expr_loop(self, expression);
            return;
        }
        self.unknown();
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        if let Pat::Ident(binding) = &local.pat {
            self.mutexes.remove(&binding.ident.to_string());
            let name = binding.ident.to_string();
            self.bindings.remove(&name);
            self.parameters.remove(&name);
            if let Some(value) = &local.init {
                self.local_origin(&name, &value.expr);
            }
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
        let parameters = self.parameters.clone();
        let local_paths = self.local_paths.clone();
        let fixed_paths = self.fixed_paths.clone();
        let local_temporaries = self.local_temporaries.clone();
        let mutexes = self.mutexes.clone();
        for statement in &block.stmts {
            self.visit_stmt(statement);
            if matches!(
                statement,
                Stmt::Expr(Expr::Return(_) | Expr::Break(_) | Expr::Continue(_), _)
            ) {
                self.unknown();
                break;
            }
        }
        self.bindings = bindings;
        self.parameters = parameters;
        self.local_paths = local_paths;
        self.fixed_paths = fixed_paths;
        self.local_temporaries = local_temporaries;
        self.mutexes = mutexes;
    }

    fn visit_expr_assign(&mut self, assignment: &'ast syn::ExprAssign) {
        self.unknown();
        if let Expr::Path(path) = assignment.left.as_ref()
            && let Some(name) = path.path.get_ident()
        {
            self.bindings.remove(&name.to_string());
            self.parameters.remove(&name.to_string());
            self.local_paths.remove(&name.to_string());
            self.fixed_paths.remove(&name.to_string());
            self.local_temporaries.remove(&name.to_string());
        }
        visit::visit_expr_assign(self, assignment);
    }

    fn visit_expr_if(&mut self, expression: &'ast syn::ExprIf) {
        if self.harness.is_empty() {
            visit::visit_expr_if(self, expression);
        } else {
            self.bindings.clear();
            self.unknown();
        }
    }

    fn visit_expr_match(&mut self, expression: &'ast syn::ExprMatch) {
        if self.harness.is_empty() {
            visit::visit_expr_match(self, expression);
        } else {
            self.bindings.clear();
            self.unknown();
        }
    }

    fn visit_expr_for_loop(&mut self, expression: &'ast syn::ExprForLoop) {
        if !self.method.is_empty() || self.constructor {
            self.unknown();
            return;
        }
        if self.uncertain {
            visit::visit_expr_for_loop(self, expression);
            return;
        }
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
                    for (position, dependencies) in positions {
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
                                && dependencies.iter().all(|position| {
                                    let Some(Expr::Path(argument)) = call.args.iter().nth(*position) else { return false; };
                                    let Some(argument) = argument.path.get_ident() else { return false; };
                                    if invalid.contains(&argument.to_string()) { return false; }
                                    let Some(index) = pattern.elems.iter().position(|value| matches!(value, Pat::Ident(binding) if binding.ident == *argument)) else { return false; };
                                    let Some(Expr::Lit(value)) = tuple.elems.iter().nth(index) else { return false; };
                                    let syn::Lit::Str(value) = &value.lit else { return false; };
                                    matches!(Self::relative(&value.value()), Some(parts) if parts.len() == 1 && !parts[0].contains('\0'))
                                })
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
        if item
            .attrs
            .iter()
            .any(|attribute| attribute.path().is_ident("macro_use"))
        {
            self.unknown();
        }
        if !self.method.is_empty() || self.constructor {
            self.unknown();
            return;
        }
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
        if value.path.is_ident("macro_rules") {
            self.unknown();
        }
        if (!self.method.is_empty() || self.constructor)
            && !matches!(
                value.path.get_ident().map(ToString::to_string).as_deref(),
                Some("format" | "env")
            )
        {
            self.unknown();
        }
        if value.path.is_ident("include")
            && let Ok(path) = syn::parse2::<syn::LitStr>(value.tokens.clone())
        {
            self.modules.insert(self.file_base.join(path.value()));
        }
        visit::visit_macro(self, value);
    }
}
