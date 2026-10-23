# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_bionic_reader_global_optspecs
    string join \n t/text= fraction= every= min-word-length= style= config= exclude= h/help V/version
end

function __fish_bionic_reader_needs_command
    # Figure out if the current invocation already has a command.
    set -l cmd (commandline -opc)
    set -e cmd[1]
    argparse -s (__fish_bionic_reader_global_optspecs) -- $cmd 2>/dev/null
    or return
    if set -q argv[1]
        # Also print the command, so this can be used to figure out what it is.
        echo $argv[1]
        return 1
    end
    return 0
end

function __fish_bionic_reader_using_subcommand
    set -l cmd (__fish_bionic_reader_needs_command)
    test -z "$cmd"
    and return 1
    contains -- $cmd[1] $argv
end

complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -s t -l text -d 'Transform this text instead of reading stdin' -r
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -l fraction -d 'Fraction of each selected word to emphasize (0 through 1)' -r
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -l every -d 'Emphasize the first eligible word and then every Nth word' -r
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -l min-word-length -d 'Minimum number of letters a word must have to receive emphasis' -r
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t''
always\t''
never\t''"
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -s h -l help -d 'Print help'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -s V -l version -d 'Print version'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "session" -d 'Start an interactive reading session'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "init" -d 'Print an opt-in initialization snippet for your shell configuration'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "completions" -d 'Print completion definitions for your shell'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "on" -d 'Enable emphasis in the current reading session'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "off" -d 'Disable emphasis in the current reading session'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "toggle" -d 'Toggle emphasis in the current reading session'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "reload" -d 'Reload settings at the next shell command boundary'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "bypass" -d 'Run a command without emphasis, restoring the prior state afterward'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l shell -d 'Bash, Zsh, or fish executable (defaults to SHELL)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l fraction -d 'Fraction of each selected word to emphasize (0 through 1)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l every -d 'Emphasize the first eligible word and then every Nth word' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l min-word-length -d 'Minimum number of letters a word must have to receive emphasis' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t''
always\t''
never\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -s h -l help -d 'Print help'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l fraction -d 'Fraction of each selected word to emphasize (0 through 1)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l every -d 'Emphasize the first eligible word and then every Nth word' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l min-word-length -d 'Minimum number of letters a word must have to receive emphasis' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t''
always\t''
never\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -s h -l help -d 'Print help'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l fraction -d 'Fraction of each selected word to emphasize (0 through 1)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l every -d 'Emphasize the first eligible word and then every Nth word' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l min-word-length -d 'Minimum number of letters a word must have to receive emphasis' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t''
always\t''
never\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -s h -l help -d 'Print help'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l fraction -d 'Fraction of each selected word to emphasize (0 through 1)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l every -d 'Emphasize the first eligible word and then every Nth word' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l min-word-length -d 'Minimum number of letters a word must have to receive emphasis' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t''
always\t''
never\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -s h -l help -d 'Print help'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l fraction -d 'Fraction of each selected word to emphasize (0 through 1)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l every -d 'Emphasize the first eligible word and then every Nth word' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l min-word-length -d 'Minimum number of letters a word must have to receive emphasis' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t''
always\t''
never\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -s h -l help -d 'Print help'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l fraction -d 'Fraction of each selected word to emphasize (0 through 1)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l every -d 'Emphasize the first eligible word and then every Nth word' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l min-word-length -d 'Minimum number of letters a word must have to receive emphasis' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t''
always\t''
never\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -s h -l help -d 'Print help'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l fraction -d 'Fraction of each selected word to emphasize (0 through 1)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l every -d 'Emphasize the first eligible word and then every Nth word' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l min-word-length -d 'Minimum number of letters a word must have to receive emphasis' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t''
always\t''
never\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -s h -l help -d 'Print help'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l fraction -d 'Fraction of each selected word to emphasize (0 through 1)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l every -d 'Emphasize the first eligible word and then every Nth word' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l min-word-length -d 'Minimum number of letters a word must have to receive emphasis' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t''
always\t''
never\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -s h -l help -d 'Print help'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from session init completions on off toggle reload bypass help" -f -a "session" -d 'Start an interactive reading session'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from session init completions on off toggle reload bypass help" -f -a "init" -d 'Print an opt-in initialization snippet for your shell configuration'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from session init completions on off toggle reload bypass help" -f -a "completions" -d 'Print completion definitions for your shell'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from session init completions on off toggle reload bypass help" -f -a "on" -d 'Enable emphasis in the current reading session'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from session init completions on off toggle reload bypass help" -f -a "off" -d 'Disable emphasis in the current reading session'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from session init completions on off toggle reload bypass help" -f -a "toggle" -d 'Toggle emphasis in the current reading session'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from session init completions on off toggle reload bypass help" -f -a "reload" -d 'Reload settings at the next shell command boundary'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from session init completions on off toggle reload bypass help" -f -a "bypass" -d 'Run a command without emphasis, restoring the prior state afterward'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from session init completions on off toggle reload bypass help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
