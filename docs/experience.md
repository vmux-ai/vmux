# Vmux — UX Philosophy

The line between consuming and creating is blurring. AI agents are turning professionals
into generalists — giving anyone with an idea and tokens the power not just to search,
but to build software.

Modern tools have split into two camps: chat-first productivity hubs on one side, and
developer IDEs (like Cursor) on the other. **Vmux bridges the divide by blending both
worlds.**

- **Co-working** — an agent-first workspace, natively collaborative with humans. People
  and agents work, build, and orchestrate tasks alongside each other, in real time, in one
  shared space — from hands-on pairing to full autonomy. Watch a run and grab the keyboard
  to steer, or turn agents loose in their own space: find your own balance, and let it shift
  as you trust agents more.
- **Known by heart** — it looks and acts like a standard web browser. No learning curve;
  everyone already knows how to use it.
- **IDE power** — beneath the surface, tech-savvy users get advanced tools,
  keyboard-driven workflows, and deep environment control.

Vmux brings the efficiency of a high-end developer workspace to the everyday web.

For how it's built, see [Architecture](architecture.md).

---

## Layout: browser simplicity, tmux power

At first glance Vmux is the browser you expect — clean, familiar, intuitive. Underneath
the standard exterior sits a flexible UI system inspired by **tmux**.

Instead of being trapped in rigid tabs, your workspace is completely malleable: split
horizontally or vertically, stack views, and tile panes to fit your workflow. A
side-by-side comparison or a complex multi-pane dashboard — Vmux can represent any
layout you imagine.

---

## Input: talk, type, click

Vmux orders interaction from abstract delegation down to mechanical control.

1. **Agent prompts — type or talk.** *First priority.* Direct the whole workspace in
   natural language. **Type** for silent, high-precision tasks; **talk** for hands-free
   speed and live layout manipulation.
2. **Keyboard shortcuts.** *Second priority.* High-velocity control with near-zero
   learning curve, split intentionally to avoid friction:
   - **Chrome-style** — standard browser actions (open tabs, navigate history, refresh)
     use the native shortcuts you already know by heart.
   - **Tmux-style** — layout commands (split panes, switch windows, cycle layouts) use a
     powerful `<leader>`-prefixed scheme built for terminal efficiency.
3. **Mouse.** *Third priority.* Plain, intuitive point-and-click that keeps Vmux
   grounded in predictable browser behavior.

---

## Platform: more OS than app

The same shift that turns people into generalists is happening to the app itself. Vmux is
less a single-purpose app than an OS-like layer for everything you do — and like an OS, it's
built to live wherever you do: desktop, phone, AR/VR, wearables. What carries across is Vmux
itself, not a one-size-fits-all interface — the same workspace and agents, reshaped to the
device in front of you.

So the experience adapts instead of flattening. A desktop leans on the keyboard and dense
tiling; a phone puts touch and voice first, with input priorities tuned to the form factor.
Each platform honors its own conventions too, so Vmux feels native everywhere — never a
port stretched across screens it wasn't built for.

Today it runs on **macOS** (the lead platform) and **Linux**, with a portable core ready to
follow you onto the rest.
