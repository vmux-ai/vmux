use bevy::prelude::*;
use vmux_api::address::{
    AddressCrumb, AddressIntent, AddressOption, PageAddress, PageAddressUiState,
    PageAddressUiStatePatch,
};
use vmux_api::protocol::{ClientMessage, ProcessId};
use vmux_ecs::page::{AddressContribution, AddressIntentRequest};
use vmux_ecs::service::{
    ServiceConnected, ServiceMessageAppExt, ServiceMessageSet, ServiceRequest,
};
use vmux_ecs::{Cwd, PageMetadata, UiStateWrite};

use super::input_queue::TerminalProcessIndex;
use super::launch::TerminalLaunch;
use crate::Terminal;

pub(super) struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_service_message::<Received>()
            .add_service_message::<Entries>()
            .add_message::<ServiceRequest>()
            .add_observer(request)
            .add_systems(Update, (seed, receive).chain().after(ServiceMessageSet))
            .add_systems(
                PostUpdate,
                publish.before(vmux_layout::projection::LayoutUiProjectionSet::SourceFlush),
            );
    }
}

#[vmux_api::service_message(ProcessDirectory)]
struct Received {
    process_id: ProcessId,
    cwd: String,
    ready: bool,
}

#[vmux_api::service_message(ProcessDirectories)]
struct Entries {
    process_id: ProcessId,
    cwd: String,
    path: String,
    directories: Vec<String>,
    error: Option<String>,
}

#[derive(Component, Default)]
struct Listing {
    path: String,
    directories: Vec<String>,
    pending: bool,
    ready: bool,
}

fn seed(
    terminals: Query<
        (Entity, &TerminalLaunch, Option<&Cwd>),
        (With<Terminal>, Changed<TerminalLaunch>),
    >,
    mut commands: Commands,
) {
    for (entity, launch, current) in &terminals {
        if !launch.cwd.is_empty() && current.is_none_or(|cwd| cwd.0.as_os_str().is_empty()) {
            commands
                .entity(entity)
                .insert((Cwd(launch.cwd.clone().into()), Listing::default()));
        }
    }
}

fn receive(
    mut changed: MessageReader<Received>,
    mut entries: MessageReader<Entries>,
    index: Single<&TerminalProcessIndex>,
    connected: Query<(), With<ServiceConnected>>,
    mut terminals: Query<(&Cwd, &mut Listing), With<Terminal>>,
    mut commands: Commands,
) {
    if connected.is_empty() {
        for (_, mut listing) in &mut terminals {
            listing.ready = false;
            listing.pending = false;
        }
        let _ = changed.read().count();
        let _ = entries.read().count();
        return;
    }
    for event in changed.read() {
        let Some(entity) = index.get(&event.process_id) else {
            continue;
        };
        if !std::path::Path::new(&event.cwd).is_absolute() {
            continue;
        }
        if let Ok((cwd, mut listing)) = terminals.get_mut(entity) {
            if cwd.0.as_path() != std::path::Path::new(&event.cwd) {
                *listing = Listing {
                    ready: event.ready,
                    ..Default::default()
                };
                commands
                    .entity(entity)
                    .insert(Cwd(event.cwd.clone().into()));
            } else {
                listing.ready = event.ready;
            }
        } else {
            commands.entity(entity).insert((
                Cwd(event.cwd.clone().into()),
                Listing {
                    ready: event.ready,
                    ..Default::default()
                },
            ));
        }
    }
    for event in entries.read() {
        let Some(entity) = index.get(&event.process_id) else {
            continue;
        };
        let Ok((cwd, mut listing)) = terminals.get_mut(entity) else {
            continue;
        };
        if cwd.0.as_path() != std::path::Path::new(&event.cwd) || listing.path != event.path {
            continue;
        }
        listing.pending = false;
        listing.directories = event.directories.clone();
        if let Some(error) = &event.error {
            warn!(%error, "terminal directory listing failed");
        }
    }
}

