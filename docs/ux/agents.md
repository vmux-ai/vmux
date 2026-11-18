# Agents

An agent is an assistant you can give a task: research a trip, compare products, plan a meal, or fix software. A Session keeps the conversation and its results together.

## Start a conversation

Select an available agent for a new Session. If it needs setup or sign-in, complete that first. Then describe the result you want and the details that matter.

Development profiles using an isolated data directory keep app settings and provider configuration inside that directory. Codex uses its private `.codex` home, Claude Code uses private configuration and secure-storage directories, and Vibe uses `.vibe`. Explicit agent environment overrides cannot redirect those paths to personal configuration.

Vibe 3.1.2 uses an owner-only plaintext `.env` in its private home when the native keyring is disabled for isolation. This provider-specific fallback also disables experimental OIDC keyring persistence. Codex's credential backend remains configurable, and Claude Code still uses its native keychain backend with configuration-derived service names; these settings do not imply that every provider avoids native keychain access.

For shopping, include the budget and delivery country. For cooking, include ingredients, portions, and dietary requirements. For software, select the Project and describe how to reproduce the problem.

## Let tools support the task

Connected agents can use the available tools alongside you: project management to choose the working context, Browser for sources, Knowledge for saved notes, Editor for local files, Git for changes, and Terminal for commands.

Keep those pages beside the Session when you want to inspect the work. You can read the source, edit a note, inspect a diff, or take over a terminal yourself.

## Steer and review

Follow up in the same Session with a correction or extra constraint. For example: “Include baggage in the flight price,” or “Use the ingredients I already have.”

Review requests to use capabilities or perform an action in the context of the task. Before a booking, purchase, or published code change, inspect the completed details or diff and authorize the action you want.

## Continue later

Return to the same Session for a follow-up. Start another for a separate task or independent review, and keep related Sessions in the same Project. Ask the agent to save a useful result in Knowledge when another task should reuse it.

See [the task examples](getting-things-done.md), [Knowledge](knowledge.md), and [Projects and Sessions](workspace.md).
