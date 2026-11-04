using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using System.Windows.Automation;
using System.Windows.Threading;

namespace Lshot
{
    internal sealed class HoverWatcher : IDisposable
    {
        private static readonly TimeSpan IdleInterval = TimeSpan.FromMilliseconds(130);
        private static readonly TimeSpan ActiveInterval = TimeSpan.FromMilliseconds(70);

        private readonly PreviewWindow _preview;
        private readonly DispatcherTimer _timer;
        private bool _paused;
        private bool _disposed;
        private bool _resolving;
        private bool _shiftWasDown;
        private bool _suppressUntilShiftRelease;
        private int _generation;
        private int _lastX = int.MinValue;
        private int _lastY = int.MinValue;
        private string? _currentPreviewPath;

        public HoverWatcher(PreviewWindow preview)
        {
            _preview = preview;
            _timer = new DispatcherTimer(DispatcherPriority.Background)
            {
                Interval = IdleInterval
            };
            _timer.Tick += OnTick;
            _timer.Start();
        }

        public bool Paused
        {
            get => _paused;
            set
            {
                if (_paused == value)
                {
                    return;
                }

                _paused = value;
                _generation++;

                if (value)
                {
                    _preview.HidePreview();
                    _currentPreviewPath = null;
                    _lastX = int.MinValue;
                    _lastY = int.MinValue;
                }
                else if (Native.IsShiftDown())
                {
                    _suppressUntilShiftRelease = true;
                    _shiftWasDown = true;
                    _lastX = int.MinValue;
                    _lastY = int.MinValue;
                }
            }
        }

        private async void OnTick(object? sender, EventArgs e)
        {
            if (_disposed || _paused)
            {
                return;
            }

            try
            {
                bool shiftDown = Native.IsShiftDown();

                if (_suppressUntilShiftRelease)
                {
                    if (shiftDown)
                    {
                        _timer.Interval = IdleInterval;
                        return;
                    }
                    _suppressUntilShiftRelease = false;
                }

                if (!shiftDown)
                {
                    if (_shiftWasDown || _currentPreviewPath != null)
                    {
                        _preview.HidePreview();
                        _currentPreviewPath = null;
                    }
                    _shiftWasDown = false;
                    _lastX = int.MinValue;
                    _lastY = int.MinValue;
                    _timer.Interval = IdleInterval;
                    return;
                }

                _timer.Interval = ActiveInterval;

                if (!_shiftWasDown)
                {
                    _shiftWasDown = true;
                    _lastX = int.MinValue;
                    _lastY = int.MinValue;
                }

                if (_resolving)
                {
                    return;
                }

                if (!Native.GetCursorPosition(out Native.POINT cursor))
                {
                    return;
                }

                bool moved = _lastX == int.MinValue ||
                             Math.Abs(cursor.X - _lastX) >= 3 ||
                             Math.Abs(cursor.Y - _lastY) >= 3;
                if (!moved)
                {
                    return;
                }

                _lastX = cursor.X;
                _lastY = cursor.Y;

                if (!Native.GetRootWindowAt(cursor, out IntPtr root, out bool isDesktop))
                {
                    if (_currentPreviewPath != null)
                    {
                        _preview.HidePreview();
                        _currentPreviewPath = null;
                    }
                    return;
                }

                _resolving = true;
                int generation = _generation;
                string? path;
                try
                {
                    path = await Task.Run(() => HoverResolver.Resolve(cursor, root, isDesktop)).ConfigureAwait(true);
                }
                finally
                {
                    _resolving = false;
                }

                if (_disposed || _paused || generation != _generation)
                {
                    return;
                }

                if (!Native.IsShiftDown())
                {
                    _preview.HidePreview();
                    _currentPreviewPath = null;
                    _shiftWasDown = false;
                    return;
                }

                if (!Native.GetCursorPosition(out Native.POINT current))
                {
                    return;
                }

                if (Math.Abs(current.X - cursor.X) > 10 || Math.Abs(current.Y - cursor.Y) > 10)
                {
                    return;
                }

                if (path == null)
                {
                    if (_currentPreviewPath != null)
                    {
                        _preview.HidePreview();
                        _currentPreviewPath = null;
                    }
                    return;
                }

                if (!string.Equals(path, _currentPreviewPath, StringComparison.OrdinalIgnoreCase))
                {
                    if (_preview.ShowFile(path, current))
                    {
                        _currentPreviewPath = path;
                    }
                    else
                    {
                        _preview.HidePreview();
                        _currentPreviewPath = null;
                    }
                }
            }
            catch
            {
                _preview.HidePreview();
                _currentPreviewPath = null;
            }
        }

