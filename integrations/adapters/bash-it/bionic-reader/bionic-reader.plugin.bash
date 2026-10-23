# Install this directory as a Bash-it plugin and enable `bionic-reader`.
if [[ $- == *i* && -t 0 && -t 1 && ${BIONIC_READER_DISABLE-} != 1 ]] && command -v bionic-reader >/dev/null 2>&1; then
  eval "$(bionic-reader init bash)"
fi
