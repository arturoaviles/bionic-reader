# Add this package to Oh My Fish.
if status is-interactive; and isatty stdin; and isatty stdout; and test "$BIONIC_READER_DISABLE" != 1; and type -q bionic-reader
    eval (bionic-reader init fish | string collect)
end
