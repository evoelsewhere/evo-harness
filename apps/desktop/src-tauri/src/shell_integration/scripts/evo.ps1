# evo shell integration for PowerShell 7 and Windows PowerShell 5.1
# (OSC 133: A prompt start, B prompt end, C output start, D;<exit> done).
# Loaded with `-NoExit -Command ". '<this file>'"`.
# Spec: docs/features/agent-terminal (FR-16).

if (-not $global:__EvoIntegrated) {
    $global:__EvoIntegrated = $true
    $global:__EvoRunning = $false
    $global:__EvoOriginalPrompt = $function:prompt
    if (-not $global:__EvoOriginalPrompt) {
        $global:__EvoOriginalPrompt = { "PS $($executionContext.SessionState.Path.CurrentLocation)$('>' * ($nestedPromptLevel + 1)) " }
    }

    function global:prompt {
        $succeeded = $?
        $lastExit = $global:LASTEXITCODE
        $esc = [char]27
        $bel = [char]7
        $marks = ''
        if ($global:__EvoRunning) {
            $code = 0
            if (-not $succeeded) {
                $code = 1
                if ($null -ne $lastExit -and $lastExit -ne 0) { $code = $lastExit }
            }
            $marks += "$esc]133;D;$code$bel"
            $global:__EvoRunning = $false
        }
        $marks += "$esc]133;A$bel"
        [Console]::Write($marks)
        $global:LASTEXITCODE = $lastExit
        $text = & $global:__EvoOriginalPrompt
        return "$text$esc]133;B$bel"
    }

    # C goes out when Enter accepts a command, before it runs.
    try {
        Import-Module PSReadLine -ErrorAction Stop
        Set-PSReadLineKeyHandler -Key Enter -ScriptBlock {
            $line = $null
            $cursor = $null
            [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState([ref]$line, [ref]$cursor)
            if ($line -and $line.Trim()) {
                [Console]::Write("$([char]27)]133;C$([char]7)")
                $global:__EvoRunning = $true
            }
            [Microsoft.PowerShell.PSConsoleReadLine]::AcceptLine()
        }
    } catch {
        # Without PSReadLine there is no C marker; A, B and D still work.
    }
}
