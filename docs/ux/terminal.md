# Terminal

Use Terminal to run the project's existing commands while keeping their output beside the code, Browser, or Session that needs it.

## Run in the right checkout

Open a terminal in the task's Space and confirm its working directory before running a build, test, or mutation. Give independent tasks their own terminal/worktree context when they need different branches.

A common loop is to start the development server, inspect its page in Browser, edit Code, and watch the output. Another is to run a targeted test beside the diff the agent prepared.

## Observe ongoing work

Terminal pages display and control processes owned by the background service. Moving to another page or hiding the view does not recreate the process. Return to the same work to inspect its output or continue using it.

Use an explicit stop/shutdown action when a process should end. Closing a view alone does not necessarily stop its background work.

## Share a useful result

A command's result should support the Session's goal: a passing test, a reproduced error, a running service, or a prepared artifact. Check the exit status and the relevant output before treating a command as successful.

Keep build/test invocations sequential when they share one workspace's output or locks. Review destructive commands and permission requests in the current checkout's context.

See [Get something done](getting-things-done.md) for task checkpoints and [Validation and release](../dx/validation-and-release.md) for Vmux development commands.