fn request(
    event: On<AddressIntentRequest>,
    connected: Query<(), With<ServiceConnected>>,
    mut terminals: Query<(&ProcessId, &Cwd, &AddressContribution, &mut Listing), With<Terminal>>,
    mut requests: MessageWriter<ServiceRequest>,
) {
    if connected.is_empty() {
        return;
    }
    let Ok((process_id, cwd, address, mut listing)) = terminals.get_mut(event.entity) else {
        return;
    };
    if address.0.revision != event.revision || address.0.url != event.url {
        return;
    }
    let cwd = cwd.0.to_string_lossy().into_owned();
    match &event.intent {
        AddressIntent::TerminalDirectory { path } if listing.ready => {
            requests.write(ServiceRequest(ClientMessage::ChangeProcessDirectory {
                process_id: *process_id,
                cwd,
                path: path.clone(),
            }));
        }
        AddressIntent::TerminalDirectories { path } => {
            if listing.pending && listing.path == *path {
                return;
            }
            listing.pending = true;
            listing.path = path.clone();
            requests.write(ServiceRequest(ClientMessage::ListProcessDirectories {
                process_id: *process_id,
                cwd,
                path: path.clone(),
            }));
        }
        _ => {}
    }
}

fn publish(
    terminals: Query<
        (
            Entity,
            &PageMetadata,
            &Cwd,
            &Listing,
            Option<&AddressContribution>,
        ),
        With<Terminal>,
    >,
    mut commands: Commands,
) {
    for (entity, metadata, cwd, listing, current) in &terminals {
        let path = cwd.0.to_string_lossy().into_owned();
        let mut options = Vec::new();
        for directory in &listing.directories {
            let label = if cwd
                .0
                .parent()
                .is_some_and(|parent| parent == std::path::Path::new(directory))
            {
                let name = std::path::Path::new(directory)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "/".to_string());
                format!("↑ {name}")
            } else {
                std::path::Path::new(directory)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            };
            let mut option = AddressOption::file(label, directory.clone(), true);
            option.intent = listing.ready.then(|| AddressIntent::TerminalDirectory {
                path: directory.clone(),
            });
            options.push(option);
        }
        let mut entry =
            AddressOption::file(vmux_path::DisplayPath::project(&cwd.0), path.clone(), true);
        entry.intent = None;
        let mut address = PageAddress {
            owner: entity.to_bits(),
            revision: current.map_or(1, |current| current.0.revision),
            url: metadata.url.clone(),
            current: AddressOption {
                label_key: "command-terminal".into(),
                icon: Some(vmux_api::BuiltinIcon::Terminal),
                intent: Some(AddressIntent::Navigate {
                    url: metadata.url.clone(),
                }),
                ..Default::default()
            },
            paths: vec![AddressCrumb {
                entry,
                options,
                on_open: Some(AddressIntent::TerminalDirectories { path }),
                pending: listing.pending,
                active: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        if current.is_some_and(|current| current.0 == address) {
            continue;
        }
        if let Some(current) = current {
            address.revision = current.0.revision.saturating_add(1);
        }
        commands
            .entity(entity)
            .insert(AddressContribution(address.clone()));
        commands.trigger(UiStateWrite::<PageAddressUiState>::from_event(
            entity,
            &PageAddressUiStatePatch {
                address: Some(address),
                ..Default::default()
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use vmux_api::protocol::ServiceMessage;
    use vmux_ecs::service::ServiceInbound;

    struct TerminalFixture {
        app: App,
        terminal: Entity,
        process_id: ProcessId,
    }

    impl TerminalFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                super::super::input_queue::InputQueuePlugin,
                Plugin,
            ))
            .add_message::<ServiceInbound>();
            let process_id = ProcessId::new();
            app.world_mut().spawn(ServiceConnected);
            let terminal = app
                .world_mut()
                .spawn((
                    Terminal,
                    process_id,
                    TerminalLaunch {
                        command: "nu".into(),
                        args: Vec::new(),
                        cwd: "/launch".into(),
                        env: Vec::new(),
                    },
                    PageMetadata {
                        url: "vmux://terminal/4242".into(),
                        ..Default::default()
                    },
                ))
                .id();
            app.update();
            Self {
                app,
                terminal,
                process_id,
            }
        }

        fn receive(&mut self, message: ServiceMessage) {
            self.app.world_mut().write_message(ServiceInbound(message));
            self.app.update();
        }

        fn request(&mut self, intent: AddressIntent) {
            let address = self
                .app
                .world()
                .get::<AddressContribution>(self.terminal)
                .unwrap()
                .0
                .clone();
            self.app.world_mut().trigger(AddressIntentRequest {
                entity: self.terminal,
                revision: address.revision,
                url: address.url,
                intent,
            });
            self.app.update();
        }

        fn address(&self) -> &PageAddress {
            &self
                .app
                .world()
                .get::<AddressContribution>(self.terminal)
                .unwrap()
                .0
        }
    }

    #[test]
    fn terminal_directory_replay_replaces_launch_context_and_selects_the_same_process() {
        let mut fixture = TerminalFixture::new();
        assert_eq!(fixture.address().paths[0].entry.path, "/launch");
        fixture.receive(ServiceMessage::ProcessDirectory {
            process_id: fixture.process_id,
            cwd: "/current".into(),
            ready: true,
        });
        assert_eq!(fixture.address().paths[0].entry.path, "/current");
        assert!(fixture.address().fixed.is_empty());
        fixture.request(AddressIntent::TerminalDirectories {
            path: "/current".into(),
        });
        fixture.receive(ServiceMessage::ProcessDirectories {
            process_id: fixture.process_id,
            cwd: "/current".into(),
            path: "/current".into(),
            directories: vec!["/current/next".into()],
            error: None,
        });
        assert!(!fixture.address().paths[0].pending);
        let intent = fixture.address().paths[0].options[0]
            .intent
            .clone()
            .unwrap();
        fixture.request(intent);
        let requests = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<ServiceRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert!(requests.iter().any(|request| matches!(&request.0, ClientMessage::ChangeProcessDirectory { process_id, cwd, path } if *process_id == fixture.process_id && cwd == "/current" && path == "/current/next")));
        fixture.receive(ServiceMessage::ProcessDirectory {
            process_id: fixture.process_id,
            cwd: "/current/next".into(),
            ready: true,
        });
        assert_eq!(fixture.address().paths[0].entry.path, "/current/next");
        assert_eq!(
            fixture
                .app
                .world()
                .get::<TerminalLaunch>(fixture.terminal)
                .unwrap()
                .cwd,
            "/launch"
        );
    }

    #[test]
    fn late_directory_lists_and_busy_directory_requests_cannot_change_the_terminal() {
        let mut fixture = TerminalFixture::new();
        fixture.receive(ServiceMessage::ProcessDirectory {
            process_id: fixture.process_id,
            cwd: "/current".into(),
            ready: true,
        });
        fixture.request(AddressIntent::TerminalDirectories {
            path: "/current".into(),
        });
        fixture.receive(ServiceMessage::ProcessDirectory {
            process_id: fixture.process_id,
            cwd: "/other".into(),
            ready: false,
        });
        fixture.receive(ServiceMessage::ProcessDirectories {
            process_id: fixture.process_id,
            cwd: "/current".into(),
            path: "/current".into(),
            directories: vec!["/current/late".into()],
            error: None,
        });
        assert_eq!(fixture.address().paths[0].entry.path, "/other");
        assert!(fixture.address().paths[0].options.is_empty());
        fixture.request(AddressIntent::TerminalDirectory {
            path: "/current/late".into(),
        });
        let requests = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<ServiceRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert!(
            !requests
                .iter()
                .any(|request| matches!(request.0, ClientMessage::ChangeProcessDirectory { .. }))
        );
    }
}
