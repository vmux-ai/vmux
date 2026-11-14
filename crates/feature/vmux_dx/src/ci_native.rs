use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use proc_macro2::{Group, LineColumn, Span, TokenStream, TokenTree};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, ImplItem, Item, Meta, Token, TraitItem};

use crate::Result;
use crate::execution::Execution;

pub struct Changes {
    root: PathBuf,
}

impl Changes {
    pub fn new(root: PathBuf) -> Result<Self> {
        Ok(Self {
            root: root.canonicalize()?,
        })
    }

    pub fn run(&self, args: &[OsString]) -> Result<()> {
        let mut base = std::env::var("VMUX_CI_BASE")
            .ok()
            .filter(|base| !base.is_empty());
        let mut head = String::from("HEAD");
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            let value = args
                .next()
                .ok_or("Missing native CI comparison reference")?;
            let value = value.to_str().ok_or("Invalid native CI reference")?;
            if value.is_empty() || value.starts_with('-') {
                return Err("Invalid native CI reference".into());
            }
            match arg.to_str() {
                Some("--base") => base = Some(value.to_owned()),
                Some("--head") => head = value.to_owned(),
                _ => return Err(format!("Unknown native CI argument: {arg:?}").into()),
            }
        }
        let Some(base) = base else {
            println!("native=true");
            return Ok(());
        };
        let output = self.git(&["merge-base", "--", &base, &head])?;
        let base = String::from_utf8(output.stdout)?.trim().to_owned();
        let output = self.git(&[
            "diff",
            "--name-only",
            "--no-renames",
            "-z",
            &base,
            &head,
            "--",
        ])?;
        let paths = String::from_utf8(output.stdout)?;
        for path in paths.split_terminator('\0') {
            if self.requires_native(&base, &head, path) {
                eprintln!("Native CI required by {path}");
                println!("native=true");
                return Ok(());
            }
        }
        eprintln!("Native CI unchanged: only portable test or documentation inputs changed");
        println!("native=false");
        Ok(())
    }

    fn requires_native(&self, base: &str, head: &str, path: &str) -> bool {
        let file = Path::new(path);
        if file.starts_with("docs") || path == "ecs-policy.ron" {
            return false;
        }
        if !file.starts_with("crates") || file.extension().is_none_or(|extension| extension != "rs")
        {
            return true;
        }
        if file.file_name().is_some_and(|name| name == "build.rs") {
            return true;
        }
        if file.parent() == Some(Path::new("crates")) {
            return true;
        }
        if file.starts_with("crates/feature/vmux_dx") {
            return true;
        }
        let Ok(before_tree) = self.git(&["ls-tree", base, "--", path]) else {
            return true;
        };
        let Ok(after_tree) = self.git(&["ls-tree", head, "--", path]) else {
            return true;
        };
        let before_mode = before_tree.stdout.get(..6);
        let after_mode = after_tree.stdout.get(..6);
        if before_mode != after_mode || !matches!(before_mode, Some(b"100644" | b"100755")) {
            return true;
        }
        let Ok(before) = self.source(base, path) else {
            return true;
        };
        let Ok(after) = self.source(head, path) else {
            return true;
        };
        if before.changed(&after) {
            return true;
        }
        let Ok(roots) = self.library_roots(file) else {
            return true;
        };
        !Modules::new(self, base, file).portable(&roots)
            || !Modules::new(self, head, file).portable(&roots)
    }

    fn library_roots(&self, file: &Path) -> Result<Vec<PathBuf>> {
        let mut command =
            Command::new(std::env::var_os("CARGO_BIN").unwrap_or_else(|| "cargo".into()));
        command
            .args(["metadata", "--locked", "--no-deps", "--format-version=1"])
            .current_dir(&self.root);
        let metadata = Execution::new(command).output(None)?;
        let mut command = Command::new("jq");
        command.args(["-j", r#".packages[] | .manifest_path as $manifest | .targets[] | select(any(.kind[]; . == "lib" or . == "proc-macro")) | $manifest, "\u0000", .src_path, "\u0000""#]);
        let output = Execution::new(command).output(Some(&metadata.stdout))?;
        let records = String::from_utf8(output.stdout)?;
        let mut fields = records.split_terminator('\0');
        let mut roots = Vec::new();
        let mut depth = 0;
        while let Some(manifest) = fields.next() {
            let source = fields.next().ok_or("Missing Cargo library source")?;
            let directory = Path::new(manifest)
                .parent()
                .ok_or("Missing Cargo package directory")?
                .strip_prefix(&self.root)?;
            if !file.starts_with(directory) {
                continue;
            }
            let length = directory.components().count();
            if length < depth {
                continue;
            }
            if length > depth {
                roots.clear();
                depth = length;
            }
            roots.push(Path::new(source).strip_prefix(&self.root)?.to_path_buf());
        }
        if roots.is_empty() {
            return Err("Cannot establish the Rust library module roots".into());
        }
        Ok(roots)
    }

    fn syntax(&self, revision: &str, path: &Path) -> Result<syn::File> {
        let path = path.to_str().ok_or("Invalid Rust module path")?;
        let output = self.git(&["show", &format!("{revision}:{path}")])?;
        Ok(syn::parse_file(&String::from_utf8(output.stdout)?)?)
    }

    fn source(&self, revision: &str, path: &str) -> Result<Source> {
        let output = self.git(&["show", &format!("{revision}:{path}")])?;
        Source::parse(&String::from_utf8(output.stdout)?)
    }

    fn git(&self, args: &[&str]) -> Result<Output> {
        let mut command = Command::new("git");
        command.args(args).current_dir(&self.root);
        for name in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_COMMON_DIR",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_PREFIX",
            "GIT_CONFIG",
            "GIT_CONFIG_PARAMETERS",
            "GIT_CONFIG_COUNT",
            "GIT_SHALLOW_FILE",
        ] {
            command.env_remove(name);
        }
        Execution::new(command).output(None)
    }
}

