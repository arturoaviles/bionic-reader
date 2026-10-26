# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_bionic_reader_global_optspecs
    string join \n t/text= fraction= every= min-word-length= style= config= exclude= profile= length-rule= emphasis= input-format= format= controls= skip-code= low-latency= h/help V/version
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
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t'Enable emphasis when output is a terminal'
always\t'Enable emphasis regardless of the output destination'
never\t'Disable added emphasis; configured format and input-control processing still apply'"
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -l profile -d 'Named fixation preference; explicit numeric flags override its values' -r -f -a "subtle\t'Emphasize fewer, longer words with a shorter prefix'
balanced\t'Emphasize half of each eligible word'
strong\t'Emphasize a longer prefix of every eligible word'"
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -l length-rule -d 'Per-length fraction in MAX:FRACTION form; repeat in increasing MAX order' -r
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -l emphasis -d 'Emphasis attribute: bold, underline, or reverse video' -r -f -a "bold\t'Use bold intensity, restoring the original bold and faint attributes'
underline\t'Use underlining without changing text colors or intensity'
reverse\t'Use reverse video without selecting a brighter foreground color'"
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -l input-format -d 'Interpret ordinary text, Markdown source, or trusted terminal output' -r -f -a "text\t'Ordinary text with the configured terminal-control policy'
markdown\t'`CommonMark` source with protected syntax and code spans'
terminal\t'Trusted terminal output whose existing controls must be preserved'"
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -l format -d 'Emit ANSI, unstyled plain text, Markdown emphasis, or escaped HTML' -r -f -a "ansi\t'Terminal emphasis, subject to the style policy'
plain\t'Original text without terminal escape sequences or added emphasis'
markdown\t'Markdown source with emphasis syntax around selected prefixes'
html\t'Escaped HTML with semantic emphasis elements'"
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -l controls -d 'Remove active document controls, or preserve trusted terminal controls' -r -f -a "safe\t'Permit presentation styling and safe hyperlinks; remove active controls'
preserve\t'Preserve terminal control bytes for trusted sources'"
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -l skip-code -d 'Protect Markdown code from emphasis (default true); use --skip-code=false to include it' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -l low-latency -d 'Flush stalled partial words unchanged after 50 ms; disables content-only determinism' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -s V -l version -d 'Print version'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "read" -d 'Read a document with scrolling, search, and adjustable fixation settings'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "preview" -d 'Preview fixation settings on a sample passage or supplied text'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "session" -d 'Start an interactive reading session'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "init" -d 'Print an opt-in initialization snippet for your shell configuration'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "completions" -d 'Print completion definitions for your shell'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "on" -d 'Enable emphasis in the current reading session'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "off" -d 'Disable emphasis in the current reading session'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "toggle" -d 'Toggle emphasis in the current reading session'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "reload" -d 'Reload settings at the next shell command boundary'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "bypass" -d 'Run a command without emphasis, restoring the prior state afterward'
complete -c bionic-reader -n "__fish_bionic_reader_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand read" -l fraction -d 'Fraction of each selected word to emphasize (0 through 1)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand read" -l every -d 'Emphasize the first eligible word and then every Nth word' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand read" -l min-word-length -d 'Minimum number of letters a word must have to receive emphasis' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand read" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t'Enable emphasis when output is a terminal'
always\t'Enable emphasis regardless of the output destination'
never\t'Disable added emphasis; configured format and input-control processing still apply'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand read" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand read" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand read" -l profile -d 'Named fixation preference; explicit numeric flags override its values' -r -f -a "subtle\t'Emphasize fewer, longer words with a shorter prefix'
balanced\t'Emphasize half of each eligible word'
strong\t'Emphasize a longer prefix of every eligible word'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand read" -l length-rule -d 'Per-length fraction in MAX:FRACTION form; repeat in increasing MAX order' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand read" -l emphasis -d 'Emphasis attribute: bold, underline, or reverse video' -r -f -a "bold\t'Use bold intensity, restoring the original bold and faint attributes'
underline\t'Use underlining without changing text colors or intensity'
reverse\t'Use reverse video without selecting a brighter foreground color'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand read" -l input-format -d 'Interpret ordinary text, Markdown source, or trusted terminal output' -r -f -a "text\t'Ordinary text with the configured terminal-control policy'
markdown\t'`CommonMark` source with protected syntax and code spans'
terminal\t'Trusted terminal output whose existing controls must be preserved'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand read" -l format -d 'Emit ANSI, unstyled plain text, Markdown emphasis, or escaped HTML' -r -f -a "ansi\t'Terminal emphasis, subject to the style policy'
plain\t'Original text without terminal escape sequences or added emphasis'
markdown\t'Markdown source with emphasis syntax around selected prefixes'
html\t'Escaped HTML with semantic emphasis elements'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand read" -l controls -d 'Remove active document controls, or preserve trusted terminal controls' -r -f -a "safe\t'Permit presentation styling and safe hyperlinks; remove active controls'
preserve\t'Preserve terminal control bytes for trusted sources'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand read" -l skip-code -d 'Protect Markdown code from emphasis (default true); use --skip-code=false to include it' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand read" -l low-latency -d 'Flush stalled partial words unchanged after 50 ms; disables content-only determinism' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand read" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand preview" -l text -d 'Passage for the interactive preview' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand preview" -l fraction -d 'Fraction of each selected word to emphasize (0 through 1)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand preview" -l every -d 'Emphasize the first eligible word and then every Nth word' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand preview" -l min-word-length -d 'Minimum number of letters a word must have to receive emphasis' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand preview" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t'Enable emphasis when output is a terminal'
always\t'Enable emphasis regardless of the output destination'
never\t'Disable added emphasis; configured format and input-control processing still apply'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand preview" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand preview" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand preview" -l profile -d 'Named fixation preference; explicit numeric flags override its values' -r -f -a "subtle\t'Emphasize fewer, longer words with a shorter prefix'
balanced\t'Emphasize half of each eligible word'
strong\t'Emphasize a longer prefix of every eligible word'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand preview" -l length-rule -d 'Per-length fraction in MAX:FRACTION form; repeat in increasing MAX order' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand preview" -l emphasis -d 'Emphasis attribute: bold, underline, or reverse video' -r -f -a "bold\t'Use bold intensity, restoring the original bold and faint attributes'
underline\t'Use underlining without changing text colors or intensity'
reverse\t'Use reverse video without selecting a brighter foreground color'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand preview" -l input-format -d 'Interpret ordinary text, Markdown source, or trusted terminal output' -r -f -a "text\t'Ordinary text with the configured terminal-control policy'
markdown\t'`CommonMark` source with protected syntax and code spans'
terminal\t'Trusted terminal output whose existing controls must be preserved'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand preview" -l format -d 'Emit ANSI, unstyled plain text, Markdown emphasis, or escaped HTML' -r -f -a "ansi\t'Terminal emphasis, subject to the style policy'
plain\t'Original text without terminal escape sequences or added emphasis'
markdown\t'Markdown source with emphasis syntax around selected prefixes'
html\t'Escaped HTML with semantic emphasis elements'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand preview" -l controls -d 'Remove active document controls, or preserve trusted terminal controls' -r -f -a "safe\t'Permit presentation styling and safe hyperlinks; remove active controls'
preserve\t'Preserve terminal control bytes for trusted sources'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand preview" -l skip-code -d 'Protect Markdown code from emphasis (default true); use --skip-code=false to include it' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand preview" -l low-latency -d 'Flush stalled partial words unchanged after 50 ms; disables content-only determinism' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand preview" -l print -d 'Print the preview instead of opening the interactive reader'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand preview" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l shell -d 'Bash, Zsh, or fish executable (defaults to SHELL)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l fraction -d 'Fraction of each selected word to emphasize (0 through 1)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l every -d 'Emphasize the first eligible word and then every Nth word' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l min-word-length -d 'Minimum number of letters a word must have to receive emphasis' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t'Enable emphasis when output is a terminal'
always\t'Enable emphasis regardless of the output destination'
never\t'Disable added emphasis; configured format and input-control processing still apply'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l profile -d 'Named fixation preference; explicit numeric flags override its values' -r -f -a "subtle\t'Emphasize fewer, longer words with a shorter prefix'
balanced\t'Emphasize half of each eligible word'
strong\t'Emphasize a longer prefix of every eligible word'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l length-rule -d 'Per-length fraction in MAX:FRACTION form; repeat in increasing MAX order' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l emphasis -d 'Emphasis attribute: bold, underline, or reverse video' -r -f -a "bold\t'Use bold intensity, restoring the original bold and faint attributes'
underline\t'Use underlining without changing text colors or intensity'
reverse\t'Use reverse video without selecting a brighter foreground color'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l input-format -d 'Interpret ordinary text, Markdown source, or trusted terminal output' -r -f -a "text\t'Ordinary text with the configured terminal-control policy'
markdown\t'`CommonMark` source with protected syntax and code spans'
terminal\t'Trusted terminal output whose existing controls must be preserved'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l format -d 'Emit ANSI, unstyled plain text, Markdown emphasis, or escaped HTML' -r -f -a "ansi\t'Terminal emphasis, subject to the style policy'
plain\t'Original text without terminal escape sequences or added emphasis'
markdown\t'Markdown source with emphasis syntax around selected prefixes'
html\t'Escaped HTML with semantic emphasis elements'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l controls -d 'Remove active document controls, or preserve trusted terminal controls' -r -f -a "safe\t'Permit presentation styling and safe hyperlinks; remove active controls'
preserve\t'Preserve terminal control bytes for trusted sources'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l skip-code -d 'Protect Markdown code from emphasis (default true); use --skip-code=false to include it' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -l low-latency -d 'Flush stalled partial words unchanged after 50 ms; disables content-only determinism' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand session" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l fraction -d 'Fraction of each selected word to emphasize (0 through 1)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l every -d 'Emphasize the first eligible word and then every Nth word' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l min-word-length -d 'Minimum number of letters a word must have to receive emphasis' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t'Enable emphasis when output is a terminal'
always\t'Enable emphasis regardless of the output destination'
never\t'Disable added emphasis; configured format and input-control processing still apply'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l profile -d 'Named fixation preference; explicit numeric flags override its values' -r -f -a "subtle\t'Emphasize fewer, longer words with a shorter prefix'
balanced\t'Emphasize half of each eligible word'
strong\t'Emphasize a longer prefix of every eligible word'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l length-rule -d 'Per-length fraction in MAX:FRACTION form; repeat in increasing MAX order' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l emphasis -d 'Emphasis attribute: bold, underline, or reverse video' -r -f -a "bold\t'Use bold intensity, restoring the original bold and faint attributes'
underline\t'Use underlining without changing text colors or intensity'
reverse\t'Use reverse video without selecting a brighter foreground color'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l input-format -d 'Interpret ordinary text, Markdown source, or trusted terminal output' -r -f -a "text\t'Ordinary text with the configured terminal-control policy'
markdown\t'`CommonMark` source with protected syntax and code spans'
terminal\t'Trusted terminal output whose existing controls must be preserved'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l format -d 'Emit ANSI, unstyled plain text, Markdown emphasis, or escaped HTML' -r -f -a "ansi\t'Terminal emphasis, subject to the style policy'
plain\t'Original text without terminal escape sequences or added emphasis'
markdown\t'Markdown source with emphasis syntax around selected prefixes'
html\t'Escaped HTML with semantic emphasis elements'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l controls -d 'Remove active document controls, or preserve trusted terminal controls' -r -f -a "safe\t'Permit presentation styling and safe hyperlinks; remove active controls'
preserve\t'Preserve terminal control bytes for trusted sources'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l skip-code -d 'Protect Markdown code from emphasis (default true); use --skip-code=false to include it' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -l low-latency -d 'Flush stalled partial words unchanged after 50 ms; disables content-only determinism' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand init" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l fraction -d 'Fraction of each selected word to emphasize (0 through 1)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l every -d 'Emphasize the first eligible word and then every Nth word' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l min-word-length -d 'Minimum number of letters a word must have to receive emphasis' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t'Enable emphasis when output is a terminal'
always\t'Enable emphasis regardless of the output destination'
never\t'Disable added emphasis; configured format and input-control processing still apply'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l profile -d 'Named fixation preference; explicit numeric flags override its values' -r -f -a "subtle\t'Emphasize fewer, longer words with a shorter prefix'
balanced\t'Emphasize half of each eligible word'
strong\t'Emphasize a longer prefix of every eligible word'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l length-rule -d 'Per-length fraction in MAX:FRACTION form; repeat in increasing MAX order' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l emphasis -d 'Emphasis attribute: bold, underline, or reverse video' -r -f -a "bold\t'Use bold intensity, restoring the original bold and faint attributes'
underline\t'Use underlining without changing text colors or intensity'
reverse\t'Use reverse video without selecting a brighter foreground color'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l input-format -d 'Interpret ordinary text, Markdown source, or trusted terminal output' -r -f -a "text\t'Ordinary text with the configured terminal-control policy'
markdown\t'`CommonMark` source with protected syntax and code spans'
terminal\t'Trusted terminal output whose existing controls must be preserved'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l format -d 'Emit ANSI, unstyled plain text, Markdown emphasis, or escaped HTML' -r -f -a "ansi\t'Terminal emphasis, subject to the style policy'
plain\t'Original text without terminal escape sequences or added emphasis'
markdown\t'Markdown source with emphasis syntax around selected prefixes'
html\t'Escaped HTML with semantic emphasis elements'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l controls -d 'Remove active document controls, or preserve trusted terminal controls' -r -f -a "safe\t'Permit presentation styling and safe hyperlinks; remove active controls'
preserve\t'Preserve terminal control bytes for trusted sources'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l skip-code -d 'Protect Markdown code from emphasis (default true); use --skip-code=false to include it' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -l low-latency -d 'Flush stalled partial words unchanged after 50 ms; disables content-only determinism' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand completions" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l fraction -d 'Fraction of each selected word to emphasize (0 through 1)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l every -d 'Emphasize the first eligible word and then every Nth word' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l min-word-length -d 'Minimum number of letters a word must have to receive emphasis' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t'Enable emphasis when output is a terminal'
always\t'Enable emphasis regardless of the output destination'
never\t'Disable added emphasis; configured format and input-control processing still apply'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l profile -d 'Named fixation preference; explicit numeric flags override its values' -r -f -a "subtle\t'Emphasize fewer, longer words with a shorter prefix'
balanced\t'Emphasize half of each eligible word'
strong\t'Emphasize a longer prefix of every eligible word'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l length-rule -d 'Per-length fraction in MAX:FRACTION form; repeat in increasing MAX order' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l emphasis -d 'Emphasis attribute: bold, underline, or reverse video' -r -f -a "bold\t'Use bold intensity, restoring the original bold and faint attributes'
underline\t'Use underlining without changing text colors or intensity'
reverse\t'Use reverse video without selecting a brighter foreground color'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l input-format -d 'Interpret ordinary text, Markdown source, or trusted terminal output' -r -f -a "text\t'Ordinary text with the configured terminal-control policy'
markdown\t'`CommonMark` source with protected syntax and code spans'
terminal\t'Trusted terminal output whose existing controls must be preserved'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l format -d 'Emit ANSI, unstyled plain text, Markdown emphasis, or escaped HTML' -r -f -a "ansi\t'Terminal emphasis, subject to the style policy'
plain\t'Original text without terminal escape sequences or added emphasis'
markdown\t'Markdown source with emphasis syntax around selected prefixes'
html\t'Escaped HTML with semantic emphasis elements'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l controls -d 'Remove active document controls, or preserve trusted terminal controls' -r -f -a "safe\t'Permit presentation styling and safe hyperlinks; remove active controls'
preserve\t'Preserve terminal control bytes for trusted sources'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l skip-code -d 'Protect Markdown code from emphasis (default true); use --skip-code=false to include it' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -l low-latency -d 'Flush stalled partial words unchanged after 50 ms; disables content-only determinism' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand on" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l fraction -d 'Fraction of each selected word to emphasize (0 through 1)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l every -d 'Emphasize the first eligible word and then every Nth word' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l min-word-length -d 'Minimum number of letters a word must have to receive emphasis' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t'Enable emphasis when output is a terminal'
always\t'Enable emphasis regardless of the output destination'
never\t'Disable added emphasis; configured format and input-control processing still apply'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l profile -d 'Named fixation preference; explicit numeric flags override its values' -r -f -a "subtle\t'Emphasize fewer, longer words with a shorter prefix'
balanced\t'Emphasize half of each eligible word'
strong\t'Emphasize a longer prefix of every eligible word'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l length-rule -d 'Per-length fraction in MAX:FRACTION form; repeat in increasing MAX order' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l emphasis -d 'Emphasis attribute: bold, underline, or reverse video' -r -f -a "bold\t'Use bold intensity, restoring the original bold and faint attributes'
underline\t'Use underlining without changing text colors or intensity'
reverse\t'Use reverse video without selecting a brighter foreground color'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l input-format -d 'Interpret ordinary text, Markdown source, or trusted terminal output' -r -f -a "text\t'Ordinary text with the configured terminal-control policy'
markdown\t'`CommonMark` source with protected syntax and code spans'
terminal\t'Trusted terminal output whose existing controls must be preserved'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l format -d 'Emit ANSI, unstyled plain text, Markdown emphasis, or escaped HTML' -r -f -a "ansi\t'Terminal emphasis, subject to the style policy'
plain\t'Original text without terminal escape sequences or added emphasis'
markdown\t'Markdown source with emphasis syntax around selected prefixes'
html\t'Escaped HTML with semantic emphasis elements'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l controls -d 'Remove active document controls, or preserve trusted terminal controls' -r -f -a "safe\t'Permit presentation styling and safe hyperlinks; remove active controls'
preserve\t'Preserve terminal control bytes for trusted sources'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l skip-code -d 'Protect Markdown code from emphasis (default true); use --skip-code=false to include it' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -l low-latency -d 'Flush stalled partial words unchanged after 50 ms; disables content-only determinism' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand off" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l fraction -d 'Fraction of each selected word to emphasize (0 through 1)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l every -d 'Emphasize the first eligible word and then every Nth word' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l min-word-length -d 'Minimum number of letters a word must have to receive emphasis' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t'Enable emphasis when output is a terminal'
always\t'Enable emphasis regardless of the output destination'
never\t'Disable added emphasis; configured format and input-control processing still apply'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l profile -d 'Named fixation preference; explicit numeric flags override its values' -r -f -a "subtle\t'Emphasize fewer, longer words with a shorter prefix'
balanced\t'Emphasize half of each eligible word'
strong\t'Emphasize a longer prefix of every eligible word'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l length-rule -d 'Per-length fraction in MAX:FRACTION form; repeat in increasing MAX order' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l emphasis -d 'Emphasis attribute: bold, underline, or reverse video' -r -f -a "bold\t'Use bold intensity, restoring the original bold and faint attributes'
underline\t'Use underlining without changing text colors or intensity'
reverse\t'Use reverse video without selecting a brighter foreground color'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l input-format -d 'Interpret ordinary text, Markdown source, or trusted terminal output' -r -f -a "text\t'Ordinary text with the configured terminal-control policy'
markdown\t'`CommonMark` source with protected syntax and code spans'
terminal\t'Trusted terminal output whose existing controls must be preserved'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l format -d 'Emit ANSI, unstyled plain text, Markdown emphasis, or escaped HTML' -r -f -a "ansi\t'Terminal emphasis, subject to the style policy'
plain\t'Original text without terminal escape sequences or added emphasis'
markdown\t'Markdown source with emphasis syntax around selected prefixes'
html\t'Escaped HTML with semantic emphasis elements'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l controls -d 'Remove active document controls, or preserve trusted terminal controls' -r -f -a "safe\t'Permit presentation styling and safe hyperlinks; remove active controls'
preserve\t'Preserve terminal control bytes for trusted sources'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l skip-code -d 'Protect Markdown code from emphasis (default true); use --skip-code=false to include it' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -l low-latency -d 'Flush stalled partial words unchanged after 50 ms; disables content-only determinism' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand toggle" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l fraction -d 'Fraction of each selected word to emphasize (0 through 1)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l every -d 'Emphasize the first eligible word and then every Nth word' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l min-word-length -d 'Minimum number of letters a word must have to receive emphasis' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t'Enable emphasis when output is a terminal'
always\t'Enable emphasis regardless of the output destination'
never\t'Disable added emphasis; configured format and input-control processing still apply'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l profile -d 'Named fixation preference; explicit numeric flags override its values' -r -f -a "subtle\t'Emphasize fewer, longer words with a shorter prefix'
balanced\t'Emphasize half of each eligible word'
strong\t'Emphasize a longer prefix of every eligible word'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l length-rule -d 'Per-length fraction in MAX:FRACTION form; repeat in increasing MAX order' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l emphasis -d 'Emphasis attribute: bold, underline, or reverse video' -r -f -a "bold\t'Use bold intensity, restoring the original bold and faint attributes'
underline\t'Use underlining without changing text colors or intensity'
reverse\t'Use reverse video without selecting a brighter foreground color'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l input-format -d 'Interpret ordinary text, Markdown source, or trusted terminal output' -r -f -a "text\t'Ordinary text with the configured terminal-control policy'
markdown\t'`CommonMark` source with protected syntax and code spans'
terminal\t'Trusted terminal output whose existing controls must be preserved'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l format -d 'Emit ANSI, unstyled plain text, Markdown emphasis, or escaped HTML' -r -f -a "ansi\t'Terminal emphasis, subject to the style policy'
plain\t'Original text without terminal escape sequences or added emphasis'
markdown\t'Markdown source with emphasis syntax around selected prefixes'
html\t'Escaped HTML with semantic emphasis elements'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l controls -d 'Remove active document controls, or preserve trusted terminal controls' -r -f -a "safe\t'Permit presentation styling and safe hyperlinks; remove active controls'
preserve\t'Preserve terminal control bytes for trusted sources'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l skip-code -d 'Protect Markdown code from emphasis (default true); use --skip-code=false to include it' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -l low-latency -d 'Flush stalled partial words unchanged after 50 ms; disables content-only determinism' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand reload" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l fraction -d 'Fraction of each selected word to emphasize (0 through 1)' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l every -d 'Emphasize the first eligible word and then every Nth word' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l min-word-length -d 'Minimum number of letters a word must have to receive emphasis' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l style -d 'When to apply emphasis: automatically for terminals, always, or never' -r -f -a "auto\t'Enable emphasis when output is a terminal'
always\t'Enable emphasis regardless of the output destination'
never\t'Disable added emphasis; configured format and input-control processing still apply'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l config -d 'Read an explicit TOML configuration file' -r -F
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l exclude -d 'Add a command name to the interactive passthrough exclusions' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l profile -d 'Named fixation preference; explicit numeric flags override its values' -r -f -a "subtle\t'Emphasize fewer, longer words with a shorter prefix'
balanced\t'Emphasize half of each eligible word'
strong\t'Emphasize a longer prefix of every eligible word'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l length-rule -d 'Per-length fraction in MAX:FRACTION form; repeat in increasing MAX order' -r
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l emphasis -d 'Emphasis attribute: bold, underline, or reverse video' -r -f -a "bold\t'Use bold intensity, restoring the original bold and faint attributes'
underline\t'Use underlining without changing text colors or intensity'
reverse\t'Use reverse video without selecting a brighter foreground color'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l input-format -d 'Interpret ordinary text, Markdown source, or trusted terminal output' -r -f -a "text\t'Ordinary text with the configured terminal-control policy'
markdown\t'`CommonMark` source with protected syntax and code spans'
terminal\t'Trusted terminal output whose existing controls must be preserved'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l format -d 'Emit ANSI, unstyled plain text, Markdown emphasis, or escaped HTML' -r -f -a "ansi\t'Terminal emphasis, subject to the style policy'
plain\t'Original text without terminal escape sequences or added emphasis'
markdown\t'Markdown source with emphasis syntax around selected prefixes'
html\t'Escaped HTML with semantic emphasis elements'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l controls -d 'Remove active document controls, or preserve trusted terminal controls' -r -f -a "safe\t'Permit presentation styling and safe hyperlinks; remove active controls'
preserve\t'Preserve terminal control bytes for trusted sources'"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l skip-code -d 'Protect Markdown code from emphasis (default true); use --skip-code=false to include it' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -l low-latency -d 'Flush stalled partial words unchanged after 50 ms; disables content-only determinism' -r -f -a "true\t''
false\t''"
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand bypass" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from read preview session init completions on off toggle reload bypass help" -f -a "read" -d 'Read a document with scrolling, search, and adjustable fixation settings'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from read preview session init completions on off toggle reload bypass help" -f -a "preview" -d 'Preview fixation settings on a sample passage or supplied text'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from read preview session init completions on off toggle reload bypass help" -f -a "session" -d 'Start an interactive reading session'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from read preview session init completions on off toggle reload bypass help" -f -a "init" -d 'Print an opt-in initialization snippet for your shell configuration'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from read preview session init completions on off toggle reload bypass help" -f -a "completions" -d 'Print completion definitions for your shell'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from read preview session init completions on off toggle reload bypass help" -f -a "on" -d 'Enable emphasis in the current reading session'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from read preview session init completions on off toggle reload bypass help" -f -a "off" -d 'Disable emphasis in the current reading session'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from read preview session init completions on off toggle reload bypass help" -f -a "toggle" -d 'Toggle emphasis in the current reading session'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from read preview session init completions on off toggle reload bypass help" -f -a "reload" -d 'Reload settings at the next shell command boundary'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from read preview session init completions on off toggle reload bypass help" -f -a "bypass" -d 'Run a command without emphasis, restoring the prior state afterward'
complete -c bionic-reader -n "__fish_bionic_reader_using_subcommand help; and not __fish_seen_subcommand_from read preview session init completions on off toggle reload bypass help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
