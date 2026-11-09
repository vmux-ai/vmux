use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::address::{
    AddressEffect, AddressIntent, AddressOption, PageAddress, PageAddressPublish,
    PageAddressUiState, PageAddressUiStatePatch,
};

use crate::manifest::FeatureManifest;
use crate::page::{AddressContribution, AddressIntentRequest, HostsPage, PageManifest};
use crate::{PageMetadata, PageOpenRequest, PageOpenTarget, UiStatePlugin, UiStateWrite};

pub struct AddressPlugin;

impl Plugin for AddressPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiStatePlugin::<PageAddressUiState>::with_post_update_flush())
            .add_message::<PageOpenRequest>()
            .add_plugins(UiEventPlugin::<(PageAddressPublish,)>::default())
            .add_observer(publish)
            .add_observer(activate)
            .add_systems(PostUpdate, project.in_set(AddressProjectionSet));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AddressProjectionSet;

#[derive(Component)]
struct RouteAddress(PageAddress);

type Sources<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PageMetadata,
        Option<&'static AddressContribution>,
        Option<&'static RouteAddress>,
    ),
    With<HostsPage>,
>;

fn project(routes: Query<&PageManifest>, pages: Sources, mut commands: Commands) {
    for (owner, metadata, current, automatic) in &pages {
        if let Some(current) = current
            && current.0.url != metadata.url
            && let Some(address) = current.0.for_navigation(&metadata.url)
        {
            commands
                .entity(owner)
                .remove::<RouteAddress>()
                .insert(AddressContribution(address));
            continue;
        }
        if let Some(current) = current
            && automatic.is_none()
            && current.0.url != metadata.url
            && current.0.document_url() == metadata.url.split('#').next().unwrap_or_default()
        {
            let mut address = current.0.clone();
            address.url.clone_from(&metadata.url);
            address.revision = address.revision.saturating_add(1);
            commands
                .entity(owner)
                .insert(AddressContribution(address.clone()));
            commands.trigger(UiStateWrite::<PageAddressUiState>::from_event(
                owner,
                &PageAddressUiStatePatch {
                    address: Some(address),
                    ..Default::default()
                },
            ));
            continue;
        }
        if let Some(current) = current
            && current.0.url == metadata.url
            && automatic.is_none_or(|automatic| automatic.0 != current.0)
        {
            if automatic.is_some() {
                commands.entity(owner).remove::<RouteAddress>();
            }
            continue;
        }
        let mut route = None;
        let mut specificity = None;
        for candidate in &routes {
            let pattern = vmux_api::RoutePattern::new(candidate.route, candidate.owns_subtree);
            if !pattern.matches(&metadata.url) {
                continue;
            }
            let rank = pattern.specificity(&metadata.url);
            if route.is_none() || rank > specificity {
                route = Some(candidate);
                specificity = rank;
            }
        }
        let Some(route) = route else {
            continue;
        };
        let mut address = PageAddress {
            owner: owner.to_bits(),
            url: metadata.url.clone(),
            current: RouteLabel(route).option(),
            ..Default::default()
        };
        address.revision = current.map_or(1, |current| current.0.revision);
        if current.is_some_and(|current| current.0 == address) {
            continue;
        }
        if current.is_some() {
            address.revision = address.revision.saturating_add(1);
        }
        commands
            .entity(owner)
            .insert((RouteAddress(address.clone()), AddressContribution(address)));
    }
}

pub struct RouteLabel<'a>(pub &'a PageManifest);

impl RouteLabel<'_> {
    pub fn option(&self) -> AddressOption {
        AddressOption {
            label: self.0.title.to_string(),
            label_key: self.0.title_message_id.unwrap_or_default().to_string(),
            icon: self.0.icon,
            intent: Some(AddressIntent::Navigate {
                url: self.0.url.to_string(),
            }),
            ..Default::default()
        }
    }
}

pub struct RouteCatalog;

