use rustc_errors::DiagDecorator;
use rustc_hir::def_id::DefId;
use rustc_lint::LateContext;

use crate::VMUX_SYSTEM_NAME;

impl crate::VmuxExportedSystem<'_> {
    pub(super) fn check_name(&mut self, cx: &LateContext<'_>, function: DefId) {
        if self.named_systems.contains(&function) {
            return;
        }
        let Some(reason) = self.reason(cx, function) else {
            return;
        };
        self.named_systems.insert(function);
        cx.tcx.emit_node_span_lint(
            VMUX_SYSTEM_NAME,
            cx.tcx.local_def_id_to_hir_id(function.expect_local()),
            cx.tcx.def_span(function),
            DiagDecorator(|diagnostic| {
                diagnostic.primary_message(format!(
                    "registered system `{}` must use its module's operation name: {reason}",
                    cx.tcx.item_name(function)
                ));
            }),
        );
    }

    fn reason(&self, cx: &LateContext<'_>, function: DefId) -> Option<String> {
        let name = cx.tcx.item_name(function).to_string();
        let operation = name.strip_prefix("on_").unwrap_or(&name);
        let words: Vec<_> = operation.split('_').collect();
        for redundant in ["feature", "plugin", "system"] {
            if words.contains(&redundant) {
                return Some(format!("remove `{redundant}`"));
            }
        }
        let package = std::env::var("CARGO_PKG_NAME")
            .unwrap_or_else(|_| cx.tcx.crate_name(function.krate).to_string());
        let path = cx.tcx.def_path_str(function);
        let mut contexts = Vec::new();
        if let Some(package) = package.strip_prefix("vmux_") {
            contexts.push(package.to_string());
        }
        let modules: Vec<_> = path.split("::").collect();
        for module in modules.iter().take(modules.len().saturating_sub(1)) {
            if ![
                "bin", "host", "lib", "main", "plugin", "runtime", "src", "test", "tests", "ui",
            ]
            .contains(module)
            {
                contexts.push(module.to_string());
            }
        }
        for context in contexts {
            if operation == context {
                continue;
            }
            let context_words: Vec<_> = context.split('_').collect();
            if context_words.len() <= words.len()
                && words
                    .windows(context_words.len())
                    .any(|window| window == context_words)
            {
                return Some(format!("remove repeated `{context}` context"));
            }
        }
        None
    }
}
