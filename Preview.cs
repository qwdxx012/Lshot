using System;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Effects;
using System.Windows.Media.Imaging;
using System.Windows.Threading;

namespace Lshot
{
    internal sealed class LoadedImage : IDisposable
    {
        public BitmapSource[] Frames { get; init; } = Array.Empty<BitmapSource>();
        public int[] Delays { get; init; } = Array.Empty<int>();
        public int Width { get; init; }
        public int Height { get; init; }
        public long FileBytes { get; init; }
        public string FormatName { get; init; } = string.Empty;
        public MemoryStream? Stream { get; init; }

        public void Dispose()
        {
            Stream?.Dispose();
        }
    }

    internal static class ImageLoader
    {
        private const int MaxGifFrames = 400;
        private const long MaxGifCompositionPixels = 45_000_000;

        public static LoadedImage? Load(string path)
        {
            MemoryStream? memory = null;
            try
            {
                memory = ReadToMemory(path);
                BitmapDecoder decoder = BitmapDecoder.Create(
                    memory,
                    BitmapCreateOptions.PreservePixelFormat,
                    BitmapCacheOption.OnLoad);

                if (decoder.Frames.Count == 0)
                {
                    memory.Dispose();
                    return null;
                }

                BitmapFrame first = decoder.Frames[0];
                long fileBytes = memory.Length;
                string formatName = GetFormatName(path);
                bool isGif = path.EndsWith(".gif", StringComparison.OrdinalIgnoreCase);

                long totalPixels = (long)decoder.Frames.Count * first.PixelWidth * first.PixelHeight;
                if (isGif &&
                    decoder.Frames.Count > 1 &&
                    decoder.Frames.Count <= MaxGifFrames &&
                    totalPixels <= MaxGifCompositionPixels &&
                    TryComposeGif(decoder, out BitmapSource[] composedFrames, out int[] composedDelays, out int composedWidth, out int composedHeight))
                {
                    return new LoadedImage
                    {
                        Frames = composedFrames,
                        Delays = composedDelays,
                        Width = composedWidth,
                        Height = composedHeight,
                        FileBytes = fileBytes,
                        FormatName = formatName,
                        Stream = memory
                    };
                }

                BitmapSource source = ApplyOrientation(first, first);
                if (source.PixelWidth > 1800 || source.PixelHeight > 1800)
                {
                    double scale = 1800.0 / Math.Max(source.PixelWidth, source.PixelHeight);
                    var reduced = new TransformedBitmap(source, new ScaleTransform(scale, scale));
                    reduced.Freeze();
                    source = reduced;
                }

                if (source.CanFreeze)
                {
                    source.Freeze();
                }

                return new LoadedImage
                {
                    Frames = new[] { source },
                    Delays = new[] { 100 },
                    Width = first.PixelWidth,
                    Height = first.PixelHeight,
                    FileBytes = fileBytes,
                    FormatName = formatName,
                    Stream = memory
                };
            }
            catch
            {
                memory?.Dispose();
                BitmapSource? thumbnail = Native.GetShellThumbnail(path, 1400, 1000);
                if (thumbnail == null)
                {
                    return null;
                }

                return new LoadedImage
                {
                    Frames = new[] { thumbnail },
                    Delays = new[] { 100 },
                    Width = thumbnail.PixelWidth,
                    Height = thumbnail.PixelHeight,
                    FileBytes = GetFileSize(path),
                    FormatName = GetFormatName(path)
                };
            }
        }

        private static MemoryStream ReadToMemory(string path)
        {
            using (var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete))
            {
                var memory = new MemoryStream();
                file.CopyTo(memory);
                memory.Position = 0;
                return memory;
            }
        }

