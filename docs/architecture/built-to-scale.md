# ECS, explained

> Part of the [Vmux Architecture](../architecture.md) overview.

Vmux's host runs on **Bevy**, an **Entity-Component-System (ECS)**. If you've never touched
one, the name sounds exotic — but the whole model is three nouns, and each maps cleanly onto
something you already use in React. Here it is from the ground up.

The one move that feels new: ECS **splits the atom of a React component** into *data* and
*behavior*. State lives in one place, the functions that act on it live in another, and an id
ties them together.

## Entity — just an id

An entity is **not** an object. No methods, no fields, no class to extend — it's a bare id,
like a primary key in a table or the `key` prop on a list item. A pane, a tab, a space, a
browser surface: each *is* one entity, and on its own that entity is nothing but a number.

What an entity actually *is* comes entirely from the components you attach to it. Spawn one
and you get back its id:

```rust
let pane = commands.spawn(Pane).id();
```

## Component — your state

A component is **plain data** attached to an entity: a struct with no behavior. Think of the
typed columns of a database row, or a single `useState` slice lifted out of the component and
stored on its own.

```rust
#[derive(Component)]
pub struct Terminal;

#[derive(Component)]
pub struct Active;
```

Both are *tag* components — empty structs whose mere presence on an entity is the data
("this entity is a terminal", "this one is focused"). Components carry fields when there's
something to store, but they never carry logic.

Because state is just components you bolt on, **capabilities compose** instead of being
inherited. A plain web-view entity *becomes* a shell the moment you add a `Terminal`
component — no subclass, no base class, no `extends`. You add a capability; you don't inherit
one.

## System — your behavior

A system is an ordinary function that runs over **every entity matching a query**. It's your
`useEffect` or your reducer — except instead of being bound to one component instance, it runs
across the whole world at once.

```rust
fn focus_active(panes: Query<&Terminal, With<Active>>) {
    for terminal in &panes {
        // …
    }
}
```

Read the query out loud and it's obvious: *"for each entity that has a `Terminal` and is
`Active`, do this."* It's an `array.filter(…).forEach(…)` — and the engine calls it for you on
a schedule, tick after tick, so you never wire up the *when*.

## The world — your single store

Every entity and component lives in one **`World`**: a single Redux store, an in-memory
database. It is the one source of truth. Systems read from it and write to it; nothing else
holds state on the side. A query is your `array.filter(…)`; the world is the array.

## Messages — your actions

Systems don't call each other directly. To make something happen elsewhere, a system **sends
a message**, and another system reads it on a later tick:

```rust
#[derive(Message)]
pub struct TerminalSpawnRequest {
    pub target_stack: Option<Entity>,
}
```

That's `dispatch(action)` and the reducer that handles it — sender and receiver stay
decoupled, exactly like a Redux action and the slice that reacts to it.

## Why split data from behavior

Pulling state (components) apart from behavior (systems) buys two things for free:

- **Composition over inheritance** — every capability is a component you add, never a class
  you extend, so features stack instead of tangling.
- **Real parallelism, scheduled for you** — because each system declares the data it touches,
  Bevy runs every non-conflicting system across CPU cores at once. You describe *what* you
  need; the engine decides *when* it runs.

That's the whole model. For the side-by-side React / Redux → Rust mapping in real code, see
**[Rust for React JS developers](rust-without-the-headaches.md)**. To go deeper, the official
[Bevy guides](https://bevyengine.org/learn/) and the
[Bevy Cheat Book](https://bevy-cheatbook.github.io) cover it in an afternoon.
