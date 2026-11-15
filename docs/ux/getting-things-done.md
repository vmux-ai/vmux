# Get something done

Start with the result you want: fix a bug, investigate a project, ship a feature, or produce an answer you can review. Vmux keeps the project, agent conversations, browser references, code, and commands together as that goal becomes work.

## 1. Choose the Project

Open or create the Project for the relevant repository or working directory. A Project groups related work; its sessions remain discoverable when you leave and return. Use the Space associated with that Project to keep the right checkout and pages in view.

For research, keep the source pages and any notes beside the conversation. For software work, open Code, Git, and Terminal against the checkout the task should change.

## 2. Give a Session a clear outcome

Start a Session with an ACP agent and describe the goal, relevant context, constraints, and what would count as done. For example: “Find why this checkout fails the login test, make a focused fix, and show the test result.”

Keep a coherent investigation and small fix in one Session. Split work when parts can progress independently or need different checkouts, agents, or review context. A useful split for a feature is investigation, implementation, and validation/review. Start dependent work when its inputs are ready rather than asking every Session to rediscover the same context.

## 3. Work in parallel deliberately

Give each Session a specific task and expected output. Keep related Sessions in the same Project, while using separate Spaces or worktrees for independent code changes. A background agent stays anchored to its own working directory even when you view another Space.

One Session can inspect behavior in the Browser while another prepares a code change. Review the investigation before implementation relies on it, and share the relevant conclusion or artifact with the next task.

## 4. Inspect and steer

Read agent activity and results, inspect the code/diff, and run the appropriate commands in Terminal. Review permission requests in their task context. Ask for a narrower change or explain a new constraint in the same Session when the goal has not changed.

Keep important references and the running product visible beside the work. A useful checkpoint is an observable result: an explanation with sources, a focused diff, a passing test, or a prepared app build.

## 5. Review the outcome

For a code task, review the diff and validation, try the prepared app when needed, and decide whether to create or merge a PR. For research, check the sources and whether the answer addresses the original question. Retain the Session with its Project so another task can start from the accepted result.

Continue with [Workspace](workspace.md), [Browser](browser.md), [Code and Git](code-and-git.md), [Terminal](terminal.md), or [Agents](agents.md).
