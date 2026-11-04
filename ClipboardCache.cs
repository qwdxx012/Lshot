using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;

namespace Lshot
{
    internal sealed class ClipboardCacheEntry
    {
        public ClipboardCacheEntry(
            string filePath,
            long bytes,
            DateTime timestamp,
            int width,
            int height,
            BitmapSource thumbnail)
        {
            FilePath = filePath;
            Bytes = bytes;
            Timestamp = timestamp;
            Width = width;
            Height = height;
            Thumbnail = thumbnail;
        }

        public string FilePath { get; }
        public long Bytes { get; }
        public DateTime Timestamp { get; }
        public int Width { get; }
        public int Height { get; }
        public BitmapSource Thumbnail { get; }
    }

    internal static class ClipboardCache
    {
        private const int ThumbnailMaxWidth = 360;
        private const int ThumbnailMaxHeight = 220;

        public static string CacheDirectory => Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "Lshot",
            "ClipboardCache");

        public static string? Save(BitmapSource image)
        {
            try
            {
                Directory.CreateDirectory(CacheDirectory);
                string fileName = "Lshot_" +
                                  DateTime.Now.ToString("yyyyMMdd_HHmmss_fff", CultureInfo.InvariantCulture) +
                                  "_" +
                                  Guid.NewGuid().ToString("N").Substring(0, 8) +
                                  ".png";
                string path = Path.Combine(CacheDirectory, fileName);

                var encoder = new PngBitmapEncoder();
                encoder.Frames.Add(BitmapFrame.Create(image));
                using (var stream = new FileStream(path, FileMode.CreateNew, FileAccess.Write, FileShare.None))
                {
                    encoder.Save(stream);
                }

                return path;
            }
            catch
            {
                return null;
            }
        }

        public static List<ClipboardCacheEntry> LoadEntries()
        {
            var entries = new List<ClipboardCacheEntry>();

            try
            {
                string directory = CacheDirectory;
                if (!Directory.Exists(directory))
                {
                    return entries;
                }

                foreach (string path in Directory.EnumerateFiles(directory, "*.png"))
                {
                    ClipboardCacheEntry? entry = LoadEntry(path);
                    if (entry != null)
                    {
                        entries.Add(entry);
                    }
                }
            }
            catch
            {
            }

            entries.Sort((left, right) =>
            {
                int compare = right.Timestamp.CompareTo(left.Timestamp);
                return compare != 0 ? compare : string.CompareOrdinal(right.FilePath, left.FilePath);
            });

            return entries;
        }

        private static ClipboardCacheEntry? LoadEntry(string path)
        {
            try
            {
                var info = new FileInfo(path);
                if (!info.Exists || info.Length <= 0)
                {
                    return null;
                }

                using (var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete))
                {
                    BitmapDecoder decoder = BitmapDecoder.Create(
                        stream,
                        BitmapCreateOptions.PreservePixelFormat,
                        BitmapCacheOption.OnLoad);

                    if (decoder.Frames.Count == 0)
                    {
                        return null;
                    }

                    BitmapFrame frame = decoder.Frames[0];
                    int width = frame.PixelWidth;
                    int height = frame.PixelHeight;
                    double scale = Math.Min(
                        1.0,
                        Math.Min(
                            (double)ThumbnailMaxWidth / Math.Max(1, width),
                            (double)ThumbnailMaxHeight / Math.Max(1, height)));

                    BitmapSource thumbnail = frame;
                    if (scale < 1.0)
                    {
                        var reduced = new TransformedBitmap(frame, new ScaleTransform(scale, scale));
                        reduced.Freeze();
                        thumbnail = reduced;
                    }
                    else if (thumbnail.CanFreeze)
                    {
                        thumbnail.Freeze();
                    }

                    return new ClipboardCacheEntry(
                        path,
                        info.Length,
                        info.LastWriteTime,
                        width,
                        height,
                        thumbnail);
                }
            }
            catch
            {
                return null;
            }
        }

        public static long GetTotalBytes()
        {
            long total = 0;
            try
            {
                string directory = CacheDirectory;
                if (!Directory.Exists(directory))
                {
                    return 0;
                }

                foreach (string path in Directory.EnumerateFiles(directory))
                {
                    try
                    {
                        total += new FileInfo(path).Length;
                    }
                    catch
                    {
                    }
                }
            }
            catch
            {
            }

            return total;
        }

        public static bool Delete(string path, out string? error)
        {
            error = null;
            try
            {
                if (File.Exists(path))
                {
                    File.Delete(path);
                }
                return true;
            }
            catch (Exception ex)
            {
                error = ex.Message;
                return false;
            }
        }

        public static bool Clear(out string? error)
        {
            error = null;
            try
            {
                string directory = CacheDirectory;
                if (!Directory.Exists(directory))
                {
                    return true;
                }

                foreach (string path in Directory.EnumerateFiles(directory))
                {
                    try
                    {
                        File.Delete(path);
                    }
                    catch (Exception ex)
                    {
                        error ??= ex.Message;
                    }
                }
            }
            catch (Exception ex)
            {
                error ??= ex.Message;
            }

            return error == null;
        }

        public static BitmapSource LoadImage(string path)
        {
            using (var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete))
            {
                BitmapDecoder decoder = BitmapDecoder.Create(
                    stream,
                    BitmapCreateOptions.PreservePixelFormat,
                    BitmapCacheOption.OnLoad);

                if (decoder.Frames.Count == 0)
                {
                    throw new InvalidOperationException("The cached image is empty.");
                }

                BitmapFrame frame = decoder.Frames[0];
                if (frame.CanFreeze)
                {
                    frame.Freeze();
                }
                return frame;
            }
        }

        public static void CopyToClipboard(BitmapSource image)
        {
            Exception? lastError = null;
            for (int attempt = 0; attempt < 4; attempt++)
            {
                try
                {
                    Clipboard.SetImage(image);
                    return;
                }
                catch (ExternalException ex)
                {
                    lastError = ex;
                    Thread.Sleep(40);
                }
            }

            throw lastError ?? new InvalidOperationException("The clipboard is busy.");
        }

        public static string FormatSize(long bytes)
        {
            if (bytes <= 0)
            {
                return "0 " + L.T("UnitBytes");
            }
            if (bytes < 1024)
            {
                return bytes.ToString(CultureInfo.InvariantCulture) + " " + L.T("UnitBytes");
            }

            double kilobytes = bytes / 1024.0;
            if (kilobytes < 1024.0)
            {
                return kilobytes.ToString("0.#", CultureInfo.InvariantCulture) + " " + L.T("UnitKb");
            }

            double megabytes = kilobytes / 1024.0;
            if (megabytes < 1024.0)
            {
                return megabytes.ToString("0.#", CultureInfo.InvariantCulture) + " " + L.T("UnitMb");
            }

            return (megabytes / 1024.0).ToString("0.#", CultureInfo.InvariantCulture) + " " + L.T("UnitGb");
        }

        public static string FormatBufferSize(long bytes)
        {
            if (bytes <= 0)
            {
                return "0 " + L.T("UnitKb");
            }
            return FormatSize(bytes);
        }
    }
}
