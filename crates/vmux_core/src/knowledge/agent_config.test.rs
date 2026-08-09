use super::*;

fn paths(root: &Path) -> AgentConfigPaths {
    AgentConfigPaths {
        claude_instructions: root.join("claude/CLAUDE.md"),
        claude_settings: root.join("claude/settings.json"),
        claude_skills: root.join("claude/skills"),
        codex_instructions: root.join("codex/AGENTS.md"),
        codex_config: root.join("codex/config.toml"),
        codex_memories: root.join("codex/memories"),
        codex_extension_memories: root.join("codex/memories_extensions"),
        vibe_instructions: root.join("vibe/AGENTS.md"),
        vibe_config: root.join("vibe/config.toml"),
    }
}

#[cfg(unix)]
#[test]
fn syncs_native_agent_config_and_points_agents_at_canonical() {
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    let skills = temp.path().join("knowledge/skills");
    let memories = temp.path().join("knowledge/memories");
    let canonical = temp.path().join("knowledge/AGENTS.md");
    std::fs::create_dir_all(skills.join("caveman")).unwrap();
    std::fs::create_dir_all(&paths.codex_memories).unwrap();
    std::fs::write(skills.join("caveman/SKILL.md"), "skill").unwrap();
    std::fs::create_dir_all(paths.claude_instructions.parent().unwrap()).unwrap();
    std::fs::create_dir_all(paths.codex_config.parent().unwrap()).unwrap();
    std::fs::create_dir_all(paths.vibe_config.parent().unwrap()).unwrap();
    let claude_file = format!("claude user\n\n{KNOWLEDGE_START}\nold memory\n{KNOWLEDGE_END}\n");
    let block_only = format!("{KNOWLEDGE_START}\nold memory\n{KNOWLEDGE_END}\n");
    std::fs::write(&paths.claude_instructions, &claude_file).unwrap();
    std::fs::write(&paths.claude_settings, "{\"userSetting\":true}\n").unwrap();
    std::fs::write(&paths.codex_instructions, &block_only).unwrap();
    std::fs::write(&paths.vibe_instructions, &block_only).unwrap();
    std::fs::write(&paths.codex_config, "model = \"gpt\"\n").unwrap();
    std::fs::write(
        &paths.vibe_config,
        "skill_paths = [\n  \"/existing\",\n]\nactive_model = \"model\"\n",
    )
    .unwrap();

    sync_external_agent_configs_from(&paths, &skills, &memories, &canonical).unwrap();
    sync_external_agent_configs_from(&paths, &skills, &memories, &canonical).unwrap();

    let canonical_text = std::fs::read_to_string(&canonical).unwrap();
    assert_eq!(canonical_text, "claude user\n");
    assert!(!canonical_text.contains("old memory"));
    assert_eq!(canonical_text.matches(KNOWLEDGE_START).count(), 0);
    for path in [
        &paths.claude_instructions,
        &paths.codex_instructions,
        &paths.vibe_instructions,
    ] {
        assert!(
            std::fs::symlink_metadata(path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(std::fs::read_link(path).unwrap(), canonical);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "claude user\n");
    }
    let mut claude_backup = paths.claude_instructions.clone().into_os_string();
    claude_backup.push(".bak");
    assert_eq!(
        std::fs::read_to_string(&claude_backup).unwrap(),
        claude_file
    );

    let claude_settings = std::fs::read_to_string(&paths.claude_settings)
        .unwrap()
        .parse::<serde_json::Value>()
        .unwrap();
    assert_eq!(
        claude_settings
            .get("userSetting")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        claude_settings
            .get("autoMemoryDirectory")
            .and_then(|value| value.as_str()),
        Some(memories.join("claude/auto").to_string_lossy().as_ref())
    );
    let codex = std::fs::read_to_string(&paths.codex_config).unwrap();
    assert!(codex.contains("model = \"gpt\""));
    assert!(codex.contains(&skills.join("caveman").to_string_lossy().to_string()));
    assert_eq!(codex.matches(CODEX_SKILLS_START).count(), 1);
    assert!(
        codex
            .parse::<toml::Value>()
            .unwrap()
            .get("project_doc_max_bytes")
            .and_then(toml::Value::as_integer)
            .is_some_and(|value| value >= MIN_CODEX_PROJECT_DOC_BYTES as i64)
    );
    let vibe = std::fs::read_to_string(&paths.vibe_config)
        .unwrap()
        .parse::<toml::Value>()
        .unwrap();
    assert_eq!(
        vibe.get("skill_paths")
            .and_then(toml::Value::as_array)
            .unwrap()
            .iter()
            .filter_map(toml::Value::as_str)
            .collect::<Vec<_>>(),
        vec!["/existing", skills.to_string_lossy().as_ref()]
    );
    assert_eq!(
        std::fs::read_link(paths.claude_skills.join("caveman")).unwrap(),
        skills.join("caveman")
    );
    assert_eq!(
        std::fs::read_link(&paths.codex_memories).unwrap(),
        memories.join("codex/local")
    );
    assert_eq!(
        std::fs::read_link(&paths.codex_extension_memories).unwrap(),
        memories.join("codex/extensions")
    );
}

#[cfg(unix)]
#[test]
fn link_instructions_skips_missing_path_when_vault_empty() {
    let temp = tempfile::tempdir().unwrap();
    let canonical = temp.path().join("knowledge/AGENTS.md");
    let path = temp.path().join("claude/CLAUDE.md");

    link_instructions(&path, &canonical).unwrap();

    // A user who has not populated the vault is untouched: no file planted, no canonical created.
    assert!(!path.exists());
    assert!(std::fs::symlink_metadata(&path).is_err());
    assert!(!canonical.exists());
}

#[cfg(unix)]
#[test]
fn link_instructions_wires_missing_path_when_canonical_has_content() {
    let temp = tempfile::tempdir().unwrap();
    let canonical = temp.path().join("knowledge/AGENTS.md");
    std::fs::create_dir_all(canonical.parent().unwrap()).unwrap();
    std::fs::write(&canonical, "shared config\n").unwrap();
    let path = temp.path().join("claude/CLAUDE.md");

    link_instructions(&path, &canonical).unwrap();

    assert_eq!(std::fs::read_link(&path).unwrap(), canonical);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "shared config\n");
}

