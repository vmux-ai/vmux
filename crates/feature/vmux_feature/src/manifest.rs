use std::any::TypeId;
use std::collections::HashMap;
#[cfg(feature = "ecs")]
use std::marker::PhantomData;
use std::sync::{Mutex, OnceLock};

#[cfg(feature = "ecs")]
use bevy_app::{App, Plugin};
#[cfg(feature = "ecs")]
use bevy_ecs::prelude::*;
use ron::value::RawValue;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use vmux_api::JsonSchema;
use vmux_api::address::{AddressIntent, AddressOption};

use crate::cli::CliManifest;
#[cfg(feature = "ecs")]
pub use crate::composition::{CompositionPlugin, Profile};
#[cfg(feature = "hosting")]
pub use crate::hosting;
pub use crate::metadata::{Dependency, Lifecycle, Metadata};
#[cfg(feature = "ecs")]
pub use crate::registry::Registry;
#[cfg(feature = "ecs")]
use crate::registry::RegistryPlugin;

#[derive(Clone, Deserialize)]
#[cfg_attr(feature = "ecs", derive(Component))]
#[serde(deny_unknown_fields)]
pub struct FeatureManifest {
    #[serde(default)]
    pub metadata: Option<Metadata>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub root: bool,
    #[serde(default)]
    pub flags: Vec<crate::flag::Declaration>,
    #[serde(default)]
    pub auth_providers: Vec<AuthProvider>,
    #[serde(default)]
    pub pages: Vec<Page>,
    #[serde(default)]
    pub commands: Vec<Command>,
    #[serde(default)]
    pub command_bar: Vec<CommandBarEntry>,
    #[serde(default)]
    pub search_engines: Vec<SearchEngineManifest>,
    #[serde(default)]
    pub tool_providers: Vec<ToolProviderManifest>,
    #[serde(default)]
    pub tools: Vec<Tool>,
    #[serde(default)]
    pub cli: Option<CliManifest>,
    #[serde(default)]
    pub mcp_servers: Vec<McpServer>,
    #[serde(default)]
    settings: Option<Box<RawValue>>,
    #[serde(default)]
    policy: Option<Box<RawValue>>,
    #[serde(default)]
    policies: Option<Box<RawValue>>,
    #[serde(default, deserialize_with = "FeatureManifest::section")]
    session: Option<Box<RawValue>>,
}

