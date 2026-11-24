$env.config.shell_integration.osc133 = false
$env.config.hooks.pre_execution = ($env.config.hooks.pre_execution? | default [] | append {|| print -rn $"\u{1b}]2;(commandline)\u{7}\u{1b}]133;C\u{7}" })
$env.config.hooks.pre_prompt = ($env.config.hooks.pre_prompt? | default [] | append {|| print -rn $"\u{1b}]133;D;($env.LAST_EXIT_CODE)\u{7}\u{1b}]7;file://localhost($env.PWD | str replace --all '%' '%25' | str replace --all '\' '%5C' | str replace --all '#' '%23' | str replace --all '?' '%3F')\u{7}" })
