pub struct LanguageIconPath;

impl LanguageIconPath {
    pub fn from_hint(hint: &str) -> Option<String> {
        let hint = hint
            .trim()
            .trim_matches(['"', '\'', '`', ',', ':', ';', '(', ')', '[', ']', '{', '}']);
        if hint.is_empty() || hint.contains("://") {
            return None;
        }
        let name = hint.rsplit(['/', '\\']).next().unwrap_or(hint);
        let normalized = name.to_ascii_lowercase();
        let extension = match normalized.as_str() {
            "dockerfile" => "dockerfile",
            "cmakelists.txt" => "cmake",
            _ => normalized
                .rsplit_once('.')
                .map(|(_, extension)| extension)
                .unwrap_or(&normalized),
        };
        let extension = match extension {
            "rust" | "rs" | "rustc" | "cargo" => "rs",
            "typescript" | "ts" | "mts" | "cts" | "deno" | "bun" => "ts",
            "typescriptreact" | "typescript react" | "tsx" => "tsx",
            "javascript" | "js" | "mjs" | "cjs" | "node" | "nodejs" => "js",
            "javascriptreact" | "javascript react" | "jsx" => "jsx",
            "python" | "python3" | "py" | "pyw" | "pyi" => "py",
            "go" | "golang" => "go",
            "ruby" | "rb" | "gemspec" | "rake" => "rb",
            "shell" | "bash" | "zsh" | "sh" | "ksh" => "sh",
            "lua" => "lua",
            "swift" => "swift",
            "c" | "h" => "c",
            "c++" | "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => "cpp",
            "php" => "php",
            "kotlin" | "kt" | "kts" => "kt",
            "dart" => "dart",
            "elixir" | "ex" | "exs" | "heex" => "ex",
            "haskell" | "hs" | "lhs" => "hs",
            "scala" | "sbt" | "sc" => "scala",
            "zig" => "zig",
            "nim" | "nims" => "nim",
            "ocaml" | "ml" | "mli" => "ml",
            "clojure" | "clj" | "cljs" | "cljc" | "edn" => "clj",
            "elm" => "elm",
            "erlang" | "erl" | "hrl" => "erl",
            "crystal" | "cr" => "cr",
            "julia" | "jl" => "jl",
            "r" => "r",
            "perl" | "pl" | "pm" => "pl",
            "f#" | "fsharp" | "fs" | "fsx" | "fsi" => "fs",
            "v" => "v",
            "solidity" | "sol" => "sol",
            "markdown" | "md" | "mdx" => "md",
            "html" | "htm" | "xhtml" => "html",
            "css" => "css",
            "sass" | "scss" => "scss",
            "vue" => "vue",
            "svelte" => "svelte",
            "astro" => "astro",
            "graphql" | "gql" => "graphql",
            "json" | "jsonc" | "json5" => "json",
            "yaml" | "yml" => "yaml",
            "toml" => "toml",
            "docker" => "dockerfile",
            "terraform" | "tf" | "tfvars" | "hcl" => "tf",
            "gradle" => "gradle",
            "cmake" => "cmake",
            "nix" | "nixos" => "nix",
            "prisma" => "prisma",
            "jupyter" | "ipynb" => "ipynb",
            "vim" => "vim",
            "webassembly" | "wasm" | "wat" => "wasm",
            "powershell" | "ps1" | "psm1" | "psd1" => "ps1",
            "groovy" => "groovy",
            "sql" | "sqlite" | "sqlite3" | "db" => "sqlite",
            _ => return None,
        };
        Some(format!("language.{extension}"))
    }

    pub fn from_text(text: &str) -> Option<String> {
        text.split_whitespace().find_map(Self::from_hint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_language_names_commands_and_paths() {
        assert_eq!(
            LanguageIconPath::from_hint("Rust"),
            Some("language.rs".into())
        );
        assert_eq!(
            LanguageIconPath::from_hint("python3"),
            Some("language.py".into())
        );
        assert_eq!(
            LanguageIconPath::from_hint("src/main.tsx"),
            Some("language.tsx".into())
        );
        assert_eq!(
            LanguageIconPath::from_text("python -c 'print(1)'"),
            Some("language.py".into())
        );
        assert_eq!(LanguageIconPath::from_hint("read_file"), None);
    }
}
