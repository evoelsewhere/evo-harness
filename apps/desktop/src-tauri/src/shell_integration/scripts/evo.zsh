# evo shell integration for zsh (OSC 133: A, B, C, D;<exit>).
# Sourced at the end of the person's .zshrc by the ZDOTDIR shim.
# Spec: docs/features/agent-terminal (FR-16).

if [[ -z "$__evo_integrated" ]]; then
    typeset -g __evo_integrated=1
    typeset -g __evo_running=0

    # First precmd hook: report the finished command, then start the prompt.
    __evo_precmd() {
        local ec=$?
        if (( __evo_running )); then
            printf '\033]133;D;%s\007' "$ec"
            __evo_running=0
        fi
        printf '\033]133;A\007'
    }

    # Last precmd hook (after prompt themes): end the prompt with the B marker.
    __evo_precmd_end() {
        if [[ "$PS1" != *'133;B'* ]]; then
            PS1="$PS1%{"$'\033]133;B\007'"%}"
        fi
    }

    __evo_preexec() {
        __evo_running=1
        printf '\033]133;C\007'
    }

    precmd_functions=(__evo_precmd ${precmd_functions:#__evo_precmd})
    precmd_functions+=(__evo_precmd_end)
    preexec_functions+=(__evo_preexec)
fi
