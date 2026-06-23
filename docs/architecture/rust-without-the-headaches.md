# Rust for React JS developers

> Part of the [Vmux Architecture](../architecture.md) overview.

If you write JavaScript or TypeScript, the word "Rust" probably conjures borrow-checker
fights, lifetimes, and FFI boilerplate. The surprise: in day-to-day feature work, almost
none of that shows up. The scary Rust lives deep in the engine — the code you write looks a
lot like the front-end you already write.

## The UI is React — in Rust

Vmux's own surfaces — header, command bar, settings, error pages — are built with
**[Dioxus](https://dioxuslabs.com)**, which ports React's model to Rust almost one-to-one:
function components, a JSX-like `rsx!` macro, and hooks.

```rust
#[component]
fn Counter(start: i32) -> Element {
    let mut count = use_signal(|| start);
    rsx! {
        button {
            class: "rounded bg-accent px-3 py-1",
            onclick: move |_| count += 1,
            "clicked {count} times"
        }
    }
}
```

If you've written a React function component, that reads exactly how you'd expect: state in
a hook, markup returned declaratively, a click handler that updates state — and the view
re-renders on its own. The names barely change:

| React | Dioxus (Rust) |
| :--- | :--- |
| JSX | the `rsx!` macro |
| function component → JSX | `fn` → `Element` |
| `useState` | `use_signal` |
| `useEffect` | `use_effect` |
| `useMemo` | `use_memo` |
| `useContext` | `use_context` |
| props object | typed `#[component]` arguments |
| `className` + Tailwind | `class:` + the same Tailwind utilities |

Same component-and-hook mental model, now type-checked end to end — no `undefined is not a
function`, no prop-shape drift. The toolkit (`crates/vmux_ui`) even mirrors **shadcn/ui** on
Dioxus primitives, so the dialogs, dropdowns, and popovers are the ones you already reach for.

## Coming from React? You already know this

Below the UI, the host is built on **Bevy**, a data-oriented **Entity-Component-System
(ECS)**. Squint and it's a shape you've seen:

- The **world** is one big store — a single Redux store, or an in-memory database.
- **Entities** are rows; **components** are typed columns (your state).
- **Systems** are functions that run over the rows matching a query (your effects/reducers).
- **Messages** are dispatched events (your actions).

| React / Redux / JS | Vmux (Bevy ECS, Rust) |
| :--- | :--- |
| `useState` / store slice | a **component** on an entity |
| `useEffect` / reducer | a **system** — runs when its data matches |
| `dispatch(action)` | send a **message** |
| `array.filter(…).map(…)` | a **query** over the world |
| single source of truth | the ECS **world** |
| `package.json` / npm | `Cargo.toml` / cargo |
| TS: caught at compile, not runtime | Rust: same — extended to memory + data races |

A system is just *"for each entity with `A` and `B`, do X"* — an `array.filter` that runs
every frame. Components are your state, systems your effects, messages your events; a web
dev ramps fast. New to ECS? The official [Bevy guides](https://bevyengine.org/learn/) and
the [Bevy Cheat Book](https://bevy-cheatbook.github.io) cover it in an afternoon.

## The borrow checker, where you'll actually meet it

"Will I fight the borrow checker?" — rarely. Feature work is querying components and sending
messages, and the compiler already proved that's safe. Ownership, lifetimes, and
`Arc<Mutex<T>>` live in engine plumbing, not in the systems you write. When the compiler
*does* stop you, read it like a TypeScript error you can't dismiss: it caught a real bug — a
data race, a dangling reference — before it ran, the kind that in JS surfaces as a
heisenbug in prod.

## Concurrent state without the locks

In JS, "concurrency" is an event loop and `await`: one thread, never truly parallel. Vmux
runs real parallelism — and you still don't manage it. A system declares the data it touches
(its queries and resources); the scheduler reads those declarations and runs every
non-conflicting system across cores at once. No `await` chains, no locks, no `Arc<Mutex<T>>`
in your code — you describe *what* you need, the engine schedules the *when*.

## Rust is the universal glue

Beyond the ECS, Rust is what lets one language span every boundary this app needs — over
near zero-cost FFI, no Node-style native-addon bridge:

- **C / C++** — CEF (Chromium) is driven over its C API.
- **JS / WASM** — the UI compiles to `wasm32` and binds to the browser via `wasm-bindgen`
  and `web-sys`; web pages talk back over JS.
- **Objective-C / AppKit** — native macOS surfaces (windowed CEF `NSView`s, the glass
  window, menus, tray) via `objc2`; Swift is reachable over the same C/Obj-C ABI.

The whole stack is one language end to end — not a polyglot pile glued together by services.
