# Terminal

Terminal runs commands on your computer and keeps their output beside the task. Use it when you need to start a development server, run a test, or work with a command-line tool.

## Run and watch a command

Open Terminal in the Project's working folder. Use the project's normal instructions for the command you need. A connected agent can also run a command in a visible terminal, where you can follow it or take over.

For a web-app change, keep Terminal's development server beside Browser and Code. For a test, read the result and any failure details before treating the change as finished.

## Return to a running process

Moving to another page does not recreate the process. Return to the same terminal to see its output or continue using it. Use an explicit stop action when the process should end; closing its view alone does not necessarily stop the work.

## Connect the output to the task

Ask the agent to explain a failure or use the output in its next step. Run builds and tests one at a time when they share the same project output directory.

See [Fix a bug and make a PR](make-a-pr.md) and [DX build and test commands](../dx/validation-and-release.md).
