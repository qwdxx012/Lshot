using System;
using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Media.Imaging;
using WinForms = System.Windows.Forms;
using Drawing = System.Drawing;

namespace Lshot
{
    public partial class App : Application
    {
        private const int HotkeyRegion = 1;
        private const int HotkeyFullscreen = 2;
        private const int HotkeyClipboard = 3;

        private Mutex? _mutex;
        private bool _ownsMutex;
        private WinForms.NotifyIcon? _tray;
        private WinForms.ContextMenuStrip? _trayMenu;
        private Drawing.Icon? _trayIcon;
        private HwndSource? _hotkeySource;
        private SettingsWindow? _settingsWindow;
        private ClipboardHistoryWindow? _clipboardWindow;
        private PreviewWindow? _preview;
        private HoverWatcher? _hover;
        private RegionEditorWindow? _editor;
        private bool _captureInProgress;
        private bool _settingsWasVisible;
        private bool _clipboardWasVisible;
        private bool _isExiting;
        private string? _lastSavedPath;

        public AppSettings Settings { get; private set; } = new AppSettings();

        protected override void OnStartup(StartupEventArgs e)
        {
            base.OnStartup(e);

            _mutex = new Mutex(true, @"Local\Lshot.SingleInstance", out bool createdNew);
            _ownsMutex = createdNew;
            if (!createdNew)
            {
                Shutdown();
                return;
            }

            DispatcherUnhandledException += (_, args) =>
            {
                args.Handled = true;
                ShowBalloon(L.T("ErrorCaption"), args.Exception.Message, WinForms.ToolTipIcon.Error);
            };

            Settings = SettingsStore.Load();
            Settings.RunAtStartup = StartupManager.IsEnabled();
            L.SetLanguage(Settings.Language);

            StartupManager.CleanupLegacyEntry();
            if (Settings.RunAtStartup)
            {
                try
                {
                    StartupManager.SetEnabled(true);
                }
                catch
                {
                }
            }

            try
            {
                if (!File.Exists(SettingsStore.FilePath))
                {
                    SettingsStore.Save(Settings);
                }
            }
            catch
            {
            }

            CreateTray();
            CreatePreviewAndHover();
            CreateHotkeyWindow();
            RegisterHotkeys();
        }

        private void CreateTray()
        {
            _trayIcon = LoadApplicationIcon();
            _tray = new WinForms.NotifyIcon
            {
                Icon = _trayIcon,
                Text = L.T("TrayTooltip"),
                Visible = true
            };

            _tray.MouseUp += (_, args) =>
            {
                if (args.Button == WinForms.MouseButtons.Left)
                {
                    Dispatcher.BeginInvoke(new Action(OpenSettings));
                }
            };
            _tray.BalloonTipClicked += (_, _) => OpenLastSavedFileInExplorer();

            BuildTrayMenu();
        }

        private void BuildTrayMenu()
        {
            if (_tray == null)
            {
                return;
            }

            var menu = new WinForms.ContextMenuStrip
            {
                Font = new Drawing.Font("Segoe UI", 9F),
                ShowImageMargin = false
            };

            var settingsItem = new WinForms.ToolStripMenuItem(L.T("TraySettings"));
            settingsItem.Click += (_, _) => OpenSettings();

            var aboutItem = new WinForms.ToolStripMenuItem(L.T("TrayAbout"));
            aboutItem.Click += (_, _) => ShowAbout();

            var exitItem = new WinForms.ToolStripMenuItem(L.T("TrayExit"));
            exitItem.Click += (_, _) => ExitApplication();

            menu.Items.Add(settingsItem);
            menu.Items.Add(aboutItem);
            menu.Items.Add(exitItem);

            WinForms.ContextMenuStrip? oldMenu = _trayMenu;
            _trayMenu = menu;
            _tray.ContextMenuStrip = menu;
            _tray.Text = L.T("TrayTooltip");
            oldMenu?.Dispose();
        }

