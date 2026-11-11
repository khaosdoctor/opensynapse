# fish completion for opensynapse
# Button names and lighting zones come from the connected mouse (read-only).

function __opensynapse_buttons
    opensynapse list 2>/dev/null | string replace -rf '^\s*buttons: ' '' | string split ' '
end

function __opensynapse_zones
    echo all
    opensynapse rgb 2>/dev/null | string match -rv '^via' | string replace -r '\s.*' ''
end

# the command and its arguments typed so far, skipping flags and their values
function __opensynapse_args
    set -l skip 0
    for w in (commandline -opc)[2..]
        if test $skip -eq 1
            set skip 0
            continue
        end
        switch $w
            case --profile --layer --device --active
                set skip 1
            case '-*'
            case '*'
                echo $w
        end
    end
end

# true when the command is $cmd and $n arguments have been typed after it
function __opensynapse_at -a cmd n
    set -l a (__opensynapse_args)
    test "$a[1]" = $cmd; and test (math (count $a) - 1) -eq $n
end

function __opensynapse_no_command
    set -l a (__opensynapse_args)
    test (count $a) -eq 0
end

complete -c opensynapse -f

complete -c opensynapse -n __opensynapse_no_command -a init -d 'start a device file for a mouse that has none'
complete -c opensynapse -n __opensynapse_no_command -a probe -d 'print the read commands a hidraw node answers'
complete -c opensynapse -n __opensynapse_no_command -a list -d 'connected mice and their buttons'
complete -c opensynapse -n __opensynapse_no_command -a dump -d "every button's action"
complete -c opensynapse -n __opensynapse_no_command -a get -d "one button's action"
complete -c opensynapse -n __opensynapse_no_command -a set -d 'remap a button'
complete -c opensynapse -n __opensynapse_no_command -a dpi -d 'show or set DPI'
complete -c opensynapse -n __opensynapse_no_command -a stages -d 'show or set DPI stages'
complete -c opensynapse -n __opensynapse_no_command -a watch -d 'print buttons as you press them'
complete -c opensynapse -n __opensynapse_no_command -a rgb -d 'lighting zones (needs OpenRazer)'
complete -c opensynapse -n __opensynapse_no_command -a export -d 'save a profile to JSON'
complete -c opensynapse -n __opensynapse_no_command -a import -d 'write a saved profile to the mouse'
complete -c opensynapse -n __opensynapse_no_command -a protocol -d 'generate a protocol file for this mouse'
complete -c opensynapse -n __opensynapse_no_command -a help -d 'show the help'

complete -c opensynapse -l profile -x -a '0 1 2 3 4 5' -d 'profile slot'
complete -c opensynapse -l layer -x -a 'normal hypershift' -d 'button layer'
complete -c opensynapse -l device -x -d 'mouse PID, like 008d'
complete -c opensynapse -l active -x -a '1 2 3 4 5' -d 'active DPI stage'
complete -c opensynapse -l force -d 'overwrite an existing file'
complete -c opensynapse -l dry-run -d 'print the issue instead of opening it'
complete -c opensynapse -l yes -d 'skip the confirmation'

complete -c opensynapse -n '__opensynapse_at get 0' -a '(__opensynapse_buttons)'
complete -c opensynapse -n '__opensynapse_at set 0' -a '(__opensynapse_buttons)'
complete -c opensynapse -n '__opensynapse_at set 1' -a 'key: turbo: keyturbo: raw: disabled hypershift
    mouse:left mouse:right mouse:middle mouse:back mouse:forward mouse:wheelup mouse:wheeldown mouse:wheelleft mouse:wheelright
    media:playpause media:play media:pause media:next media:prev media:stop media:mute media:volumeup media:volumedown
    dpi:up dpi:down dpi:up-wrap dpi:down-wrap profile:next profile:prev profile:next-wrap profile:prev-wrap'
complete -c opensynapse -n '__opensynapse_at rgb 0' -a '(__opensynapse_zones)'
complete -c opensynapse -n '__opensynapse_at rgb 1' -a 'off static breath spectrum wave reactive brightness'
complete -c opensynapse -n '__opensynapse_at export 0; or __opensynapse_at import 0; or __opensynapse_at init 0' -F
complete -c opensynapse -n '__opensynapse_at protocol 0' -a send -F
complete -c opensynapse -n '__opensynapse_at probe 0' -a '(__fish_complete_path /dev/hidraw)'
