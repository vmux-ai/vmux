use ron::value::RawValue;
use serde::Deserialize;
use serde::de::DeserializeOwned;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct CliManifest {
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub commands: Vec<CliCommandManifest>,
    #[serde(default)]
    providers: Option<Box<RawValue>>,
}

impl CliManifest {
    pub fn providers<T: DeserializeOwned>(&self) -> Result<Option<T>, String> {
        self.providers
            .as_deref()
            .map(|value| ron::from_str(value.get_ron()))
            .transpose()
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct CliCommandManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub about: Option<String>,
    #[serde(default)]
    pub arguments: Vec<CliArgumentManifest>,
    #[serde(default)]
    pub commands: Vec<CliCommandManifest>,
    #[serde(default)]
    pub subcommand_required: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct CliArgumentManifest {
    pub id: String,
    #[serde(default)]
    pub long: Option<String>,
    #[serde(default)]
    pub short: Option<char>,
    #[serde(default)]
    pub value_name: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default)]
    pub flag: bool,
    #[serde(default)]
    pub index: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_command_manifest() {
        let manifest = ron::from_str::<CliManifest>(
            r#"(
                default: Some("app.open"),
                commands: [(
                    id: "tool",
                    name: "tools",
                    commands: [(
                        id: "tool.import.npm",
                        name: "npm",
                        arguments: [(
                            id: "path",
                            index: Some(1),
                        )],
                    )],
                )],
            )"#,
        )
        .unwrap();

        assert_eq!(manifest.default.as_deref(), Some("app.open"));
        assert_eq!(manifest.commands[0].commands[0].id, "tool.import.npm");
    }
}
