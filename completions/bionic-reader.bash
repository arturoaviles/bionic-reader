_bionic__reader() {
    local i cur prev opts cmd
    COMPREPLY=()
    if [[ "${BASH_VERSINFO[0]}" -ge 4 ]]; then
        cur="$2"
    else
        cur="${COMP_WORDS[COMP_CWORD]}"
    fi
    prev="$3"
    cmd=""
    opts=""

    for i in "${COMP_WORDS[@]:0:COMP_CWORD}"
    do
        case "${cmd},${i}" in
            ",$1")
                cmd="bionic__reader"
                ;;
            bionic__reader,bypass)
                cmd="bionic__reader__subcmd__bypass"
                ;;
            bionic__reader,completions)
                cmd="bionic__reader__subcmd__completions"
                ;;
            bionic__reader,help)
                cmd="bionic__reader__subcmd__help"
                ;;
            bionic__reader,init)
                cmd="bionic__reader__subcmd__init"
                ;;
            bionic__reader,off)
                cmd="bionic__reader__subcmd__off"
                ;;
            bionic__reader,on)
                cmd="bionic__reader__subcmd__on"
                ;;
            bionic__reader,reload)
                cmd="bionic__reader__subcmd__reload"
                ;;
            bionic__reader,session)
                cmd="bionic__reader__subcmd__session"
                ;;
            bionic__reader,toggle)
                cmd="bionic__reader__subcmd__toggle"
                ;;
            bionic__reader__subcmd__help,bypass)
                cmd="bionic__reader__subcmd__help__subcmd__bypass"
                ;;
            bionic__reader__subcmd__help,completions)
                cmd="bionic__reader__subcmd__help__subcmd__completions"
                ;;
            bionic__reader__subcmd__help,help)
                cmd="bionic__reader__subcmd__help__subcmd__help"
                ;;
            bionic__reader__subcmd__help,init)
                cmd="bionic__reader__subcmd__help__subcmd__init"
                ;;
            bionic__reader__subcmd__help,off)
                cmd="bionic__reader__subcmd__help__subcmd__off"
                ;;
            bionic__reader__subcmd__help,on)
                cmd="bionic__reader__subcmd__help__subcmd__on"
                ;;
            bionic__reader__subcmd__help,reload)
                cmd="bionic__reader__subcmd__help__subcmd__reload"
                ;;
            bionic__reader__subcmd__help,session)
                cmd="bionic__reader__subcmd__help__subcmd__session"
                ;;
            bionic__reader__subcmd__help,toggle)
                cmd="bionic__reader__subcmd__help__subcmd__toggle"
                ;;
            *)
                ;;
        esac
    done

    case "${cmd}" in
        bionic__reader)
            opts="-t -h -V --text --fraction --every --min-word-length --style --config --exclude --help --version session init completions on off toggle reload bypass help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 1 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --text)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -t)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --fraction)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --every)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --min-word-length)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --style)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --exclude)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        bionic__subcmd__reader__subcmd__bypass)
            opts="-h --fraction --every --min-word-length --style --config --exclude --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --fraction)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --every)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --min-word-length)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --style)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --exclude)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        bionic__subcmd__reader__subcmd__completions)
            opts="-h --fraction --every --min-word-length --style --config --exclude --help bash zsh fish"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --fraction)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --every)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --min-word-length)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --style)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --exclude)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        bionic__subcmd__reader__subcmd__help)
            opts="session init completions on off toggle reload bypass help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        bionic__subcmd__reader__subcmd__help__subcmd__bypass)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        bionic__subcmd__reader__subcmd__help__subcmd__completions)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        bionic__subcmd__reader__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        bionic__subcmd__reader__subcmd__help__subcmd__init)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        bionic__subcmd__reader__subcmd__help__subcmd__off)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        bionic__subcmd__reader__subcmd__help__subcmd__on)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        bionic__subcmd__reader__subcmd__help__subcmd__reload)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        bionic__subcmd__reader__subcmd__help__subcmd__session)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        bionic__subcmd__reader__subcmd__help__subcmd__toggle)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        bionic__subcmd__reader__subcmd__init)
            opts="-h --fraction --every --min-word-length --style --config --exclude --help bash zsh fish"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --fraction)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --every)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --min-word-length)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --style)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --exclude)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        bionic__subcmd__reader__subcmd__off)
            opts="-h --fraction --every --min-word-length --style --config --exclude --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --fraction)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --every)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --min-word-length)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --style)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --exclude)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        bionic__subcmd__reader__subcmd__on)
            opts="-h --fraction --every --min-word-length --style --config --exclude --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --fraction)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --every)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --min-word-length)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --style)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --exclude)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        bionic__subcmd__reader__subcmd__reload)
            opts="-h --fraction --every --min-word-length --style --config --exclude --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --fraction)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --every)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --min-word-length)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --style)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --exclude)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        bionic__subcmd__reader__subcmd__session)
            opts="-h --shell --fraction --every --min-word-length --style --config --exclude --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --shell)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --fraction)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --every)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --min-word-length)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --style)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --exclude)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        bionic__subcmd__reader__subcmd__toggle)
            opts="-h --fraction --every --min-word-length --style --config --exclude --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --fraction)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --every)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --min-word-length)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --style)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --exclude)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
    esac
}

if [[ "${BASH_VERSINFO[0]}" -eq 4 && "${BASH_VERSINFO[1]}" -ge 4 || "${BASH_VERSINFO[0]}" -gt 4 ]]; then
    complete -F _bionic__reader -o nosort -o bashdefault -o default bionic-reader
else
    complete -F _bionic__reader -o bashdefault -o default bionic-reader
fi
