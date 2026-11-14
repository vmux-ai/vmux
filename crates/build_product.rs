use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct ProductMetadata {
    values: BTreeMap<&'static str, String>,
}

impl ProductMetadata {
    pub fn current(root: &Path) -> Result<Self, String> {
        let mut values = BTreeMap::new();
        for (key, value) in std::env::vars_os() {
            let Some(name) = key.to_str() else {
                continue;
            };
            if !name.starts_with("VMUX_HOST_") {
                continue;
            }
            let value = value
                .into_string()
                .map_err(|_| format!("{name} must be UTF-8"))?;
            values.insert(name.into(), value);
        }
        Self::parse(root, &values)
    }

    pub fn parse(root: &Path, input: &BTreeMap<String, String>) -> Result<Self, String> {
        let name = Self::input(input, "VMUX_HOST_NAME", "Vmux");
        let namespace = Self::input(input, "VMUX_HOST_NAMESPACE", "ai.vmux");
        Self::segment(&name)?;
        Self::namespace(&namespace)?;
        let custom = namespace != "ai.vmux";
        if custom != (name != "Vmux") {
            return Err(
                "A custom host requires a distinct VMUX_HOST_NAME and VMUX_HOST_NAMESPACE together"
                    .into(),
            );
        }
        if custom && name.eq_ignore_ascii_case("Vmux") {
            return Err(
                "A custom host name cannot alias Vmux on a case-insensitive filesystem".into(),
            );
        }
        let config = if custom {
            format!(".{}", namespace.replace('.', "-"))
        } else {
            ".vmux".into()
        };
        let mut product = Self {
            values: BTreeMap::new(),
        };
        for (key, value) in [
            ("VMUX_HOST_NAME", name.clone()),
            ("VMUX_HOST_NAMESPACE", namespace.clone()),
            (
                "VMUX_HOST_DATA_NAME",
                Self::input(
                    input,
                    "VMUX_HOST_DATA_NAME",
                    if custom { &namespace } else { &name },
                ),
            ),
            (
                "VMUX_HOST_CONFIG_NAME",
                Self::input(input, "VMUX_HOST_CONFIG_NAME", &config),
            ),
            ("VMUX_HOST_SERVICE_NAME", format!("{name} Service")),
            ("VMUX_HOST_SERVICE_BUNDLE", format!("{name} Service.app")),
            ("VMUX_HOST_BUNDLE_ID", format!("{namespace}.desktop")),
            ("VMUX_HOST_MOBILE_ID", format!("{namespace}.mobile")),
            (
                "VMUX_HOST_REMOTE_DATA_NAME",
                if custom {
                    format!("{namespace} Remote")
                } else {
                    "Vmux Remote".into()
                },
            ),
            ("VMUX_HOST_SERVICE_ID", format!("{namespace}.service")),
            (
                "VMUX_HOST_AGENT_PLIST",
                format!("{namespace}.service.plist"),
            ),
            (
                "VMUX_HOST_RUNTIME_STEM",
                if custom {
                    namespace.replace('.', "-")
                } else {
                    "vmux".into()
                },
            ),
            ("VMUX_HOST_CLI_ID", format!("{namespace}.cli")),
            (
                "VMUX_HOST_KEYCHAIN_SERVICE",
                format!("{namespace}.safe-storage"),
            ),
            ("VMUX_HOST_KEYCHAIN_LABEL", format!("{name} Safe Storage")),
        ] {
            Self::segment(&value)?;
            product.values.insert(key, value);
        }
        if custom
            && (product
                .value("VMUX_HOST_DATA_NAME")
                .eq_ignore_ascii_case("Vmux")
                || product
                    .value("VMUX_HOST_CONFIG_NAME")
                    .eq_ignore_ascii_case(".vmux"))
        {
            return Err(
                "A custom host cannot share the Vmux data or configuration directory".into(),
            );
        }
        for (key, default) in [
            ("VMUX_HOST_ICON", "packaging/macos/Vmux.icns"),
            ("VMUX_HOST_TRAY_ICON", "packaging/macos/tray.png"),
            (
                "VMUX_HOST_ENTITLEMENTS",
                "packaging/macos/Vmux.entitlements",
            ),
            (
                "VMUX_HOST_DEV_ENTITLEMENTS",
                "packaging/macos/VmuxDev.entitlements",
            ),
            (
                "VMUX_HOST_USAGE_DESCRIPTIONS",
                "packaging/macos/usage-descriptions.plist",
            ),
            (
                "VMUX_HOST_DMG_BACKGROUND",
                "packaging/macos/dmg-background.png",
            ),
            (
                "VMUX_HOST_MOBILE_ICON",
                "packaging/ios/Assets.xcassets/AppIcon.appiconset/AppIcon-1024.png",
            ),
            ("VMUX_HOST_MOBILE_PLIST", "packaging/ios/Info.plist"),
            (
                "VMUX_HOST_MOBILE_LAUNCH",
                "packaging/ios/LaunchScreen.storyboard",
            ),
            (
                "VMUX_HOST_MOBILE_PRIVACY",
                "packaging/ios/PrivacyInfo.xcprivacy",
            ),
            (
                "VMUX_HOST_MOBILE_ENTITLEMENTS",
                "packaging/ios/Vmux.entitlements",
            ),
        ] {
            let path = Self::input(input, key, default);
            let path = Self::asset(root, &path)?;
            product.values.insert(key, path);
        }
        for (key, default) in [
            ("VMUX_HOST_STABLE_ENDPOINT", "https://vmux.ai/updates.json"),
            (
                "VMUX_HOST_PREVIEW_ENDPOINT",
                "https://github.com/vmux-ai/vmux/releases/download/nightly/updates-preview.json",
            ),
        ] {
            let endpoint = Self::input(input, key, if custom { "" } else { default });
            if !Self::endpoint(&endpoint) {
                return Err(format!(
                    "{key} must be an HTTPS URL or empty to disable the channel"
                ));
            }
            product.values.insert(key, endpoint);
        }
        for key in [
            "VMUX_HOST_CAMERA",
            "VMUX_HOST_MICROPHONE",
            "VMUX_HOST_SCREEN",
        ] {
            let value = Self::input(input, key, if custom { "false" } else { "true" });
            if !matches!(value.as_str(), "true" | "false") {
                return Err(format!("{key} must be true or false"));
            }
            product.values.insert(key, value);
        }
        Ok(product)
    }