        public void Dispose()
        {
            if (_disposed)
            {
                return;
            }

            _disposed = true;
            _timer.Stop();
            _timer.Tick -= OnTick;
            _preview.HidePreview();
            _currentPreviewPath = null;
        }
    }

    internal static class HoverResolver
    {
        private static readonly HashSet<string> ImageExtensions = new HashSet<string>(StringComparer.OrdinalIgnoreCase)
        {
            ".jpg",
            ".jpeg",
            ".png",
            ".webp",
            ".gif",
            ".bmp"
        };

        private static readonly object Gate = new object();
        private static IntPtr _cachedWindow;
        private static string? _cachedFolder;
        private static DateTime _folderCacheExpiry;
        private static string _cachedItemsFolder = string.Empty;
        private static List<ItemEntry> _cachedItems = new List<ItemEntry>();
        private static DateTime _itemsCacheExpiry;

        public static string? Resolve(Native.POINT point, IntPtr rootWindow, bool isDesktop)
        {
            var candidates = new List<string>(CollectCandidateNames(point));
            string? resolved = TryResolveCandidates(candidates, rootWindow, isDesktop);
            if (resolved != null)
            {
                return resolved;
            }

            IntPtr windowUnderCursor = Native.GetWindowAt(point);
            string? accessibleName = Native.GetAccessibleItemName(point, windowUnderCursor);
            if (!string.IsNullOrWhiteSpace(accessibleName))
            {
                int countBefore = candidates.Count;
                AddCandidate(candidates, accessibleName);
                if (candidates.Count != countBefore)
                {
                    resolved = TryResolveCandidates(candidates, rootWindow, isDesktop);
                    if (resolved != null)
                    {
                        return resolved;
                    }
                }
            }

            return null;
        }

        private static string? TryResolveCandidates(List<string> candidates, IntPtr rootWindow, bool isDesktop)
        {
            if (candidates.Count == 0)
            {
                return null;
            }

            string[] names = candidates.ToArray();
            if (isDesktop)
            {
                return ResolveOnDesktop(names);
            }

            string? folder = GetExplorerFolder(rootWindow);
            if (string.IsNullOrEmpty(folder))
            {
                return null;
            }

            return MatchInFolder(folder, names);
        }

        private static string? ResolveOnDesktop(string[] candidates)
        {
            string userDesktop = Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
            if (!string.IsNullOrEmpty(userDesktop) && Directory.Exists(userDesktop))
            {
                string? found = MatchInFolder(userDesktop, candidates);
                if (found != null)
                {
                    return found;
                }
            }

            string commonDesktop = Environment.GetFolderPath(Environment.SpecialFolder.CommonDesktopDirectory);
            if (!string.IsNullOrEmpty(commonDesktop) && Directory.Exists(commonDesktop))
            {
                return MatchInFolder(commonDesktop, candidates);
            }

            return null;
        }

        private static string[] CollectCandidateNames(Native.POINT point)
        {
            var names = new List<string>();

            try
            {
                AutomationElement? element = AutomationElement.FromPoint(new System.Windows.Point(point.X, point.Y));
                AutomationElement? current = element;

                for (int depth = 0; depth < 4 && current != null; depth++)
                {
                    ControlType controlType;
                    string name = string.Empty;

                    try
                    {
                        controlType = current.Current.ControlType;
                        name = current.Current.Name ?? string.Empty;
                    }
                    catch
                    {
                        break;
                    }

                    if (!Equals(controlType, ControlType.Window))
                    {
                        AddCandidate(names, name);
                    }

                    AutomationElement? parent = null;
                    try
                    {
                        parent = TreeWalker.RawViewWalker.GetParent(current);
                    }
                    catch
                    {
                    }

                    if (parent == null)
                    {
                        break;
                    }

                    bool parentIsWindow = false;
                    try
                    {
                        parentIsWindow = Equals(parent.Current.ControlType, ControlType.Window);
                    }
                    catch
                    {
                    }

                    if (parentIsWindow)
                    {
                        break;
                    }

                    current = parent;
                }
            }
            catch
            {
            }

            return names.ToArray();
        }

        private static void AddCandidate(List<string> names, string rawName)
        {
            string name = (rawName ?? string.Empty).Trim();
            if (name.Length == 0 || name.Length > 260)
            {
                return;
            }

            if (name.StartsWith("Name: ", StringComparison.OrdinalIgnoreCase))
            {
                name = name.Substring(6).Trim();
            }

            if (name.EndsWith(" - Shortcut", StringComparison.OrdinalIgnoreCase))
            {
                name = name.Substring(0, name.Length - " - Shortcut".Length).Trim();
            }

            if (name.Length == 0)
            {
                return;
            }

            foreach (string existing in names)
            {
                if (string.Equals(existing, name, StringComparison.OrdinalIgnoreCase))
                {
                    return;
                }
            }

            names.Add(name);
        }

