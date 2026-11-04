using System;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Interop;
using WinForms = System.Windows.Forms;

namespace Lshot
{
    public partial class SettingsWindow : Window
    {
        private readonly AppSettings _working;
        private readonly Func<AppSettings, string?> _apply;
        private readonly Action<string> _languagePreview;

        private HotkeyGesture? _regionGesture;
        private HotkeyGesture? _fullscreenGesture;
        private HotkeyGesture? _clipboardGesture;
        private TextBox? _capturingHotkeyBox;
        private bool _loading = true;

        public SettingsWindow(AppSettings current, Func<AppSettings, string?> apply, Action<string> languagePreview)
        {
            _working = current.Clone();
            _apply = apply;
            _languagePreview = languagePreview;

            InitializeComponent();

            _regionGesture = HotkeyGesture.Parse(_working.RegionHotkey) ??
                             HotkeyGesture.Parse(HotkeyGesture.DefaultRegion);
            _fullscreenGesture = HotkeyGesture.Parse(_working.FullscreenHotkey) ??
                                 HotkeyGesture.Parse(HotkeyGesture.DefaultFullscreen);
            _clipboardGesture = HotkeyGesture.Parse(_working.ClipboardHotkey) ??
                                HotkeyGesture.Parse(HotkeyGesture.DefaultClipboard);

            LanguageCombo.SelectedIndex = string.Equals(_working.Language, "en", StringComparison.OrdinalIgnoreCase) ? 1 : 0;
            FormatCombo.SelectedIndex = string.Equals(_working.ImageFormat, "jpeg", StringComparison.OrdinalIgnoreCase) ? 1 : 0;
            FolderBox.Text = _working.SaveFolder;
            NotifyCheck.IsChecked = _working.ShowSaveNotification;
            StartupCheck.IsChecked = StartupManager.IsEnabled();
            QualitySlider.Value = Math.Clamp(_working.Quality, 1, 100);
            QualityValue.Text = Math.Clamp(_working.Quality, 1, 100) + "%";
            RefreshHotkeyBoxes();

            _loading = false;
            ApplyLocalization();

            SourceInitialized += (_, _) => Native.EnableDarkTitleBar(new WindowInteropHelper(this).Handle);
        }

        private void ApplyLocalization()
        {
            Title = L.T("SettingsTitle");
            TitleText.Text = "Lshot";
            TaglineText.Text = L.T("AppTagline");

            GeneralTab.Header = L.T("TabGeneral");
            HotkeysTab.Header = L.T("TabHotkeys");
            FormatTab.Header = L.T("TabFormat");

            LanguageLabel.Text = L.T("Language");
            NotifyCheck.Content = L.T("ShowNotification");
            SaveFolderLabel.Text = L.T("SaveFolder");
            BrowseButton.Content = L.T("Browse");
            StartupCheck.Content = L.T("Autostart");

            RegionHotkeyLabel.Text = L.T("HotkeyRegion");
            FullscreenHotkeyLabel.Text = L.T("HotkeyFullscreen");
            ClipboardHotkeyLabel.Text = L.T("HotkeyClipboard");
            HotkeysHint.Text = L.T("HotkeysHint");
            HotkeysInfo.Text = L.T("HotkeysInfo");
            RegionHotkeyBox.ToolTip = L.T("HotkeysHint");
            FullscreenHotkeyBox.ToolTip = L.T("HotkeysHint");
            ClipboardHotkeyBox.ToolTip = L.T("HotkeysHint");

            FormatLabel.Text = L.T("ImageFormat");
            QualityLabel.Text = L.T("Quality");
            QualityHint.Text = L.T("QualityHint");

            SaveButton.Content = L.T("Save");
            CancelButton.Content = L.T("Cancel");
        }

        private void LanguageCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (_loading || LanguageCombo.SelectedItem is not ComboBoxItem item)
            {
                return;
            }

            string language = item.Tag as string ?? "ru";
            _working.Language = language;
            L.SetLanguage(language);
            ApplyLocalization();
            _languagePreview(language);
        }

        private void FormatCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (_loading || FormatCombo.SelectedItem is not ComboBoxItem item)
            {
                return;
            }

