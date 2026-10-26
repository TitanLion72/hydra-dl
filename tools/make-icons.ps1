# Hydra DL ikonlarini uretir (PS 5.1 uyumlu, ASCII).
# Kullanim: powershell -ExecutionPolicy Bypass -File tools\make-icons.ps1
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$outDir = Join-Path $root "src-tauri\icons"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

function New-HydraBitmap([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)

    $bg = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 11, 18, 32))
    $radius = [Math]::Max(2, [int]($size * 0.22))
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = $radius * 2
    $w = $size
    $path.AddArc(0, 0, $d, $d, 180, 90)
    $path.AddArc($w - $d, 0, $d, $d, 270, 90)
    $path.AddArc($w - $d, $w - $d, $d, $d, 0, 90)
    $path.AddArc(0, $w - $d, $d, $d, 90, 90)
    $path.CloseFigure()
    $g.FillPath($bg, $path)

    # Uc yilan basi = paralel baglanti
    $heads = @(
        @{ X = 0.30; Y = 0.30; C = @(34, 211, 238) },
        @{ X = 0.70; Y = 0.30; C = @(74, 222, 128) },
        @{ X = 0.50; Y = 0.24; C = @(167, 139, 250) }
    )
    $r = [float]($size * 0.085)
    foreach ($h in $heads) {
        $brush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, $h.C[0], $h.C[1], $h.C[2]))
        $cx = [float]($h.X * $size)
        $cy = [float]($h.Y * $size)
        $g.FillEllipse($brush, $cx - $r, $cy - $r, $r * 2, $r * 2)
        $brush.Dispose()
    }

    # Ortada asagi bakan ok
    $arrowColor = [System.Drawing.Color]::FromArgb(255, 226, 232, 240)
    $pen = New-Object System.Drawing.Pen($arrowColor, [float]([Math]::Max(1.5, $size * 0.06)))
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
    $g.DrawLine($pen, [float]($size * 0.5), [float]($size * 0.38), [float]($size * 0.5), [float]($size * 0.62))
    $pts = @(
        (New-Object System.Drawing.PointF([float]($size * 0.5), [float]($size * 0.74))),
        (New-Object System.Drawing.PointF([float]($size * 0.34), [float]($size * 0.55))),
        (New-Object System.Drawing.PointF([float]($size * 0.66), [float]($size * 0.55)))
    )
    $g.FillPolygon((New-Object System.Drawing.SolidBrush($arrowColor)), $pts)

    $pen.Dispose()
    $g.Dispose()
    $path.Dispose()
    $bg.Dispose()
    return $bmp
}

function Save-Png([int]$size, [string]$name) {
    $bmp = New-HydraBitmap $size
    $bmp.Save((Join-Path $outDir $name), [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
}

Save-Png 32 "32x32.png"
Save-Png 128 "128x128.png"
Save-Png 256 "128x128@2x.png"

$icoBmp = New-HydraBitmap 256
$handle = $icoBmp.GetHicon()
$icon = [System.Drawing.Icon]::FromHandle($handle)
$stream = [System.IO.File]::Create((Join-Path $outDir "icon.ico"))
$icon.Save($stream)
$stream.Close()
$icon.Dispose()
$icoBmp.Dispose()

Write-Host "Ikonlar yazildi: $outDir"
