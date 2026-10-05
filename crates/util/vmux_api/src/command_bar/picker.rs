#[vmux_api::contract(Default, Eq, Hash)]
#[serde(transparent)]
pub struct CommandBarPicker(pub String);

impl CommandBarPicker {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn is(&self, id: &str) -> bool {
        self.0 == id
    }
}

#[vmux_api::contract(Eq)]
pub enum CommandBarPick {
    Picker(CommandBarPicker),
    Typed {
        picker: CommandBarPicker,
        value: String,
    },
}

#[vmux_api::contract(Eq)]
pub struct CommandBarPickRow {
    pub label: String,
    pub pick: CommandBarPick,
}
