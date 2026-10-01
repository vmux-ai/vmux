#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use syn::spanned::Spanned;
    use syn::visit::Visit;
    use syn::{Item, ItemMod};

    #[test]
    fn imports_are_at_the_top_of_every_module() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("workspace root");
        let mut files = Vec::new();
        RustFiles::collect(&workspace.join("crates"), &mut files);
        let mut offenders = Vec::new();
        for path in files {
            let source = fs::read_to_string(&path).expect("Rust source");
            let file = syn::parse_file(&source).expect("valid Rust source");
            ModuleImports::audit(&path, &file.items, &mut offenders);
        }
        offenders.sort();
        assert!(
            offenders.is_empty(),
            "Move every use declaration to its module top:\n{}",
            offenders.join("\n")
        );
    }

    struct RustFiles;

    impl RustFiles {
        fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
            let entries = fs::read_dir(dir).expect("source directory");
            for entry in entries {
                let path = entry.expect("source entry").path();
                if path.is_dir() {
                    Self::collect(&path, files);
                } else if path.extension().is_some_and(|extension| extension == "rs") {
                    files.push(path);
                }
            }
        }
    }

    struct ModuleImports;

    impl ModuleImports {
        fn audit(path: &Path, items: &[Item], offenders: &mut Vec<String>) {
            let mut body_started = false;
            for item in items {
                match item {
                    Item::Use(item) if body_started => {
                        Self::push(path, item.span().start().line, "module", offenders);
                    }
                    Item::Use(_) => {}
                    Item::ExternCrate(_) if !body_started => {}
                    Item::Mod(module) => {
                        body_started = true;
                        Self::module(path, module, offenders);
                    }
                    item => {
                        body_started = true;
                        let mut visitor = BlockImports { path, offenders };
                        visitor.visit_item(item);
                    }
                }
            }
        }

        fn module(path: &Path, module: &ItemMod, offenders: &mut Vec<String>) {
            let Some((_, items)) = &module.content else {
                return;
            };
            Self::audit(path, items, offenders);
        }

        fn push(path: &Path, line: usize, scope: &str, offenders: &mut Vec<String>) {
            offenders.push(format!("{}:{} ({scope})", path.display(), line));
        }
    }

    struct BlockImports<'a> {
        path: &'a Path,
        offenders: &'a mut Vec<String>,
    }

    impl Visit<'_> for BlockImports<'_> {
        fn visit_item_mod(&mut self, _: &ItemMod) {}

        fn visit_item_use(&mut self, item: &syn::ItemUse) {
            ModuleImports::push(self.path, item.span().start().line, "block", self.offenders);
        }
    }
}
