use bevy::prelude::*;
use vmux_api::Id;
use vmux_auth::{CanAccess, Connection, ConnectionState, ProviderKey};
use vmux_connector::{
    CanonicalField, ExternalId, ExternalKey, ExternalLink, ExternalLinkIdentity, ExternalLinkIndex,
    ExternalLinkSpec, ExternalScope, ExternalScopeRef, ExternalUrl, PrimaryWorkItem, ResourceKind,
    SyncPolicy, SynchronizedFields,
};
use vmux_ecs::{OperationError, OperationState, Revision};
use vmux_editor::Project;
use vmux_session::Session;

use crate::model::{GithubCoordinates, GithubIssue, GithubRepository};

use super::{GithubCompletion, GithubFailure, GithubScopeAccess, GithubSet, TaskPlugin};

pub(super) struct AssociationPlugin;

impl Plugin for AssociationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<GithubCompletion<RepositoryAssociation>>()
            .add_message::<GithubCompletion<IssueAssociation>>()
            .add_message::<GithubFailure<RepositoryAssociation>>()
            .add_message::<GithubFailure<IssueAssociation>>()
            .add_plugins((
                TaskPlugin::<RepositoryAssociation>::default(),
                TaskPlugin::<IssueAssociation>::default(),
            ))
            .add_systems(
                Update,
                (associate_repository, associate_issue)
                    .chain()
                    .in_set(GithubSet::Complete),
            );
    }
}

pub(super) struct RepositoryAssociation {
    pub(super) connection: Id<Connection>,
    pub(super) repository: GithubRepository,
}

pub(super) struct IssueAssociation {
    pub(super) connection: Id<Connection>,
    pub(super) issue: GithubIssue,
    pub(super) sync_policy: SyncPolicy,
    pub(super) expected: vmux_session::mutation::Revision,
}

fn associate_repository(
    mut associations: MessageReader<GithubCompletion<RepositoryAssociation>>,
    projects: Query<(), With<Project>>,
    mut connections: AssociationAccess,
    mut index: ResMut<ExternalLinkIndex>,
    mut scope_registry: GithubScopeAccess,
    mut commands: Commands,
) {
    let mut associations = associations.read();
    let Some(first) = associations.next() else {
        return;
    };
    for completion in std::iter::once(first).chain(associations) {
        let operation = completion.operation;
        let project = completion.target;
        let association = &completion.output;
        if !projects.contains(project) {
            commands.entity(operation).insert(OperationState::Canceled);
            continue;
        }
        let Some(scope) = association.repository.scope_id() else {
            commands.entity(operation).insert((
                OperationState::Failed,
                OperationError::new("GitHub repository owner identity is unavailable")
                    .expect("valid operation metadata"),
            ));
            continue;
        };
        if !connections.authorize(&association.connection, &scope) {
            commands.entity(operation).insert((
                OperationState::Failed,
                OperationError::new("GitHub connection cannot access this repository")
                    .expect("valid operation metadata"),
            ));
            continue;
        }
        let external_id = association.repository.external_id();
        if index
            .reserve(ExternalLinkSpec {
                identity: ExternalLinkIdentity {
                    provider: ProviderKey("github".into()),
                    scope: scope.clone(),
                    kind: ResourceKind::repository(),
                    id: ExternalId(external_id.clone()),
                },
                parent: project,
                primary: false,
                sync_policy: SyncPolicy::ReferenceOnly,
                fields: &[],
            })
            .is_err()
        {
            commands.entity(operation).insert((
                OperationState::Failed,
                OperationError::new("Repository is already associated")
                    .expect("valid operation metadata"),
            ));
            continue;
        }
        scope_registry.ensure(&scope, &association.repository.owner_login);
        commands.spawn((
            ExternalLink,
            ChildOf(project),
            ProviderKey("github".into()),
            ExternalScopeRef(scope),
            ResourceKind::repository(),
            ExternalId(external_id),
            ExternalKey(association.repository.full_name.clone()),
            ExternalUrl(association.repository.html_url.clone()),
            SyncPolicy::ReferenceOnly,
            SynchronizedFields::default(),
            GithubCoordinates {
                owner: association.repository.owner_login.clone(),
                repository: association.repository.name.clone(),
                number: None,
            },
            association.repository.clone(),
            Revision::new(0),
        ));
        commands.entity(operation).insert(OperationState::Succeeded);
    }
}

