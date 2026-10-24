# bionic-reader generated fish integration. Evaluate with: eval (bionic-reader init fish | string collect)
if test "$BIONIC_READER_DISABLE" != 1; and status is-interactive; and isatty stdin; and isatty stdout
    if test "$BIONIC_READER_SESSION" = 1; and set -q BIONIC_READER_MARKER_TOKEN
        if not set -q _BIONIC_READER_FISH_HOOKS
            set -g _BIONIC_READER_FISH_HOOKS 1
            set -g _BIONIC_READER_FISH_ACTIVE 0
            function __bionic_reader_marker --argument-names kind metadata
                printf '\eP+BIONIC_READER;%s;%s:%s\e\\' "$BIONIC_READER_MARKER_TOKEN" "$kind" "$metadata" >&2
            end
            function __bionic_reader_preexec --on-event fish_preexec
                if test "$_BIONIC_READER_FISH_ACTIVE" != 1
                    set -g _BIONIC_READER_FISH_ACTIVE 1
                    set -l encoded (string escape --style=script -- "$argv[1]")
                    __bionic_reader_marker BEGIN "$encoded"
                end
            end
            function __bionic_reader_postexec --on-event fish_postexec
                if test "$_BIONIC_READER_FISH_ACTIVE" = 1
                    __bionic_reader_marker END
                    set -g _BIONIC_READER_FISH_ACTIVE 0
                end
            end
        end
    else if test "$BIONIC_READER_SESSION" != 1
        exec bionic-reader session --shell fish
    end
end