            _working.ImageFormat = item.Tag as string ?? "png";
        }

        private void QualitySlider_ValueChanged(object sender, RoutedPropertyChangedEventArgs<double> e)
        {
            int value = (int)Math.Round(e.NewValue);
            _working.Quality = Math.Clamp(value, 1, 100);
            if (!_loading && QualityValue != null)
            {
                QualityValue.Text = _working.Quality + "%";
            }
        }

        private void Browse_Click(object sender, RoutedEventArgs e)
        {
            using (var dialog = new WinForms.FolderBrowserDialog
            {
                Description = L.T("FolderBrowseDescription"),
                UseDescriptionForTitle = true,
                ShowNewFolderButton = true,
                SelectedPath = Directory.Exists(FolderBox.Text) ? FolderBox.Text : string.Empty
            })
            {
                if (dialog.ShowDialog() == WinForms.DialogResult.OK)
                {
                    FolderBox.Text = dialog.SelectedPath;
                }
            }

            Activate();
        }

        private void HotkeyBox_GotKeyboardFocus(object sender, KeyboardFocusChangedEventArgs e)
        {
            if (sender is not TextBox box)
            {
                return;
            }

            if (_capturingHotkeyBox != null && !ReferenceEquals(_capturingHotkeyBox, box))
            {
                EndHotkeyCapture();
            }

            _capturingHotkeyBox = box;
            box.Text = L.T("HotkeyPressPrompt");
        }

        private void HotkeyBox_LostKeyboardFocus(object sender, KeyboardFocusChangedEventArgs e)
        {
            if (sender is TextBox box && ReferenceEquals(_capturingHotkeyBox, box))
            {
                EndHotkeyCapture();
            }
        }

        private void HotkeyBox_PreviewKeyDown(object sender, KeyEventArgs e)
        {
            if (sender is not TextBox box)
            {
                return;
            }

            if (!ReferenceEquals(_capturingHotkeyBox, box))
            {
                _capturingHotkeyBox = box;
            }

            Key key = e.Key == Key.System ? e.SystemKey : e.Key;

            if (key == Key.Tab)
            {
                e.Handled = false;
                EndHotkeyCapture();
                return;
            }

            if (key == Key.Escape)
            {
                e.Handled = true;
                EndHotkeyCapture();
                return;
            }

            if (key == Key.Enter)
            {
                e.Handled = true;
                EndHotkeyCapture();
                return;
            }

            if (key == Key.Back || key == Key.Delete)
            {
                e.Handled = true;
                string tag = box.Tag as string ?? "region";
                string fallback = string.Equals(tag, "fullscreen", StringComparison.Ordinal)
                    ? HotkeyGesture.DefaultFullscreen
                    : string.Equals(tag, "clipboard", StringComparison.Ordinal)
                        ? HotkeyGesture.DefaultClipboard
                        : HotkeyGesture.DefaultRegion;
                HotkeyGesture? reset = HotkeyGesture.Parse(fallback);
                if (reset != null)
                {
                    SetGesture(box, reset);
                }
                RefreshHotkeyBoxes();
                return;
            }

            e.Handled = true;

            if (IsModifierKey(key))
            {
                return;
            }

            HotkeyGesture? gesture = HotkeyGesture.FromKey(key, Keyboard.Modifiers);
            if (gesture == null)
            {
                box.Text = Keyboard.Modifiers == ModifierKeys.None
                    ? L.T("HotkeyModifierRequired")
                    : L.T("HotkeyInvalid");
                return;
            }

            SetGesture(box, gesture);
            RefreshHotkeyBoxes();
        }

        private void SetGesture(TextBox box, HotkeyGesture gesture)
        {
            string tag = box.Tag as string ?? "region";
            if (string.Equals(tag, "region", StringComparison.Ordinal))
            {
                _regionGesture = gesture;
            }
            else if (string.Equals(tag, "clipboard", StringComparison.Ordinal))
            {
                _clipboardGesture = gesture;
            }
            else
            {
                _fullscreenGesture = gesture;
            }
        }

        private void EndHotkeyCapture()
        {
            _capturingHotkeyBox = null;
            RefreshHotkeyBoxes();
        }

        private void RefreshHotkeyBoxes()
        {
            RegionHotkeyBox.Text = _regionGesture?.Display ?? string.Empty;
            FullscreenHotkeyBox.Text = _fullscreenGesture?.Display ?? string.Empty;
            ClipboardHotkeyBox.Text = _clipboardGesture?.Display ?? string.Empty;
        }

        private static bool IsModifierKey(Key key)
        {
            return key == Key.LeftCtrl ||
                   key == Key.RightCtrl ||
                   key == Key.LeftAlt ||
                   key == Key.RightAlt ||
                   key == Key.LeftShift ||
                   key == Key.RightShift ||
                   key == Key.LWin ||
                   key == Key.RWin ||
                   key == Key.System;
        }

        private void Save_Click(object sender, RoutedEventArgs e)
        {
            string folder;
            try
            {
                folder = Environment.ExpandEnvironmentVariables(FolderBox.Text.Trim().Trim('"'));
            }
            catch
            {
                folder = FolderBox.Text.Trim();
            }

            if (string.IsNullOrWhiteSpace(folder))
            {
                ShowError(L.T("FolderRequired"));
                return;
            }

            try
            {
                Directory.CreateDirectory(folder);
            }
            catch (Exception ex)
            {
                ShowError(L.T("FolderCreateError", ex.Message));
                return;
            }

            if (_regionGesture == null || _fullscreenGesture == null || _clipboardGesture == null)
            {
                ShowError(L.T("HotkeyInvalid"));
                return;
            }

            if (_regionGesture.Equals(_fullscreenGesture) ||
                _regionGesture.Equals(_clipboardGesture) ||
                _fullscreenGesture.Equals(_clipboardGesture))
            {
                ShowError(L.T("HotkeyDuplicate"));
                return;
            }

            _working.SaveFolder = folder;
            _working.ShowSaveNotification = NotifyCheck.IsChecked == true;
            _working.RunAtStartup = StartupCheck.IsChecked == true;
            _working.RegionHotkey = _regionGesture.Display;
            _working.FullscreenHotkey = _fullscreenGesture.Display;
            _working.ClipboardHotkey = _clipboardGesture.Display;
            _working.ImageFormat = FormatCombo.SelectedItem is ComboBoxItem formatItem
                ? formatItem.Tag as string ?? "png"
                : "png";
            _working.Quality = Math.Clamp((int)Math.Round(QualitySlider.Value), 1, 100);
            _working.Language = (LanguageCombo.SelectedItem as ComboBoxItem)?.Tag as string ?? "ru";
            _working.Normalize();

            string? error = _apply(_working);
            if (error != null)
            {
                ShowError(error);
                return;
            }

            Close();
        }

        private void Cancel_Click(object sender, RoutedEventArgs e)
        {
            Close();
        }

        private void ShowError(string message)
        {
            MessageBox.Show(this, message, L.T("ErrorCaption"), MessageBoxButton.OK, MessageBoxImage.Warning);
        }
    }
}
