# Install this directory in custom/plugins/bionic-reader and enable the plugin.
if [[ -o interactive && -t 0 && -t 1 && ${BIONIC_READER_DISABLE-} != 1 ]] && (( $+commands[bionic-reader] )); then
  eval "$(bionic-reader init zsh)"
fi