        private void ShowAbout()
        {
            string version = Assembly.GetExecutingAssembly().GetName().Version?.ToString(3) ?? "2.0.0";
            string text = L.T("AboutText", version);
            if (_settingsWindow != null && _settingsWindow.IsVisible)
            {
                MessageBox.Show(_settingsWindow, text, L.T("AboutCaption"), MessageBoxButton.OK, MessageBoxImage.Information);
            }
            else
            {
                MessageBox.Show(text, L.T("AboutCaption"), MessageBoxButton.OK, MessageBoxImage.Information);
            }
        }

        private const int BaseTrayIconSize = 24;
        private const int ContentThreshold = 30;

        private static int GetTrayIconSize()
        {
            double scale = 1.0;
            try
            {
                using (Drawing.Graphics graphics = Drawing.Graphics.FromHwnd(IntPtr.Zero))
                {
                    scale = graphics.DpiX / 96.0;
                }
            }
            catch
            {
            }

            return Math.Max(16, (int)Math.Round(BaseTrayIconSize * scale));
        }

        private static Drawing.Icon LoadApplicationIcon()
        {
            string baseDirectory = AppContext.BaseDirectory;

            string pngPath = Path.Combine(baseDirectory, "logo.png");
            if (File.Exists(pngPath))
            {
                try
                {
                    using (var source = new Drawing.Bitmap(pngPath))
                    {
                        return CreateIconFromBitmap(source);
                    }
                }
                catch
                {
                }
            }

            string iconPath = Path.Combine(baseDirectory, "logo.ico");
            if (File.Exists(iconPath))
            {
                try
                {
                    int iconSize = GetTrayIconSize();
                    using (var icon = new Drawing.Icon(iconPath, iconSize, iconSize))
                    {
                        return (Drawing.Icon)icon.Clone();
                    }
                }
                catch
                {
                }
            }

            using (Drawing.Bitmap fallback = DrawFallbackTrayBitmap())
            {
                return CreateIconFromBitmap(fallback);
            }
        }

        private static Drawing.Icon CreateIconFromBitmap(Drawing.Bitmap source)
        {
            using (Drawing.Bitmap cropped = CropEmptyMargins(source, 1))
            {
                ThresholdToPixelArt(cropped);

                int iconSize = GetTrayIconSize();
                using (var resized = new Drawing.Bitmap(iconSize, iconSize, Drawing.Imaging.PixelFormat.Format32bppArgb))
                using (Drawing.Graphics graphics = Drawing.Graphics.FromImage(resized))
                {
                    graphics.Clear(Drawing.Color.Transparent);
                    graphics.CompositingMode = Drawing.Drawing2D.CompositingMode.SourceCopy;
                    graphics.InterpolationMode = Drawing.Drawing2D.InterpolationMode.NearestNeighbor;
                    graphics.PixelOffsetMode = Drawing.Drawing2D.PixelOffsetMode.Half;
                    graphics.SmoothingMode = Drawing.Drawing2D.SmoothingMode.None;

                    using (var attributes = new Drawing.Imaging.ImageAttributes())
                    {
                        attributes.SetWrapMode(Drawing.Drawing2D.WrapMode.TileFlipXY);
                        graphics.DrawImage(
                            cropped,
                            new Drawing.Rectangle(0, 0, iconSize, iconSize),
                            0,
                            0,
                            cropped.Width,
                            cropped.Height,
                            Drawing.GraphicsUnit.Pixel,
                            attributes);
                    }

                    IntPtr handle = resized.GetHicon();
                    try
                    {
                        using (Drawing.Icon temporary = Drawing.Icon.FromHandle(handle))
                        {
                            return (Drawing.Icon)temporary.Clone();
                        }
                    }
                    finally
                    {
                        Native.DestroyIconHandle(handle);
                    }
                }
            }
        }

