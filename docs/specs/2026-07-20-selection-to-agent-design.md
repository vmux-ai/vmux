# Selection to Agent

## Goal

Let the user select text or an item in any page and press `Command+Shift+L` to attach that
selection to the current agent composer.

## Interaction

- `Command+Shift+L` on macOS and `Ctrl+Shift+L` elsewhere runs **Add Selection to Agent**.
- The command targets the most recently active agent in the current tab.
- If the current tab has no agent, vmux opens the default page agent in the current pane.
- The agent composer receives a source-aware context chip and keyboard focus.
- The shortcut never submits the prompt.
- Repeated invocations append context chips.
- With no selection, the command only focuses or opens the agent.

## Context model

Each captured context is an immutable snapshot containing:

- source kind;
- short label;
- source URL or path;
- selected text.

File selections include line ranges. Terminal selections include the terminal title and working
directory. Browser selections include the page title and URL.

The composer renders context separately from the editable prompt. Removing a chip excludes it
from submission. Context is sent through the private agent context field, keeping the visible
user message limited to text the user typed.

## Capture adapters

| Page | Capture |
| --- | --- |
| Terminal | Materialize the terminal selection through the service. |
| File editor | Read the native editor selection and preserve line numbers. |
| Diff and directory | Read the DOM selection. |
| Browser and internal pages | Read the DOM or focused text-control selection. |
| Agent transcript | Read the DOM selection and attach it back to the agent. |

Password controls are never captured. Selection text is capped with an explicit truncation
marker rather than silently cut.

## Target behavior

Page and ACP agents receive structured context chips. CLI agents receive the same context rendered
into their prompt without submission. Targeting stays inside the source tab to avoid surprising
cross-workspace jumps.

## Deferred

- Floating selection toolbar.
- Context-menu action.
- Jump-back navigation from chips.
- Cross-origin iframe selection.
- Multi-item directory selection.
