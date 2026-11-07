# evo shell integration for fish (OSC 133: A, B, C, D;<exit>).
# Loaded with `fish --init-command "source <this file>"`.
# Spec: docs/features/agent-terminal (FR-16).

if not set -q __evo_integrated
    set -g __evo_integrated 1

    function __evo_preexec --on-event fish_preexec
        printf '\033]133;C\007'
    end

    function __evo_postexec --on-event fish_postexec
        printf '\033]133;D;%s\007' $status
    end

    # Hand the last status back to the original prompt, which reads $status.
    function __evo_restore_status
        return $argv[1]
    end

    if functions -q fish_prompt
        functions -c fish_prompt __evo_orig_prompt
    else
        function __evo_orig_prompt
        end
    end

    function fish_prompt
        set -l __evo_last_status $status
        printf '\033]133;A\007'
        __evo_restore_status $__evo_last_status
        __evo_orig_prompt
        printf '\033]133;B\007'
    end
end