        private static void ThresholdToPixelArt(Drawing.Bitmap bitmap)
        {
            var bounds = new Drawing.Rectangle(0, 0, bitmap.Width, bitmap.Height);
            Drawing.Imaging.BitmapData data = bitmap.LockBits(
                bounds,
                Drawing.Imaging.ImageLockMode.ReadWrite,
                Drawing.Imaging.PixelFormat.Format32bppArgb);

            try
            {
                int stride = data.Stride;
                int bufferSize = stride * bitmap.Height;
                byte[] buffer = new byte[bufferSize];
                System.Runtime.InteropServices.Marshal.Copy(data.Scan0, buffer, 0, bufferSize);

                for (int y = 0; y < bitmap.Height; y++)
                {
                    int rowOffset = y * stride;
                    for (int x = 0; x < bitmap.Width; x++)
                    {
                        int offset = rowOffset + x * 4;
                        int luminance = (buffer[offset + 2] * 30 + buffer[offset + 1] * 59 + buffer[offset]) / 100;
                        byte value = luminance >= 48 ? (byte)255 : (byte)0;
                        buffer[offset] = value;
                        buffer[offset + 1] = value;
                        buffer[offset + 2] = value;
                    }
                }

                System.Runtime.InteropServices.Marshal.Copy(buffer, 0, data.Scan0, bufferSize);
            }
            finally
            {
                bitmap.UnlockBits(data);
            }
        }

        private static Drawing.Bitmap CropEmptyMargins(Drawing.Bitmap source, int padding)
        {
            Drawing.Bitmap working = source;
            bool disposeWorking = false;

            if (source.PixelFormat != Drawing.Imaging.PixelFormat.Format32bppArgb)
            {
                working = new Drawing.Bitmap(source.Width, source.Height, Drawing.Imaging.PixelFormat.Format32bppArgb);
                using (Drawing.Graphics graphics = Drawing.Graphics.FromImage(working))
                {
                    graphics.DrawImage(source, new Drawing.Rectangle(0, 0, source.Width, source.Height));
                }
                disposeWorking = true;
            }

            try
            {
                int minX = working.Width;
                int minY = working.Height;
                int maxX = -1;
                int maxY = -1;

                var bounds = new Drawing.Rectangle(0, 0, working.Width, working.Height);
                Drawing.Imaging.BitmapData data = working.LockBits(
                    bounds,
                    Drawing.Imaging.ImageLockMode.ReadOnly,
                    Drawing.Imaging.PixelFormat.Format32bppArgb);

                try
                {
                    int stride = data.Stride;
                    byte[] row = new byte[stride];
                    for (int y = 0; y < working.Height; y++)
                    {
                        IntPtr rowPointer = IntPtr.Add(data.Scan0, y * stride);
                        System.Runtime.InteropServices.Marshal.Copy(rowPointer, row, 0, stride);

                        for (int x = 0; x < working.Width; x++)
                        {
                            int offset = x * 4;
                            byte blue = row[offset];
                            byte green = row[offset + 1];
                            byte red = row[offset + 2];
                            byte alpha = row[offset + 3];

                            if (alpha > 24 && (red > ContentThreshold || green > ContentThreshold || blue > ContentThreshold))
                            {
                                if (x < minX)
                                {
                                    minX = x;
                                }
                                if (x > maxX)
                                {
                                    maxX = x;
                                }
                                if (y < minY)
                                {
                                    minY = y;
                                }
                                if (y > maxY)
                                {
                                    maxY = y;
                                }
                            }
                        }
                    }
                }
                finally
                {
                    working.UnlockBits(data);
                }

                if (maxX < 0)
                {
                    return new Drawing.Bitmap(working);
                }

                int contentWidth = maxX - minX + 1;
                int contentHeight = maxY - minY + 1;
                int maxSide = Math.Min(working.Width, working.Height);
                int side = Math.Min(Math.Max(contentWidth, contentHeight) + padding * 2, maxSide);

                int centerX = (minX + maxX) / 2;
                int centerY = (minY + maxY) / 2;
                int left = Math.Clamp(centerX - side / 2, 0, working.Width - side);
                int top = Math.Clamp(centerY - side / 2, 0, working.Height - side);

                return working.Clone(
                    new Drawing.Rectangle(left, top, side, side),
                    Drawing.Imaging.PixelFormat.Format32bppArgb);
            }
            finally
            {
                if (disposeWorking)
                {
                    working.Dispose();
                }
            }
        }