        private static bool TryComposeGif(
            BitmapDecoder decoder,
            out BitmapSource[] frames,
            out int[] delays,
            out int width,
            out int height)
        {
            frames = Array.Empty<BitmapSource>();
            delays = Array.Empty<int>();
            width = 0;
            height = 0;

            try
            {
                int frameCount = decoder.Frames.Count;
                int maxRight = 0;
                int maxBottom = 0;
                for (int i = 0; i < frameCount; i++)
                {
                    BitmapFrame probe = decoder.Frames[i];
                    int left = Math.Max(0, ReadMetadataInt(probe, "/imgdesc/Left", 0));
                    int top = Math.Max(0, ReadMetadataInt(probe, "/imgdesc/Top", 0));
                    maxRight = Math.Max(maxRight, left + probe.PixelWidth);
                    maxBottom = Math.Max(maxBottom, top + probe.PixelHeight);
                }

                width = Math.Max(1, maxRight);
                height = Math.Max(1, maxBottom);
                long totalPixels = (long)width * height * frameCount;
                if (totalPixels > MaxGifCompositionPixels)
                {
                    return false;
                }

                int stride = width * 4;
                byte[] canvas = new byte[stride * height];
                var composed = new BitmapSource[frameCount];
                var frameDelays = new int[frameCount];

                int previousDisposal = 0;
                int previousLeft = 0;
                int previousTop = 0;
                int previousWidth = 0;
                int previousHeight = 0;
                bool hasPrevious = false;

                byte[]? restorePixels = null;
                int restoreLeft = 0;
                int restoreTop = 0;
                int restoreWidth = 0;
                int restoreHeight = 0;

                for (int i = 0; i < frameCount; i++)
                {
                    BitmapFrame frame = decoder.Frames[i];

                    if (hasPrevious)
                    {
                        if (previousDisposal == 2)
                        {
                            ClearRect(canvas, stride, width, height, previousLeft, previousTop, previousWidth, previousHeight);
                        }
                        else if (previousDisposal == 3 && restorePixels != null)
                        {
                            RestoreRect(canvas, stride, width, height, restorePixels, restoreLeft, restoreTop, restoreWidth, restoreHeight);
                        }
                    }

                    int left = Math.Max(0, ReadMetadataInt(frame, "/imgdesc/Left", 0));
                    int top = Math.Max(0, ReadMetadataInt(frame, "/imgdesc/Top", 0));
                    int descriptorWidth = Math.Max(0, ReadMetadataInt(frame, "/imgdesc/Width", frame.PixelWidth));
                    int descriptorHeight = Math.Max(0, ReadMetadataInt(frame, "/imgdesc/Height", frame.PixelHeight));
                    int disposal = ReadMetadataInt(frame, "/grctlext/Disposal", 0);

                    restorePixels = null;
                    if (disposal == 3)
                    {
                        restoreLeft = left;
                        restoreTop = top;
                        restoreWidth = Math.Min(descriptorWidth, width - left);
                        restoreHeight = Math.Min(descriptorHeight, height - top);
                        if (restoreWidth > 0 && restoreHeight > 0)
                        {
                            restorePixels = SnapshotRect(canvas, stride, width, height, restoreLeft, restoreTop, restoreWidth, restoreHeight);
                        }
                        else
                        {
                            restoreWidth = 0;
                            restoreHeight = 0;
                        }
                    }

                    int drawWidth = Math.Min(frame.PixelWidth, width - left);
                    int drawHeight = Math.Min(frame.PixelHeight, height - top);
                    if (drawWidth > 0 && drawHeight > 0)
                    {
                        byte[] framePixels = CopyFramePixels(frame, frame.PixelWidth, frame.PixelHeight);
                        BlendRect(canvas, stride, left, top, drawWidth, drawHeight, framePixels, frame.PixelWidth * 4);
                    }

                    byte[] snapshot = new byte[canvas.Length];
                    Buffer.BlockCopy(canvas, 0, snapshot, 0, canvas.Length);
                    BitmapSource composedFrame = BitmapSource.Create(
                        width,
                        height,
                        96,
                        96,
                        PixelFormats.Bgra32,
                        null,
                        snapshot,
                        stride);
                    composedFrame.Freeze();
                    composed[i] = composedFrame;
                    frameDelays[i] = ReadGifDelay(frame);

                    previousDisposal = disposal;
                    previousLeft = left;
                    previousTop = top;
                    previousWidth = descriptorWidth;
                    previousHeight = descriptorHeight;
                    hasPrevious = true;
                }

                frames = composed;
                delays = frameDelays;
                return true;
            }
            catch
            {
                frames = Array.Empty<BitmapSource>();
                delays = Array.Empty<int>();
                return false;
            }
        }

