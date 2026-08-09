use super::*;
use bevy::ecs::system::RunSystemOnce;
use vmux_core::LastActivatedAt;
use vmux_core::agent::AgentKind;

fn spawn_team_stack(world: &mut World, space: Entity) -> Entity {
    world
        .spawn((
            Stack::default(),
            PageMetadata {
                url: TEAM_PAGE_URL.to_string(),
                ..default()
            },
            ChildOf(space),
        ))
        .id()
}

fn lookup(app: &mut App, space: Entity) -> Option<Entity> {
    app.world_mut()
        .run_system_once(
            move |stacks: Query<(Entity, &PageMetadata), With<Stack>>,
                  child_of: Query<&ChildOf>,
                  spaces: Query<(), With<Space>>| {
                open_team_stack_in_space(space, &stacks, &child_of, &spaces)
            },
        )
        .unwrap()
}

#[test]
fn done_unseen_sets_row_flag() {
    let row = team_member_row(
        Entity::PLACEHOLDER,
        &Profile::agent(AgentKind::Claude),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        false,
        false,
        true,
    );
    assert!(row.is_done_unseen);
}

#[test]
fn finds_open_team_stack_in_active_space() {
    let mut app = App::new();
    let space = app.world_mut().spawn(Space).id();
    let stack = spawn_team_stack(app.world_mut(), space);
    assert_eq!(lookup(&mut app, space), Some(stack));
}

#[test]
fn ignores_team_stack_in_other_space() {
    let mut app = App::new();
    let active = app.world_mut().spawn(Space).id();
    let other = app.world_mut().spawn(Space).id();
    spawn_team_stack(app.world_mut(), other);
    assert_eq!(lookup(&mut app, active), None);
}

#[test]
fn ignores_non_team_stack_in_active_space() {
    let mut app = App::new();
    let space = app.world_mut().spawn(Space).id();
    app.world_mut().spawn((
        Stack::default(),
        PageMetadata {
            url: "https://example.com".to_string(),
            ..default()
        },
        ChildOf(space),
    ));
    assert_eq!(lookup(&mut app, space), None);
}

#[test]
fn parse_member_entity_roundtrips_and_rejects_garbage() {
    let mut app = App::new();
    let entity = app.world_mut().spawn_empty().id();
    let bits = entity.to_bits().to_string();
    assert_eq!(parse_member_entity(&bits), Some(entity));
    assert_eq!(parse_member_entity("not-a-number"), None);
    assert_eq!(parse_member_entity(""), None);
}

#[test]
fn team_page_open_titles_webview_team() {
    use vmux_core::page_open::{PageOpenId, PageOpenTask};
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_plugins(WarmPagePlugin::<Team>::default());

    let stack = app.world_mut().spawn(Stack::default()).id();
    app.world_mut().spawn(PageOpenTask {
        id: PageOpenId::new(),
        stack,
        url: TEAM_PAGE_URL.to_string(),
        request_id: None,
    });
    app.update();

    let title = app
        .world_mut()
        .query_filtered::<&PageMetadata, With<Team>>()
        .single(app.world())
        .expect("team webview spawned")
        .title
        .clone();
    assert_eq!(title, "Team");
}

fn command_app() -> App {
    let mut app = App::new();
    app.add_message::<AppCommand>()
        .add_message::<vmux_command::CommandIssued>()
        .add_observer(on_team_command);
    app
}

#[test]
fn agent_avatar_click_focuses_agent_stack() {
    let mut app = command_app();
    let space = app.world_mut().spawn(Space).id();
    app.insert_resource(ActiveSpaceEntity(Some(space)));
    let stack = app
        .world_mut()
        .spawn((
            Stack::default(),
            Agent {
                sid: "s".to_string(),
                kind: Some(AgentKind::Claude),
            },
            ChildOf(space),
        ))
        .id();

    app.world_mut().trigger(BinReceive::<TeamCommandEvent> {
        webview: Entity::PLACEHOLDER,
        payload: TeamCommandEvent {
            command: "focus".to_string(),
            member_id: Some(stack.to_bits().to_string()),
        },
    });
    app.world_mut().flush();

    assert!(app.world().get::<LastActivatedAt>(stack).is_some());
    assert_eq!(lookup(&mut app, space), None);
}

#[test]
fn user_click_reuses_open_team_stack() {
    let mut app = command_app();
    let space = app.world_mut().spawn(Space).id();
    app.insert_resource(ActiveSpaceEntity(Some(space)));
    let team = spawn_team_stack(app.world_mut(), space);

    app.world_mut().trigger(BinReceive::<TeamCommandEvent> {
        webview: Entity::PLACEHOLDER,
        payload: TeamCommandEvent {
            command: "open".to_string(),
            member_id: None,
        },
    });
    app.world_mut().flush();

    assert!(app.world().get::<LastActivatedAt>(team).is_some());
}

#[test]
fn acp_agent_appears_in_roster_with_registry_icon() {
    let mut app = App::new();
    let space = app.world_mut().spawn(Space).id();
    app.insert_resource(ActiveSpaceEntity(Some(space)));
    app.world_mut().spawn((Profile::user(), User));
    app.world_mut().spawn((
        Profile::registry("Mistral Vibe", "mistral-vibe"),
        Agent {
            sid: "sid-1".to_string(),
            kind: None,
        },
        PageMetadata {
            url: "vmux://agent/mistral-vibe".to_string(),
            icon: vmux_core::PageIcon::favicon("https://cdn.example/vibe.svg"),
            ..default()
        },
        ChildOf(space),
    ));

    let rows = app
        .world_mut()
        .run_system_once(
            |active: Res<ActiveSpaceEntity>,
             user_q: Query<(Entity, &Profile), With<User>>,
             agent_q: Query<(
                Entity,
                &Profile,
                &Agent,
                Option<&AgentRunState>,
                Option<&SessionId>,
                Option<&vmux_core::notify::AgentDoneUnseen>,
            )>,
             child_of: Query<&ChildOf>,
             space_marker: Query<(), With<Space>>,
             meta_q: Query<&PageMetadata>,
             children_q: Query<&Children>| {
                build_team_members(
                    &active,
                    &user_q,
                    &agent_q,
                    &child_of,
                    &space_marker,
                    &meta_q,
                    &children_q,
                )
            },
        )
        .unwrap();

    let agent = rows
        .iter()
        .find(|r| !r.is_user)
        .expect("acp agent in roster");
    assert_eq!(agent.name, "Mistral Vibe");
    assert_eq!(agent.icon, "https://cdn.example/vibe.svg");
    // ACP rows carry their page url so the frontend resolves the brand favicon.
    assert_eq!(agent.url, "vmux://agent/mistral-vibe");
}
