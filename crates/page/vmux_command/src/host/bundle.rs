/// The command bar's host-side entity.
///
/// It was called `Modal` and lived in `vmux_layout::window`, which made the layout the owner of a
/// surface it only ever hosted: every piece of the command bar's state — its reveal, its rendered
/// and painted acks, its sizing — was already written and read here. The name said "some modal",
/// and there is only one, so it now says which.
#[derive(bevy_ecs::component::Component)]
pub struct CommandBar;
