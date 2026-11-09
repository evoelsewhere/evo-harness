# A window that takes a click only while the physical mouse button is down,
# as apps that read raw input do (computer app control benchmark, spec FR-19 /
# SC-5). Its label counts the clicks it took; a background click (a posted
# message, no button pressed) leaves it at 0, which the tool must report as a
# step that did not reach the app.
Add-Type -AssemblyName System.Windows.Forms
$form = New-Object System.Windows.Forms.Form
$form.Text = 'active-only-probe'
$form.Width = 420
$form.Height = 300
$label = New-Object System.Windows.Forms.Label
$label.Text = 'clicks: 0'
$label.AutoSize = $true
$label.Top = 10
$label.Left = 10
$form.Controls.Add($label)
$count = 0
$form.Add_MouseDown({
    if ([System.Windows.Forms.Control]::MouseButtons -eq [System.Windows.Forms.MouseButtons]::None) { return }
    $script:count++
    $label.Text = "clicks: $script:count"
})
[System.Windows.Forms.Application]::Run($form)
