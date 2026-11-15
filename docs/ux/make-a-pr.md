# Fix a bug and make a PR

Suppose a web app's login button stays disabled after entering an email address. A pull request (PR) is the code change you share for review. This example follows the problem from Browser to a PR you can merge.

## 1. Open the project and reproduce the problem

Choose the app's Project and folder. Open the running app in Browser, its files in Code, and a Terminal for its development commands. Follow the project's README to start the app if necessary.

Try the login flow and describe what you expected and what happened. Keep the page visible so you and the agent can refer to the same result.

## 2. Give the Session a focused task

> Fix the login button staying disabled after a valid email is entered. Reproduce the issue, make the change on a task branch, add a regression test, and run the project's checks. Show me the diff and the result before publishing it.

The project-management tools select the folder and prepare an isolated worktree when the code needs one. The Editor tools let the agent search, read, and show the relevant files. Git tracks the branch and changes; Terminal runs the app and tests.

Keep a small fix in one Session. For a larger change, use another Session for an independent review after the implementation is ready. Give that reviewer the problem, branch, and test results.

## 3. Review the result

Refresh the app in Browser and repeat the login flow. Check a nearby normal case too. Open Git to inspect the changed files and diff, and read the test output in Terminal or the Session.

If the result needs correction, explain it in the same Session. Ask the agent to save a useful explanation in Knowledge when it would help another task understand this app.

## 4. Create the PR

Once you accept the change, tell the agent:

> Commit this fix, push the task branch, and open a PR in this repository. Explain the original problem, the new behavior, and the checks that passed.

Use your own fork when that is where you want the change reviewed. The agent needs access to the selected repository and your normal Git hosting authentication. Review any requested authorization for publishing the branch.

## 5. Follow review and merge

Open the PR link in Browser. Read feedback and CI results. Return to the Session with any review changes, then review the updated diff and repeat the affected checks.

Merge when the result is accepted and the required checks pass. Keep the PR link with the Session so a future task can find the reason for the change.

See [Code and Git](code-and-git.md), [Terminal](terminal.md), and [Projects and Sessions](workspace.md). To change Vmux itself, start with [DX](../dx/getting-started.md).
