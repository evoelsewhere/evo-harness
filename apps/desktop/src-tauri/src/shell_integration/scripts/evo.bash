# evo shell integration for bash (OSC 133: A prompt start, B prompt end,
# C output start, D;<exit> command done). Loaded with `bash --init-file`.
# Spec: docs/features/agent-terminal (FR-16).

# Start up like an interactive login shell would, so the person's PATH and
# aliases are there: /etc/profile, then the first profile file, else ~/.bashrc.
[ -f /etc/profile ] && . /etc/profile
if [ -f "$HOME/.bash_profile" ]; then
    . "$HOME/.bash_profile"
elif [ -f "$HOME/.bash_login" ]; then
    . "$HOME/.bash_login"
elif [ -f "$HOME/.profile" ]; then
    . "$HOME/.profile"
elif [ -f "$HOME/.bashrc" ]; then
    . "$HOME/.bashrc"
fi

if [ -z "$__evo_integrated" ]; then
    __evo_integrated=1
    __evo_running=0
    __evo_in_prompt=0
    __evo_prompted=0

    # Bash 4.4 and later expand PS0 once a command line is accepted, before it
    # runs: the C marker for every command, also `(cd x && make)` and `{ ...; }`,
    # which the DEBUG trap below never sees. Older bash falls back to the trap.
    if (( BASH_VERSINFO[0] > 4 || (BASH_VERSINFO[0] == 4 && BASH_VERSINFO[1] >= 4) )); then
        __evo_ps0=1
        PS0="${PS0}"$'\033]133;C\007'
    else
        __evo_ps0=0
    fi

    # Runs first in PROMPT_COMMAND: report the finished command, then start the prompt.
    __evo_precmd() {
        local __evo_ec=$?
        __evo_in_prompt=1
        if [ "$__evo_ps0" = 1 ]; then
            # Every prompt after the first ends a command (an empty line gives a bare D).
            if [ "$__evo_prompted" = 1 ]; then
                printf '\033]133;D;%s\007' "$__evo_ec"
            fi
            __evo_prompted=1
        elif [ "$__evo_running" = 1 ]; then
            printf '\033]133;D;%s\007' "$__evo_ec"
            __evo_running=0
        fi
        printf '\033]133;A\007'
        # Keep the person's prompt and end it with the B marker (once).
        case "$PS1" in
            *'133;B'*) ;;
            *) PS1="$PS1"'\[\033]133;B\007\]' ;;
        esac
        return $__evo_ec
    }

    # Runs last in PROMPT_COMMAND: from here on, commands are the person's.
    __evo_postcmd() {
        __evo_in_prompt=0
    }

    # DEBUG trap (bash before 4.4): the C marker once per command, not for the prompt hooks.
    __evo_preexec() {
        [ "$__evo_in_prompt" = 1 ] && return
        [ "$__evo_running" = 1 ] && return
        [ -n "$COMP_LINE" ] && return
        case "$BASH_COMMAND" in
            __evo_*) return ;;
        esac
        __evo_running=1
        printf '\033]133;C\007'
    }

    if [[ "$(declare -p PROMPT_COMMAND 2>/dev/null)" == "declare -a"* ]]; then
        PROMPT_COMMAND=(__evo_precmd "${PROMPT_COMMAND[@]}" __evo_postcmd)
    else
        PROMPT_COMMAND="__evo_precmd;${PROMPT_COMMAND:+$PROMPT_COMMAND;}__evo_postcmd"
    fi
    [ "$__evo_ps0" = 1 ] || trap '__evo_preexec' DEBUG
fi
