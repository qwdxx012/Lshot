using System;
using System.Collections.Generic;
using System.Windows.Input;

namespace Lshot
{
    internal sealed class HotkeyGesture : IEquatable<HotkeyGesture>
    {
        private static readonly Dictionary<string, Key> NamedKeys =
            new Dictionary<string, Key>(StringComparer.OrdinalIgnoreCase);

        private static readonly Dictionary<Key, string> DisplayNames =
            new Dictionary<Key, string>();

        static HotkeyGesture()
        {
            for (char c = 'A'; c <= 'Z'; c++)
            {
                var key = (Key)Enum.Parse(typeof(Key), c.ToString(), true);
                AddKey(key, c.ToString());
            }

            for (int i = 0; i <= 9; i++)
            {
                var key = (Key)Enum.Parse(typeof(Key), "D" + i.ToString(), true);
                AddKey(key, i.ToString());
            }

            for (int i = 1; i <= 24; i++)
            {
                var key = (Key)Enum.Parse(typeof(Key), "F" + i.ToString(), true);
                AddKey(key, "F" + i.ToString());
            }

            AddKey(Key.Snapshot, "PrintScreen");
            AddKey(Key.Insert, "Insert");
            AddKey(Key.Delete, "Delete");
            AddKey(Key.Home, "Home");
            AddKey(Key.End, "End");
            AddKey(Key.Prior, "PageUp");
            AddKey(Key.Next, "PageDown");
            AddKey(Key.Space, "Space");
            AddKey(Key.Return, "Enter");
            AddKey(Key.Tab, "Tab");
            AddKey(Key.Back, "Backspace");
            AddKey(Key.Left, "Left");
            AddKey(Key.Right, "Right");
            AddKey(Key.Up, "Up");
            AddKey(Key.Down, "Down");
            AddKey(Key.Escape, "Esc");
            AddKey(Key.Capital, "CapsLock");
            AddKey(Key.Pause, "Pause");
            AddKey(Key.Scroll, "ScrollLock");

            NamedKeys["Print Screen"] = Key.Snapshot;
            NamedKeys["PrtSc"] = Key.Snapshot;
            NamedKeys["Snapshot"] = Key.Snapshot;
            NamedKeys["PgUp"] = Key.Prior;
            NamedKeys["PgDn"] = Key.Next;
            NamedKeys["Escape"] = Key.Escape;
            NamedKeys["Return"] = Key.Return;
        }

        private HotkeyGesture(uint modifiers, uint virtualKey, string display)
        {
            Modifiers = modifiers;
            VirtualKey = virtualKey;
            Display = display;
        }

        public uint Modifiers { get; }
        public uint VirtualKey { get; }
        public string Display { get; }

        public static string DefaultRegion => "Ctrl+Shift+S";
        public static string DefaultFullscreen => "Alt+PrintScreen";
        public static string DefaultClipboard => "Ctrl+Shift+V";

        private static void AddKey(Key key, string display)
        {
            NamedKeys[display] = key;
            DisplayNames[key] = display;
        }

        public static HotkeyGesture? FromKey(Key key, ModifierKeys modifiers)
        {
            if (!DisplayNames.TryGetValue(key, out string? keyName))
            {
                return null;
            }

            uint windowsModifiers = 0;
            if ((modifiers & ModifierKeys.Control) != 0)
            {
                windowsModifiers |= Native.MOD_CONTROL;
            }
            if ((modifiers & ModifierKeys.Alt) != 0)
            {
                windowsModifiers |= Native.MOD_ALT;
            }
            if ((modifiers & ModifierKeys.Shift) != 0)
            {
                windowsModifiers |= Native.MOD_SHIFT;
            }
            if ((modifiers & ModifierKeys.Windows) != 0)
            {
                windowsModifiers |= Native.MOD_WIN;
            }

            uint virtualKey = (uint)KeyInterop.VirtualKeyFromKey(key);
            if (virtualKey == 0)
            {
                return null;
            }

            if (windowsModifiers == 0 && virtualKey != Native.VK_SNAPSHOT)
            {
                return null;
            }

            return new HotkeyGesture(windowsModifiers, virtualKey, BuildDisplay(windowsModifiers, keyName));
        }

        public static HotkeyGesture? Parse(string? text)
        {
            if (string.IsNullOrWhiteSpace(text))
            {
                return null;
            }

            string[] parts = text.Split(new[] { '+' }, StringSplitOptions.RemoveEmptyEntries);
            if (parts.Length == 0)
            {
                return null;
            }

            uint modifiers = 0;
            string? keyToken = null;

            foreach (string rawPart in parts)
            {
                string part = rawPart.Trim();
                if (part.Length == 0)
                {
                    continue;
                }

                if (part.Equals("Ctrl", StringComparison.OrdinalIgnoreCase) ||
                    part.Equals("Control", StringComparison.OrdinalIgnoreCase))
                {
                    modifiers |= Native.MOD_CONTROL;
                }
                else if (part.Equals("Alt", StringComparison.OrdinalIgnoreCase))
                {
                    modifiers |= Native.MOD_ALT;
                }
                else if (part.Equals("Shift", StringComparison.OrdinalIgnoreCase))
                {
                    modifiers |= Native.MOD_SHIFT;
                }
                else if (part.Equals("Win", StringComparison.OrdinalIgnoreCase) ||
                         part.Equals("Windows", StringComparison.OrdinalIgnoreCase))
                {
                    modifiers |= Native.MOD_WIN;
                }
                else
                {
                    if (keyToken != null)
                    {
                        return null;
                    }
                    keyToken = part;
                }
            }

            if (keyToken == null)
            {
                return null;
            }

            string normalized = keyToken.Replace(" ", string.Empty);
            if (!NamedKeys.TryGetValue(normalized, out Key key))
            {
                return null;
            }

            uint virtualKey = (uint)KeyInterop.VirtualKeyFromKey(key);
            if (virtualKey == 0)
            {
                return null;
            }

            if (modifiers == 0 && virtualKey != Native.VK_SNAPSHOT)
            {
                return null;
            }

            return new HotkeyGesture(modifiers, virtualKey, BuildDisplay(modifiers, DisplayNames[key]));
        }

        public static string Normalize(string? text, string fallback)
        {
            return Parse(text)?.Display ?? fallback;
        }

        private static string BuildDisplay(uint modifiers, string keyName)
        {
            var parts = new List<string>(5);
            if ((modifiers & Native.MOD_CONTROL) != 0)
            {
                parts.Add("Ctrl");
            }
            if ((modifiers & Native.MOD_ALT) != 0)
            {
                parts.Add("Alt");
            }
            if ((modifiers & Native.MOD_SHIFT) != 0)
            {
                parts.Add("Shift");
            }
            if ((modifiers & Native.MOD_WIN) != 0)
            {
                parts.Add("Win");
            }
            parts.Add(keyName);
            return string.Join("+", parts);
        }

        public bool Equals(HotkeyGesture? other)
        {
            return other != null && Modifiers == other.Modifiers && VirtualKey == other.VirtualKey;
        }

        public override bool Equals(object? obj)
        {
            return obj is HotkeyGesture other && Equals(other);
        }

        public override int GetHashCode()
        {
            unchecked
            {
                return ((int)Modifiers * 397) ^ (int)VirtualKey;
            }
        }
    }
}