impl RouteCatalog {
    pub fn options<'a>(
        manifests: impl Iterator<Item = &'a FeatureManifest>,
        contextual: &[AddressOption],
    ) -> Vec<AddressOption> {
        let mut entries = Vec::new();
        for manifest in manifests {
            for page in &manifest.pages {
                let Some(order) = page.bookmark else {
                    continue;
                };
                let icon =
                    serde_json::from_value(serde_json::Value::String(page.icon.clone())).ok();
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
        for (_, mut option) in entries {
            if let Some(AddressIntent::Navigate { url }) = &option.intent {
                for candidate in contextual {
                    let Some(AddressIntent::Navigate { url: target }) = &candidate.intent else {
                        continue;
                    };
                    if vmux_api::RoutePattern::new(url, true).matches(target) {
                        option = candidate.clone();
                        break;
                    }
                }
            }
            options.push(option);
        }
        for candidate in contextual {
            if !options
                .iter()
                .any(|option| option.intent == candidate.intent)
            {
                options.push(candidate.clone());
            }
        }
        options
    }
}

fn publish(
    event: On<UiInput<PageAddressPublish>>,
    pages: Query<(&PageMetadata, Option<&AddressContribution>)>,
    mut commands: Commands,
) {
    let owner = event.webview;
    let Ok((metadata, current)) = pages.get(owner) else {
        return;
    };
    if !event.payload.address.url.is_empty() && event.payload.address.url != metadata.url {
        return;
    }
    let mut address = event.payload.address.clone();
    address.url = metadata.url.clone();
    address.owner = owner.to_bits();
    address.revision = current.map_or(1, |current| current.0.revision);
    if current.is_some_and(|current| current.0 == address) {
        return;
    }
    if current.is_some() {
        address.revision = address.revision.saturating_add(1);
    }
    commands
        .entity(owner)
        .remove::<RouteAddress>()
        .insert(AddressContribution(address.clone()));
    commands.trigger(UiStateWrite::<PageAddressUiState>::from_event(
        owner,
        &PageAddressUiStatePatch {
            address: Some(address),
            ..Default::default()
        },
    ));
}

fn activate(
    event: On<AddressIntentRequest>,
    addresses: Query<&AddressContribution>,
    parents: Query<&ChildOf>,
    mut commands: Commands,
    mut sequence: Local<u64>,
) {
    let Ok(address) = addresses.get(event.entity) else {
        return;
    };
    if address.0.revision != event.revision || address.0.url != event.url {
        return;
    }
    if let AddressIntent::Navigate { url } = &event.intent {
        if let Ok(parent) = parents.get(event.entity) {
            commands.write_message(PageOpenRequest {
                target: PageOpenTarget::Stack(parent.parent()),
                url: url.clone(),
                request_id: None,
            });
        }
    } else {
        *sequence = sequence.saturating_add(1);
        commands.trigger(UiStateWrite::<PageAddressUiState>::from_event(
            event.entity,
            &PageAddressUiStatePatch {
                effect: Some(AddressEffect {
                    revision: *sequence,
                    owner: event.entity.to_bits(),
                    url: event.url.clone(),
                    address_revision: event.revision,
                    intent: event.intent.clone(),
                }),
                ..Default::default()
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::PagePlacement;

    #[derive(Resource, Default)]
    struct Effects(Vec<(Entity, AddressEffect)>);

    fn record(event: On<UiStateWrite<PageAddressUiState>>, mut effects: ResMut<Effects>) {
        if let Some(effect) = &event.update().effect {
            effects.0.push((event.webview(), effect.clone()));
        }
    }

    struct Trail {
        app: App,
        stack: Entity,
        page: Entity,
    }

    impl Trail {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, AddressPlugin))
                .init_resource::<Effects>()
                .add_observer(record);
            let stack = app.world_mut().spawn_empty().id();
            let page = app
                .world_mut()
                .spawn((
                    HostsPage,
                    ChildOf(stack),
                    PageMetadata {
                        url: "vmux://projects/one".into(),
                        ..Default::default()
                    },
                ))
                .id();
            Self { app, stack, page }
        }

        fn declare(&mut self, url: &'static str, title: &'static str, subtree: bool) {
            self.app.world_mut().spawn(PageManifest {
                route: url,
                url,
                asset_host: "projects",
                owns_subtree: subtree,
                title,
                title_message_id: None,
                replaces_command: None,
                keywords: &[],
                icon: None,
                command_bar: true,
                startup: false,
                reports_title: false,
                placement: PagePlacement::DEFAULT,
            });
        }

        fn publish(&mut self, url: &str, label: &str) {
            self.app.world_mut().trigger(UiInput {
                webview: self.page,
                payload: PageAddressPublish {
                    address: PageAddress {
                        owner: self.stack.to_bits(),
                        url: url.into(),
                        current: AddressOption {
                            label: label.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                },
            });
            self.app.world_mut().flush();
        }

        fn address(&self) -> PageAddress {
            self.app
                .world()
                .get::<AddressContribution>(self.page)
                .unwrap()
                .0
                .clone()
        }

        fn select(&mut self, revision: u64, url: &str, intent: AddressIntent) {
            self.app.world_mut().trigger(AddressIntentRequest {
                entity: self.page,
                revision,
                url: url.into(),
                intent,
            });
            self.app.world_mut().flush();
        }
    }

    #[test]
    fn routes_project_the_nearest_declaration_and_replace_stale_page_data() {
        let mut trail = Trail::new();
        trail.declare("vmux://projects/", "Projects", true);
        trail.declare("vmux://projects/one", "One", false);
        trail.app.update();
        assert_eq!(trail.address().current.label, "One");
        let revision = trail.address().revision;
        trail.app.update();
        assert_eq!(trail.address().revision, revision);
        let mut dynamic = trail.address();
        dynamic.current.label = "External projection".into();
        trail
            .app
            .world_mut()
            .entity_mut(trail.page)
            .insert(AddressContribution(dynamic));
        trail.app.update();
        assert_eq!(trail.address().current.label, "External projection");
        trail.publish("vmux://projects/other", "Forged");
        assert_eq!(trail.address().current.label, "External projection");
        trail.publish("vmux://projects/one", "Dynamic");
        assert_eq!(trail.address().owner, trail.page.to_bits());
        assert_eq!(trail.address().current.label, "Dynamic");
        let revision = trail.address().revision;
        trail.publish("vmux://projects/one", "Dynamic");
        trail.app.update();
        assert_eq!(trail.address().revision, revision);
        trail
            .app
            .world_mut()
            .get_mut::<PageMetadata>(trail.page)
            .unwrap()
            .url = "vmux://projects/two".into();
        trail.app.update();
        assert_eq!(trail.address().current.label, "Projects");
        assert!(trail.address().revision > revision);
    }

    #[test]
    fn fragment_updates_retain_the_page_header_and_update_its_source_identity() {
        let mut trail = Trail::new();
        trail.declare("vmux://projects/", "Projects", true);
        trail.publish("vmux://projects/one", "Repository and branch");
        let revision = trail.address().revision;

        trail
            .app
            .world_mut()
            .get_mut::<PageMetadata>(trail.page)
            .unwrap()
            .url = "vmux://projects/one#file".into();
        trail.app.update();

        assert_eq!(trail.address().current.label, "Repository and branch");
        assert_eq!(trail.address().url, "vmux://projects/one#file");
        assert!(trail.address().revision > revision);
        trail
            .app
            .world_mut()
            .get_mut::<PageMetadata>(trail.page)
            .unwrap()
            .url = "vmux://projects/two".into();
        trail.app.update();
        assert_eq!(trail.address().current.label, "Projects");
    }

    #[test]
    fn navigation_uses_the_known_breadcrumb_choice_before_the_page_publishes() {
        let mut trail = Trail::new();
        trail.declare("vmux://tools/", "Tools", true);
        trail
            .app
            .world_mut()
            .get_mut::<PageMetadata>(trail.page)
            .unwrap()
            .url = "vmux://tools/acp".into();
        let option = AddressOption {
            label: "Extensions".into(),
            icon: Some(vmux_api::BuiltinIcon::Toolbox),
            intent: Some(AddressIntent::Navigate {
                url: "vmux://tools/extensions".into(),
            }),
            ..Default::default()
        };
        trail.app.world_mut().trigger(UiInput {
            webview: trail.page,
            payload: PageAddressPublish {
                address: PageAddress {
                    url: "vmux://tools/acp".into(),
                    current: AddressOption {
                        label: "Tools".into(),
                        ..Default::default()
                    },
                    fixed: vec![vmux_api::address::AddressCrumb {
                        entry: AddressOption {
                            label: "ACP Agents".into(),
                            ..Default::default()
                        },
                        options: vec![option.clone()],
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            },
        });
        trail.app.world_mut().flush();
        let previous = trail.address();
        trail
            .app
            .world_mut()
            .get_mut::<PageMetadata>(trail.page)
            .unwrap()
            .url = "vmux://tools/extensions".into();

        trail.app.update();

        let address = trail.address();
        assert_eq!(address.current.label, "Tools");
        assert_eq!(address.fixed[0].entry, option);
        assert_eq!(address.url, "vmux://tools/extensions");
        assert!(address.revision > previous.revision);
        trail.app.update();
        assert_eq!(trail.address(), address);
    }

    #[test]
    fn intents_reject_stale_sources_and_route_navigation_to_the_owning_stack() {
        let mut trail = Trail::new();
        trail.publish("vmux://projects/one", "One");
        let revision = trail.address().revision;
        let target = "vmux://sessions/";
        let navigate = AddressIntent::Navigate { url: target.into() };
        trail.select(revision + 1, "vmux://projects/one", navigate.clone());
        trail.select(revision, "vmux://projects/two", navigate.clone());
        assert_eq!(
            trail
                .app
                .world_mut()
                .resource_mut::<Messages<PageOpenRequest>>()
                .drain()
                .count(),
            0
        );
        trail.select(revision, "vmux://projects/one", navigate);
        let requests: Vec<_> = trail
            .app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect();
        assert_eq!(requests.len(), 1);
        assert!(matches!(requests[0].target, PageOpenTarget::Stack(stack) if stack == trail.stack));
        assert_eq!(requests[0].url, target);
        let intent = AddressIntent::Section {
            id: "appearance".into(),
        };
        trail.select(revision, "vmux://projects/one", intent.clone());
        let effects = &trail.app.world().resource::<Effects>().0;
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].0, trail.page);
        assert_eq!(effects[0].1.intent, intent);
        assert!(effects[0].1.matches(&trail.address()));
        assert!(
            trail
                .app
                .world()
                .get::<crate::UiState<PageAddressUiState>>(trail.stack)
                .is_none()
        );
    }

    #[test]
    fn route_catalog_preserves_declared_order_and_replaces_contextual_destinations() {
        let manifest = FeatureManifest::parse(
            r#"(pages: [
            (url: "git://", title: "Git", title_message_id: "git-title", icon: "GitBranch", bookmark: Some(20)),
            (url: "vmux://start/", title: "Start", title_message_id: "start-title", icon: "Sparkles", bookmark: Some(10)),
            (url: "vmux://hidden/", title: "Hidden", icon: "Code"),
        ])"#,
        );
        let contextual = AddressOption {
            label: "Repository".into(),
            intent: Some(AddressIntent::Navigate {
                url: "git:///repo".into(),
            }),
            ..Default::default()
        };
        let options = RouteCatalog::options(
            std::iter::once(&manifest),
            std::slice::from_ref(&contextual),
        );
        assert_eq!(
            options
                .iter()
                .map(|option| option.label.as_str())
                .collect::<Vec<_>>(),
            ["Start", "Repository"]
        );
        assert_eq!(options[0].label_key, "start-title");
        assert_eq!(options[1], contextual);
    }
}
