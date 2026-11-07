# evo shell integration for Nushell (OSC 133: A, B, C, D;<exit>).
# Loaded with `nu --execute "source '<this file>'"`.
# Spec: docs/features/agent-terminal (FR-16).

$env.EVO_RUNNING = false

let evo_pre_prompt = {||
    let esc = (char esc)
    let bel = (char bel)
    if $env.EVO_RUNNING {
        print -n $"($esc)]133;D;($env.LAST_EXIT_CODE)($bel)"
        $env.EVO_RUNNING = false
    }
    print -n $"($esc)]133;A($bel)"
}

let evo_pre_execution = {||
    $env.EVO_RUNNING = true
    print -n $"(char esc)]133;C(char bel)"
}

def evo-hooks-list [current: any, extra: closure] {
    if ($current | describe | str starts-with "list") {
        $current | append $extra
    } else if $current == null {
        [$extra]
    } else {
        [$current $extra]
    }
}

$env.config = ($env.config
    | upsert hooks.pre_prompt (evo-hooks-list ($env.config.hooks?.pre_prompt? | default null) $evo_pre_prompt)
    | upsert hooks.pre_execution (evo-hooks-list ($env.config.hooks?.pre_execution? | default null) $evo_pre_execution))

# End the prompt with the B marker.
$env.PROMPT_INDICATOR = $"(($env.PROMPT_INDICATOR? | default '> '))(char esc)]133;B(char bel)"
