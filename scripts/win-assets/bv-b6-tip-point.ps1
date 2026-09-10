param([long]$Hwnd, [int]$Width, [int]$Height)
$ErrorActionPreference = 'Stop'
if ($Hwnd -le 0 -or $Width -le 1 -or $Height -le 1) { throw 'Invalid tip target or display dimensions' }
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, UIAutomationClientsideProviders
Add-Type -ReferencedAssemblies UIAutomationClient, UIAutomationTypes -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class BvTipNative {
    [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.NoInlining)] public static void InitializeUIA() { var name = typeof(System.Windows.Automation.AutomationElement).Assembly.GetName(); name.Name = "UIAutomationClientsideProviders"; System.Windows.Automation.ClientSettings.RegisterClientSideProviderAssembly(name); }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr hwnd, uint flags);
    public static long OwnerAt(int x, int y) {
        IntPtr previous = SetThreadDpiAwarenessContext(new IntPtr(-4));
        if (previous == IntPtr.Zero) throw new InvalidOperationException("Physical point context unavailable");
        try { return GetAncestor(WindowFromPoint(new Point { X = x, Y = y }), 3).ToInt64(); }
        finally { SetThreadDpiAwarenessContext(previous); }
    }
}
'@
[BvTipNative]::InitializeUIA(); if (![BvTipNative]::IsWindowVisible([IntPtr]$Hwnd)) { throw 'Tip target is absent or hidden' }
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$Hwnd)
if ($null -eq $root) { throw 'No UI Automation root for tip target' }
$type = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
$name = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::NameProperty, 'Got it')
$condition = [System.Windows.Automation.AndCondition]::new($type, $name)
$matches = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition)
$visible = @(foreach ($candidate in $matches) { if (!$candidate.Current.IsOffscreen) { $candidate } })
if ($visible.Count -eq 0) {
    Write-Output "BVTIPPOINT hwnd=$Hwnd state=not-found"; return
}
if ($visible.Count -ne 1) { throw 'Ambiguous visible Got it buttons' }
$button = $visible[0]
if (!$button.Current.IsEnabled -or $button.Current.ProcessId -ne $root.Current.ProcessId) {
    throw 'Tip button disabled or from a different process'
}
# UI Automation BoundingRectangle is in physical screen coordinates.
$rect = $button.Current.BoundingRectangle
if ($rect.IsEmpty -or $rect.Width -le 0 -or $rect.Height -le 0) { throw 'Tip button has no rectangle' }
$x = [int][Math]::Floor($rect.Left + $rect.Width / 2.0)
$y = [int][Math]::Floor($rect.Top + $rect.Height / 2.0)
if ($x -lt 0 -or $y -lt 0 -or $x -ge $Width -or $y -ge $Height) { throw 'Tip button is outside display' }
$owner = [BvTipNative]::OwnerAt($x, $y)
if ($owner -ne $Hwnd) { throw 'Tip point is covered or owned by another window' }
Write-Output "BVTIPPOINT hwnd=$Hwnd state=present x=$x y=$y owner=$owner"