        private static Drawing.Bitmap DrawFallbackTrayBitmap()
        {
            const int scale = 8;
            const int grid = 32;

            var bitmap = new Drawing.Bitmap(grid * scale, grid * scale, Drawing.Imaging.PixelFormat.Format32bppArgb);
            using (Drawing.Graphics graphics = Drawing.Graphics.FromImage(bitmap))
            using (var black = new Drawing.SolidBrush(Drawing.Color.Black))
            using (var white = new Drawing.SolidBrush(Drawing.Color.White))
            {
                graphics.SmoothingMode = Drawing.Drawing2D.SmoothingMode.None;
                graphics.PixelOffsetMode = Drawing.Drawing2D.PixelOffsetMode.Half;
                graphics.Clear(Drawing.Color.Black);
                graphics.FillRectangle(black, 0, 0, grid * scale, grid * scale);

                void Fill(int x, int y, int width, int height)
                {
                    graphics.FillRectangle(white, x * scale, y * scale, width * scale, height * scale);
                }

                // Pixel-art dashed frame.
                for (int x = 1; x <= 27; x += 4)
                {
                    Fill(x, 1, 2, 2);
                    Fill(x, 29, 2, 2);
                }
                for (int y = 1; y <= 27; y += 4)
                {
                    Fill(1, y, 2, 2);
                    Fill(29, y, 2, 2);
                }

                // Letter L.
                Fill(10, 8, 4, 13);
                Fill(10, 17, 10, 4);

                // Pixel scissors over the bottom-right corner.
                Fill(18, 17, 2, 2);
                Fill(20, 19, 2, 2);
                Fill(22, 21, 2, 2);
                Fill(26, 17, 2, 2);
                Fill(24, 19, 2, 2);
                Fill(22, 21, 2, 2);
                Fill(18, 22, 4, 4);
                Fill(19, 23, 2, 2);
                Fill(25, 22, 4, 4);
                Fill(26, 23, 2, 2);
            }

            return bitmap;
        }

        private void CreatePreviewAndHover()
        {
            _preview = new PreviewWindow();
            _hover = new HoverWatcher(_preview);
        }

        private void CreateHotkeyWindow()
        {
            try
            {
                var parameters = new HwndSourceParameters("LshotHotkeyWindow")
                {
                    Width = 0,
                    Height = 0,
                    WindowStyle = 0,
                    ExtendedWindowStyle = Native.WS_EX_TOOLWINDOW,
                    ParentWindow = Native.HWND_MESSAGE
                };

                _hotkeySource = new HwndSource(parameters);
                if (_hotkeySource.Handle == IntPtr.Zero)
                {
                    throw new InvalidOperationException("The hotkey message window could not be created.");
                }
                _hotkeySource.AddHook(HotkeyWndProc);
            }
            catch (Exception ex)
            {
                _hotkeySource?.Dispose();
                _hotkeySource = null;
                Dispatcher.BeginInvoke(new Action(() =>
                    ShowBalloon(L.T("ErrorCaption"), ex.Message, WinForms.ToolTipIcon.Warning)));
            }
        }

