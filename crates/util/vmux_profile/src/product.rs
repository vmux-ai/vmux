pub struct Product;

impl Product {
    pub fn display_name() -> String {
        let name = Self::name();
        match crate::build_profile() {
            "release" => name.into(),
            "local" => format!("{name} ({})", crate::git_hash()),
            "dev" => format!("{name} Dev ({})", crate::git_hash()),
            other => format!("{name} ({other})"),
        }
    }

    pub const fn name() -> &'static str {
        env!("VMUX_HOST_NAME")
    }

    pub const fn namespace() -> &'static str {
        env!("VMUX_HOST_NAMESPACE")
    }

    pub const fn tray_icon() -> &'static [u8] {
        include_bytes!(env!("VMUX_HOST_TRAY_ICON"))
    }

    pub const fn data_name() -> &'static str {
        env!("VMUX_HOST_DATA_NAME")
    }

    pub const fn config_name() -> &'static str {
        env!("VMUX_HOST_CONFIG_NAME")
    }

    pub const fn service_name() -> &'static str {
        env!("VMUX_HOST_SERVICE_NAME")
    }

    pub const fn bundle_id() -> &'static str {
        env!("VMUX_HOST_BUNDLE_ID")
    }

    pub const fn service_id() -> &'static str {
        env!("VMUX_HOST_SERVICE_ID")
    }

    pub const fn service_bundle() -> &'static str {
        env!("VMUX_HOST_SERVICE_BUNDLE")
    }

    pub const fn agent_plist() -> &'static str {
        env!("VMUX_HOST_AGENT_PLIST")
    }

    pub const fn runtime_stem() -> &'static str {
        env!("VMUX_HOST_RUNTIME_STEM")
    }

    pub const fn cli_id() -> &'static str {
        env!("VMUX_HOST_CLI_ID")
    }

    pub const fn keychain_service() -> &'static str {
        env!("VMUX_HOST_KEYCHAIN_SERVICE")
    }

    pub const fn keychain_label() -> &'static str {
        env!("VMUX_HOST_KEYCHAIN_LABEL")
    }

    pub const fn stable_endpoint() -> &'static str {
        env!("VMUX_HOST_STABLE_ENDPOINT")
    }

    pub const fn preview_endpoint() -> &'static str {
        env!("VMUX_HOST_PREVIEW_ENDPOINT")
    }

    pub const fn camera() -> bool {
        matches!(env!("VMUX_HOST_CAMERA").as_bytes(), b"true")
    }

    pub const fn microphone() -> bool {
        matches!(env!("VMUX_HOST_MICROPHONE").as_bytes(), b"true")
    }

    pub const fn screen() -> bool {
        matches!(env!("VMUX_HOST_SCREEN").as_bytes(), b"true")
    }
}