        private static string? GetExplorerFolder(IntPtr rootWindow)
        {
            if (rootWindow == IntPtr.Zero)
            {
                return null;
            }

            lock (Gate)
            {
                if (_cachedWindow == rootWindow &&
                    _cachedFolder != null &&
                    DateTime.UtcNow < _folderCacheExpiry)
                {
                    return _cachedFolder;
                }
            }

            string? folder = QueryShellWindows(rootWindow);
            if (string.IsNullOrEmpty(folder))
            {
                folder = QueryAddressBar(rootWindow);
            }

            if (!string.IsNullOrEmpty(folder) && !Directory.Exists(folder))
            {
                folder = null;
            }

            lock (Gate)
            {
                _cachedWindow = rootWindow;
                _cachedFolder = folder;
                _folderCacheExpiry = DateTime.UtcNow.AddSeconds(2);
            }

            return folder;
        }

        private static string? QueryShellWindows(IntPtr rootWindow)
        {
            object? shell = null;
            object? windows = null;

            try
            {
                Type? shellType = Type.GetTypeFromProgID("Shell.Application");
                if (shellType == null)
                {
                    return null;
                }

                shell = Activator.CreateInstance(shellType);
                if (shell == null)
                {
                    return null;
                }

                dynamic shellDispatch = shell;
                windows = shellDispatch.Windows();
                if (windows == null)
                {
                    return null;
                }

                dynamic windowCollection = windows;
                int count = (int)windowCollection.Count;

                for (int index = 0; index < count; index++)
                {
                    object? window = null;
                    try
                    {
                        window = windowCollection.Item(index);
                        if (window == null)
                        {
                            continue;
                        }

                        dynamic windowDispatch = window;
                        IntPtr windowHandle;
                        try
                        {
                            windowHandle = new IntPtr(Convert.ToInt64(windowDispatch.HWND));
                        }
                        catch
                        {
                            continue;
                        }

                        if (windowHandle != rootWindow)
                        {
                            continue;
                        }

                        string? folderPath = null;
                        try
                        {
                            dynamic document = windowDispatch.Document;
                            dynamic folder = document.Folder;
                            dynamic self = folder.Self;
                            folderPath = (string?)self.Path;
                        }
                        catch
                        {
                        }

                        if (string.IsNullOrEmpty(folderPath))
                        {
                            try
                            {
                                folderPath = UrlToLocalPath((string?)windowDispatch.LocationURL);
                            }
                            catch
                            {
                            }
                        }

                        if (!string.IsNullOrEmpty(folderPath) && Directory.Exists(folderPath))
                        {
                            return folderPath;
                        }
                    }
                    catch
                    {
                    }
                    finally
                    {
                        Native.ReleaseComObject(window);
                    }
                }
            }
            catch
            {
            }
            finally
            {
                Native.ReleaseComObject(windows);
                Native.ReleaseComObject(shell);
            }

            return null;
        }

        private static string? QueryAddressBar(IntPtr rootWindow)
        {
            try
            {
                AutomationElement window = AutomationElement.FromHandle(rootWindow);
                var editCondition = new PropertyCondition(AutomationElement.ControlTypeProperty, ControlType.Edit);
                AutomationElementCollection edits = window.FindAll(TreeScope.Descendants, editCondition);

                foreach (AutomationElement edit in edits)
                {
                    string? value = null;
                    try
                    {
                        if (edit.TryGetCurrentPattern(ValuePattern.Pattern, out object? pattern))
                        {
                            value = ((ValuePattern)pattern).Current.Value;
                        }
                    }
                    catch
                    {
                    }

                    string? path = NormalizePath(value);
                    if (path != null && Directory.Exists(path))
                    {
                        return path;
                    }
                }
            }
            catch
            {
            }

            return null;
        }

        private static string? NormalizePath(string? value)
        {
            if (string.IsNullOrWhiteSpace(value))
            {
                return null;
            }

            string path = value.Trim().Trim('"');
            if (path.StartsWith("file://", StringComparison.OrdinalIgnoreCase))
            {
                path = UrlToLocalPath(path) ?? path;
            }

            return path;
        }

        private static string? UrlToLocalPath(string? url)
        {
            if (string.IsNullOrWhiteSpace(url))
            {
                return null;
            }

            try
            {
                if (Uri.TryCreate(url, UriKind.Absolute, out Uri? uri) && uri.IsFile)
                {
                    return uri.LocalPath;
                }
            }
            catch
            {
            }

            return null;
        }

