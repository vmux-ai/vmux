# Agents

An agent is an assistant you can give a task: research a trip, compare products, plan a meal, or fix software. A Session keeps the conversation and its results together.

## Start a conversation

Select an available agent for a new Session. Opening setup from Tools selects that agent without starting sign-in; an unfinished setup keeps its current agent and conversation. If it needs setup or sign-in, complete that first. Setup progress stays in the conversation. The normal composer remains visible with its message input and attachment control disabled until setup finishes; its send position offers Continue, Stop, or Resume as the current action. Choose 1. Connect to sign in through the provider page, or 2. Use an API key, then Continue. Press 1 or 2 to select a method. Confirmed choices appear as your messages in the conversation and remain visible when you return; an unfinished API-key response reopens at the same step. Provider sign-in opens a compact popup showing the site favicon and domain. Stop pauses active setup; Resume continues the current step. Then describe the result you want and the details that matter.

For shopping, include the budget and delivery country. For cooking, include ingredients, portions, and dietary requirements. For software, select the Project and describe how to reproduce the problem.

## Isolated development profiles

Fresh development profiles keep Codex, Claude Code, and Mistral Vibe configuration in their own tool home. Agent-provided environment overrides cannot redirect Codex or Claude Code configuration and credential directories to a personal directory. Their native credential-store behavior remains controlled by the provider CLI; Claude Code still uses macOS Keychain with a name derived from its isolated credential directory. Vibe stores its API key in an owner-only local file instead of the personal system keychain. Restarting the same profile retains its local state; another fresh profile starts separately. This development isolation does not change ordinary sign-in storage.

## Let tools support the task

Connected agents can use the available tools alongside you: project management to choose the working context, Browser for sources, Knowledge for saved notes, Editor for local files, Git for changes, and Terminal for commands.

Keep those pages beside the Session when you want to inspect the work. You can read the source, edit a note, inspect a diff, or take over a terminal yourself.

## Steer and review

Follow up in the same Session with a correction or extra constraint. For example: “Include baggage in the flight price,” or “Use the ingredients I already have.”

Review requests to use capabilities or perform an action in the context of the task. Before a booking, purchase, or published code change, inspect the completed details or diff and authorize the action you want.

## Continue later

Return to the same Session for a follow-up. Start another for a separate task or independent review, and keep related Sessions in the same Project. Ask the agent to save a useful result in Knowledge when another task should reuse it.

See [the task examples](getting-things-done.md), [Knowledge](knowledge.md), and [Projects and Sessions](workspace.md).