struct Modules<'a> {
    changes: &'a Changes,
    revision: &'a str,
    target: &'a Path,
    visited: BTreeSet<PathBuf>,
    found: bool,
    covered: bool,
    unknown: bool,
}

impl<'a> Modules<'a> {
    fn new(changes: &'a Changes, revision: &'a str, target: &'a Path) -> Self {
        Self {
            changes,
            revision,
            target,
            visited: BTreeSet::new(),
            found: false,
            covered: true,
            unknown: false,
        }
    }

    fn portable(mut self, roots: &[PathBuf]) -> bool {
        for root in roots {
            let Some(directory) = root.parent() else {
                return false;
            };
            self.file(root, directory, true);
        }
        self.found && self.covered && !self.unknown
    }

    fn file(&mut self, path: &Path, directory: &Path, inherited: bool) {
        if !self.visited.insert(path.to_path_buf()) {
            self.unknown = true;
            return;
        }
        let Ok(file) = self.changes.syntax(self.revision, path) else {
            self.unknown = true;
            return;
        };
        let covered = inherited && Condition::linux_attributes(&file.attrs);
        if path == self.target {
            self.found = true;
            self.covered &= covered;
            return;
        }
        self.items(&file.items, directory, covered);
    }

    fn items(&mut self, items: &[Item], directory: &Path, inherited: bool) {
        for item in items {
            let Item::Mod(module) = item else {
                if matches!(item, Item::Macro(_)) {
                    self.unknown = true;
                }
                continue;
            };
            if Condition::module_path_unknown(&module.attrs) {
                self.unknown = true;
                continue;
            }
            let child = directory.join(module.ident.to_string());
            let ordinary = child.with_extension("rs");
            let mod_rs = child.join("mod.rs");
            if self.target != ordinary && self.target != mod_rs && !self.target.starts_with(&child)
            {
                continue;
            }
            let covered = inherited && Condition::linux_attributes(&module.attrs);
            if let Some((_, items)) = &module.content {
                self.items(items, &child, covered);
                continue;
            }
            let ordinary_exists = self.exists(&ordinary);
            let mod_exists = self.exists(&mod_rs);
            match (ordinary_exists, mod_exists) {
                (true, false) => self.file(&ordinary, &child, covered),
                (false, true) => self.file(&mod_rs, &child, covered),
                _ => self.unknown = true,
            }
        }
    }

