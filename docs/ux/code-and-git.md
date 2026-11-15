# Code and Git

Use Code to inspect and edit the checkout for the task. Use Git to understand what changed and prepare a reviewable result.

## Start from the Project

Open Code from the Project or its Space. Vmux selects a useful README or entry file before falling back to directory navigation. Keep the active checkout aligned with the task; a preview of another checkout remains read-only until activation.

Use the file view, search, and the available language support to find the behavior being changed. Keep a Browser reference or agent Session beside the relevant code instead of copying all context into the prompt.

## Make a focused change

For an agent task, specify the expected behavior and let the Session produce a diff you can inspect. For manual work, use Terminal for the project's build and test commands. Separate independent code tasks into worktrees and Sessions with clear ownership of their result.

Draft edits, selection, and reading position should survive refreshes and navigation. Review unsaved work before switching or closing a surface.

## Review with Git

Inspect repository status and the diff in the Git view. Check that the changed files fit the goal and that the appropriate validation ran. Compare the result with the original acceptance criteria, including visible behavior when the task concerns interaction.

Use the project's normal commit and PR workflow to share the change. Keep validation results and the PR associated with the Session so a follow-up or review starts from the same context.

For changing Vmux itself, follow [Getting started](../dx/getting-started.md) and [Feature development](../dx/feature-development.md).