    pub fn value(&self, key: &str) -> &str {
        self.values.get(key).expect("accepted host metadata key")
    }

    pub fn values(&self) -> &BTreeMap<&'static str, String> {
        &self.values
    }

    pub fn emit(&self) {
        for (key, value) in &self.values {
            println!("cargo::rerun-if-env-changed={key}");
            println!("cargo::rustc-env={key}={value}");
            if matches!(
                *key,
                "VMUX_HOST_ICON"
                    | "VMUX_HOST_TRAY_ICON"
                    | "VMUX_HOST_ENTITLEMENTS"
                    | "VMUX_HOST_DEV_ENTITLEMENTS"
                    | "VMUX_HOST_USAGE_DESCRIPTIONS"
                    | "VMUX_HOST_DMG_BACKGROUND"
                    | "VMUX_HOST_MOBILE_ICON"
                    | "VMUX_HOST_MOBILE_PLIST"
                    | "VMUX_HOST_MOBILE_LAUNCH"
                    | "VMUX_HOST_MOBILE_PRIVACY"
                    | "VMUX_HOST_MOBILE_ENTITLEMENTS"
            ) {
                println!("cargo::rerun-if-changed={value}");
            }
        }
    }

    pub fn json(&self) -> String {
        let mut output = String::from("{");
        for (index, (key, value)) in self.values.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            Self::quoted(key, &mut output);
            output.push(':');
            Self::quoted(value, &mut output);
        }
        output.push('}');
        output
    }

    fn quoted(value: &str, output: &mut String) {
        output.push('"');
        for character in value.chars() {
            match character {
                '"' => output.push_str("\\\""),
                '\\' => output.push_str("\\\\"),
                other => output.push(other),
            }
        }
        output.push('"');
    }

    fn input(input: &BTreeMap<String, String>, key: &str, default: &str) -> String {
        match input.get(key) {
            Some(value) => value.clone(),
            None => default.into(),
        }
    }

    fn segment(value: &str) -> Result<(), String> {
        if value.is_empty()
            || value.len() > 255
            || matches!(value, "." | "..")
            || value.trim() != value
            || value
                .chars()
                .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':'))
        {
            return Err(format!("Invalid host name or path segment: {value:?}"));
        }
        Ok(())
    }

    fn namespace(value: &str) -> Result<(), String> {
        let mut count = 0;
        for segment in value.split('.') {
            let mut bytes = segment.bytes();
            if !bytes.next().is_some_and(|byte| byte.is_ascii_lowercase())
                || !bytes
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            {
                return Err(format!(
                    "Invalid reverse-domain VMUX_HOST_NAMESPACE: {value:?}"
                ));
            }
            count += 1;
        }
        if count < 2 {
            return Err("VMUX_HOST_NAMESPACE requires a reverse-domain identifier".into());
        }
        Ok(())
    }

    fn endpoint(value: &str) -> bool {
        if value.is_empty() {
            return true;
        }
        let Some(rest) = value.strip_prefix("https://") else {
            return false;
        };
        for byte in value.bytes() {
            if !byte.is_ascii_graphic()
                || matches!(byte, b'\\' | b'"' | b'<' | b'>' | b'`' | b'{' | b'}' | b'#')
            {
                return false;
            }
        }
        let mut bytes = value.bytes();
        while let Some(byte) = bytes.next() {
            if byte == b'%'
                && (!bytes.next().is_some_and(|byte| byte.is_ascii_hexdigit())
                    || !bytes.next().is_some_and(|byte| byte.is_ascii_hexdigit()))
            {
                return false;
            }
        }
        let authority = rest.split(['/', '?']).next().unwrap_or_default();
        let (host, port) = if let Some(ipv6) = authority.strip_prefix('[') {
            let Some((address, suffix)) = ipv6.split_once(']') else {
                return false;
            };
            if address.parse::<std::net::Ipv6Addr>().is_err() {
                return false;
            }
            let port = match suffix.strip_prefix(':') {
                Some(port) => Some(port),
                None if suffix.is_empty() => None,
                None => return false,
            };
            (address, port)
        } else {
            let (host, port) = match authority.split_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (authority, None),
            };
            if host.is_empty() || host.len() > 253 {
                return false;
            }
            if host
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'.')
            {
                if host.parse::<std::net::Ipv4Addr>().is_err() {
                    return false;
                }
            } else {
                for label in host.strip_suffix('.').unwrap_or(host).split('.') {
                    if label.is_empty()
                        || label.len() > 63
                        || !label.as_bytes()[0].is_ascii_alphanumeric()
                        || !label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                        || !label
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                    {
                        return false;
                    }
                }
            }
            (host, port)
        };
        if host.is_empty() {
            return false;
        }
        if let Some(port) = port
            && (port.is_empty()
                || !port.bytes().all(|byte| byte.is_ascii_digit())
                || !port.parse::<u16>().is_ok_and(|port| port != 0))
        {
            return false;
        }
        true
    }

    fn asset(root: &Path, value: &str) -> Result<String, String> {
        let path = PathBuf::from(value);
        let path = if path.is_absolute() {
            path
        } else {
            root.join(path)
        };
        let path = path
            .canonicalize()
            .map_err(|error| format!("Host asset {}: {error}", path.display()))?;
        if !path.is_file() {
            return Err(format!("Host asset is not a file: {}", path.display()));
        }
        let value = path.to_str().ok_or("Host asset path must be UTF-8")?;
        if value.chars().any(char::is_control) {
            return Err("Host asset path contains a control character".into());
        }
        Ok(value.into())
    }
}
