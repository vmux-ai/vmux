use bevy::prelude::*;

use crate::{FileViewModeRequest, GlobalSearchRequest};

pub struct ContractPlugin;

impl Plugin for ContractPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<FileViewModeRequest>()
            .add_message::<GlobalSearchRequest>();
    }

    fn is_unique(&self) -> bool {
        false
    }
}
