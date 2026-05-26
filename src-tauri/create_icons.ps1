$ErrorActionPreference = 'Stop'
$iconsDir = "c:\project\Jeeves\src-tauri\icons"

Add-Type -AssemblyName System.Drawing

function New-ProperIcon {
    param([string]$outPath)

    $sizes = @(16, 32, 48, 256)
    $images = @()

    foreach ($size in $sizes) {
        $bitmap = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
        $graphics.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit

        $bgColor = [System.Drawing.Color]::FromArgb(30, 64, 175)
        $graphics.Clear($bgColor)

        $fontSize = [Math]::Max(8, [int]($size * 0.45))
        $font = New-Object System.Drawing.Font("Arial", $fontSize, [System.Drawing.FontStyle]::Bold)
        $brush = [System.Drawing.Brushes]::White
        $sf = New-Object System.Drawing.StringFormat
        $sf.Alignment = [System.Drawing.StringAlignment]::Center
        $sf.LineAlignment = [System.Drawing.StringAlignment]::Center
        $rect = New-Object System.Drawing.RectangleF(0, 0, $size, $size)
        $graphics.DrawString("N", $font, $brush, $rect, $sf)

        $graphics.Dispose()
        $font.Dispose()
        $images += @{Size = $size; Bitmap = $bitmap}
    }

    $ms = New-Object System.IO.MemoryStream
    $bw = New-Object System.IO.BinaryWriter($ms)

    $bw.Write([Int16]0)
    $bw.Write([Int16]1)
    $bw.Write([Int16]$images.Count)

    $imageDataOffset = 6 + ($images.Count * 16)
    $imageDataList = @()

    foreach ($img in $images) {
        $imgMs = New-Object System.IO.MemoryStream
        $img.Bitmap.Save($imgMs, [System.Drawing.Imaging.ImageFormat]::Png)
        $imgData = $imgMs.ToArray()
        $imgMs.Dispose()
        $img.Bitmap.Dispose()

        $width = if ($img.Size -ge 256) { 0 } else { [byte]$img.Size }
        $height = if ($img.Size -ge 256) { 0 } else { [byte]$img.Size }

        $bw.Write([byte]$width)
        $bw.Write([byte]$height)
        $bw.Write([byte]0)
        $bw.Write([byte]0)
        $bw.Write([Int16]1)
        $bw.Write([Int16]32)
        $bw.Write([Int32]$imgData.Length)
        $bw.Write([Int32]$imageDataOffset)

        $imageDataOffset += $imgData.Length
        $imageDataList += ,$imgData
    }

    foreach ($imgData in $imageDataList) {
        $bw.Write($imgData)
    }

    $bw.Flush()
    [System.IO.File]::WriteAllBytes($outPath, $ms.ToArray())

    $bw.Dispose()
    $ms.Dispose()

    Write-Host "Created proper ICO: $outPath"
}

New-ProperIcon -outPath "$iconsDir\icon.ico"

$bitmap256 = New-Object System.Drawing.Bitmap(256, 256, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$g = [System.Drawing.Graphics]::FromImage($bitmap256)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.Clear([System.Drawing.Color]::FromArgb(30, 64, 175))
$font = New-Object System.Drawing.Font("Arial", 110, [System.Drawing.FontStyle]::Bold)
$g.DrawString("N", $font, [System.Drawing.Brushes]::White, 128, 128, (New-Object System.Drawing.StringFormat))
$g.Dispose()
$font.Dispose()

$bitmap256.Save("$iconsDir\32x32.png", [System.Drawing.Imaging.ImageFormat]::Png)
$bitmap128 = New-Object System.Drawing.Bitmap($bitmap256, 128, 128)
$bitmap128.Save("$iconsDir\128x128.png", [System.Drawing.Imaging.ImageFormat]::Png)
$bitmap128.Dispose()
$bitmap256.Dispose()

Copy-Item "$iconsDir\32x32.png" "$iconsDir\128x128@2x.png" -Force
Copy-Item "$iconsDir\32x32.png" "$iconsDir\icon.icns" -Force

Write-Host "All icons created successfully!"