    fn exists(&mut self, path: &Path) -> bool {
        let Some(path) = path.to_str() else {
            self.unknown = true;
            return false;
        };
        match self.changes.git(&["ls-tree", self.revision, "--", path]) {
            Ok(output) => !output.stdout.is_empty(),
            Err(_) => {
                self.unknown = true;
                false
            }
        }
    }
}

struct Source {
    tokens: TokenStream,
    excluded: Vec<Range>,
    platform: Vec<Range>,
}

impl Source {
    fn parse(text: &str) -> Result<Self> {
        let tokens: TokenStream = text.parse()?;
        let file: syn::File = syn::parse2(tokens.clone())?;
        let mut collector = Collector::default();
        collector.inspect(&file.attrs, file.span());
        collector.visit_file(&file);
        Ok(Self {
            tokens,
            excluded: collector.excluded,
            platform: collector.platform,
        })
    }

    fn changed(&self, other: &Self) -> bool {
        self.filtered(&self.excluded, false) != other.filtered(&other.excluded, false)
            || self.filtered(&self.platform, true) != other.filtered(&other.platform, true)
    }

    fn filtered(&self, ranges: &[Range], retain: bool) -> String {
        Range::filter(self.tokens.clone(), ranges, retain).to_string()
    }
}

struct Range {
    start: LineColumn,
    end: LineColumn,
}

impl Range {
    fn of(span: Span) -> Self {
        Self {
            start: span.start(),
            end: span.end(),
        }
    }

    fn contains(&self, span: Span) -> bool {
        self.start <= span.start() && span.end() <= self.end
    }

    fn filter(tokens: TokenStream, ranges: &[Self], retain: bool) -> TokenStream {
        let mut result = TokenStream::new();
        for token in tokens {
            let included = ranges.iter().any(|range| range.contains(token.span()));
            if included {
                if retain {
                    result.extend([token]);
                }
                continue;
            }
            if let TokenTree::Group(group) = token {
                let inner = Self::filter(group.stream(), ranges, retain);
                if retain && inner.is_empty() {
                    continue;
                }
                let mut replacement = Group::new(group.delimiter(), inner);
                replacement.set_span(group.span());
                result.extend([TokenTree::Group(replacement)]);
            } else if !retain {
                result.extend([token]);
            }
        }
        result
    }
}

enum Condition {
    True,
    False,
    Unknown,
}

impl Condition {
    fn of(meta: &Meta) -> Self {
        Self::evaluate(meta, false)
    }

    fn evaluate(meta: &Meta, linux_tests: bool) -> Self {
        match meta {
            Meta::Path(path) if path.is_ident("test") => {
                if linux_tests {
                    Self::True
                } else {
                    Self::False
                }
            }
            Meta::Path(path) if linux_tests && (path.is_ident("unix") || path.is_ident("host")) => {
                Self::True
            }
            Meta::List(list) => {
                let Ok(parts) =
                    list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                else {
                    return Self::Unknown;
                };
                if list.path.is_ident("not") && parts.len() == 1 {
                    return match Self::evaluate(&parts[0], linux_tests) {
                        Self::False => Self::True,
                        Self::True => Self::False,
                        Self::Unknown => Self::Unknown,
                    };
                }
                let all = list.path.is_ident("all");
                if !all && !list.path.is_ident("any") {
                    return Self::Unknown;
                }
                let mut unknown = false;
                for part in parts {
                    match Self::evaluate(&part, linux_tests) {
                        Self::False if all => return Self::False,
                        Self::True if !all => return Self::True,
                        Self::Unknown => unknown = true,
                        _ => {}
                    }
                }
                if unknown {
                    Self::Unknown
                } else if all {
                    Self::True
                } else {
                    Self::False
                }
            }
            _ => Self::Unknown,
        }
    }

