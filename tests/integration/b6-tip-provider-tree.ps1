param([long]$Hwnd)
# Retain provider observations when a native contract cannot find its button.
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$Hwnd)
$nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.Condition]::TrueCondition)
[Console]::Error.WriteLine("UIA root=$($root.Current.Name) descendants=$($nodes.Count)")
foreach ($node in $nodes) {
    $value = $node.Current
    [Console]::Error.WriteLine("UIA name=[$($value.Name)] type=$($value.ControlType.ProgrammaticName) offscreen=$($value.IsOffscreen) enabled=$($value.IsEnabled) pid=$($value.ProcessId) bounds=$($value.BoundingRectangle)")
}