        private void RegisterHotkeys()
        {
            if (_hotkeySource == null || _isExiting)
            {
                return;
            }

            Native.UnregisterGlobalHotKey(_hotkeySource.Handle, HotkeyRegion);
            Native.UnregisterGlobalHotKey(_hotkeySource.Handle, HotkeyFullscreen);
            Native.UnregisterGlobalHotKey(_hotkeySource.Handle, HotkeyClipboard);

            HotkeyGesture? region = HotkeyGesture.Parse(Settings.RegionHotkey);
            HotkeyGesture? fullscreen = HotkeyGesture.Parse(Settings.FullscreenHotkey);
            HotkeyGesture? clipboard = HotkeyGesture.Parse(Settings.ClipboardHotkey);
            if (region == null || fullscreen == null || clipboard == null)
            {
                ShowBalloon(L.T("ErrorCaption"), L.T("HotkeyInvalid"), WinForms.ToolTipIcon.Warning);
                return;
            }

            var failures = new System.Collections.Generic.List<string>();
            if (!Native.RegisterGlobalHotKey(
                _hotkeySource.Handle,
                HotkeyRegion,
                region.Modifiers | Native.MOD_NOREPEAT,
                region.VirtualKey))
            {
                failures.Add(region.Display);
            }

            if (!Native.RegisterGlobalHotKey(
                _hotkeySource.Handle,
                HotkeyFullscreen,
                fullscreen.Modifiers | Native.MOD_NOREPEAT,
                fullscreen.VirtualKey))
            {
                failures.Add(fullscreen.Display);
            }

            if (!Native.RegisterGlobalHotKey(
                _hotkeySource.Handle,
                HotkeyClipboard,
                clipboard.Modifiers | Native.MOD_NOREPEAT,
                clipboard.VirtualKey))
            {
                failures.Add(clipboard.Display);
            }

            if (failures.Count > 0)
            {
                ShowBalloon(
                    L.T("ErrorCaption"),
                    L.T("HotkeyRegisterError", string.Join(", ", failures)),
                    WinForms.ToolTipIcon.Warning);
            }
        }

        private void UnregisterHotkeys()
        {
            if (_hotkeySource == null)
            {
                return;
            }

            Native.UnregisterGlobalHotKey(_hotkeySource.Handle, HotkeyRegion);
            Native.UnregisterGlobalHotKey(_hotkeySource.Handle, HotkeyFullscreen);
            Native.UnregisterGlobalHotKey(_hotkeySource.Handle, HotkeyClipboard);
        }

        private IntPtr HotkeyWndProc(IntPtr hwnd, int message, IntPtr wParam, IntPtr lParam, ref bool handled)
        {
            if (message == Native.WM_HOTKEY)
            {
                int id = wParam.ToInt32();
                if (id == HotkeyRegion)
                {
                    handled = true;
                    _ = CaptureRegionAsync();
                }
                else if (id == HotkeyFullscreen)
                {
                    handled = true;
                    _ = CaptureFullscreenAsync();
                }
                else if (id == HotkeyClipboard)
                {
                    handled = true;
                    ToggleClipboardHistory();
                }
            }

            return IntPtr.Zero;
        }

        private void ToggleClipboardHistory()
        {
            if (_isExiting || _captureInProgress)
            {
                return;
            }

            if (_clipboardWindow != null)
            {
                _clipboardWindow.Close();
                return;
            }

            var window = new ClipboardHistoryWindow();
            window.Closed += (_, _) =>
            {
                if (ReferenceEquals(_clipboardWindow, window))
                {
                    _clipboardWindow = null;
                }
            };

            _clipboardWindow = window;
            window.Show();
            window.Activate();
        }

        private void OpenSettings()
        {
            if (_isExiting || _captureInProgress)
            {
                return;
            }

            if (_clipboardWindow != null)
            {
                _clipboardWindow.Close();
            }

            if (_settingsWindow != null)
            {
                if (!_settingsWindow.IsVisible)
                {
                    _settingsWindow.Show();
                }
                _settingsWindow.Activate();
                return;
            }

            UnregisterHotkeys();

            var window = new SettingsWindow(Settings, ApplySettings, PreviewLanguage);
            window.Closed += (_, _) =>
            {
                if (!ReferenceEquals(_settingsWindow, window))
                {
                    return;
                }

                _settingsWindow = null;
                if (_isExiting)
                {
                    return;
                }

                L.SetLanguage(Settings.Language);
                BuildTrayMenu();
                RegisterHotkeys();
            };

            _settingsWindow = window;
            window.Show();
            window.Activate();
        }

