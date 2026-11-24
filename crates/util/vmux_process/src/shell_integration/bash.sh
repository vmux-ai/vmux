[ -r "$HOME/.bashrc" ] && . "$HOME/.bashrc"
if [[ $- == *i* ]]; then
  __vmux_at_prompt=1
  __vmux_preexec() { [ -n "$__vmux_at_prompt" ] || return; __vmux_at_prompt=; printf '\033]2;%s\007\033]133;C\007' "$1"; }
  __vmux_precmd() { local s=$? p=$PWD; p=${p//%/%25}; p=${p//\\/%5C}; p=${p//#/%23}; p=${p//\?/%3F}; printf '\033]133;D;%s\007\033]7;file://localhost%s\007' "$s" "$p"; __vmux_at_prompt=1; }
  trap '__vmux_preexec "$BASH_COMMAND"' DEBUG
  PROMPT_COMMAND="${PROMPT_COMMAND:+$PROMPT_COMMAND;}"'__vmux_precmd'
fi
