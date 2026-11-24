ZDOTDIR="${__VMUX_ZDOTDIR_ORIG:-$HOME}"
[ -r "$ZDOTDIR/.zshrc" ] && source "$ZDOTDIR/.zshrc"
autoload -Uz add-zsh-hook 2>/dev/null
__vmux_pe() { printf '\033]2;%s\007\033]133;C\007' "$1" }
__vmux_pc() { local s=$? p=$PWD; p=${p//%/%25}; p=${p//\\/%5C}; p=${p//#/%23}; p=${p//\?/%3F}; printf '\033]133;D;%s\007\033]7;file://localhost%s\007' "$s" "$p" }
add-zsh-hook preexec __vmux_pe 2>/dev/null
add-zsh-hook precmd __vmux_pc 2>/dev/null
