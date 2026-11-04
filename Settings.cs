using System;
using System.IO;
using System.Text.Json;
using Microsoft.Win32;

namespace Lshot
{
    public sealed class AppSettings
    {
        public string Language { get; set; } = "ru";
        public bool ShowSaveNotification { get; set; } = true;
        public string SaveFolder { get; set; } = DefaultFolder();
        public bool RunAtStartup { get; set; }
        public string RegionHotkey { get; set; } = HotkeyGesture.DefaultRegion;
        public string FullscreenHotkey { get; set; } = HotkeyGesture.DefaultFullscreen;
        public string ClipboardHotkey { get; set; } = HotkeyGesture.DefaultClipboard;
        public string ImageFormat { get; set; } = "png";
        public int Quality { get; set; } = 95;

        public static string DefaultFolder()
        {
            return Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.MyPictures), "Lshot");
        }

        public AppSettings Clone()
        {
            return new AppSettings
            {
                Language = Language,
                ShowSaveNotification = ShowSaveNotification,
                SaveFolder = SaveFolder,
                RunAtStartup = RunAtStartup,
                RegionHotkey = RegionHotkey,
                FullscreenHotkey = FullscreenHotkey,
                ClipboardHotkey = ClipboardHotkey,
                ImageFormat = ImageFormat,
                Quality = Quality
            };
        }

        public void Normalize()
        {
            Language = string.Equals(Language, "en", StringComparison.OrdinalIgnoreCase) ? "en" : "ru";

            if (string.IsNullOrWhiteSpace(SaveFolder))
            {
                SaveFolder = DefaultFolder();
            }
            else
            {
                SaveFolder = Environment.ExpandEnvironmentVariables(SaveFolder.Trim().Trim('"'));
            }

            ImageFormat = (ImageFormat ?? string.Empty).Trim().ToLowerInvariant().StartsWith("jp", StringComparison.Ordinal)
                ? "jpeg"
                : "png";

            Quality = Math.Clamp(Quality, 1, 100);

            RegionHotkey = HotkeyGesture.Normalize(RegionHotkey, HotkeyGesture.DefaultRegion);
            FullscreenHotkey = HotkeyGesture.Normalize(FullscreenHotkey, HotkeyGesture.DefaultFullscreen);
            ClipboardHotkey = HotkeyGesture.Normalize(ClipboardHotkey, HotkeyGesture.DefaultClipboard);

            if (string.Equals(RegionHotkey, FullscreenHotkey, StringComparison.OrdinalIgnoreCase))
            {
                FullscreenHotkey = HotkeyGesture.Parse(HotkeyGesture.DefaultFullscreen)!.Display;
                if (string.Equals(RegionHotkey, FullscreenHotkey, StringComparison.OrdinalIgnoreCase))
                {
                    RegionHotkey = HotkeyGesture.Parse(HotkeyGesture.DefaultRegion)!.Display;
                }
            }

            if (string.Equals(ClipboardHotkey, RegionHotkey, StringComparison.OrdinalIgnoreCase) ||
                string.Equals(ClipboardHotkey, FullscreenHotkey, StringComparison.OrdinalIgnoreCase))
            {
                ClipboardHotkey = HotkeyGesture.Parse(HotkeyGesture.DefaultClipboard)!.Display;
                if (string.Equals(ClipboardHotkey, RegionHotkey, StringComparison.OrdinalIgnoreCase) ||
                    string.Equals(ClipboardHotkey, FullscreenHotkey, StringComparison.OrdinalIgnoreCase))
                {
                    ClipboardHotkey = "Alt+C";
                }
                if (string.Equals(ClipboardHotkey, RegionHotkey, StringComparison.OrdinalIgnoreCase) ||
                    string.Equals(ClipboardHotkey, FullscreenHotkey, StringComparison.OrdinalIgnoreCase))
                {
                    ClipboardHotkey = "Ctrl+Alt+C";
                }
            }
        }
    }

    internal static class SettingsStore
    {
        private static readonly JsonSerializerOptions JsonOptions = new JsonSerializerOptions { WriteIndented = true };
        private static string? _overridePath;

        private static string PrimaryPath => Path.Combine(AppContext.BaseDirectory, "settings.json");

        private static string FallbackPath => Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "Lshot",
            "settings.json");

        public static string FilePath
        {
            get
            {
                if (_overridePath != null)
                {
                    return _overridePath;
                }

                string primary = PrimaryPath;
                if (File.Exists(primary))
                {
                    return primary;
                }

                string fallback = FallbackPath;
                if (File.Exists(fallback))
                {
                    _overridePath = fallback;
                    return fallback;
                }

                return primary;
            }
        }

        public static AppSettings Load()
        {
            try
            {
                string path = FilePath;
                if (File.Exists(path))
                {
                    AppSettings? settings = JsonSerializer.Deserialize<AppSettings>(File.ReadAllText(path));
                    if (settings != null)
                    {
                        settings.Normalize();
                        return settings;
                    }
                }
            }
            catch
            {
            }

            var defaults = new AppSettings();
            defaults.Normalize();
            return defaults;
        }

        public static void Save(AppSettings settings)
        {
            settings.Normalize();

            try
            {
                WriteTo(settings, FilePath);
            }
            catch (Exception ex) when (ex is UnauthorizedAccessException || ex is IOException)
            {
                string fallbackPath = FallbackPath;
                string? fallbackDirectory = Path.GetDirectoryName(fallbackPath);
                if (!string.IsNullOrEmpty(fallbackDirectory))
                {
                    Directory.CreateDirectory(fallbackDirectory);
                }
                _overridePath = fallbackPath;
                WriteTo(settings, fallbackPath);
            }
        }

        private static void WriteTo(AppSettings settings, string path)
        {
            string? directory = Path.GetDirectoryName(path);
            if (!string.IsNullOrEmpty(directory))
            {
                Directory.CreateDirectory(directory);
            }

            string json = JsonSerializer.Serialize(settings, JsonOptions);
            string tempPath = path + ".tmp";
            File.WriteAllText(tempPath, json);
            File.Move(tempPath, path, true);
        }
    }

    internal static class StartupManager
    {
        private const string RunKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Run";
        private const string ValueName = "Lshot";
        private const string LegacyValueName = "HoverShot";

        public static bool IsEnabled()
        {
            try
            {
                using RegistryKey? key = Registry.CurrentUser.OpenSubKey(RunKeyPath, false);
                return key?.GetValue(ValueName) is string value && !string.IsNullOrWhiteSpace(value);
            }
            catch
            {
                return false;
            }
        }

        public static void CleanupLegacyEntry()
        {
            try
            {
                using RegistryKey? key = Registry.CurrentUser.OpenSubKey(RunKeyPath, true);
                key?.DeleteValue(LegacyValueName, false);
            }
            catch
            {
            }
        }

        public static void SetEnabled(bool enabled)
        {
            using RegistryKey? key = Registry.CurrentUser.CreateSubKey(RunKeyPath, true);
            if (key == null)
            {
                throw new InvalidOperationException("Could not open the Windows startup registry key.");
            }

            if (!enabled)
            {
                key.DeleteValue(ValueName, false);
                return;
            }

            string? executable = Environment.ProcessPath;
            if (string.IsNullOrEmpty(executable))
            {
                throw new InvalidOperationException("Could not determine the executable path.");
            }

            key.SetValue(ValueName, "\"" + executable + "\"", RegistryValueKind.String);
        }
    }
}
