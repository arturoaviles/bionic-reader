# Install this directory as a Fisher plugin.
if status is-interactive; and isatty stdin; and isatty stdout; and test "$BIONIC_READER_DISABLE" != 1; and type -q bionic-reader
    bionic-reader init fish | source
end
