# Install this directory as an Oh My Bash plugin and add `bionic-reader` to plugins.
if [[ $- == *i* && -t 0 && -t 1 && ${BIONIC_READER_DISABLE-} != 1 ]] && command -v bionic-reader >/dev/null 2>&1; then
  eval "$(bionic-reader init bash)"
fi