#[cfg(unix)]
#[test]
fn link_instructions_migrates_preamble_backs_up_and_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let canonical = temp.path().join("knowledge/AGENTS.md");
    let path = temp.path().join("claude/CLAUDE.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = format!("hand config\n\n{KNOWLEDGE_START}\nmemory\n{KNOWLEDGE_END}\n");
    std::fs::write(&path, &original).unwrap();

    link_instructions(&path, &canonical).unwrap();

    assert_eq!(
        std::fs::read_to_string(&canonical).unwrap(),
        "hand config\n"
    );
    assert!(
        std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read_link(&path).unwrap(), canonical);
    let mut backup = path.clone().into_os_string();
    backup.push(".bak");
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), original);

    std::fs::write(&canonical, "hand config\nedited\n").unwrap();
    link_instructions(&path, &canonical).unwrap();
    assert_eq!(
        std::fs::read_to_string(&canonical).unwrap(),
        "hand config\nedited\n"
    );
    assert_eq!(std::fs::read_link(&path).unwrap(), canonical);
}

#[cfg(unix)]
#[test]
fn link_instructions_keeps_existing_canonical_content() {
    let temp = tempfile::tempdir().unwrap();
    let canonical = temp.path().join("knowledge/AGENTS.md");
    std::fs::create_dir_all(canonical.parent().unwrap()).unwrap();
    std::fs::write(&canonical, "shared config\n").unwrap();
    let path = temp.path().join("codex/AGENTS.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = format!("codex preamble\n{KNOWLEDGE_START}\nmemory\n{KNOWLEDGE_END}\n");
    std::fs::write(&path, &original).unwrap();

    link_instructions(&path, &canonical).unwrap();

    assert_eq!(
        std::fs::read_to_string(&canonical).unwrap(),
        "shared config\n"
    );
    assert_eq!(std::fs::read_link(&path).unwrap(), canonical);
    let mut backup = path.clone().into_os_string();
    backup.push(".bak");
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn link_instructions_leaves_pristine_user_file_untouched() {
    let temp = tempfile::tempdir().unwrap();
    let canonical = temp.path().join("knowledge/AGENTS.md");
    let path = temp.path().join("claude/CLAUDE.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "my own rules\n").unwrap();

    link_instructions(&path, &canonical).unwrap();

    assert!(
        !std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "my own rules\n");
    assert!(read_optional(&canonical).unwrap().is_empty());
    let mut backup = path.clone().into_os_string();
    backup.push(".bak");
    assert!(!Path::new(&backup).exists());
}

#[cfg(unix)]
#[test]
fn link_instructions_leaves_foreign_symlink_untouched() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let canonical = temp.path().join("knowledge/AGENTS.md");
    let dotfiles = temp.path().join("dotfiles/CLAUDE.md");
    std::fs::create_dir_all(dotfiles.parent().unwrap()).unwrap();
    std::fs::write(&dotfiles, "dotfiles rules\n").unwrap();
    let path = temp.path().join("claude/CLAUDE.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    symlink(&dotfiles, &path).unwrap();

    link_instructions(&path, &canonical).unwrap();

    assert!(
        std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read_link(&path).unwrap(), dotfiles);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "dotfiles rules\n");
    assert!(read_optional(&canonical).unwrap().is_empty());
}

#[test]
fn rejects_incomplete_managed_sections() {
    assert!(
        merge_managed_section(KNOWLEDGE_START, "memory", KNOWLEDGE_START, KNOWLEDGE_END).is_err()
    );
    assert!(
        merge_managed_section(KNOWLEDGE_END, "memory", KNOWLEDGE_START, KNOWLEDGE_END).is_err()
    );
}

#[cfg(unix)]
#[test]
fn preserves_nonempty_native_memory_directory() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("codex/memories");
    let target = temp.path().join("knowledge/codex/local");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("existing.md"), "memory").unwrap();

    redirect_empty_directory(&source, &target).unwrap();

    assert!(source.is_dir());
    assert_eq!(
        std::fs::read_to_string(source.join("existing.md")).unwrap(),
        "memory"
    );
}
