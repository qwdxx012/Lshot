using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Text;
using Accessibility;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Imaging;

namespace Lshot
{
    internal static class Native
    {
        public const int WM_HOTKEY = 0x0312;

        public const uint MOD_ALT = 0x0001;
        public const uint MOD_CONTROL = 0x0002;
        public const uint MOD_SHIFT = 0x0004;
        public const uint MOD_WIN = 0x0008;
        public const uint MOD_NOREPEAT = 0x4000;

        public const uint VK_SNAPSHOT = 0x2C;
        public const uint VK_SHIFT = 0x10;

        public const int GA_ROOT = 2;

        public const int GWL_EXSTYLE = -20;
        public const int WS_EX_TOOLWINDOW = 0x00000080;
        public const int WS_EX_TRANSPARENT = 0x00000020;
        public const int WS_EX_NOACTIVATE = 0x08000000;
        public const int WS_EX_APPWINDOW = 0x00040000;

        public const uint SWP_NOSIZE = 0x0001;
        public const uint SWP_NOMOVE = 0x0002;
        public const uint SWP_NOACTIVATE = 0x0010;
        public const uint SWP_SHOWWINDOW = 0x0040;
        public const uint SWP_NOOWNERZORDER = 0x0200;

        public const int DWMWA_EXTENDED_FRAME_BOUNDS = 9;
        public const int DWMWA_USE_IMMERSIVE_DARK_MODE = 20;
        public const int DWMWA_USE_IMMERSIVE_DARK_MODE_OLD = 19;

        public const int SM_XVIRTUALSCREEN = 76;
        public const int SM_YVIRTUALSCREEN = 77;
        public const int SM_CXVIRTUALSCREEN = 78;
        public const int SM_CYVIRTUALSCREEN = 79;

        public const uint MONITOR_DEFAULTTONEAREST = 2;

        public const int SIIGBF_RESIZETOFIT = 0x00;
        public const int SIIGBF_BIGGERSIZEOK = 0x01;
        public const int SIIGBF_THUMBNAILONLY = 0x08;

        public static readonly IntPtr HWND_TOPMOST = new IntPtr(-1);
        public static readonly IntPtr HWND_MESSAGE = new IntPtr(-3);

        [StructLayout(LayoutKind.Sequential)]
        public struct POINT
        {
            public int X;
            public int Y;

            public POINT(int x, int y)
            {
                X = x;
                Y = y;
            }
        }

        [StructLayout(LayoutKind.Sequential)]
        public struct RECT
        {
            public int Left;
            public int Top;
            public int Right;
            public int Bottom;

            public int Width => Right - Left;
            public int Height => Bottom - Top;

            public bool IsEmpty => Width <= 0 || Height <= 0;

            public static RECT Intersect(RECT a, RECT b)
            {
                return new RECT
                {
                    Left = Math.Max(a.Left, b.Left),
                    Top = Math.Max(a.Top, b.Top),
                    Right = Math.Min(a.Right, b.Right),
                    Bottom = Math.Min(a.Bottom, b.Bottom)
                };
            }
        }

        [StructLayout(LayoutKind.Sequential)]
        public struct MONITORINFO
        {
            public int cbSize;
            public RECT rcMonitor;
            public RECT rcWork;
            public uint dwFlags;
        }

