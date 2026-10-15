# SPDX-License-Identifier: Apache-2.0
#
# Lets a long test recording run with nobody at the computer.
#
# TEST LONG RECORDING.bat starts this when you say you will be away. For as many minutes as you give it, it:
#   * stops Windows from going to sleep or turning the screen off,
#   * plays a gentle looping sound through the default sound output, and
#   * shows a window with things moving in it,
# so the recording has real picture changes and real sound without you. It closes itself when the time is up.
# It records nothing and sends nothing anywhere. If anything goes wrong it writes what happened to the
# log file named by -LogPath, and the recording script says so.
#
# The sound and picture are made up, not your real use, so the numbers can differ a little from a test you sit
# through. The window moves about 60 times a second, which is more than most screens change, so the processor
# figure is on the cautious side.

param(
    [int]$Minutes = 63,
    [string]$ReadyPath = (Join-Path ([System.IO.Path]::GetTempPath()) 'openstage-helper-ready.txt'),
    [string]$LogPath = (Join-Path ([System.IO.Path]::GetTempPath()) 'openstage-helper-log.txt')
)

# Make a short sound that loops without a click: four chords of two seconds each, every one swelling up from
# silence and back to silence, at different loudnesses, so the recorder's loudness readings change as they would
# with music. Mono, 16 bits, 44100 samples a second.
function New-TestSoundFile {
    param([Parameter(Mandatory = $true)][string]$Path)
    $rate = 44100
    $seconds = 8
    $count = $rate * $seconds
    $roots = 261.63, 329.63, 392.00, 523.25
    $loudness = 0.9, 0.5, 0.75, 0.35
    $stream = [System.IO.File]::Create($Path)
    $writer = New-Object System.IO.BinaryWriter($stream)
    try {
        $dataBytes = $count * 2
        $writer.Write([System.Text.Encoding]::ASCII.GetBytes('RIFF'))
        $writer.Write([int32](36 + $dataBytes))
        $writer.Write([System.Text.Encoding]::ASCII.GetBytes('WAVE'))
        $writer.Write([System.Text.Encoding]::ASCII.GetBytes('fmt '))
        $writer.Write([int32]16)
        $writer.Write([int16]1)          # plain PCM
        $writer.Write([int16]1)          # one channel
        $writer.Write([int32]$rate)
        $writer.Write([int32]($rate * 2))
        $writer.Write([int16]2)
        $writer.Write([int16]16)
        $writer.Write([System.Text.Encoding]::ASCII.GetBytes('data'))
        $writer.Write([int32]$dataBytes)
        for ($i = 0; $i -lt $count; $i++) {
            $t = $i / $rate
            $chord = [int][math]::Floor($t / 2)
            $local = $t - 2 * $chord
            $swell = [math]::Pow([math]::Sin([math]::PI * $local / 2), 2)
            $f = $roots[$chord % 4]
            $tone = 0.6 * [math]::Sin(2 * [math]::PI * $f * $t) + 0.4 * [math]::Sin(2 * [math]::PI * $f * 1.5 * $t)
            $sample = $tone * $swell * $loudness[$chord % 4] * 14000
            $writer.Write([int16][math]::Round($sample))
        }
    }
    finally {
        $writer.Close()
        $stream.Close()
    }
}

function Start-HandsFree {
    param([int]$Minutes, [string]$ReadyPath)

    # 1. Stay awake: ES_CONTINUOUS (0x80000000) with ES_SYSTEM_REQUIRED (1) and ES_DISPLAY_REQUIRED (2).
    Add-Type -Namespace OpenStage -Name Power -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("kernel32.dll")]
public static extern uint SetThreadExecutionState(uint flags);
'@
    [void][OpenStage.Power]::SetThreadExecutionState([uint32]2147483651)

    # 2. A sound to record.
    $soundPath = Join-Path ([System.IO.Path]::GetTempPath()) 'openstage-test-sound.wav'
    New-TestSoundFile -Path $soundPath
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing
    $player = New-Object System.Media.SoundPlayer $soundPath
    $player.PlayLooping()

    # 3. Something moving on the screen.
    $script:tick = 0
    $script:font = New-Object System.Drawing.Font('Segoe UI', 14)
    $form = New-Object System.Windows.Forms.Form
    $form.Text = 'OpenStage test picture (closes by itself)'
    $form.Width = 900
    $form.Height = 620
    $form.StartPosition = 'CenterScreen'
    $form.TopMost = $true
    $form.GetType().GetProperty('DoubleBuffered', [System.Reflection.BindingFlags]'Instance,NonPublic').SetValue($form, $true, $null)
    $form.Add_Paint({
        param($sender, $e)
        $g = $e.Graphics
        $g.Clear([System.Drawing.Color]::FromArgb(20, 24, 40))
        for ($k = 0; $k -lt 12; $k++) {
            $x = 20 + (($script:tick * (3 + $k)) % 760)
            $y = 20 + $k * 42
            $colour = [System.Drawing.Color]::FromArgb(255, (40 + $k * 17) % 256, (200 - $k * 11) % 256, (90 + $script:tick % 160))
            $brush = New-Object System.Drawing.SolidBrush $colour
            $g.FillEllipse($brush, $x, $y, 60, 30)
            $brush.Dispose()
        }
        $g.DrawString("test picture $($script:tick)", $script:font, [System.Drawing.Brushes]::White, 10, 540)
    })
    $mover = New-Object System.Windows.Forms.Timer
    $mover.Interval = 16
    $mover.Add_Tick({ $script:tick++; $form.Invalidate() })
    $closer = New-Object System.Windows.Forms.Timer
    $closer.Interval = [int]($Minutes * 60 * 1000)
    $closer.Add_Tick({ $form.Close() })
    $form.Add_Shown({
        # Tell the recording script the sound and the picture are going.
        Set-Content -Path $ReadyPath -Value 'ready'
        $mover.Start()
        $closer.Start()
    })
    try {
        [void]$form.ShowDialog()
    }
    finally {
        $mover.Stop()
        $closer.Stop()
        $player.Stop()
        Remove-Item -Path $soundPath -ErrorAction SilentlyContinue
        Remove-Item -Path $ReadyPath -ErrorAction SilentlyContinue
        # Let Windows sleep again.
        [void][OpenStage.Power]::SetThreadExecutionState([uint32]2147483648)
    }
}

# Run it, unless this file was only loaded to use its functions (the tests do that).
if ($MyInvocation.InvocationName -ne '.') {
    try {
        Start-HandsFree -Minutes $Minutes -ReadyPath $ReadyPath
    }
    catch {
        Set-Content -Path $LogPath -Value ("The hands-free helper stopped: " + $_.Exception.Message)
        exit 1
    }
}