        private static byte[] CopyFramePixels(BitmapSource frame, int width, int height)
        {
            BitmapSource source = frame;
            if (source.Format != PixelFormats.Bgra32)
            {
                var converted = new FormatConvertedBitmap();
                converted.BeginInit();
                converted.Source = source;
                converted.DestinationFormat = PixelFormats.Bgra32;
                converted.EndInit();
                converted.Freeze();
                source = converted;
            }

            int stride = width * 4;
            var pixels = new byte[stride * height];
            source.CopyPixels(pixels, stride, 0);
            return pixels;
        }

        private static void BlendRect(
            byte[] canvas,
            int canvasStride,
            int left,
            int top,
            int width,
            int height,
            byte[] source,
            int sourceStride)
        {
            for (int y = 0; y < height; y++)
            {
                int sourceRow = y * sourceStride;
                int destinationRow = (top + y) * canvasStride + left * 4;
                for (int x = 0; x < width; x++)
                {
                    int s = sourceRow + x * 4;
                    int alpha = source[s + 3];
                    if (alpha == 0)
                    {
                        continue;
                    }

                    int d = destinationRow + x * 4;
                    if (alpha == 255)
                    {
                        canvas[d] = source[s];
                        canvas[d + 1] = source[s + 1];
                        canvas[d + 2] = source[s + 2];
                        canvas[d + 3] = 255;
                        continue;
                    }

                    int destinationAlpha = canvas[d + 3];
                    int inverse = 255 - alpha;
                    int denominator = alpha * 255 + destinationAlpha * inverse;
                    if (denominator <= 0)
                    {
                        continue;
                    }

                    canvas[d] = (byte)((source[s] * alpha * 255 + canvas[d] * destinationAlpha * inverse + denominator / 2) / denominator);
                    canvas[d + 1] = (byte)((source[s + 1] * alpha * 255 + canvas[d + 1] * destinationAlpha * inverse + denominator / 2) / denominator);
                    canvas[d + 2] = (byte)((source[s + 2] * alpha * 255 + canvas[d + 2] * destinationAlpha * inverse + denominator / 2) / denominator);
                    canvas[d + 3] = (byte)((denominator + 127) / 255);
                }
            }
        }

        private static void ClearRect(byte[] canvas, int stride, int canvasWidth, int canvasHeight, int left, int top, int width, int height)
        {
            int clippedLeft = Math.Clamp(left, 0, canvasWidth);
            int clippedTop = Math.Clamp(top, 0, canvasHeight);
            int clippedWidth = Math.Min(Math.Max(0, width), canvasWidth - clippedLeft);
            int clippedHeight = Math.Min(Math.Max(0, height), canvasHeight - clippedTop);
            for (int y = 0; y < clippedHeight; y++)
            {
                Array.Clear(canvas, (clippedTop + y) * stride + clippedLeft * 4, clippedWidth * 4);
            }
        }

        private static byte[] SnapshotRect(byte[] canvas, int stride, int canvasWidth, int canvasHeight, int left, int top, int width, int height)
        {
            int clippedLeft = Math.Clamp(left, 0, canvasWidth);
            int clippedTop = Math.Clamp(top, 0, canvasHeight);
            int clippedWidth = Math.Min(Math.Max(0, width), canvasWidth - clippedLeft);
            int clippedHeight = Math.Min(Math.Max(0, height), canvasHeight - clippedTop);
            int rowBytes = clippedWidth * 4;
            var snapshot = new byte[rowBytes * clippedHeight];
            for (int y = 0; y < clippedHeight; y++)
            {
                Buffer.BlockCopy(
                    canvas,
                    (clippedTop + y) * stride + clippedLeft * 4,
                    snapshot,
                    y * rowBytes,
                    rowBytes);
            }
            return snapshot;
        }