        private void PreviewLanguage(string language)
        {
            L.SetLanguage(language);
            BuildTrayMenu();
            _clipboardWindow?.Relocalize();
        }

        private string? ApplySettings(AppSettings candidate)
        {
            candidate.Normalize();

            try
            {
                StartupManager.SetEnabled(candidate.RunAtStartup);
            }
            catch (Exception ex)
            {
                return L.T("StartupError", ex.Message);
            }

            try
            {
                SettingsStore.Save(candidate);
            }
            catch (Exception ex)
            {
                return L.T("SettingsSaveError", ex.Message);
            }

            Settings = candidate;
            L.SetLanguage(candidate.Language);
            BuildTrayMenu();
            _clipboardWindow?.Relocalize();
            return null;
        }

        private async Task HideOwnWindowsAsync()
        {
            _preview?.HidePreview();
            _settingsWasVisible = _settingsWindow != null && _settingsWindow.IsVisible;
            if (_settingsWasVisible)
            {
                _settingsWindow!.Hide();
            }

            _clipboardWasVisible = _clipboardWindow != null && _clipboardWindow.IsVisible;
            if (_clipboardWasVisible)
            {
                _clipboardWindow!.Hide();
            }

            await Task.Delay(160);
        }

        private void RestoreOwnWindows()
        {
            if (_settingsWasVisible && _settingsWindow != null)
            {
                _settingsWindow.Show();
                _settingsWindow.Activate();
            }
            _settingsWasVisible = false;

            if (_clipboardWasVisible && _clipboardWindow != null)
            {
                _clipboardWindow.Show();
                _clipboardWindow.Activate();
                _clipboardWindow.RefreshEntries();
            }
            _clipboardWasVisible = false;
        }

        private async Task CaptureRegionAsync()
        {
            if (_captureInProgress || _isExiting)
            {
                return;
            }

            _captureInProgress = true;
            if (_hover != null)
            {
                _hover.Paused = true;
            }

            try
            {
                await HideOwnWindowsAsync();
                Native.GetCursorPosition(out Native.POINT cursor);
                Native.MonitorData monitor = Native.GetMonitorFromPoint(cursor);
                BitmapSource frozen = ScreenCapture.Capture(monitor.Monitor);
                OpenEditor(frozen, monitor.Monitor, monitor.Scale, false);
            }
            catch (Exception ex)
            {
                ResetCaptureState();
                ShowBalloon(L.T("ErrorCaption"), ex.Message, WinForms.ToolTipIcon.Error);
            }
        }

        private async Task CaptureFullscreenAsync()
        {
            if (_captureInProgress || _isExiting)
            {
                return;
            }

            _captureInProgress = true;
            if (_hover != null)
            {
                _hover.Paused = true;
            }

            try
            {
                await HideOwnWindowsAsync();
                Native.GetCursorPosition(out Native.POINT cursor);
                Native.MonitorData monitor = Native.GetMonitorFromPoint(cursor);
                Native.RECT bounds = Native.GetVirtualScreenRect();
                BitmapSource frozen = ScreenCapture.Capture(bounds);
                OpenEditor(frozen, bounds, monitor.Scale, true);
            }
            catch (Exception ex)
            {
                ResetCaptureState();
                ShowBalloon(L.T("ErrorCaption"), ex.Message, WinForms.ToolTipIcon.Error);
            }
        }

        private void OpenEditor(BitmapSource image, Native.RECT bounds, double scale, bool fullSelection)
        {
            if (_editor != null)
            {
                return;
            }

            var editor = new RegionEditorWindow(image, bounds, scale, Settings, fullSelection);
            _editor = editor;
            editor.Saved += path =>
            {
                if (Settings.ShowSaveNotification)
                {
                    ShowSavedNotification(path);
                }
            };
            editor.Closed += (_, _) =>
            {
                if (!ReferenceEquals(_editor, editor))
                {
                    return;
                }

                _editor = null;
                _captureInProgress = false;
                if (_hover != null)
                {
                    _hover.Paused = false;
                }
                RestoreOwnWindows();
            };

            editor.Show();
            editor.Activate();
            editor.Focus();
        }

