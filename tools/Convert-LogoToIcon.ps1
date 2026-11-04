[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Source,

    [Parameter(Mandatory = $true)]
    [string]$Destination
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing

function ConvertTo-IconDib([System.Drawing.Bitmap]$Bitmap) {
    $width = $Bitmap.Width
    $height = $Bitmap.Height
    $rectangle = New-Object System.Drawing.Rectangle(0, 0, $width, $height)
    $bitmapData = $Bitmap.LockBits(
        $rectangle,
        [System.Drawing.Imaging.ImageLockMode]::ReadOnly,
        [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $stride = $bitmapData.Stride
        $pixels = New-Object byte[] ($stride * $height)
        [System.Runtime.InteropServices.Marshal]::Copy($bitmapData.Scan0, $pixels, 0, $pixels.Length)
    }
    finally {
        $Bitmap.UnlockBits($bitmapData)
    }

    $maskStride = [int]([Math]::Floor(($width + 31) / 32) * 4)
    $stream = New-Object System.IO.MemoryStream
    $writer = New-Object System.IO.BinaryWriter($stream)
    try {
        $writer.Write([uint32]40)
        $writer.Write([int32]$width)
        $writer.Write([int32]($height * 2))
        $writer.Write([uint16]1)
        $writer.Write([uint16]32)
        $writer.Write([uint32]0)
        $writer.Write([uint32]($width * $height * 4))
        $writer.Write([int32]0)
        $writer.Write([int32]0)
        $writer.Write([uint32]0)
        $writer.Write([uint32]0)

        for ($y = $height - 1; $y -ge 0; $y--) {
            $writer.Write($pixels, $y * $stride, $width * 4)
        }

        $maskRow = New-Object byte[] $maskStride
        for ($y = 0; $y -lt $height; $y++) {
            $writer.Write($maskRow)
        }

        $writer.Flush()
        return ,$stream.ToArray()
    }
    finally {
        $writer.Dispose()
        $stream.Dispose()
    }
}

$sourcePath = (Resolve-Path -LiteralPath $Source).Path
$destinationPath = [System.IO.Path]::GetFullPath($Destination)
$destinationDirectory = [System.IO.Path]::GetDirectoryName($destinationPath)
if ($destinationDirectory -and -not (Test-Path -LiteralPath $destinationDirectory)) {
    New-Item -ItemType Directory -Force -Path $destinationDirectory | Out-Null
}

$sizes = @(16, 20, 24, 32, 40, 48, 64, 128, 256)
$sourceImage = [System.Drawing.Bitmap]::FromFile($sourcePath)
$square = $null
$frames = @()

try {
    $minX = $sourceImage.Width
    $minY = $sourceImage.Height
    $maxX = -1
    $maxY = -1

    for ($y = 0; $y -lt $sourceImage.Height; $y++) {
        for ($x = 0; $x -lt $sourceImage.Width; $x++) {
            $pixel = $sourceImage.GetPixel($x, $y)
            if ($pixel.A -gt 24 -and ($pixel.R -gt 30 -or $pixel.G -gt 30 -or $pixel.B -gt 30)) {
                if ($x -lt $minX) { $minX = $x }
                if ($y -lt $minY) { $minY = $y }
                if ($x -gt $maxX) { $maxX = $x }
                if ($y -gt $maxY) { $maxY = $y }
            }
        }
    }

    if ($maxX -lt $minX -or $maxY -lt $minY) {
        $minX = 0
        $minY = 0
        $maxX = $sourceImage.Width - 1
        $maxY = $sourceImage.Height - 1
    }

    $contentWidth = $maxX - $minX + 1
    $contentHeight = $maxY - $minY + 1
    $side = [Math]::Max($contentWidth, $contentHeight) + 2

    $square = New-Object System.Drawing.Bitmap($side, $side, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $squareGraphics = [System.Drawing.Graphics]::FromImage($square)
    try {
        $squareGraphics.Clear([System.Drawing.Color]::Black)
        $squareGraphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::NearestNeighbor
        $squareGraphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::Half
        $offsetX = [int](($side - $contentWidth) / 2)
        $offsetY = [int](($side - $contentHeight) / 2)
        $destinationRectangle = New-Object System.Drawing.Rectangle($offsetX, $offsetY, $contentWidth, $contentHeight)
        $sourceRectangle = New-Object System.Drawing.Rectangle($minX, $minY, $contentWidth, $contentHeight)
        $squareGraphics.DrawImage($sourceImage, $destinationRectangle, $sourceRectangle, [System.Drawing.GraphicsUnit]::Pixel)
    }
    finally {
        $squareGraphics.Dispose()
    }

    foreach ($size in $sizes) {
        $frame = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $graphics = [System.Drawing.Graphics]::FromImage($frame)
        try {
            $graphics.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
            $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::NearestNeighbor
            $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::Half
            $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::None
            $graphics.DrawImage($square, 0, 0, $size, $size)

            if ($size -ge 256) {
                $stream = New-Object System.IO.MemoryStream
                try {
                    $frame.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
                    $data = $stream.ToArray()
                }
                finally {
                    $stream.Dispose()
                }
            }
            else {
                $data = ConvertTo-IconDib $frame
            }

            $frames += [PSCustomObject]@{
                Size = $size
                Data = $data
            }
        }
        finally {
            $graphics.Dispose()
            $frame.Dispose()
        }
    }

    $temporaryPath = $destinationPath + ".tmp"
    $fileStream = [System.IO.File]::Open($temporaryPath, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write)
    $writer = New-Object System.IO.BinaryWriter($fileStream)
    try {
        $writer.Write([uint16]0)
        $writer.Write([uint16]1)
        $writer.Write([uint16]$frames.Count)

        $offset = 6 + (16 * $frames.Count)
        foreach ($frameEntry in $frames) {
            $dimension = if ($frameEntry.Size -ge 256) { 0 } else { $frameEntry.Size }
            $writer.Write([byte]$dimension)
            $writer.Write([byte]$dimension)
            $writer.Write([byte]0)
            $writer.Write([byte]0)
            $writer.Write([uint16]1)
            $writer.Write([uint16]32)
            $writer.Write([uint32]$frameEntry.Data.Length)
            $writer.Write([uint32]$offset)
            $offset += $frameEntry.Data.Length
        }

        foreach ($frameEntry in $frames) {
            $writer.Write($frameEntry.Data)
        }
    }
    finally {
        $writer.Dispose()
        $fileStream.Dispose()
    }

    [System.IO.File]::Copy($temporaryPath, $destinationPath, $true)
    [System.IO.File]::Delete($temporaryPath)
    Write-Host "Generated $destinationPath"
    Write-Host ("Sizes: " + (($sizes | ForEach-Object { "$_x$_" }) -join ", "))
}
finally {
    if ($square -ne $null) { $square.Dispose() }
    $sourceImage.Dispose()
}