        private static void RestoreRect(byte[] canvas, int stride, int canvasWidth, int canvasHeight, byte[] snapshot, int left, int top, int width, int height)
        {
            int clippedLeft = Math.Clamp(left, 0, canvasWidth);
            int clippedTop = Math.Clamp(top, 0, canvasHeight);
            int clippedWidth = Math.Min(Math.Max(0, width), canvasWidth - clippedLeft);
            int clippedHeight = Math.Min(Math.Max(0, height), canvasHeight - clippedTop);
            int rowBytes = clippedWidth * 4;
            for (int y = 0; y < clippedHeight; y++)
            {
                Buffer.BlockCopy(
                    snapshot,
                    y * rowBytes,
                    canvas,
                    (clippedTop + y) * stride + clippedLeft * 4,
                    rowBytes);
            }
        }

        private static int ReadMetadataInt(BitmapFrame frame, string query, int fallback)
        {
            try
            {
                if (frame.Metadata is BitmapMetadata metadata)
                {
                    object? value = metadata.GetQuery(query);
                    switch (value)
                    {
                        case ushort ushortValue:
                            return ushortValue;
                        case short shortValue:
                            return shortValue;
                        case uint uintValue:
                            return (int)uintValue;
                        case int intValue:
                            return intValue;
                        case byte byteValue:
                            return byteValue;
                    }
                }
            }
            catch
            {
            }
            return fallback;
        }

        private static int ReadGifDelay(BitmapFrame frame)
        {
            int centiseconds = ReadMetadataInt(frame, "/grctlext/Delay", 0);
            if (centiseconds <= 1)
            {
                return 100;
            }
            return Math.Clamp(centiseconds * 10, 30, 1000);
        }

        private static long GetFileSize(string path)
        {
            try
            {
                return new FileInfo(path).Length;
            }
            catch
            {
                return 0;
            }
        }

        private static string GetFormatName(string path)
        {
            string extension = Path.GetExtension(path).TrimStart('.');
            return extension.Length == 0 ? "IMAGE" : extension.ToUpperInvariant();
        }

