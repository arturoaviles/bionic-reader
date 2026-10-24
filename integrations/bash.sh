# bionic-reader generated Bash integration. Evaluate with: eval "$(bionic-reader init bash)"
if [[ ${BIONIC_READER_DISABLE-} != 1 && $- == *i* && -t 0 && -t 1 ]]; then
  if [[ ${BIONIC_READER_SESSION-} == 1 && -n ${BIONIC_READER_MARKER_TOKEN-} ]]; then
    if [[ ${_BIONIC_READER_BASH_HOOKS-} != 1 ]]; then
      _BIONIC_READER_BASH_HOOKS=1
      _BIONIC_READER_BASH_ACTIVE=0
      _BIONIC_READER_BASH_PROMPT_ACTIVE=0
      _bionic_reader_marker() {
        local LC_ALL=C
        local _br_quoted=
        if [[ $# -gt 1 ]]; then
          builtin printf -v _br_quoted '%q' "$2"
        fi
        builtin printf '\033P+BIONIC_READER;%s;%s:%s\033\\' "$BIONIC_READER_MARKER_TOKEN" "$1" "$_br_quoted" >&2
      }
      _bionic_reader_debug() {
        local _br_status=$?
        local _br_command=${BASH_COMMAND-}
        if [[ ${_BIONIC_READER_BASH_PROMPT_ACTIVE-0} == 1 ]]; then
          if [[ $_br_command == _bionic_reader_prompt_done ]]; then
            _BIONIC_READER_BASH_PROMPT_ACTIVE=0
          fi
        elif [[ ${_BIONIC_READER_BASH_ACTIVE-0} != 1 ]]; then
          _BIONIC_READER_BASH_ACTIVE=1
          _bionic_reader_marker BEGIN "$_br_command"
        fi
        if [[ -n ${_BIONIC_READER_BASH_OLD_DEBUG_BODY-} ]]; then
          builtin eval -- "$_BIONIC_READER_BASH_OLD_DEBUG_BODY"
        fi
        return "$_br_status"
      }
      _bionic_reader_prompt() {
        local _br_status=$?
        _BIONIC_READER_BASH_LAST_STATUS=$_br_status
        if [[ ${_BIONIC_READER_BASH_ACTIVE-0} == 1 ]]; then
          _bionic_reader_marker END
          _BIONIC_READER_BASH_ACTIVE=0
        fi
        _BIONIC_READER_BASH_PROMPT_ACTIVE=1
        return "$_br_status"
      }
      _bionic_reader_prompt_done() { return "${_BIONIC_READER_BASH_LAST_STATUS-0}"; }
      _bionic_reader_preexec() {
        if [[ ${_BIONIC_READER_BASH_ACTIVE-0} != 1 ]]; then
          _BIONIC_READER_BASH_ACTIVE=1
          _bionic_reader_marker BEGIN "$1"
        fi
      }
    fi
    if [[ ${bash_preexec_imported-${__bp_imported-}} == defined ]]; then
      # Bash-it installs its DEBUG trap after the first prompt. Its public hook
      # arrays keep both integrations active without competing for that trap.
      if [[ " ${preexec_functions[*]-} " != *' _bionic_reader_preexec '* ]]; then
        preexec_functions+=(_bionic_reader_preexec)
      fi
      if [[ " ${precmd_functions[*]-} " != *' _bionic_reader_prompt '* ]]; then
        precmd_functions=(_bionic_reader_prompt "${precmd_functions[@]}")
      fi
    else
      _BIONIC_READER_BASH_OLD_DEBUG_TRAP=$(builtin trap -p DEBUG)
      if [[ -n $_BIONIC_READER_BASH_OLD_DEBUG_TRAP && $_BIONIC_READER_BASH_OLD_DEBUG_TRAP != *'_bionic_reader_debug'* ]]; then
        _BIONIC_READER_BASH_OLD_DEBUG_TRAP=${_BIONIC_READER_BASH_OLD_DEBUG_TRAP#trap -- }
        _BIONIC_READER_BASH_OLD_DEBUG_TRAP=${_BIONIC_READER_BASH_OLD_DEBUG_TRAP% DEBUG}
        builtin eval "_BIONIC_READER_BASH_OLD_DEBUG_BODY=$_BIONIC_READER_BASH_OLD_DEBUG_TRAP"
      fi

      # Keep both PROMPT_COMMAND forms supported by current Bash and 3.2.
      _BIONIC_READER_PROMPT_DECL=$(builtin declare -p PROMPT_COMMAND 2>/dev/null)
      if [[ ${PROMPT_COMMAND[*]-} == *'_bionic_reader_prompt'* ]]; then
        :
      elif [[ $_BIONIC_READER_PROMPT_DECL == 'declare -a '* || $_BIONIC_READER_PROMPT_DECL == 'declare -ax '* ]]; then
        PROMPT_COMMAND=(_bionic_reader_prompt "${PROMPT_COMMAND[@]}" _bionic_reader_prompt_done)
      elif [[ -n ${PROMPT_COMMAND-} ]]; then
        PROMPT_COMMAND="_bionic_reader_prompt;${PROMPT_COMMAND};_bionic_reader_prompt_done"
      else
        PROMPT_COMMAND='_bionic_reader_prompt;_bionic_reader_prompt_done'
      fi
      # A plugin loader can restore a function-scoped DEBUG trap on return.
      # The wrapper repeats this top-level startup pass after sourcing the rc.
      trap '_bionic_reader_debug' DEBUG
    fi
  elif [[ ${BIONIC_READER_SESSION-} != 1 ]]; then
    exec bionic-reader session --shell bash
  fi
fi
