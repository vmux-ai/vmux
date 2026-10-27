use bevy::prelude::*;
use moonshine_save::prelude::Save;
use vmux_api::Id;
use vmux_ecs::persistence::PersistenceAppExt;

use crate::{AccountExternalId, ExternalHost, ExternalNamespace, ProviderKey};

pub(crate) struct ScopePlugin;

impl Plugin for ScopePlugin {
    fn build(&self, app: &mut App) {
        app.register_persisted::<ExternalScope>()
            .register_persisted::<Id<ExternalScope>>()
            .register_persisted::<ProviderKey>()
            .register_persisted::<ExternalHost>()
            .register_persisted::<ExternalNamespace>()
            .register_type::<AccountExternalId>()
            .register_type::<CanAccess>()
            .add_plugins(vmux_ecs::IdentityIndexPlugin::<ExternalScope>::default())
            .add_systems(PreUpdate, rebuild);
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_connector"]
pub struct ExternalScope;

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[type_path = "vmux_connector"]
pub struct CanAccess(Vec<Id<ExternalScope>>);

impl CanAccess {
    pub fn new(mut scopes: Vec<Id<ExternalScope>>) -> Self {
        scopes.sort();
        scopes.dedup();
        Self(scopes)
    }

    pub fn scopes(&self) -> &[Id<ExternalScope>] {
        &self.0
    }

    pub fn grants(&self, scope: &Id<ExternalScope>) -> bool {
        self.0.binary_search(scope).is_ok()
    }
}

fn rebuild(mut index: vmux_ecs::IdentityIndexWriter<ExternalScope>) {
    index.rebuild();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_scope_ids_are_explicit() {
        let mut app = App::new();
        app.add_plugins(ScopePlugin);
        let id = Id::<ExternalScope>::new("github.com/accounts/1");
        app.world_mut().spawn((ExternalScope, id.clone()));
        app.world_mut().spawn((ExternalScope, id.clone()));

        app.update();

        assert_eq!(
            app.world()
                .resource::<vmux_ecs::IdentityIndex<ExternalScope>>()
                .entity(&id),
            Err(vmux_ecs::IdentityLookupError::Duplicate(id))
        );
    }
}
