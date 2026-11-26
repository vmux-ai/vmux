use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;

use crate::{
    Active, Busy, Collapsed, CollectionView, ContentDigest, Cwd, Description, Disabled, Failure,
    FilePath, Icon, Loading, Order, Pin, Progress, Ready, Retired, TimestampPlugin, Uuid,
    WorkspaceKind,
};

pub struct PrimitivesPlugin;

impl Plugin for PrimitivesPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<TimestampPlugin>() {
            app.add_plugins(TimestampPlugin);
        }
        app.register_type::<Cwd>()
            .register_type::<FilePath>()
            .register_type::<ContentDigest>()
            .register_type::<Description>()
            .register_type::<Icon>()
            .register_type::<CollectionView>()
            .register_type::<Order>()
            .register_type::<Active>()
            .register_type::<Pin>()
            .register_type::<Collapsed>()
            .register_type::<Disabled>()
            .register_type::<Loading>()
            .register_type::<Ready>()
            .register_type::<Busy>()
            .register_type::<Failure>()
            .register_type::<Progress>()
            .register_type::<Retired>()
            .register_type::<Uuid>()
            .register_type::<WorkspaceKind>()
            .register_type::<Children>()
            .register_type::<ChildOf>();
        #[cfg(feature = "contracts")]
        app.register_type::<crate::PageMetadata>()
            .register_type::<vmux_api::PageIcon>()
            .register_type::<vmux_api::BuiltinIcon>();
        #[cfg(all(host, feature = "headless-host"))]
        app.configure_sets(
            bevy_app::Last,
            vmux_store::AutoSaveSet::Flush.before(crate::service::ShutdownSet),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivateRequest, LastActivatedAt};

    #[test]
    fn activation_propagates_through_ancestors() {
        let mut app = App::new();
        app.add_plugins(PrimitivesPlugin);

        let root = app.world_mut().spawn(LastActivatedAt(1)).id();
        let child = app
            .world_mut()
            .spawn((LastActivatedAt(1), ChildOf(root)))
            .id();
        let leaf = app
            .world_mut()
            .spawn((LastActivatedAt(1), ChildOf(child)))
            .id();

        app.world_mut().trigger(ActivateRequest { entity: leaf });
        app.update();

        assert!(app.world().get::<LastActivatedAt>(root).unwrap().0 > 1);
        assert!(app.world().get::<LastActivatedAt>(child).unwrap().0 > 1);
        assert!(app.world().get::<LastActivatedAt>(leaf).unwrap().0 > 1);
    }

    #[test]
    fn repeated_activation_orders_siblings_even_when_the_clock_has_not_advanced() {
        let mut app = App::new();
        app.add_plugins(PrimitivesPlugin);
        let restored = LastActivatedAt(LastActivatedAt::now().0 + 5_000);
        let root = app.world_mut().spawn(restored).id();
        let first = app.world_mut().spawn((restored, ChildOf(root))).id();
        let second = app.world_mut().spawn((restored, ChildOf(root))).id();
        app.world_mut().trigger(ActivateRequest { entity: first });
        app.world_mut().trigger(ActivateRequest { entity: second });
        app.update();
        let first_at = app.world().get::<LastActivatedAt>(first).unwrap().0;
        let second_at = app.world().get::<LastActivatedAt>(second).unwrap().0;
        assert!(first_at > restored.0);
        assert!(second_at > first_at);
        assert_eq!(
            app.world().get::<LastActivatedAt>(root).unwrap().0,
            second_at
        );
    }
}
