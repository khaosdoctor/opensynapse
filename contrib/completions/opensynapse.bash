# bash completion for opensynapse
# Button names and lighting zones come from the connected mouse (read-only).

_opensynapse_buttons() {
    opensynapse list 2>/dev/null | sed -n 's/^ *buttons: //p'
}

_opensynapse_zones() {
    echo all
    opensynapse rgb 2>/dev/null | awk '!/^via/ { print $1 }'
}

_opensynapse_actions="key: turbo: keyturbo: raw: disabled hypershift
    mouse:left mouse:right mouse:middle mouse:back mouse:forward mouse:wheelup mouse:wheeldown mouse:wheelleft mouse:wheelright
    media:playpause media:play media:pause media:next media:prev media:stop media:mute media:volumeup media:volumedown
    dpi:up dpi:down dpi:up-wrap dpi:down-wrap profile:next profile:prev profile:next-wrap profile:prev-wrap"

_opensynapse() {
    local cur prev words cword
    _init_completion -n : || return

    case $prev in
        --profile) COMPREPLY=($(compgen -W "0 1 2 3 4 5" -- "$cur")); return ;;
        --layer) COMPREPLY=($(compgen -W "normal hypershift" -- "$cur")); return ;;
        --active) COMPREPLY=($(compgen -W "1 2 3 4 5" -- "$cur")); return ;;
        --device | --args | --tid) return ;;
    esac

    if [[ $cur == -* ]]; then
        COMPREPLY=($(compgen -W "--profile --layer --device --active --args --tid --force --dry-run --yes" -- "$cur"))
        return
    fi

    # the command and its arguments, skipping flags and their values
    local i cmd="" args=()
    for ((i = 1; i < cword; i++)); do
        case ${words[i]} in
            --profile | --layer | --device | --active | --args | --tid) ((i++)) ;;
            -*) ;;
            *) if [[ -z $cmd ]]; then cmd=${words[i]}; else args+=("${words[i]}"); fi ;;
        esac
    done

    case $cmd in
        "") COMPREPLY=($(compgen -W "init probe list dump get set dpi stages watch rgb poll export import protocol help" -- "$cur")) ;;
        poll) ((${#args[@]} == 0)) && COMPREPLY=($(compgen -W "125 500 1000" -- "$cur")) ;;
        probe) ((${#args[@]} == 0)) && COMPREPLY=($(compgen -W "$(compgen -G '/dev/hidraw*')" -- "$cur")) ;;
        get) ((${#args[@]} == 0)) && COMPREPLY=($(compgen -W "$(_opensynapse_buttons)" -- "$cur")) ;;
        set)
            case ${#args[@]} in
                0) COMPREPLY=($(compgen -W "$(_opensynapse_buttons)" -- "$cur")) ;;
                1)
                    COMPREPLY=($(compgen -W "$_opensynapse_actions" -- "$cur"))
                    # prefixes like key: get completed without a trailing space
                    [[ ${#COMPREPLY[@]} -eq 1 && ${COMPREPLY[0]} == *: ]] && compopt -o nospace
                    __ltrim_colon_completions "$cur"
                    ;;
            esac
            ;;
        rgb)
            case ${#args[@]} in
                0) COMPREPLY=($(compgen -W "$(_opensynapse_zones)" -- "$cur")) ;;
                1) COMPREPLY=($(compgen -W "off static breath spectrum wave reactive brightness" -- "$cur")) ;;
            esac
            ;;
        export | import) _filedir json ;;
        init) _filedir jsonc ;;
        protocol)
            ((${#args[@]} == 0)) && COMPREPLY=($(compgen -W "send" -- "$cur"))
            _filedir md
            ;;
    esac
}

complete -F _opensynapse opensynapse
