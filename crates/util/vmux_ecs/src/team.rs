use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;

pub use vmux_api::avatar::AvatarSpec;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Profile;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Avatar(pub AvatarSpec);

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct Icon(pub Option<String>);

#[derive(Bundle)]
pub struct ProfileBundle {
    profile: Profile,
    name: Name,
    avatar: Avatar,
    icon: Icon,
}

impl ProfileBundle {
    pub fn with_icon(mut self, icon: Option<String>) -> Self {
        self.icon = Icon(icon);
        self
    }

    pub fn owned_by(self, owner: Entity) -> impl Bundle {
        (self, ChildOf(owner))
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn avatar(&self) -> &AvatarSpec {
        &self.avatar.0
    }

    pub fn icon(&self) -> Option<&str> {
        self.icon.0.as_deref()
    }
}

pub struct ProfileView<'a> {
    entity: Entity,
    name: Ref<'a, Name>,
    avatar: Ref<'a, Avatar>,
    icon: Ref<'a, Icon>,
}

impl ProfileView<'_> {
    pub const fn entity(&self) -> Entity {
        self.entity
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn avatar(&self) -> &AvatarSpec {
        &self.avatar.0
    }

    pub fn icon(&self) -> Option<&str> {
        self.icon.0.as_deref()
    }

    pub fn is_changed(&self) -> bool {
        self.name.is_changed() || self.avatar.is_changed() || self.icon.is_changed()
    }
}

#[derive(SystemParam)]
pub struct Profiles<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    profiles: ProfileComponents<'w, 's>,
}

type ProfileComponents<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Ref<'static, Name>,
        Ref<'static, Avatar>,
        Ref<'static, Icon>,
    ),
    With<Profile>,
>;

impl Profiles<'_, '_> {
    pub fn get(&self, owner: Entity) -> Option<ProfileView<'_>> {
        if let Ok((entity, name, avatar, icon)) = self.profiles.get(owner) {
            return Some(ProfileView {
                entity,
                name,
                avatar,
                icon,
            });
        }
        let children = self.children.get(owner).ok()?;
        for child in children.iter() {
            let Ok((entity, name, avatar, icon)) = self.profiles.get(child) else {
                continue;
            };
            return Some(ProfileView {
                entity,
                name,
                avatar,
                icon,
            });
        }
        None
    }

    pub fn remove(&self, commands: &mut Commands, owner: Entity) {
        let Some(profile) = self.get(owner) else {
            return;
        };
        if profile.entity() == owner {
            commands.entity(owner).remove::<(Profile, Avatar, Icon)>();
        } else {
            commands.entity(profile.entity()).despawn();
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct User;

#[derive(Component, Clone, Copy, Debug)]
pub struct Tester;

#[derive(Component, Clone, Debug)]
pub struct Agent {
    pub sid: String,
}

impl Profile {
    pub fn user() -> ProfileBundle {
        ProfileBundle {
            profile: Self,
            name: Name::new("You"),
            avatar: Avatar(AvatarSpec::for_user()),
            icon: Icon::default(),
        }
    }

    pub fn user_named(name: String) -> ProfileBundle {
        let avatar = AvatarSpec::for_user_named(&name);
        ProfileBundle {
            profile: Self,
            name: Name::new(name),
            avatar: Avatar(avatar),
            icon: Icon::default(),
        }
    }

    pub fn registry(name: &str, seed: &str) -> ProfileBundle {
        ProfileBundle {
            profile: Self,
            name: Name::new(name.to_string()),
            avatar: Avatar(AvatarSpec::for_registry(name, seed)),
            icon: Icon::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_app::App;
    use bevy_ecs::system::RunSystemOnce;

    #[test]
    fn user_profile_has_default_name() {
        assert_eq!(Profile::user().name(), "You");
    }

    #[test]
    fn registry_avatar_derives_initials_and_stable_color() {
        let a = AvatarSpec::for_registry("Mistral Vibe", "mistral-vibe");
        assert_eq!(a.initials, "MV");
        assert_eq!(a.color, "#ff7000");
        assert_eq!(a.color, AvatarSpec::for_registry("X", "mistral-vibe").color);
        assert!(a.color.starts_with('#') && a.color.len() == 7);
    }

    #[test]
    fn registry_color_differs_by_seed() {
        assert_ne!(
            AvatarSpec::for_registry("A", "claude-acp").color,
            AvatarSpec::for_registry("A", "mistral-vibe").color
        );
    }

    #[test]
    fn registry_profile_uses_name() {
        assert_eq!(
            Profile::registry("Claude Agent", "claude-acp").name(),
            "Claude Agent"
        );
    }

    #[test]
    fn owned_profile_resolves_from_owner() {
        let mut app = App::new();
        let owner = app.world_mut().spawn_empty().id();
        app.world_mut()
            .spawn(Profile::registry("Claude", "claude").owned_by(owner));

        let resolved = app
            .world_mut()
            .run_system_once(move |profiles: Profiles| {
                profiles.get(owner).map(|profile| {
                    (
                        profile.name().to_string(),
                        profile.avatar().initials.clone(),
                    )
                })
            })
            .unwrap()
            .unwrap();

        assert_eq!(resolved, ("Claude".to_string(), "C".to_string()));
    }
}
