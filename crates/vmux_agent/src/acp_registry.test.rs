use super::*;

const SAMPLE: &str = r#"{
      "version": "1.0.0",
      "agents": [
        {
          "id": "claude-acp",
          "name": "Claude Agent",
          "version": "0.5.0",
          "icon": "https://cdn.example/claude-acp.svg",
          "distribution": {
            "npx": { "package": "@agentclientprotocol/claude-agent-acp", "args": ["--acp"] }
          }
        },
        {
          "id": "mistral-vibe",
          "name": "Mistral Vibe",
          "distribution": {
            "binary": {
              "darwin-aarch64": { "archive": "https://cdn.example/vibe-darwin-arm64.tar.gz", "cmd": "./vibe", "args": ["acp"] },
              "linux-x86_64":  { "archive": "https://cdn.example/vibe-linux-x64.tar.gz",  "cmd": "./vibe", "args": ["acp"] }
            }
          }
        },
        {
          "id": "fast-agent",
          "name": "fast-agent",
          "distribution": { "uvx": { "package": "fast-agent-acp", "args": ["serve"] } }
        }
      ]
    }"#;

#[test]
fn parses_all_distribution_types() {
    let reg = parse(SAMPLE).unwrap();
    assert_eq!(reg.version, "1.0.0");
    assert_eq!(reg.agents.len(), 3);

    let claude = &reg.agents[0];
    assert_eq!(claude.id, "claude-acp");
    assert_eq!(
        claude.icon.as_deref(),
        Some("https://cdn.example/claude-acp.svg")
    );
    assert_eq!(
        claude.distribution.npx.as_ref().unwrap().package,
        "@agentclientprotocol/claude-agent-acp"
    );
    assert!(claude.distribution.binary.is_none());

    let vibe = &reg.agents[1];
    assert!(
        vibe.distribution
            .binary
            .as_ref()
            .unwrap()
            .contains_key("linux-x86_64")
    );

    let fast = &reg.agents[2];
    assert_eq!(
        fast.distribution.uvx.as_ref().unwrap().package,
        "fast-agent-acp"
    );
}

#[test]
fn host_target_matches_arch() {
    let t = RegistryAgent::host_target();
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    assert_eq!(t, Some("darwin-aarch64"));
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    assert_eq!(t, Some("linux-x86_64"));
    let _ = t;
}

#[test]
fn preferred_runtime_prefers_binary_then_node_then_uv() {
    let reg = parse(SAMPLE).unwrap();
    // claude: npx only -> Node.
    assert_eq!(reg.agents[0].preferred_runtime(), Runtime::Node);
    // fast-agent: uvx only -> Uv.
    assert_eq!(reg.agents[2].preferred_runtime(), Runtime::Uv);
    // vibe: binary — on a host the sample covers, no runtime.
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    assert_eq!(reg.agents[1].preferred_runtime(), Runtime::None);
}

#[test]
fn binary_for_host_resolves_matching_target() {
    let reg = parse(SAMPLE).unwrap();
    let vibe = &reg.agents[1];
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let bin = vibe.binary_for_host().unwrap();
        assert_eq!(bin.cmd, "./vibe");
        assert_eq!(bin.args, vec!["acp".to_string()]);
    }
    let _ = vibe;
}