        public readonly struct MonitorData
        {
            public MonitorData(IntPtr handle, RECT monitor, RECT work, double scale)
            {
                Handle = handle;
                Monitor = monitor;
                Work = work;
                Scale = scale;
            }

            public IntPtr Handle { get; }
            public RECT Monitor { get; }
            public RECT Work { get; }
            public double Scale { get; }
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct SIZE
        {
            public int cx;
            public int cy;
        }

        [ComImport]
        [Guid("bcc18b79-ba16-442f-80c4-8a59c30c463b")]
        [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
        private interface IShellItemImageFactory
        {
            [PreserveSig]
            int GetImage(SIZE size, int flags, out IntPtr phbm);
        }

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GetCursorPos(out POINT lpPoint);

        [DllImport("user32.dll")]
        private static extern short GetAsyncKeyState(int vKey);

        [DllImport("user32.dll")]
        private static extern IntPtr WindowFromPoint(POINT point);

        [DllImport("user32.dll")]
        private static extern IntPtr GetAncestor(IntPtr hwnd, uint flags);

        [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);

        [DllImport("user32.dll")]
        private static extern IntPtr GetForegroundWindow();

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool IsIconic(IntPtr hWnd);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool IsWindow(IntPtr hWnd);

        [DllImport("user32.dll")]
        private static extern IntPtr MonitorFromPoint(POINT pt, uint dwFlags);

        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GetMonitorInfo(IntPtr hMonitor, ref MONITORINFO lpmi);

        [DllImport("user32.dll")]
        private static extern int GetSystemMetrics(int nIndex);

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool RegisterHotKey(IntPtr hWnd, int id, uint fsModifiers, uint vk);

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool UnregisterHotKey(IntPtr hWnd, int id);

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int x, int y, int cx, int cy, uint uFlags);

        [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW", SetLastError = true)]
        private static extern IntPtr GetWindowLongPtr(IntPtr hWnd, int nIndex);

        [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW", SetLastError = true)]
        private static extern IntPtr SetWindowLongPtr(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool DestroyIcon(IntPtr hIcon);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool SetForegroundWindow(IntPtr hWnd);

        [DllImport("user32.dll")]
        private static extern uint GetDpiForSystem();

        [DllImport("dwmapi.dll")]
        private static extern int DwmGetWindowAttribute(IntPtr hwnd, int dwAttribute, out RECT pvAttribute, int cbAttribute);

        [DllImport("dwmapi.dll")]
        private static extern int DwmSetWindowAttribute(IntPtr hwnd, int dwAttribute, ref int pvAttribute, int cbAttribute);

        [DllImport("shcore.dll")]
        private static extern int GetDpiForMonitor(IntPtr hmonitor, int dpiType, out uint dpiX, out uint dpiY);

        [DllImport("gdi32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool DeleteObject(IntPtr hObject);

        [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = false)]
        private static extern void SHCreateItemFromParsingName(
            [MarshalAs(UnmanagedType.LPWStr)] string pszPath,
            IntPtr pbc,
            ref Guid riid,
            [MarshalAs(UnmanagedType.Interface)] out IShellItemImageFactory ppv);

        [DllImport("oleacc.dll")]
        private static extern int AccessibleObjectFromWindow(
            IntPtr hwnd,
            uint dwObjectId,
            ref Guid riid,
            [MarshalAs(UnmanagedType.Interface)] out IAccessible? accessibleObject);

        public static IntPtr GetWindowAt(POINT point)
        {
            return WindowFromPoint(point);
        }

        public static string? GetAccessibleItemName(POINT screenPoint, IntPtr windowUnderPoint)
        {
            IAccessible? accessible = null;
            object? hit = null;
            try
            {
                Guid iid = new Guid("618736E0-3C3D-11CF-810C-00AA00389B71");
                int hr = AccessibleObjectFromWindow(windowUnderPoint, 0xFFFFFFFC, ref iid, out accessible);
                if (hr != 0 || accessible == null)
                {
                    return null;
                }

                try
                {
                    hit = accessible.accHitTest(screenPoint.X, screenPoint.Y);
                }
                catch
                {
                }

                if (hit is IAccessible child)
                {
                    try
                    {
                        return child.get_accName(0);
                    }
                    catch
                    {
                        return null;
                    }
                }

                if (hit is int childId && childId > 0)
                {
                    try
                    {
                        return accessible.get_accName(childId);
                    }
                    catch
                    {
                        return null;
                    }
                }
            }
            catch
            {
            }
            finally
            {
                if (hit is IAccessible hitAccessible && !ReferenceEquals(hitAccessible, accessible))
                {
                    ReleaseComObject(hitAccessible);
                }
                ReleaseComObject(accessible);
            }

            return null;
        }

        public static bool GetCursorPosition(out POINT point)
        {
            return GetCursorPos(out point);
        }

        public static bool IsShiftDown()
        {
            return (GetAsyncKeyState((int)VK_SHIFT) & 0x8000) != 0;
        }

        public static bool GetRootWindowAt(POINT point, out IntPtr root, out bool isDesktop)
        {
            root = IntPtr.Zero;
            isDesktop = false;

            IntPtr hwnd = WindowFromPoint(point);
            if (hwnd == IntPtr.Zero)
            {
                return false;
            }

            root = GetAncestor(hwnd, GA_ROOT);
            if (root == IntPtr.Zero)
            {
                root = hwnd;
            }

            string className = GetClassNameString(root);
            if (className.Equals("Progman", StringComparison.OrdinalIgnoreCase) ||
                className.Equals("WorkerW", StringComparison.OrdinalIgnoreCase))
            {
                isDesktop = true;
                return true;
            }

            if (className.Equals("CabinetWClass", StringComparison.OrdinalIgnoreCase) ||
                className.Equals("ExploreWClass", StringComparison.OrdinalIgnoreCase))
            {
                return true;
            }

            return false;
        }

        public static string GetClassNameString(IntPtr hwnd)
        {
            var sb = new StringBuilder(256);
            int length = GetClassName(hwnd, sb, sb.Capacity);
            return length > 0 ? sb.ToString(0, length) : string.Empty;
        }

        public static MonitorData GetMonitorFromPoint(POINT point)
        {
            return GetMonitorData(MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST));
        }

        private static MonitorData GetMonitorData(IntPtr monitor)
        {
            var info = new MONITORINFO { cbSize = Marshal.SizeOf<MONITORINFO>() };
            bool ok = monitor != IntPtr.Zero && GetMonitorInfo(monitor, ref info);
            if (!ok)
            {
                IntPtr primary = MonitorFromPoint(new POINT(0, 0), MONITOR_DEFAULTTONEAREST);
                info = new MONITORINFO { cbSize = Marshal.SizeOf<MONITORINFO>() };
                ok = primary != IntPtr.Zero && GetMonitorInfo(primary, ref info);
            }
            if (!ok)
            {
                RECT virtualScreen = GetVirtualScreenRect();
                info.rcMonitor = virtualScreen;
                info.rcWork = virtualScreen;
            }

            uint dpiX = 96;
            if (monitor == IntPtr.Zero || GetDpiForMonitor(monitor, 0, out dpiX, out _) != 0 || dpiX < 96)
            {
                dpiX = GetDpiForSystem();
                if (dpiX < 96)
                {
                    dpiX = 96;
                }
            }

            return new MonitorData(monitor, info.rcMonitor, info.rcWork, dpiX / 96.0);
        }

        public static RECT GetVirtualScreenRect()
        {
            int x = GetSystemMetrics(SM_XVIRTUALSCREEN);
            int y = GetSystemMetrics(SM_YVIRTUALSCREEN);
            int w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            int h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            return new RECT { Left = x, Top = y, Right = x + w, Bottom = y + h };
        }

        public static RECT GetCaptureBounds(IntPtr hwnd)
        {
            var rect = new RECT();
            if (DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, out rect, Marshal.SizeOf<RECT>()) != 0 || rect.IsEmpty)
            {
                if (!GetWindowRect(hwnd, out rect))
                {
                    return new RECT();
                }
            }
            return rect;
        }

        public static IntPtr GetForegroundWindowHandle()
        {
            return GetForegroundWindow();
        }

        public static bool IsWindowValid(IntPtr hwnd)
        {
            return hwnd != IntPtr.Zero && IsWindow(hwnd);
        }

        public static bool IsWindowMinimized(IntPtr hwnd)
        {
            return IsIconic(hwnd);
        }

        public static bool RegisterGlobalHotKey(IntPtr hwnd, int id, uint modifiers, uint virtualKey)
        {
            return RegisterHotKey(hwnd, id, modifiers, virtualKey);
        }

        public static void UnregisterGlobalHotKey(IntPtr hwnd, int id)
        {
            UnregisterHotKey(hwnd, id);
        }

        public static void SetWindowPhysicalBounds(IntPtr hwnd, int x, int y, int width, int height, bool showWithoutActivation)
        {
            uint flags = SWP_NOOWNERZORDER;
            if (showWithoutActivation)
            {
                flags |= SWP_NOACTIVATE | SWP_SHOWWINDOW;
            }
            SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, flags);
        }

        public static void MakePreviewOverlay(IntPtr hwnd)
        {
            long style = GetWindowLongPtr(hwnd, GWL_EXSTYLE).ToInt64();
            style |= WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE;
            style &= ~WS_EX_APPWINDOW;
            SetWindowLongPtr(hwnd, GWL_EXSTYLE, new IntPtr(style));
        }

        public static void MakeRegionOverlay(IntPtr hwnd)
        {
            long style = GetWindowLongPtr(hwnd, GWL_EXSTYLE).ToInt64();
            style |= WS_EX_TOOLWINDOW;
            style &= ~WS_EX_APPWINDOW;
            SetWindowLongPtr(hwnd, GWL_EXSTYLE, new IntPtr(style));
        }

        public static void EnableDarkTitleBar(IntPtr hwnd)
        {
            int enabled = 1;
            if (DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, ref enabled, sizeof(int)) != 0)
            {
                DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE_OLD, ref enabled, sizeof(int));
            }
        }

