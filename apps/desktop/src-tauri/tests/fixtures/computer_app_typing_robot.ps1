# The typing robot of the computer app control benchmark (spec SC-3): a
# window of its own, in front, that types -Text over and over for -Seconds,
# as a person working beside the agent would. It writes what it sent to
# <Out>.sent and what its text box holds to <Out>, so the benchmark can count
# keys lost or typed elsewhere while the agent drove another app.
param(
    [string]$Text = "the quick brown fox ",
    [int]$Seconds = 30,
    [string]$Out = "$env:TEMP\evo-typing-robot.txt"
)
Add-Type -AssemblyName System.Windows.Forms
$form = New-Object System.Windows.Forms.Form
$form.Text = 'typing-robot'
$form.Width = 520
$form.Height = 200
$box = New-Object System.Windows.Forms.TextBox
$box.Multiline = $true
$box.Dock = 'Fill'
$form.Controls.Add($box)
$sent = New-Object System.Text.StringBuilder
$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = 60
$started = [DateTime]::Now
$index = 0
$timer.Add_Tick({
    if (([DateTime]::Now - $started).TotalSeconds -ge $Seconds) {
        $timer.Stop()
        [System.IO.File]::WriteAllText("$Out.sent", $sent.ToString())
        [System.IO.File]::WriteAllText($Out, $box.Text)
        $form.Close()
        return
    }
    if (-not $form.ContainsFocus) { $form.Activate(); $box.Focus() | Out-Null }
    $char = $Text[$index % $Text.Length]
    $index++
    # SendKeys reserves + ^ % ~ ( ) { } [ ]; the text uses none of them.
    [System.Windows.Forms.SendKeys]::SendWait([string]$char)
    [void]$sent.Append($char)
})
$form.Add_Shown({ $form.Activate(); $box.Focus() | Out-Null; $timer.Start() })
[System.Windows.Forms.Application]::Run($form)
