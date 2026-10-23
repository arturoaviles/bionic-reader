# Add this module to Prezto's pmodule-dirs and enable `bionic-reader` in pmodule.
if [[ -o interactive && -t 0 && -t 1 && ${BIONIC_READER_DISABLE-} != 1 ]] && (( $+commands[bionic-reader] )); then
  eval "$(bionic-reader init zsh)"
fi