        public static void ActivateWindow(IntPtr hwnd)
        {
            SetForegroundWindow(hwnd);
        }

        public static void DestroyIconHandle(IntPtr hIcon)
        {
            if (hIcon != IntPtr.Zero)
            {
                DestroyIcon(hIcon);
            }
        }

        public static void ReleaseComObject(object? comObject)
        {
            if (comObject != null && Marshal.IsComObject(comObject))
            {
                try
                {
                    Marshal.FinalReleaseComObject(comObject);
                }
                catch
                {
                }
            }
        }

        public static BitmapSource? GetShellThumbnail(string path, int maxWidth, int maxHeight)
        {
            IShellItemImageFactory? factory = null;
            IntPtr hBitmap = IntPtr.Zero;
            try
            {
                Guid iid = typeof(IShellItemImageFactory).GUID;
                SHCreateItemFromParsingName(path, IntPtr.Zero, ref iid, out factory);
                int hr = factory.GetImage(
                    new SIZE { cx = maxWidth, cy = maxHeight },
                    SIIGBF_RESIZETOFIT | SIIGBF_BIGGERSIZEOK | SIIGBF_THUMBNAILONLY,
                    out hBitmap);

                if (hr != 0 || hBitmap == IntPtr.Zero)
                {
                    return null;
                }

                BitmapSource source = Imaging.CreateBitmapSourceFromHBitmap(
                    hBitmap,
                    IntPtr.Zero,
                    Int32Rect.Empty,
                    BitmapSizeOptions.FromEmptyOptions());
                source.Freeze();
                return source;
            }
            catch
            {
                return null;
            }
            finally
            {
                if (hBitmap != IntPtr.Zero)
                {
                    DeleteObject(hBitmap);
                }
                ReleaseComObject(factory);
            }
        }
    }

    internal static class ScreenCapture
    {
        public static BitmapSource Capture(Native.RECT region)
        {
            if (region.IsEmpty)
            {
                throw new ArgumentException("The capture region is empty.", nameof(region));
            }

            int width = region.Width;
            int height = region.Height;

            using (var bitmap = new Bitmap(width, height, System.Drawing.Imaging.PixelFormat.Format32bppRgb))
            {
                using (Graphics graphics = Graphics.FromImage(bitmap))
                {
                    graphics.CopyFromScreen(region.Left, region.Top, 0, 0, new System.Drawing.Size(width, height), CopyPixelOperation.SourceCopy);
                }

                BitmapData data = bitmap.LockBits(
                    new System.Drawing.Rectangle(0, 0, width, height),
                    ImageLockMode.ReadOnly,
                    System.Drawing.Imaging.PixelFormat.Format32bppRgb);

                try
                {
                    BitmapSource source = BitmapSource.Create(
                        width,
                        height,
                        96,
                        96,
                        PixelFormats.Bgr32,
                        null,
                        data.Scan0,
                        data.Stride * height,
                        data.Stride);
                    source.Freeze();
                    return source;
                }
                finally
                {
                    bitmap.UnlockBits(data);
                }
            }
        }
    }
}