impl FeatureManifest {
    pub fn of<M: FeatureManifestSource>() -> &'static Self {
        static PARSED: OnceLock<Mutex<HashMap<TypeId, &'static OnceLock<FeatureManifest>>>> =
            OnceLock::new();
        let cell = {
            let mut parsed = PARSED
                .get_or_init(|| Mutex::new(HashMap::new()))
                .lock()
                .expect("feature manifest cache");
            let id = TypeId::of::<M>();
            if let Some(cell) = parsed.get(&id) {
                *cell
            } else {
                let cell: &'static OnceLock<FeatureManifest> = Box::leak(Box::new(OnceLock::new()));
                parsed.insert(id, cell);
                cell
            }
        };
        cell.get_or_init(|| Self::parse(M::SOURCE))
    }

    pub fn parse(source: &str) -> Self {
        Self::try_parse(source)
            .unwrap_or_else(|error| panic!("invalid embedded feature manifest: {error}"))
    }

    pub fn try_parse(source: &str) -> Result<Self, String> {
        let manifest: Self = ron::from_str(source).map_err(|error| error.to_string())?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn address_options<'a>(
        manifests: impl IntoIterator<Item = &'a Self>,
    ) -> Vec<AddressOption> {
        let mut entries = Vec::new();
        for manifest in manifests {
            for page in &manifest.pages {
                let Some(order) = page.bookmark else {
                    continue;
                };
                let icon = page.builtin_icon().expect("validated page icon");
                entries.push((
                    order,
                    AddressOption {
                        label: page.title.clone(),
                        label_key: page.title_message_id.clone(),
                        icon,
                        intent: Some(AddressIntent::Navigate {
                            url: page.url.clone(),
                        }),
                        ..Default::default()
                    },
                ));
            }
        }
        entries.sort_by_key(|(order, _)| *order);
        let mut options = Vec::new();
        for (_, option) in entries {
            options.push(option);
        }
        options
    }

    pub fn policy<T: DeserializeOwned>(&self) -> Result<Option<T>, String> {
        Self::decode(&self.policy)
    }

    pub fn policies<T: DeserializeOwned>(&self) -> Result<Option<T>, String> {
        Self::decode(&self.policies)
    }

    pub fn settings<T: DeserializeOwned>(&self) -> Result<Option<T>, String> {
        Self::decode(&self.settings)
    }

    pub fn session<T: DeserializeOwned>(&self) -> Result<Option<T>, String> {
        Self::decode(&self.session)
    }

    fn decode<T: DeserializeOwned>(value: &Option<Box<RawValue>>) -> Result<Option<T>, String> {
        value
            .as_deref()
            .map(|value| ron::from_str(value.get_ron()))
            .transpose()
            .map_err(|error| error.to_string())
    }

    fn section<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Box<RawValue>>, D::Error> {
        Box::<RawValue>::deserialize(deserializer).map(Some)
    }

    fn validate(&self) -> Result<(), String> {
        if let Some(metadata) = &self.metadata {
            metadata.validate()?;
        }
        for (index, flag) in self.flags.iter().enumerate() {
            flag.validate()?;
            if self.flags[..index]
                .iter()
                .any(|previous| previous.key == flag.key)
            {
                return Err(format!("duplicate flag declaration {}", flag.key));
            }
        }
        for (index, provider) in self.auth_providers.iter().enumerate() {
            if provider.id.trim().is_empty() || provider.label.trim().is_empty() {
                return Err("auth provider requires an id and label".into());
            }
            if self.auth_providers[..index]
                .iter()
                .any(|previous| previous.id == provider.id)
            {
                return Err(format!("duplicate auth provider {}", provider.id));
            }
        }
        for (index, page) in self.pages.iter().enumerate() {
            page.validate()?;
            if self.pages[..index]
                .iter()
                .any(|first| first.url == page.url)
            {
                return Err(format!("duplicate page route {}", page.url));
            }
        }
        for command in &self.commands {
            if let Some(mcp) = &command.mcp
                && let Some(input_schema) = &mcp.input_schema
            {
                input_schema
                    .validate()
                    .map_err(|error| format!("invalid command input schema: {error}"))?;
            }
        }
        for tool in &self.tools {
            tool.input_schema
                .validate()
                .map_err(|error| format!("invalid tool input schema: {error}"))?;
        }
        Ok(())
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthProvider {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Page {
    #[serde(default)]
    pub name: String,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub manifest_url: String,
    #[serde(default)]
    pub manifest_title: String,
    #[serde(default)]
    pub asset_host: String,
    #[serde(default)]
    pub owns_subtree: bool,
    #[serde(default)]
    pub title_message_id: String,
    #[serde(default)]
    pub replaces_command: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub command_bar: bool,
    #[serde(default)]
    pub startup: bool,
    #[serde(default)]
    pub reports_title: bool,
    #[serde(default)]
    pub bookmark: Option<i32>,
    #[serde(default)]
    pub manifest: bool,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub placement: Option<PagePlacement>,
}

impl Page {
    pub fn asset_host(&self) -> Result<&str, String> {
        if !self.asset_host.is_empty() {
            return Ok(&self.asset_host);
        }
        let url = if self.manifest_url.is_empty() {
            &self.url
        } else {
            &self.manifest_url
        };
        let Some((scheme, rest)) = url.split_once("://") else {
            return Err(format!("page {} requires an asset host", self.url));
        };
        let host = if scheme == "vmux" {
            rest.split('/').next().unwrap_or_default()
        } else {
            scheme
        };
        if host.is_empty() {
            return Err(format!("page {} requires an asset host", self.url));
        }
        Ok(host)
    }

    pub fn builtin_icon(&self) -> Result<Option<vmux_api::BuiltinIcon>, String> {
        if self.icon.is_empty() {
            return Ok(None);
        }
        let icon = serde_json::from_value(serde_json::Value::String(self.icon.clone()))
            .map_err(|error| format!("invalid page icon for {}: {error}", self.url))?;
        Ok(Some(icon))
    }

    fn validate(&self) -> Result<(), String> {
        if self.url.is_empty() {
            return Err("page requires a route".into());
        }
        for url in [&self.url, &self.manifest_url] {
            if url.is_empty() {
                continue;
            }
            let Some((scheme, rest)) = url.split_once("://") else {
                return Err(format!("invalid page route {url}"));
            };
            if scheme.is_empty()
                || !scheme.starts_with(|character: char| character.is_ascii_alphabetic())
                || !scheme
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"+-.".contains(&byte))
                || url
                    .chars()
                    .any(|character| character.is_whitespace() || character.is_control())
                || scheme == "vmux" && rest.split('/').next().unwrap_or_default().is_empty()
            {
                return Err(format!("invalid page route {url}"));
            }
        }
        let asset_host = self.asset_host()?;
        if !asset_host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        {
            return Err(format!("invalid asset host {asset_host} for {}", self.url));
        }
        self.builtin_icon()?;
        if let Some(placement) = &self.placement {
            placement.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PagePlacement {
    pub group: String,
    #[serde(default)]
    pub auxiliary: bool,
    #[serde(default)]
    pub refresh: bool,
    #[serde(default)]
    pub reuse: PageReuse,
    #[serde(default)]
    pub split: Option<PageSplitPreference>,
}

impl PagePlacement {
    fn validate(&self) -> Result<(), String> {
        if self.group.trim().is_empty() {
            return Err("page placement requires a group".into());
        }
        if let Some(split) = &self.split {
            split.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Default, Deserialize, PartialEq, Eq)]
pub enum PageReuse {
    #[default]
    Exact,
    Group,
    Route,
    Fragmentless,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PageSplitPreference {
    pub anchor_group: String,
    pub allowed_groups: Vec<String>,
    pub axis: PageSplitAxis,
}

impl PageSplitPreference {
    fn validate(&self) -> Result<(), String> {
        if self.anchor_group.trim().is_empty() || self.allowed_groups.is_empty() {
            return Err("page split requires an anchor and allowed groups".into());
        }
        for (index, group) in self.allowed_groups.iter().enumerate() {
            if group.trim().is_empty() || self.allowed_groups[..index].contains(group) {
                return Err("page split groups must be nonempty and unique".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
pub enum PageSplitAxis {
    Row,
    Column,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub enum ShortcutKind {
    Direct(String),
    Chord(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Shortcut {
    pub shortcut: ShortcutKind,
    pub when: Option<String>,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CommandMcp {
    pub description: String,
    #[serde(default)]
    pub input_schema: Option<JsonSchema>,
    #[serde(default)]
    pub allow_agent: bool,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub id: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub label: String,
    pub group: String,
    #[serde(default)]
    pub accelerator: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default = "Command::native_menu")]
    pub native_menu: bool,
    #[serde(default)]
    pub shortcut_label: Option<String>,
    #[serde(default)]
    pub shortcuts: Vec<Shortcut>,
    #[serde(default)]
    pub toolbar: Option<CommandToolbar>,
    #[serde(default)]
    pub mcp: Option<CommandMcp>,
}

impl Command {
    fn native_menu() -> bool {
        true
    }
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CommandToolbar {
    pub icon: vmux_api::BuiltinIcon,
    #[serde(default)]
    pub rank: i32,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandBarEntry {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub title_message_id: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub subtitle_message_id: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub leading: String,
    #[serde(default)]
    pub trailing: String,
    #[serde(default)]
    pub badge: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub query_prefix: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub rank: i32,
    #[serde(default = "CommandBarEntry::close")]
    pub close: bool,
}

impl CommandBarEntry {
    fn close() -> bool {
        true
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchEngineManifest {
    pub id: String,
    pub name: String,
    pub hosts: Vec<String>,
    pub query_url: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolProviderManifest {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub title_message_id: String,
    #[serde(default)]
    pub route_title: String,
    #[serde(default)]
    pub route_title_message_id: String,
    #[serde(default)]
    pub short_label: String,
    #[serde(default)]
    pub route: String,
    #[serde(default)]
    pub rank: i32,
    #[serde(default)]
    pub thumbnails: bool,
    #[serde(default)]
    pub apply: bool,
    #[serde(default)]
    pub brewfile: bool,
}

#[derive(Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolAvailability {
    #[default]
    Always,
    OutsideAcpSession,
    WithoutAcpTerminals,
}

impl ToolAvailability {
    pub fn allows(self, acp_session: bool, acp_terminals: bool) -> bool {
        match self {
            Self::Always => true,
            Self::OutsideAcpSession => !acp_session,
            Self::WithoutAcpTerminals => !acp_terminals,
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub description: String,
    pub input_schema: JsonSchema,
    #[serde(default)]
    pub availability: ToolAvailability,
    #[serde(default)]
    pub shell_aware: bool,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpServer {
    #[serde(default)]
    pub setup_url: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_id_env: Option<String>,
    #[serde(default)]
    pub account_endpoint: Option<String>,
    pub id: String,
    pub name: String,
    pub url: String,
    pub scopes: Vec<String>,
}

pub trait FeatureManifestSource: Send + Sync + 'static {
    const SOURCE: &'static str;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "ecs", derive(Component))]
pub struct FeatureId(TypeId);

impl FeatureId {
    pub fn of<M: FeatureManifestSource>() -> Self {
        Self(TypeId::of::<M>())
    }
}

#[cfg(feature = "ecs")]
#[derive(Component)]
pub struct FeatureManifestOwner<M>(PhantomData<fn() -> M>);

#[cfg(feature = "ecs")]
pub struct FeaturePlugin<M> {
    manifest: Option<FeatureManifest>,
    marker: PhantomData<fn() -> M>,
}

#[cfg(feature = "ecs")]
impl<M> FeaturePlugin<M> {
    pub fn from_manifest(manifest: FeatureManifest) -> Self {
        Self {
            manifest: Some(manifest),
            marker: PhantomData,
        }
    }
}

#[cfg(feature = "ecs")]
impl<M> Default for FeaturePlugin<M> {
    fn default() -> Self {
        Self {
            manifest: None,
            marker: PhantomData,
        }
    }
}

#[cfg(feature = "ecs")]
impl<M: FeatureManifestSource> Plugin for FeaturePlugin<M> {
    fn build(&self, app: &mut App) {
        assert!(
            !app.world().contains_resource::<Registry>(),
            "feature registry is frozen"
        );
        if !app.is_plugin_added::<RegistryPlugin>() {
            app.add_plugins(RegistryPlugin);
        }
        let mut sources = app
            .world_mut()
            .query_filtered::<Entity, With<FeatureManifestOwner<M>>>();
        if sources.iter(app.world()).next().is_some() {
            assert!(
                self.manifest.is_none(),
                "external feature manifest is already registered"
            );
            return;
        }
        let manifest = match &self.manifest {
            Some(manifest) => manifest.clone(),
            None => FeatureManifest::of::<M>().clone(),
        };
        manifest.validate().expect("valid feature manifest");
        if manifest.metadata.is_some() && !app.is_plugin_added::<CompositionPlugin>() {
            app.add_plugins(CompositionPlugin::default());
        }
        app.world_mut().spawn((
            Name::new(std::any::type_name::<M>()),
            FeatureId::of::<M>(),
            FeatureManifestOwner::<M>(PhantomData),
            manifest,
        ));
    }

    fn is_unique(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"(
        pages: [(
            url: "vmux://example/",
            title: "Example",
            permissions: ["PageReady"],
        )],
        commands: [(id: "open", label: "Open", group: "File")],
        tools: [(
            name: "open_file",
            description: "Open a file",
            input_schema: (type: Object),
        )],
        cli: Some((
            default: Some("app.open"),
            providers: Some({"example": (enabled: true)}),
        )),
        policy: Some((enabled: true)),
        settings: Some((enabled: true)),
        session: (enabled: true),
    )"#;

    #[cfg(feature = "ecs")]
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Policy {
        enabled: bool,
    }

    #[cfg(feature = "ecs")]
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Provider {
        enabled: bool,
    }

    struct Feature;
    struct OtherFeature;
    struct InvalidFeature;

    impl FeatureManifestSource for Feature {
        const SOURCE: &'static str = SOURCE;
    }

    impl FeatureManifestSource for OtherFeature {
        const SOURCE: &'static str = SOURCE;
    }

    impl FeatureManifestSource for InvalidFeature {
        const SOURCE: &'static str = "invalid manifest";
    }

    #[test]
    fn portable_parser_refuses_invalid_page_registration_metadata() {
        for declaration in [
            r#"(url: "", title: "Invalid")"#,
            r#"(url: "relative/path", title: "Invalid", asset_host: "files")"#,
            r#"(url: "vmux:///missing-host", title: "Invalid")"#,
            r#"(url: "vmux://valid/", title: "Invalid", manifest_url: "relative/path")"#,
            r#"(url: "vmux://valid/", title: "Invalid", asset_host: "bad/host")"#,
            r#"(url: "vmux://valid/", title: "Invalid", icon: "UnknownBuiltinIcon")"#,
            r#"(url: "vmux://valid/", title: "Invalid", placement: Some((group: "")))"#,
            r#"(url: "vmux://valid/", title: "Invalid", placement: Some((group: "files", split: Some((anchor_group: "files", allowed_groups: [], axis: Row)))))"#,
            r#"(url: "vmux://valid/", title: "Invalid", placement: Some((group: "files", split: Some((anchor_group: "files", allowed_groups: ["files", "files"], axis: Row)))))"#,
        ] {
            let source = format!("(pages: [{declaration}])");
            assert!(FeatureManifest::try_parse(&source).is_err(), "{source}");
        }
        assert!(
            FeatureManifest::try_parse(
                r#"(pages: [
                (name: "first", url: "vmux://duplicate/", title: "First"),
                (name: "second", url: "vmux://duplicate/", title: "Second"),
            ])"#,
            )
            .is_err()
        );
        FeatureManifest::try_parse(
            r#"(pages: [
                (url: "file://", title: "Files", asset_host: "files", icon: "Code"),
                (url: "vmux://logical/", title: "Logical", manifest_url: "vmux://document/"),
            ])"#,
        )
        .unwrap();
    }

    #[test]
    fn typed_declarations_refuse_incompatible_defaults_and_unsafe_offline_lifetimes() {
        let valid = r#"(key: "remote.live", type: Boolean, class: Release,
            default: Boolean(true), lifecycle: Temporary, owner: "vmux",
            expiry: Some("2027-01-01T00:00:00Z"), offline: RetainOff)"#;
        FeatureManifest::try_parse(&format!("(flags: [{valid}])")).unwrap();
        for invalid in [
            valid.replace("Boolean(true)", "String(\"true\")"),
            valid.replace("owner: \"vmux\"", "owner: \"\""),
            valid.replace("Some(\"2027-01-01T00:00:00Z\")", "None"),
            valid.replace("00:00:00Z", "00:00:00+01:00"),
            valid.replace("remote.live", "Remote.live"),
            valid
                .replace("type: Boolean", "type: Integer")
                .replace("Boolean(true)", "Integer(1)"),
        ] {
            assert!(FeatureManifest::try_parse(&format!("(flags: [{invalid}])")).is_err());
        }
        assert!(FeatureManifest::try_parse(&format!("(flags: [{valid}, {valid}])")).is_err());
        assert!(FeatureManifest::try_parse("(flagz: [])").is_err());
        FeatureManifest::try_parse(&format!(
            "(flags: [{}])",
            valid.replace("2027-01-01", "2020-01-01")
        ))
        .unwrap();
    }

    #[test]
    fn cached_declarations_keep_owner_identity_and_isolate_invalid_sources() {
        let first = FeatureManifest::of::<Feature>();
        assert!(std::panic::catch_unwind(FeatureManifest::of::<InvalidFeature>).is_err());
        let repeated = FeatureManifest::of::<Feature>();
        assert!(std::ptr::eq(first, repeated));
        assert!(!std::ptr::eq(first, FeatureManifest::of::<OtherFeature>()));
        assert_eq!(first.pages[0].permissions, ["PageReady"]);
    }

    #[cfg(feature = "ecs")]
    #[test]
    fn one_feature_entity_owns_all_parsed_sections() {
        let mut app = App::new();
        app.add_plugins(FeaturePlugin::<Feature>::default());
        app.update();

        let manifests = app
            .world_mut()
            .query::<&FeatureManifest>()
            .iter(app.world())
            .collect::<Vec<_>>();
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0].pages[0].url, "vmux://example/");
        assert_eq!(manifests[0].pages[0].permissions, ["PageReady"]);
        assert_eq!(manifests[0].commands[0].id, "open");
        assert_eq!(manifests[0].tools[0].name, "open_file");
        assert_eq!(
            manifests[0].cli.as_ref().unwrap().default.as_deref(),
            Some("app.open")
        );
        let providers = manifests[0]
            .cli
            .as_ref()
            .unwrap()
            .providers::<std::collections::BTreeMap<String, Provider>>()
            .unwrap()
            .unwrap();
        let policy = manifests[0].policy::<Policy>().unwrap();
        let settings = manifests[0].settings::<Policy>().unwrap();
        let session = manifests[0].session::<Policy>().unwrap();
        assert_eq!(providers["example"], Provider { enabled: true });
        assert_eq!(policy, Some(Policy { enabled: true }));
        assert_eq!(settings, Some(Policy { enabled: true }));
        assert_eq!(session, Some(Policy { enabled: true }));
    }

    #[cfg(feature = "ecs")]
    #[test]
    fn identical_sources_for_distinct_features_create_distinct_entities() {
        let mut app = App::new();
        app.add_plugins((
            FeaturePlugin::<Feature>::default(),
            FeaturePlugin::<OtherFeature>::default(),
        ));
        app.update();

        let manifests = app
            .world_mut()
            .query::<&FeatureManifest>()
            .iter(app.world())
            .count();
        assert_eq!(manifests, 2);
    }

    #[cfg(feature = "ecs")]
    #[test]
    fn repeated_feature_registration_reuses_the_source_entity() {
        let mut app = App::new();
        app.add_plugins((
            FeaturePlugin::<Feature>::default(),
            FeaturePlugin::<Feature>::default(),
        ));

        let manifests = app
            .world_mut()
            .query_filtered::<&FeatureManifest, With<FeatureManifestOwner<Feature>>>()
            .iter(app.world())
            .count();
        assert_eq!(manifests, 1);
    }

    #[test]
    fn declaration_conversion_orders_bookmarks_and_excludes_unlisted_pages() {
        let manifest = FeatureManifest::parse(
            r#"(pages: [
                (url: "git://", title: "Git", icon: "GitBranch", bookmark: Some(20)),
                (url: "vmux://start/", title: "Start", title_message_id: "start-title", icon: "Sparkles", bookmark: Some(10)),
                (url: "vmux://hidden/", title: "Hidden", icon: "Code"),
            ])"#,
        );
        let options = FeatureManifest::address_options(std::iter::once(&manifest));
        assert_eq!(
            options
                .iter()
                .map(|option| option.label.as_str())
                .collect::<Vec<_>>(),
            ["Start", "Git"]
        );
        assert_eq!(options[0].label_key, "start-title");
        assert_eq!(options[1].icon, Some(vmux_api::BuiltinIcon::GitBranch));
    }
}