        private static string? MatchInFolder(string folder, string[] candidates)
        {
            foreach (string candidate in candidates)
            {
                try
                {
                    string directPath = Path.Combine(folder, candidate);
                    if (IsImageFile(directPath) && File.Exists(directPath))
                    {
                        return directPath;
                    }
                }
                catch
                {
                }
            }

            List<ItemEntry> items = GetFolderItems(folder);
            foreach (ItemEntry item in items)
            {
                foreach (string candidate in candidates)
                {
                    if (Matches(candidate, item.DisplayName, item.FullPath))
                    {
                        return item.FullPath;
                    }
                }
            }

            return null;
        }

        private static bool Matches(string candidate, string displayName, string fullPath)
        {
            string name = candidate.Trim();
            if (name.Length == 0)
            {
                return false;
            }

            if (string.Equals(name, displayName, StringComparison.OrdinalIgnoreCase))
            {
                return true;
            }

            string fileName = Path.GetFileName(fullPath);
            if (string.Equals(name, fileName, StringComparison.OrdinalIgnoreCase))
            {
                return true;
            }

            string stem = Path.GetFileNameWithoutExtension(fullPath);
            return string.Equals(name, stem, StringComparison.OrdinalIgnoreCase);
        }

        private static List<ItemEntry> GetFolderItems(string folder)
        {
            lock (Gate)
            {
                if (string.Equals(_cachedItemsFolder, folder, StringComparison.OrdinalIgnoreCase) &&
                    DateTime.UtcNow < _itemsCacheExpiry)
                {
                    return _cachedItems;
                }
            }

            var items = new List<ItemEntry>();
            object? shell = null;
            object? namespaceObject = null;
            object? collection = null;

            try
            {
                Type? shellType = Type.GetTypeFromProgID("Shell.Application");
                if (shellType != null)
                {
                    shell = Activator.CreateInstance(shellType);
                    if (shell != null)
                    {
                        dynamic shellDispatch = shell;
                        namespaceObject = shellDispatch.NameSpace(folder);
                        if (namespaceObject != null)
                        {
                            dynamic folderDispatch = namespaceObject;
                            collection = folderDispatch.Items();
                            if (collection != null)
                            {
                                dynamic itemCollection = collection;
                                int count = Math.Min((int)itemCollection.Count, 20000);

                                for (int index = 0; index < count; index++)
                                {
                                    object? item = null;
                                    try
                                    {
                                        item = itemCollection.Item(index);
                                        if (item == null)
                                        {
                                            continue;
                                        }

                                        dynamic itemDispatch = item;
                                        string? displayName = null;
                                        string? fullPath = null;

                                        try
                                        {
                                            displayName = (string?)itemDispatch.Name;
                                        }
                                        catch
                                        {
                                        }

                                        try
                                        {
                                            fullPath = (string?)itemDispatch.Path;
                                        }
                                        catch
                                        {
                                        }

                                        if (!string.IsNullOrEmpty(displayName) &&
                                            !string.IsNullOrEmpty(fullPath) &&
                                            IsImageFile(fullPath))
                                        {
                                            items.Add(new ItemEntry(displayName, fullPath));
                                        }
                                    }
                                    catch
                                    {
                                    }
                                    finally
                                    {
                                        Native.ReleaseComObject(item);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            catch
            {
            }
            finally
            {
                Native.ReleaseComObject(collection);
                Native.ReleaseComObject(namespaceObject);
                Native.ReleaseComObject(shell);
            }

            if (items.Count == 0)
            {
                try
                {
                    foreach (string filePath in Directory.EnumerateFiles(folder))
                    {
                        if (!IsImageFile(filePath))
                        {
                            continue;
                        }

                        items.Add(new ItemEntry(Path.GetFileNameWithoutExtension(filePath), filePath));
                        if (items.Count >= 20000)
                        {
                            break;
                        }
                    }
                }
                catch
                {
                }
            }

            lock (Gate)
            {
                _cachedItemsFolder = folder;
                _cachedItems = items;
                _itemsCacheExpiry = DateTime.UtcNow.AddSeconds(1.5);
            }

            return items;
        }

        private static bool IsImageFile(string path)
        {
            string extension = Path.GetExtension(path);
            return ImageExtensions.Contains(extension);
        }

        private sealed class ItemEntry
        {
            public ItemEntry(string displayName, string fullPath)
            {
                DisplayName = displayName;
                FullPath = fullPath;
            }

            public string DisplayName { get; }
            public string FullPath { get; }
        }
    }
}