        private static BitmapSource ApplyOrientation(BitmapSource source, BitmapFrame frame)
        {
            ushort orientation = 1;
            try
            {
                if (frame.Metadata is BitmapMetadata metadata)
                {
                    object? value = metadata.GetQuery("/app1/ifd/{ushort=274}");
                    if (value is ushort parsed)
                    {
                        orientation = parsed;
                    }
                }
            }
            catch
            {
            }

            Transform? transform = orientation switch
            {
                3 => new RotateTransform(180),
                6 => new RotateTransform(90),
                8 => new RotateTransform(270),
                _ => null
            };

            if (transform == null)
            {
                return source;
            }

            try
            {
                var rotated = new TransformedBitmap(source, transform);
                rotated.Freeze();
                return rotated;
            }
            catch
            {
                return source;
            }
        }
    }

    internal sealed class PreviewWindow : Window
    {
        private const double MaxWindowWidth = 860;
        private const double MaxWindowHeight = 680;

        private readonly Border _root;
        private readonly Image _image;
        private readonly TextBlock _nameText;
        private readonly TextBlock _metaText;
        private readonly DispatcherTimer _animationTimer;
        private LoadedImage? _loaded;
        private BitmapSource[]? _frames;
        private int[]? _delays;
        private int _frameIndex;
        private string? _currentPath;
        private bool _handlePrepared;

        public PreviewWindow()
        {
            WindowStyle = WindowStyle.None;
            ResizeMode = ResizeMode.NoResize;
            ShowInTaskbar = false;
            ShowActivated = false;
            Topmost = true;
            AllowsTransparency = true;
            Background = Brushes.Transparent;
            WindowStartupLocation = WindowStartupLocation.Manual;
            Focusable = false;
            SnapsToDevicePixels = true;
            UseLayoutRounding = true;
            Left = -20000;
            Top = -20000;
            Width = 10;
            Height = 10;

            _image = new Image
            {
                Stretch = Stretch.Uniform,
                HorizontalAlignment = HorizontalAlignment.Center,
                VerticalAlignment = VerticalAlignment.Center,
                SnapsToDevicePixels = true
            };
            RenderOptions.SetBitmapScalingMode(_image, BitmapScalingMode.HighQuality);

            _nameText = new TextBlock
            {
                FontFamily = new FontFamily("Segoe UI"),
                FontSize = 13,
                FontWeight = FontWeights.SemiBold,
                Foreground = new SolidColorBrush(Color.FromRgb(0xF2, 0xF4, 0xF7)),
                TextTrimming = TextTrimming.CharacterEllipsis,
                TextWrapping = TextWrapping.NoWrap
            };

            _metaText = new TextBlock
            {
                FontFamily = new FontFamily("Segoe UI"),
                FontSize = 11,
                Foreground = new SolidColorBrush(Color.FromRgb(0xA8, 0xB0, 0xBC)),
                Margin = new Thickness(0, 3, 0, 0),
                TextTrimming = TextTrimming.CharacterEllipsis,
                TextWrapping = TextWrapping.NoWrap
            };

            var footer = new StackPanel
            {
                Orientation = Orientation.Vertical,
                Margin = new Thickness(4, 10, 4, 2)
            };
            footer.Children.Add(_nameText);
            footer.Children.Add(_metaText);

            var stack = new StackPanel { Orientation = Orientation.Vertical };
            stack.Children.Add(_image);
            stack.Children.Add(footer);

            _root = new Border
            {
                Background = new SolidColorBrush(Color.FromArgb(0xF4, 0x15, 0x16, 0x19)),
                BorderBrush = new SolidColorBrush(Color.FromArgb(0x55, 0xFF, 0xFF, 0xFF)),
                BorderThickness = new Thickness(1),
                CornerRadius = new CornerRadius(10),
                Padding = new Thickness(10),
                ClipToBounds = true,
                Effect = new DropShadowEffect
                {
                    BlurRadius = 26,
                    ShadowDepth = 6,
                    Opacity = 0.5,
                    Color = Colors.Black
                },
                Child = stack
            };

            Content = _root;

            _animationTimer = new DispatcherTimer(DispatcherPriority.Background);
            _animationTimer.Tick += OnAnimationTick;
        }

        public bool ShowFile(string path, Native.POINT cursor)
        {
            if (!string.Equals(_currentPath, path, StringComparison.OrdinalIgnoreCase) || _image.Source == null)
            {
                LoadedImage? loaded = ImageLoader.Load(path);
                if (loaded == null || loaded.Frames.Length == 0)
                {
                    loaded?.Dispose();
                    return false;
                }

                ApplyLoaded(path, loaded);
            }

            PrepareHandle();

            if (!IsVisible)
            {
                Opacity = 0;
                Show();
            }

            PositionAtCursor(cursor);
            Opacity = 1;

            if (_frames != null && _frames.Length > 1 && !_animationTimer.IsEnabled)
            {
                _frameIndex = 0;
                _image.Source = _frames[0];
                StartAnimation();
            }

            return true;
        }

        public void HidePreview()
        {
            _animationTimer.Stop();
            ReleaseLoaded();
            _currentPath = null;
            if (IsVisible)
            {
                Hide();
            }
        }

        private void ApplyLoaded(string path, LoadedImage loaded)
        {
            _animationTimer.Stop();
            ReleaseLoaded();

            _loaded = loaded;
            _currentPath = path;
            _frames = loaded.Frames;
            _delays = loaded.Delays;
            _frameIndex = 0;
            _image.Source = loaded.Frames[0];
            _nameText.Text = Path.GetFileName(path);
            _metaText.Text = string.Format(
                "{0} x {1}  •  {2}  •  {3}",
                loaded.Width,
                loaded.Height,
                FormatBytes(loaded.FileBytes),
                loaded.FormatName);
        }

        private void ReleaseLoaded()
        {
            _image.Source = null;
            _frames = null;
            _delays = null;
            _frameIndex = 0;
            _loaded?.Dispose();
            _loaded = null;
        }

        private void StartAnimation()
        {
            if (_frames == null || _delays == null || _frames.Length < 2)
            {
                return;
            }

            _animationTimer.Interval = TimeSpan.FromMilliseconds(Math.Clamp(_delays[0], 30, 1000));
            _animationTimer.Start();
        }

        private void OnAnimationTick(object? sender, EventArgs e)
        {
            if (_frames == null || _delays == null || _frames.Length < 2)
            {
                _animationTimer.Stop();
                return;
            }

            _frameIndex = (_frameIndex + 1) % _frames.Length;
            _image.Source = _frames[_frameIndex];
            _animationTimer.Interval = TimeSpan.FromMilliseconds(Math.Clamp(_delays[_frameIndex], 30, 1000));
        }

        private void PrepareHandle()
        {
            if (_handlePrepared)
            {
                return;
            }

            IntPtr hwnd = new WindowInteropHelper(this).EnsureHandle();
            Native.MakePreviewOverlay(hwnd);
            _handlePrepared = true;
        }

        private void PositionAtCursor(Native.POINT cursor)
        {
            Native.MonitorData monitor = Native.GetMonitorFromPoint(cursor);
            double scale = monitor.Scale;

            double availableWidth = Math.Max(260, monitor.Work.Width / scale);
            double availableHeight = Math.Max(200, monitor.Work.Height / scale);
            double maxWidth = Math.Min(MaxWindowWidth, availableWidth * 0.88);
            double maxHeight = Math.Min(MaxWindowHeight, availableHeight * 0.88);

            _image.MaxWidth = Math.Max(120, maxWidth - 22);
            _image.MaxHeight = Math.Max(100, maxHeight - 84);
            if (_image.Source is BitmapSource bitmap && bitmap.PixelWidth > 0 && bitmap.PixelHeight > 0)
            {
                double fit = Math.Min(1.0, Math.Min(_image.MaxWidth / bitmap.PixelWidth, _image.MaxHeight / bitmap.PixelHeight));
                _image.Width = Math.Max(1, Math.Round(bitmap.PixelWidth * fit));
                _image.Height = Math.Max(1, Math.Round(bitmap.PixelHeight * fit));
            }
            else
            {
                _image.Width = double.NaN;
                _image.Height = double.NaN;
            }

            double textWidth = Math.Min(700, Math.Max(180, maxWidth - 30));
            _nameText.MaxWidth = textWidth;
            _metaText.MaxWidth = textWidth;

            _root.Measure(new Size(maxWidth, maxHeight));
            Size desired = _root.DesiredSize;
            double widthDip = Math.Min(maxWidth, Math.Max(180, desired.Width));
            double heightDip = Math.Min(maxHeight, Math.Max(120, desired.Height));

            Width = widthDip;
            Height = heightDip;
            UpdateLayout();

            int widthPixels = Math.Max(1, (int)Math.Round(widthDip * scale));
            int heightPixels = Math.Max(1, (int)Math.Round(heightDip * scale));
            int margin = (int)Math.Round(18 * scale);

            int x = cursor.X + margin;
            int y = cursor.Y + margin;
            Native.RECT work = monitor.Work;

            if (x + widthPixels > work.Right)
            {
                x = cursor.X - widthPixels - margin;
            }
            if (y + heightPixels > work.Bottom)
            {
                y = cursor.Y - heightPixels - margin;
            }

            int minX = work.Left + 4;
            int minY = work.Top + 4;
            int maxX = Math.Max(minX, work.Right - widthPixels - 4);
            int maxY = Math.Max(minY, work.Bottom - heightPixels - 4);
            x = Math.Clamp(x, minX, maxX);
            y = Math.Clamp(y, minY, maxY);

            IntPtr hwnd = new WindowInteropHelper(this).Handle;
            Native.SetWindowPhysicalBounds(hwnd, x, y, widthPixels, heightPixels, true);
        }

        private static string FormatBytes(long bytes)
        {
            if (bytes <= 0)
            {
                return "0 B";
            }
            if (bytes < 1024)
            {
                return bytes + " B";
            }
            if (bytes < 1024 * 1024)
            {
                return (bytes / 1024.0).ToString("0.0") + " KB";
            }
            return (bytes / (1024.0 * 1024.0)).ToString("0.0") + " MB";
        }
    }
}
