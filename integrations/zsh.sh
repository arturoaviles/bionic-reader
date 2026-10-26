# bionic-reader generated Zsh integration. Evaluate with: eval "$(bionic-reader init zsh)"
if [[ ${BIONIC_READER_DISABLE-} != 1 && -o interactive && -t 0 && -t 1 ]]; then
  if [[ ${BIONIC_READER_SESSION-} == 1 && -n ${BIONIC_READER_MARKER_TOKEN-} ]]; then
    if (( ! ${+_BIONIC_READER_ZSH_HOOKS} )); then
      typeset -g _BIONIC_READER_ZSH_HOOKS=1
      typeset -g _BIONIC_READER_ZSH_ACTIVE=0
      _bionic_reader_marker() {
        local _br_quoted=
        (( $# > 1 )) && _br_quoted=$(builtin printf '%q' "$2")
        builtin printf '\033P+BIONIC_READER;%s;%s:%s\033\\' "$BIONIC_READER_MARKER_TOKEN" "$1" "$_br_quoted" >&2
      }
      _bionic_reader_preexec() {
        if [[ $_BIONIC_READER_ZSH_ACTIVE != 1 ]]; then
          _BIONIC_READER_ZSH_ACTIVE=1
          _bionic_reader_marker BEGIN "$1"
        fi
      }
      _bionic_reader_precmd() {
        local _br_status=$?
        if [[ $_BIONIC_READER_ZSH_ACTIVE == 1 ]]; then
          _bionic_reader_marker END
          _BIONIC_READER_ZSH_ACTIVE=0
        fi
        return $_br_status
      }
      typeset -ga precmd_functions preexec_functions
      (( ${precmd_functions[(Ie)_bionic_reader_precmd]} )) || precmd_functions=(_bionic_reader_precmd "${precmd_functions[@]}")
      (( ${preexec_functions[(Ie)_bionic_reader_preexec]} )) || preexec_functions+=(_bionic_reader_preexec)
    fi
  elif [[ ${BIONIC_READER_SESSION-} != 1 ]]; then
    exec bionic-reader session --shell zsh
  fi
fi