    fn portable(meta: &Meta) -> bool {
        match meta {
            Meta::Path(path) => path.is_ident("test") || path.is_ident("unix"),
            Meta::List(list)
                if list.path.is_ident("all")
                    || list.path.is_ident("any")
                    || list.path.is_ident("not") =>
            {
                let Ok(parts) =
                    list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                else {
                    return false;
                };
                for part in parts {
                    if !Self::portable(&part) {
                        return false;
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn platform_attribute(meta: &Meta) -> bool {
        let Meta::List(list) = meta else {
            return meta.path().is_ident("path");
        };
        if !list.path.is_ident("cfg") && !list.path.is_ident("cfg_attr") {
            return list.path.is_ident("path");
        }
        let Ok(parts) = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        else {
            return true;
        };
        let Some(condition) = parts.first() else {
            return true;
        };
        if list.path.is_ident("cfg") && parts.len() != 1 {
            return true;
        }
        if !Self::portable(condition) {
            return true;
        }
        if list.path.is_ident("cfg_attr") {
            for attribute in parts.iter().skip(1) {
                if Self::platform_attribute(attribute) {
                    return true;
                }
            }
        }
        false
    }

    fn linux_attributes(attributes: &[Attribute]) -> bool {
        for attribute in attributes {
            if !Self::linux_attribute(&attribute.meta) {
                return false;
            }
        }
        true
    }

    fn linux_attribute(meta: &Meta) -> bool {
        let Meta::List(list) = meta else {
            return !meta.path().is_ident("path");
        };
        if !list.path.is_ident("cfg") && !list.path.is_ident("cfg_attr") {
            return !list.path.is_ident("path");
        }
        let Ok(parts) = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        else {
            return false;
        };
        let Some(condition) = parts.first() else {
            return false;
        };
        let value = Self::evaluate(condition, true);
        if list.path.is_ident("cfg") {
            return parts.len() == 1 && matches!(value, Self::True);
        }
        match value {
            Self::False => return true,
            Self::Unknown => return false,
            Self::True => {}
        }
        for attribute in parts.iter().skip(1) {
            if !Self::linux_attribute(attribute) {
                return false;
            }
        }
        true
    }

    fn module_path_unknown(attributes: &[Attribute]) -> bool {
        for attribute in attributes {
            let path = attribute.path();
            if path.is_ident("path") {
                return true;
            }
            if path.is_ident("cfg_attr") {
                let Meta::List(list) = &attribute.meta else {
                    return true;
                };
                let Ok(parts) =
                    list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                else {
                    return true;
                };
                for emitted in parts.iter().skip(1) {
                    if Self::path_attribute(emitted) {
                        return true;
                    }
                }
            } else if !path.is_ident("cfg")
                && !path.is_ident("allow")
                && !path.is_ident("warn")
                && !path.is_ident("deny")
                && !path.is_ident("forbid")
                && !path.is_ident("doc")
                && !path.is_ident("deprecated")
            {
                return true;
            }
        }
        false
    }

    fn path_attribute(meta: &Meta) -> bool {
        if meta.path().is_ident("path") {
            return true;
        }
        if let Meta::List(list) = meta
            && list.path.is_ident("cfg_attr")
        {
            let Ok(parts) = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            else {
                return true;
            };
            for emitted in parts.iter().skip(1) {
                if Self::path_attribute(emitted) {
                    return true;
                }
            }
        }
        false
    }
}

#[derive(Default)]
struct Collector {
    excluded: Vec<Range>,
    platform: Vec<Range>,
}

impl Collector {
    fn inspect(&mut self, attributes: &[Attribute], span: Span) {
        for attribute in attributes {
            if !attribute.path().is_ident("cfg") && !attribute.path().is_ident("cfg_attr") {
                continue;
            }
            if Condition::platform_attribute(&attribute.meta) {
                self.platform.push(Range::of(span));
            }
            let Meta::List(list) = &attribute.meta else {
                continue;
            };
            let Ok(parts) = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            else {
                continue;
            };
            let Some(condition) = parts.first() else {
                continue;
            };
            if attribute.path().is_ident("cfg")
                && parts.len() == 1
                && matches!(Condition::of(condition), Condition::False)
            {
                self.excluded.push(Range::of(span));
            }
        }
    }
}

impl<'ast> Visit<'ast> for Collector {
    fn visit_block(&mut self, node: &'ast syn::Block) {
        let mut predicates = Predicates::default();
        predicates.visit_block(node);
        if predicates.platform {
            self.platform.push(Range::of(node.span()));
        }
        visit::visit_block(self, node);
    }

    fn visit_item(&mut self, node: &'ast Item) {
        let attributes: &[Attribute] = match node {
            Item::Const(node) => &node.attrs,
            Item::Enum(node) => &node.attrs,
            Item::ExternCrate(node) => &node.attrs,
            Item::Fn(node) => &node.attrs,
            Item::ForeignMod(node) => &node.attrs,
            Item::Impl(node) => &node.attrs,
            Item::Macro(node) => &node.attrs,
            Item::Mod(node) => &node.attrs,
            Item::Static(node) => &node.attrs,
            Item::Struct(node) => &node.attrs,
            Item::Trait(node) => &node.attrs,
            Item::TraitAlias(node) => &node.attrs,
            Item::Type(node) => &node.attrs,
            Item::Union(node) => &node.attrs,
            Item::Use(node) => &node.attrs,
            _ => &[],
        };
        self.inspect(attributes, node.span());
        visit::visit_item(self, node);
    }

    fn visit_impl_item(&mut self, node: &'ast ImplItem) {
        let attributes: &[Attribute] = match node {
            ImplItem::Const(node) => &node.attrs,
            ImplItem::Fn(node) => &node.attrs,
            ImplItem::Type(node) => &node.attrs,
            ImplItem::Macro(node) => &node.attrs,
            _ => &[],
        };
        self.inspect(attributes, node.span());
        visit::visit_impl_item(self, node);
    }

    fn visit_trait_item(&mut self, node: &'ast TraitItem) {
        let attributes: &[Attribute] = match node {
            TraitItem::Const(node) => &node.attrs,
            TraitItem::Fn(node) => &node.attrs,
            TraitItem::Type(node) => &node.attrs,
            TraitItem::Macro(node) => &node.attrs,
            _ => &[],
        };
        self.inspect(attributes, node.span());
        visit::visit_trait_item(self, node);
    }

    fn visit_field(&mut self, node: &'ast syn::Field) {
        self.inspect(&node.attrs, node.span());
        visit::visit_field(self, node);
    }

    fn visit_variant(&mut self, node: &'ast syn::Variant) {
        self.inspect(&node.attrs, node.span());
        visit::visit_variant(self, node);
    }

    fn visit_local(&mut self, node: &'ast syn::Local) {
        self.inspect(&node.attrs, node.span());
        visit::visit_local(self, node);
    }

    fn visit_arm(&mut self, node: &'ast syn::Arm) {
        self.inspect(&node.attrs, node.span());
        visit::visit_arm(self, node);
    }

    fn visit_stmt_macro(&mut self, node: &'ast syn::StmtMacro) {
        self.inspect(&node.attrs, node.span());
        visit::visit_stmt_macro(self, node);
    }

    fn visit_expr(&mut self, node: &'ast Expr) {
        let attributes: &[Attribute] = match node {
            Expr::Array(node) => &node.attrs,
            Expr::Assign(node) => &node.attrs,
            Expr::Async(node) => &node.attrs,
            Expr::Await(node) => &node.attrs,
            Expr::Binary(node) => &node.attrs,
            Expr::Block(node) => &node.attrs,
            Expr::Break(node) => &node.attrs,
            Expr::Call(node) => &node.attrs,
            Expr::Cast(node) => &node.attrs,
            Expr::Closure(node) => &node.attrs,
            Expr::Const(node) => &node.attrs,
            Expr::Continue(node) => &node.attrs,
            Expr::Field(node) => &node.attrs,
            Expr::ForLoop(node) => &node.attrs,
            Expr::Group(node) => &node.attrs,
            Expr::If(node) => &node.attrs,
            Expr::Index(node) => &node.attrs,
            Expr::Infer(node) => &node.attrs,
            Expr::Let(node) => &node.attrs,
            Expr::Lit(node) => &node.attrs,
            Expr::Loop(node) => &node.attrs,
            Expr::Macro(node) => &node.attrs,
            Expr::Match(node) => &node.attrs,
            Expr::MethodCall(node) => &node.attrs,
            Expr::Paren(node) => &node.attrs,
            Expr::Path(node) => &node.attrs,
            Expr::Range(node) => &node.attrs,
            Expr::RawAddr(node) => &node.attrs,
            Expr::Reference(node) => &node.attrs,
            Expr::Repeat(node) => &node.attrs,
            Expr::Return(node) => &node.attrs,
            Expr::Struct(node) => &node.attrs,
            Expr::Try(node) => &node.attrs,
            Expr::TryBlock(node) => &node.attrs,
            Expr::Tuple(node) => &node.attrs,
            Expr::Unary(node) => &node.attrs,
            Expr::Unsafe(node) => &node.attrs,
            Expr::While(node) => &node.attrs,
            Expr::Yield(node) => &node.attrs,
            _ => &[],
        };
        self.inspect(attributes, node.span());
        visit::visit_expr(self, node);
    }
}

#[derive(Default)]
struct Predicates {
    platform: bool,
}

impl Predicates {
    fn condition(&mut self, tokens: TokenStream) {
        match syn::parse2::<Meta>(tokens) {
            Ok(meta) => self.platform |= !Condition::portable(&meta),
            Err(_) => self.platform = true,
        }
    }

    fn nested(&mut self, tokens: TokenStream) {
        let mut tokens = tokens.into_iter().peekable();
        while let Some(token) = tokens.next() {
            match token {
                TokenTree::Group(group) => self.nested(group.stream()),
                TokenTree::Ident(ident) if ident == "cfg" => {
                    if !matches!(tokens.peek(), Some(TokenTree::Punct(punct)) if punct.as_char() == '!')
                    {
                        continue;
                    }
                    tokens.next();
                    if let Some(TokenTree::Group(group)) = tokens.next() {
                        self.condition(group.stream());
                    }
                }
                _ => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for Predicates {
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if node
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "cfg")
        {
            self.condition(node.tokens.clone());
        }
        self.nested(node.tokens.clone());
        visit::visit_macro(self, node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_test_changes_preserve_the_normal_build() {
        for (before, after) in [
            (
                "fn live() {} #[cfg(test)] mod tests { fn check() { assert!(true); } }",
                "fn live() {} #[cfg(test)] mod tests { fn check() { assert!(false); } }",
            ),
            (
                "fn live() {} #[cfg(test)] fn helper() { one(); }",
                "fn live() {} #[cfg(test)] fn helper() { two(); }",
            ),
            (
                "struct Live; impl Live { fn live() {} #[cfg(test)] fn helper() { one(); } }",
                "struct Live; impl Live { fn live() {} #[cfg(test)] fn helper() { two(); } }",
            ),
            (
                "fn live() { #[cfg(test)] let sample = one(); actual(); }",
                "fn live() { #[cfg(test)] let sample = two(); actual(); }",
            ),
            (
                "#[cfg(test)] use first::Fixture; fn live() {}",
                "#[cfg(test)] use second::Fixture; fn live() {}",
            ),
            (
                "#[cfg(all(test, unix))] fn fixture() { one(); } fn live() {}",
                "#[cfg(all(test, unix))] fn fixture() { two(); } fn live() {}",
            ),
        ] {
            assert!(
                !Source::parse(before)
                    .unwrap()
                    .changed(&Source::parse(after).unwrap()),
                "{after}"
            );
        }
    }

    #[test]
    fn production_or_platform_specific_changes_require_native_validation() {
        for (before, after) in [
            (
                "fn live() { one(); } #[cfg(test)] mod tests {}",
                "fn live() { two(); } #[cfg(test)] mod tests {}",
            ),
            ("#[cfg(test)] fn helper() {}", "fn helper() {}"),
            (
                "#[cfg(any(test, feature = \"shipping\"))] fn live() { one(); }",
                "#[cfg(any(test, feature = \"shipping\"))] fn live() { two(); }",
            ),
            (
                "#[cfg(not(test))] fn live() { one(); }",
                "#[cfg(not(test))] fn live() { two(); }",
            ),
            (
                "#[cfg(test)] mod tests { #[cfg(target_os = \"macos\")] fn check() { one(); } }",
                "#[cfg(test)] mod tests { #[cfg(target_os = \"macos\")] fn check() { two(); } }",
            ),
            (
                "#[cfg(test)] mod tests { #[cfg(feature = \"package\")] fn check() { one(); } }",
                "#[cfg(test)] mod tests { #[cfg(feature = \"package\")] fn check() { two(); } }",
            ),
            (
                "fn live() { let marker = r#\"#[cfg(test)] mod tests { one(); }\"#; }",
                "fn live() { let marker = r#\"#[cfg(test)] mod tests { two(); }\"#; }",
            ),
            ("dsl! { #[cfg(test)] one() }", "dsl! { #[cfg(test)] two() }"),
            (
                "#[cfg(test)] mod tests {} fn live() { one(); }",
                "#[cfg(test)] mod tests {} fn live() { two(); }",
            ),
        ] {
            assert!(
                Source::parse(before)
                    .unwrap()
                    .changed(&Source::parse(after).unwrap()),
                "{after}"
            );
        }
    }

    #[test]
    fn unchanged_platform_tests_do_not_rebuild_for_a_portable_test_edit() {
        let before = "#[cfg(test)] mod tests { #[cfg(target_os = \"macos\")] fn native() { same(); } fn portable() { one(); } }";
        let after = "#[cfg(test)] mod tests { #[cfg(target_os = \"macos\")] fn native() { same(); } fn portable() { two(); } }";
        assert!(
            !Source::parse(before)
                .unwrap()
                .changed(&Source::parse(after).unwrap())
        );
    }

    #[test]
    fn attributes_emitted_by_cfg_attr_preserve_native_test_validation() {
        for attributes in [
            "#[cfg_attr(unix, cfg(target_os = \"macos\"))]",
            "#[cfg_attr(unix, cfg_attr(unix, cfg(target_arch = \"aarch64\")))]",
        ] {
            let before = format!(
                "#[cfg(test)] mod tests {{ {attributes} #[test] fn check() {{ assert!(true); }} }}"
            );
            let after = before.replace("assert!(true)", "assert!(false)");
            assert!(
                Source::parse(&before)
                    .unwrap()
                    .changed(&Source::parse(&after).unwrap()),
                "{attributes}"
            );
        }
    }

    #[test]
    fn cfg_macros_preserve_the_containing_native_test_body() {
        for condition in [
            "target_os = \"macos\"",
            "target_arch = \"aarch64\"",
            "feature = \"package\"",
        ] {
            let before = format!(
                "#[cfg(test)] mod tests {{ #[test] fn check() {{ if cfg!({condition}) {{ assert!(true); }} }} }}"
            );
            let after = before.replace("assert!(true)", "assert!(false)");
            assert!(
                Source::parse(&before)
                    .unwrap()
                    .changed(&Source::parse(&after).unwrap()),
                "{condition}"
            );
        }
    }
}