        private void ResetCaptureState()
        {
            if (_editor != null)
            {
                RegionEditorWindow editor = _editor;
                _editor = null;
                try
                {
                    editor.Close();
                }
                catch
                {
                }
            }

            _captureInProgress = false;
            if (_hover != null)
            {
                _hover.Paused = false;
            }
            RestoreOwnWindows();
        }

        private void ShowSavedNotification(string path)
        {
            try
            {
                if (!File.Exists(path))
                {
                    return;
                }
            }
            catch
            {
                return;
            }

            _lastSavedPath = path;
            string fileName = Path.GetFileName(path);
            string text = L.T("NotificationSavedText", fileName);
            if (text.Length > 240)
            {
                text = text.Substring(0, 237) + "...";
            }

            try
            {
                _tray?.ShowBalloonTip(6000, L.T("NotificationSavedTitle"), text, WinForms.ToolTipIcon.Info);
            }
            catch
            {
                _lastSavedPath = null;
            }
        }

        private void OpenLastSavedFileInExplorer()
        {
            string? path = _lastSavedPath;
            _lastSavedPath = null;
            if (string.IsNullOrEmpty(path) || !File.Exists(path))
            {
                return;
            }

            try
            {
                Process.Start(new ProcessStartInfo
                {
                    FileName = "explorer.exe",
                    Arguments = "/select,\"" + path + "\"",
                    UseShellExecute = true
                });
            }
            catch
            {
            }
        }

        private void ShowBalloon(string title, string text, WinForms.ToolTipIcon icon)
        {
            _lastSavedPath = null;
            if (_tray == null || _isExiting)
            {
                return;
            }

            try
            {
                if (text.Length > 250)
                {
                    text = text.Substring(0, 247) + "...";
                }
                _tray.ShowBalloonTip(4500, title, text, icon);
            }
            catch
            {
            }
        }

        private void ExitApplication()
        {
            if (_isExiting)
            {
                return;
            }
            _isExiting = true;
            if (_tray != null)
            {
                _tray.Visible = false;
            }
            Shutdown();
        }

        protected override void OnExit(ExitEventArgs e)
        {
            _isExiting = true;

            if (_hover != null)
            {
                _hover.Dispose();
                _hover = null;
            }

            if (_editor != null)
            {
                try
                {
                    _editor.Close();
                }
                catch
                {
                }
                _editor = null;
            }

            UnregisterHotkeys();
            if (_hotkeySource != null)
            {
                _hotkeySource.RemoveHook(HotkeyWndProc);
                _hotkeySource.Dispose();
                _hotkeySource = null;
            }

            if (_tray != null)
            {
                _tray.Visible = false;
                _tray.Dispose();
                _tray = null;
            }

            _trayMenu?.Dispose();
            _trayMenu = null;

            if (_trayIcon != null)
            {
                _trayIcon.Dispose();
                _trayIcon = null;
            }

            if (_preview != null)
            {
                try
                {
                    _preview.Close();
                }
                catch
                {
                }
                _preview = null;
            }

            if (_settingsWindow != null)
            {
                try
                {
                    _settingsWindow.Close();
                }
                catch
                {
                }
                _settingsWindow = null;
            }

            if (_clipboardWindow != null)
            {
                try
                {
                    _clipboardWindow.Close();
                }
                catch
                {
                }
                _clipboardWindow = null;
            }

            if (_ownsMutex && _mutex != null)
            {
                try
                {
                    _mutex.ReleaseMutex();
                }
                catch
                {
                }
            }
            _mutex?.Dispose();
            _mutex = null;

            base.OnExit(e);
        }
    }
}
