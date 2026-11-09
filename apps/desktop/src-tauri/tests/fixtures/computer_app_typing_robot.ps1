# The typing robot of the computer app control benchmark (spec SC-3): a
# window of its own, in front, that types -Text over and over for -Seconds,
# as a person working beside the agent would. It writes what it sent to
# <Out>.sent and what its text box holds to <Out>, so the benchmark can count
# keys lost while the agent drove another app. It types only while its own
# window has the focus, never into another window: a key it held back because
# another window was in front is counted in <Out>.held.
param(
    [string]$Text = "the quick brown fox ",
    [int]$Seconds = 30,
    [string]$Out = "$env:TEMP\evo-typing-robot.txt"
)
Add-Type -AssemblyName System.Windows.Forms
# The system's front window, which SendKeys types into. A form's own
# ContainsFocus said yes while another app was in front, and the keys went to
# that app: the front window is checked instead, before every key.
Add-Type -Name Front -Namespace Robot -MemberDefinition '[DllImport("user32.dll")] public static extern System.IntPtr GetForegroundWindow();'
$form = New-Object System.Windows.Forms.Form
$form.Text = 'typing-robot'
$form.Width = 520
$form.Height = 200
$box = New-Object System.Windows.Forms.TextBox
$box.Multiline = $true
$box.Dock = 'Fill'
$form.Controls.Add($box)
$sent = New-Object System.Text.StringBuilder
$script:held = 0
$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = 60
$started = [DateTime]::Now
$index = 0
$timer.Add_Tick({
    if (([DateTime]::Now - $started).TotalSeconds -ge $Seconds) {
        $timer.Stop()
        [System.IO.File]::WriteAllText("$Out.sent", $sent.ToString())
        [System.IO.File]::WriteAllText("$Out.held", [string]$script:held)
        [System.IO.File]::WriteAllText($Out, $box.Text)
        $form.Close()
        return
    }
    if ([Robot.Front]::GetForegroundWindow() -ne $form.Handle) { $form.Activate(); $box.Focus() | Out-Null }
    # Another window kept the front: hold the key back rather than type it there.
    if ([Robot.Front]::GetForegroundWindow() -ne $form.Handle -or -not $box.Focused) { $script:held++; return }
    $char = $Text[$index % $Text.Length]
    $index++
    # SendKeys reserves + ^ % ~ ( ) { } [ ]; the text uses none of them.
    [System.Windows.Forms.SendKeys]::SendWait([string]$char)
    [void]$sent.Append($char)
})
$form.Add_Shown({ $form.Activate(); $box.Focus() | Out-Null; $timer.Start() })
[System.Windows.Forms.Application]::Run($form)
