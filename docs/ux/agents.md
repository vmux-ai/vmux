# Agents

Use an ACP-compatible agent in a Session to investigate, implement, explain, or review a task. Vmux keeps that conversation beside the project surfaces you can inspect and use yourself.

## Give the task a reviewable outcome

State the goal, context, constraints, and expected result. A focused task such as “explain this failure and prepare a tested fix” is easier to steer and review than a collection of unrelated requests.

The Session's working directory anchors its relative operations. An agent running in the background does not switch projects because you focus another window or Space.

## Divide a larger goal

Keep related Sessions in one Project. Split independent tasks or reviews into separate Sessions, and provide the accepted outputs that dependent tasks need. Separate implementation worktrees where tasks would otherwise edit the same checkout.

Use Sessions as durable task context, not just disposable prompts. Follow up in the same conversation when you want a correction or extension of its result.

## Inspect activity and permissions

Read the agent's activity, inspect tool results, and review the diff or artifact. You can keep Browser, Code, and Terminal beside the Session to reproduce a problem or verify its proposed result.

Capability access is explicit and belongs to the agent/task scope. A visible approval count is feedback, not authorization. Review requests for the action and context that need them. Agent-native configuration remains owned by that agent rather than being silently copied into another configuration.

## Finish or continue the work

Review the outcome against the original goal and its validation. For code, inspect the diff and try the prepared product when needed before publishing or merging. Keep the Session linked to its Project and use its result as context for the next task.

See [Get something done](getting-things-done.md) for the complete flow and [Remote access](remote.md) for returning to work from the phone.
