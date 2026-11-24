function __vmux_pe --on-event fish_preexec; printf '\033]2;%s\007\033]133;C\007' $argv[1]; end
function __vmux_pc --on-event fish_postexec
    set -l s $status
    set -l p (string replace -a '%' '%25' -- $PWD | string replace -a '\\' '%5C' | string replace -a '#' '%23' | string replace -a '?' '%3F')
    printf '\033]133;D;%s\007\033]7;file://localhost%s\007' $s $p
end
__vmux_pc