fn associate_issue(
    mut associations: MessageReader<GithubCompletion<IssueAssociation>>,
    sessions: Query<
        (
            &vmux_session::SessionId,
            Option<&vmux_session::mutation::Revision>,
        ),
        With<Session>,
    >,
    mut connections: AssociationAccess,
    mut index: ResMut<ExternalLinkIndex>,
    mut scope_registry: GithubScopeAccess,
    mut commands: Commands,
) {
    let mut associations = associations.read();
    let Some(first) = associations.next() else {
        return;
    };
    for completion in std::iter::once(first).chain(associations) {
        let operation = completion.operation;
        let session = completion.target;
        let association = &completion.output;
        let Ok((session_id, revision)) = sessions.get(session) else {
            commands.entity(operation).insert(OperationState::Canceled);
            continue;
        };
        if revision.copied().unwrap_or_default() != association.expected {
            commands.entity(operation).insert((
                OperationState::Canceled,
                OperationError::new("Session changed before GitHub association completed").unwrap(),
            ));
            continue;
        }
        let Some(scope) = association.issue.scope_id() else {
            commands.entity(operation).insert((
                OperationState::Failed,
                OperationError::new("GitHub issue owner identity is unavailable")
                    .expect("valid operation metadata"),
            ));
            continue;
        };
        if !connections.authorize(&association.connection, &scope) {
            commands.entity(operation).insert((
                OperationState::Failed,
                OperationError::new("GitHub connection cannot access this issue")
                    .expect("valid operation metadata"),
            ));
            continue;
        }
        let external_id = association.issue.external_id();
        let synchronized = match association.sync_policy {
            SyncPolicy::ReferenceOnly => Vec::new(),
            SyncPolicy::ImportOnce | SyncPolicy::Bidirectional => {
                vec![CanonicalField::title(), CanonicalField::description()]
            }
        };
        if let Err(conflict) = index.reserve(ExternalLinkSpec {
            identity: ExternalLinkIdentity {
                provider: ProviderKey("github".into()),
                scope: scope.clone(),
                kind: ResourceKind::issue(),
                id: ExternalId(external_id.clone()),
            },
            parent: session,
            primary: true,
            sync_policy: association.sync_policy,
            fields: &synchronized,
        }) {
            commands.entity(operation).insert((
                OperationState::Failed,
                OperationError::new(conflict.to_string()).expect("valid operation metadata"),
            ));
            continue;
        }
        scope_registry.ensure(&scope, &association.issue.owner);
        if association.sync_policy != SyncPolicy::ReferenceOnly {
            commands.queue(vmux_session::mutation::Request {
                operation: format!("github.issue.associate:{}", operation.to_bits()),
                session: session_id.clone(),
                expected: Some(association.expected),
                mutation: vmux_session::mutation::Mutation::Summarize {
                    title: association.issue.title.clone(),
                    description: association.issue.body.clone(),
                },
            });
        }
        commands.spawn((
            ExternalLink,
            ChildOf(session),
            ProviderKey("github".into()),
            ExternalScopeRef(scope),
            ResourceKind::issue(),
            ExternalId(external_id),
            ExternalKey(format!(
                "{}/{}#{}",
                association.issue.owner, association.issue.repository, association.issue.number
            )),
            ExternalUrl(association.issue.html_url.clone()),
            association.sync_policy,
            SynchronizedFields(synchronized),
            PrimaryWorkItem,
            GithubCoordinates {
                owner: association.issue.owner.clone(),
                repository: association.issue.repository.clone(),
                number: Some(association.issue.number),
            },
            association.issue.clone(),
            Revision::new(0),
        ));
        commands.entity(operation).insert(OperationState::Succeeded);
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct AssociationAccess<'w, 's> {
    connections: Query<
        'w,
        's,
        (
            &'static Id<Connection>,
            &'static ProviderKey,
            &'static ConnectionState,
            &'static mut CanAccess,
        ),
        With<Connection>,
    >,
}

impl AssociationAccess<'_, '_> {
    fn authorize(&mut self, connection: &Id<Connection>, scope: &Id<ExternalScope>) -> bool {
        for (held, provider, state, mut access) in self.connections.iter_mut() {
            if held != connection || provider.0 != "github" || *state != ConnectionState::Connected
            {
                continue;
            }
            if !access.0.contains(scope) {
                access.0.push(scope.clone());
            }
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{GithubHostPlugin, GithubTaskTarget};
    use vmux_auth::CanAccess;
    use vmux_ecs::{Cwd, Description, Operation, OperationError};

    use vmux_session::SessionId;

    struct Fixture {
        app: App,
        connection: Id<Connection>,
        session: Entity,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, GithubHostPlugin))
                .add_message::<vmux_session::mutation::Committed>()
                .add_message::<vmux_session::mutation::Rejected>();
            let connection = Id::<Connection>::new("connection-1");
            app.world_mut().spawn((
                Connection,
                connection.clone(),
                ProviderKey("github".into()),
                ConnectionState::Connected,
                CanAccess(vec![Id::new("github.com/accounts/1")]),
            ));
            let session = app
                .world_mut()
                .spawn((
                    Session,
                    SessionId("session-1".into()),
                    Name::new("Local title"),
                    Description("Local description".into()),
                    Cwd::default(),
                    vmux_session::mutation::Revision(4),
                ))
                .id();
            Self {
                app,
                connection,
                session,
            }
        }

        fn issue(&self, node: &str, number: u64) -> GithubIssue {
            GithubIssue {
                id: number,
                node_id: node.to_string(),
                number,
                title: format!("Issue {number}"),
                body: format!("Body {number}"),
                state: crate::IssueState::Open,
                html_url: format!("https://github.com/vmux-ai/vmux/issues/{number}"),
                repository_url: "https://api.github.com/repos/vmux-ai/vmux".into(),
                repository: "vmux".into(),
                owner: "vmux-ai".into(),
                owner_id: 1,
                updated_at: "2026-10-24T00:00:00Z".into(),
            }
        }

        fn associate(&mut self, issue: GithubIssue, sync_policy: SyncPolicy) {
            let expected = self
                .app
                .world()
                .get::<vmux_session::mutation::Revision>(self.session)
                .copied()
                .unwrap_or_default();
            let operation = self.app.world_mut().spawn(Operation).id();
            self.app
                .world_mut()
                .resource_mut::<Messages<GithubCompletion<IssueAssociation>>>()
                .write(GithubCompletion {
                    operation,
                    target: self.session,
                    binding: GithubTaskTarget::BoundEntity {
                        entity: self.session,
                    },
                    output: IssueAssociation {
                        connection: self.connection.clone(),
                        issue,
                        sync_policy,
                        expected,
                    },
                });
            self.app.update();
        }
    }

    #[test]
    fn stale_issue_association_preserves_newer_canonical_state() {
        let mut fixture = Fixture::new();
        let issue = fixture.issue("issue-1", 1);
        let operation = fixture.app.world_mut().spawn(Operation).id();
        fixture.app.world_mut().write_message(GithubCompletion {
            operation,
            target: fixture.session,
            binding: GithubTaskTarget::BoundEntity {
                entity: fixture.session,
            },
            output: IssueAssociation {
                connection: fixture.connection.clone(),
                issue,
                sync_policy: SyncPolicy::ImportOnce,
                expected: vmux_session::mutation::Revision(3),
            },
        });
        fixture.app.update();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Name>(fixture.session)
                .unwrap()
                .as_str(),
            "Local title"
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<vmux_session::mutation::Revision>(fixture.session),
            Some(&vmux_session::mutation::Revision(4))
        );
        assert_eq!(
            fixture.app.world().get::<OperationState>(operation),
            Some(&OperationState::Canceled)
        );
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query_filtered::<Entity, With<ExternalLink>>()
                .iter(fixture.app.world())
                .count(),
            0
        );
    }

    #[test]
    fn import_once_moves_remote_fields_into_the_canonical_session() {
        let mut fixture = Fixture::new();
        fixture.associate(fixture.issue("issue-1", 1), SyncPolicy::ImportOnce);

        let mut sessions = fixture
            .app
            .world_mut()
            .query_filtered::<(&Name, &Description), With<Session>>();
        let (name, description) = sessions.single(fixture.app.world()).unwrap();
        assert_eq!(name.as_str(), "Issue 1");
        assert_eq!(description.0, "Body 1");

        let mut links = fixture
            .app
            .world_mut()
            .query_filtered::<(&ExternalId, &SyncPolicy, &ChildOf), With<PrimaryWorkItem>>();
        let (id, policy, parent) = links.single(fixture.app.world()).unwrap();
        assert_eq!(id.0, "issue-1");
        assert_eq!(*policy, SyncPolicy::ImportOnce);
        assert!(
            fixture
                .app
                .world()
                .get::<Session>(parent.parent())
                .is_some()
        );
    }

    #[test]
    fn reference_only_preserves_canonical_fields() {
        let mut fixture = Fixture::new();
        fixture.associate(fixture.issue("issue-1", 1), SyncPolicy::ReferenceOnly);

        let mut sessions = fixture
            .app
            .world_mut()
            .query_filtered::<(&Name, &Description), With<Session>>();
        let (name, description) = sessions.single(fixture.app.world()).unwrap();
        assert_eq!(name.as_str(), "Local title");
        assert_eq!(description.0, "Local description");
    }

    #[test]
    fn competing_bidirectional_authority_is_rejected() {
        let mut fixture = Fixture::new();
        fixture.associate(fixture.issue("issue-1", 1), SyncPolicy::Bidirectional);
        fixture.associate(fixture.issue("issue-2", 2), SyncPolicy::Bidirectional);

        let links = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWorkItem>>()
            .iter(fixture.app.world())
            .count();
        assert_eq!(links, 1);
        let errors = fixture
            .app
            .world_mut()
            .query::<&OperationError>()
            .iter(fixture.app.world())
            .map(|error| error.message().to_string())
            .collect::<Vec<_>>();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("already synchronizes"))
        );
    }

    #[test]
    fn session_accepts_only_one_primary_work_item() {
        let mut fixture = Fixture::new();
        fixture.associate(fixture.issue("issue-1", 1), SyncPolicy::ReferenceOnly);
        fixture.associate(fixture.issue("issue-2", 2), SyncPolicy::ReferenceOnly);

        let links = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWorkItem>>()
            .iter(fixture.app.world())
            .count();
        assert_eq!(links, 1);
        let errors = fixture
            .app
            .world_mut()
            .query::<&OperationError>()
            .iter(fixture.app.world())
            .map(|error| error.message().to_string())
            .collect::<Vec<_>>();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("primary work item"))
        );
    }
}
