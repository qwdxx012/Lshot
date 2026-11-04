using System;
using System.Collections.Generic;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;

namespace Lshot
{
    internal sealed class ClipboardHistoryWindow : Window
    {
        private static readonly Brush WindowBackground = CreateBrush(0xFF, 0x17, 0x18, 0x1B);
        private static readonly Brush Surface = CreateBrush(0xFF, 0x1D, 0x1F, 0x24);
        private static readonly Brush CardBackground = CreateBrush(0xFF, 0x1D, 0x1F, 0x24);
        private static readonly Brush CardBorder = CreateBrush(0xFF, 0x2C, 0x30, 0x37);
        private static readonly Brush ThumbnailBackground = CreateBrush(0xFF, 0x10, 0x11, 0x14);
        private static readonly Brush Divider = CreateBrush(0xFF, 0x2C, 0x2F, 0x36);
        private static readonly Brush TextPrimary = CreateBrush(0xFF, 0xF2, 0xF3, 0xF5);
        private static readonly Brush TextMuted = CreateBrush(0xFF, 0x9C, 0xA3, 0xAF);
        private static readonly Brush Accent = CreateBrush(0xFF, 0x3B, 0x82, 0xF6);
        private static readonly Brush CopiedGreen = CreateBrush(0xFF, 0x7B, 0xD8, 0x8F);
        private static readonly Brush DangerSurface = CreateBrush(0xFF, 0x2A, 0x1D, 0x1F);
        private static readonly Brush DangerHover = CreateBrush(0xFF, 0x38, 0x22, 0x25);
        private static readonly Brush DangerBorder = CreateBrush(0xFF, 0x55, 0x30, 0x34);
        private static readonly Brush DangerText = CreateBrush(0xFF, 0xFF, 0xB4, 0xB4);
        private static readonly Brush DeleteBackground = CreateBrush(0xCC, 0x15, 0x16, 0x19);
        private static readonly Brush DeleteHover = CreateBrush(0xE6, 0xC4, 0x2B, 0x35);
        private static readonly Brush HoverSurface = CreateBrush(0x24, 0xFF, 0xFF, 0xFF);
        private static readonly Brush Transparent = CreateBrush(0x00, 0x00, 0x00, 0x00);

        private TextBlock _titleText = null!;
        private TextBlock _sizeText = null!;
        private TextBlock _statusText = null!;
        private TextBlock _clearText = null!;
        private TextBlock _emptyTitle = null!;
        private TextBlock _emptyHint = null!;
        private TextBlock _footerText = null!;
        private Border _clearButton = null!;
        private Border _emptyState = null!;
        private ScrollViewer _scroll = null!;
        private WrapPanel _itemsPanel = null!;
        private readonly DispatcherTimer _statusTimer;
        private List<ClipboardCacheEntry> _entries = new List<ClipboardCacheEntry>();

        public ClipboardHistoryWindow()
        {
            Title = L.T("ClipboardTitle");
            Width = 960;
            Height = 700;
            MinWidth = 620;
            MinHeight = 440;
            WindowStartupLocation = WindowStartupLocation.CenterScreen;
            ResizeMode = ResizeMode.CanResize;
            ShowInTaskbar = false;
            Topmost = true;
            Background = WindowBackground;
            Foreground = TextPrimary;
            FontFamily = new FontFamily("Segoe UI");
            FontSize = 13;
            SnapsToDevicePixels = true;
            UseLayoutRounding = true;
            TextOptions.SetTextFormattingMode(this, TextFormattingMode.Display);

            _statusTimer = new DispatcherTimer(DispatcherPriority.Background)
            {
                Interval = TimeSpan.FromMilliseconds(1600)
            };
            _statusTimer.Tick += OnStatusTimerTick;

            Content = BuildContent();

            SourceInitialized += (_, _) => Native.EnableDarkTitleBar(new WindowInteropHelper(this).Handle);
            Closed += (_, _) => _statusTimer.Stop();
            PreviewKeyDown += OnPreviewKeyDown;

            Relocalize();
        }

        public void Relocalize()
        {
            Title = L.T("ClipboardTitle");
            _titleText.Text = L.T("ClipboardTitle");
            _clearText.Text = L.T("ClipboardClear");
            _emptyTitle.Text = L.T("ClipboardEmptyTitle");
            _emptyHint.Text = L.T("ClipboardEmptyHint");
            _footerText.Text = L.T("ClipboardHint");
            _statusText.Text = string.Empty;
            Refresh();
        }

        public void RefreshEntries()
        {
            Refresh();
        }

        private UIElement BuildContent()
        {
            var root = new Grid { Background = WindowBackground };
            root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            root.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
            root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

            var headerGrid = new Grid();
            headerGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            headerGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

            _titleText = new TextBlock
            {
                FontSize = 20,
                FontWeight = FontWeights.SemiBold,
                Foreground = TextPrimary,
                FontFamily = new FontFamily("Segoe UI")
            };

            _sizeText = new TextBlock
            {
                FontSize = 12,
                Foreground = TextMuted,
                VerticalAlignment = VerticalAlignment.Center
            };

            _statusText = new TextBlock
            {
                FontSize = 12,
                Foreground = CopiedGreen,
                Margin = new Thickness(16, 0, 0, 0),
                VerticalAlignment = VerticalAlignment.Center
            };

            var metaPanel = new StackPanel
            {
                Orientation = Orientation.Horizontal,
                Margin = new Thickness(0, 5, 0, 0)
            };
            metaPanel.Children.Add(_sizeText);
            metaPanel.Children.Add(_statusText);

            var titlePanel = new StackPanel { VerticalAlignment = VerticalAlignment.Center };
            titlePanel.Children.Add(_titleText);
            titlePanel.Children.Add(metaPanel);
            headerGrid.Children.Add(titlePanel);

            _clearText = new TextBlock
            {
                FontSize = 12.5,
                FontFamily = new FontFamily("Segoe UI"),
                Foreground = DangerText
            };

            _clearButton = new Border
            {
                Background = DangerSurface,
                BorderBrush = DangerBorder,
                BorderThickness = new Thickness(1),
                CornerRadius = new CornerRadius(6),
                Padding = new Thickness(14, 7, 14, 7),
                Cursor = Cursors.Hand,
                VerticalAlignment = VerticalAlignment.Center,
                Child = _clearText
            };
            _clearButton.MouseEnter += (_, _) =>
            {
                if (_clearButton.IsEnabled)
                {
                    _clearButton.Background = DangerHover;
                }
            };
            _clearButton.MouseLeave += (_, _) => _clearButton.Background = DangerSurface;
            _clearButton.MouseLeftButtonUp += (_, e) =>
            {
                if (!_clearButton.IsEnabled)
                {
                    return;
                }
                e.Handled = true;
                ClearBuffer();
            };
            Grid.SetColumn(_clearButton, 1);
            headerGrid.Children.Add(_clearButton);

            var header = new Border
            {
                Padding = new Thickness(24, 20, 24, 16),
                BorderBrush = Divider,
                BorderThickness = new Thickness(0, 0, 0, 1),
                Child = headerGrid
            };
            Grid.SetRow(header, 0);
            root.Children.Add(header);

            _itemsPanel = new WrapPanel
            {
                Orientation = Orientation.Horizontal,
                Background = Brushes.Transparent
            };

            _scroll = new ScrollViewer
            {
                VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
                HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled,
                Padding = new Thickness(18, 14, 18, 18),
                Background = Brushes.Transparent,
                Content = _itemsPanel
            };

            var contentGrid = new Grid();
            contentGrid.Children.Add(_scroll);

            _emptyTitle = new TextBlock
            {
                FontSize = 15.5,
                FontWeight = FontWeights.SemiBold,
                Foreground = TextPrimary,
                HorizontalAlignment = HorizontalAlignment.Center,
                TextAlignment = TextAlignment.Center
            };

            _emptyHint = new TextBlock
            {
                FontSize = 12.5,
                Foreground = TextMuted,
                Margin = new Thickness(0, 6, 0, 0),
                TextWrapping = TextWrapping.Wrap,
                TextAlignment = TextAlignment.Center,
                HorizontalAlignment = HorizontalAlignment.Center,
                MaxWidth = 420
            };

            var emptyIcon = new Border
            {
                Width = 58,
                Height = 46,
                CornerRadius = new CornerRadius(10),
                BorderBrush = CardBorder,
                BorderThickness = new Thickness(1),
                Background = Surface,
                HorizontalAlignment = HorizontalAlignment.Center,
                Margin = new Thickness(0, 0, 0, 16),
                Child = EditorIcons.Create(EditorIcon.Copy)
            };

            var emptyPanel = new StackPanel
            {
                HorizontalAlignment = HorizontalAlignment.Center,
                VerticalAlignment = VerticalAlignment.Center,
                Margin = new Thickness(40)
            };
            emptyPanel.Children.Add(emptyIcon);
            emptyPanel.Children.Add(_emptyTitle);
            emptyPanel.Children.Add(_emptyHint);

            _emptyState = new Border
            {
                Background = Brushes.Transparent,
                Child = emptyPanel
            };
            contentGrid.Children.Add(_emptyState);
            Grid.SetRow(contentGrid, 1);
            root.Children.Add(contentGrid);

            _footerText = new TextBlock
            {
                FontSize = 11.5,
                Foreground = TextMuted
            };

            var footer = new Border
            {
                Padding = new Thickness(24, 10, 24, 12),
                BorderBrush = Divider,
                BorderThickness = new Thickness(0, 1, 0, 0),
                Child = _footerText
            };
            Grid.SetRow(footer, 2);
            root.Children.Add(footer);

            return root;
        }

        private Border CreateCard(ClipboardCacheEntry entry)
        {
            var card = new Border
            {
                Width = 208,
                Margin = new Thickness(6),
                Background = CardBackground,
                BorderBrush = CardBorder,
                BorderThickness = new Thickness(1),
                CornerRadius = new CornerRadius(8),
                Cursor = Cursors.Hand,
                ToolTip = L.T("ClipboardCopy"),
                ClipToBounds = true
            };

            var cardGrid = new Grid();
            cardGrid.RowDefinitions.Add(new RowDefinition { Height = new GridLength(132) });
            cardGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

            var image = new Image
            {
                Source = entry.Thumbnail,
                Stretch = Stretch.Uniform,
                Margin = new Thickness(6)
            };
            RenderOptions.SetBitmapScalingMode(image, BitmapScalingMode.HighQuality);

            var imageHost = new Border
            {
                Background = ThumbnailBackground,
                CornerRadius = new CornerRadius(7, 7, 0, 0),
                ClipToBounds = true,
                Child = image
            };
            cardGrid.Children.Add(imageHost);

            var deleteButton = new Border
            {
                Width = 20,
                Height = 20,
                CornerRadius = new CornerRadius(4),
                Background = DeleteBackground,
                Cursor = Cursors.Hand,
                ToolTip = L.T("ClipboardDelete"),
                HorizontalAlignment = HorizontalAlignment.Right,
                VerticalAlignment = VerticalAlignment.Top,
                Margin = new Thickness(0, 6, 6, 0),
                Child = EditorIcons.Create(EditorIcon.Close)
            };
            Panel.SetZIndex(deleteButton, 2);
            deleteButton.MouseEnter += (_, _) => deleteButton.Background = DeleteHover;
            deleteButton.MouseLeave += (_, _) => deleteButton.Background = DeleteBackground;
            deleteButton.MouseLeftButtonUp += (_, e) =>
            {
                e.Handled = true;
                DeleteEntry(entry);
            };
            cardGrid.Children.Add(deleteButton);

            var infoGrid = new Grid { Margin = new Thickness(10, 8, 8, 9) };
            infoGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            infoGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

            var dimensions = new TextBlock
            {
                Text = string.Format("{0} x {1}", entry.Width, entry.Height),
                FontSize = 12.5,
                Foreground = TextPrimary,
                FontFamily = new FontFamily("Segoe UI")
            };

            var size = new TextBlock
            {
                Text = ClipboardCache.FormatSize(entry.Bytes),
                FontSize = 11.5,
                Foreground = TextMuted,
                Margin = new Thickness(0, 3, 0, 0),
                FontFamily = new FontFamily("Segoe UI")
            };

            var textPanel = new StackPanel { VerticalAlignment = VerticalAlignment.Center };
            textPanel.Children.Add(dimensions);
            textPanel.Children.Add(size);
            infoGrid.Children.Add(textPanel);

            var copyButton = new Border
            {
                Width = 24,
                Height = 24,
                CornerRadius = new CornerRadius(4),
                Background = Transparent,
                Cursor = Cursors.Hand,
                ToolTip = L.T("ClipboardCopy"),
                VerticalAlignment = VerticalAlignment.Center,
                Child = EditorIcons.Create(EditorIcon.Copy)
            };
            copyButton.MouseEnter += (_, _) => copyButton.Background = HoverSurface;
            copyButton.MouseLeave += (_, _) => copyButton.Background = Transparent;
            copyButton.MouseLeftButtonUp += (_, e) =>
            {
                e.Handled = true;
                CopyEntry(entry);
            };
            Grid.SetColumn(copyButton, 1);
            infoGrid.Children.Add(copyButton);

            Grid.SetRow(infoGrid, 1);
            cardGrid.Children.Add(infoGrid);
            card.Child = cardGrid;

            card.MouseEnter += (_, _) => card.BorderBrush = Accent;
            card.MouseLeave += (_, _) => card.BorderBrush = CardBorder;
            card.MouseLeftButtonUp += (_, e) =>
            {
                e.Handled = true;
                CopyEntry(entry);
            };

            return card;
        }

        private void CopyEntry(ClipboardCacheEntry entry)
        {
            try
            {
                BitmapSource image = ClipboardCache.LoadImage(entry.FilePath);
                ClipboardCache.CopyToClipboard(image);
                ShowStatus(L.T("ClipboardCopied"));
            }
            catch (Exception ex)
            {
                MessageBox.Show(
                    this,
                    L.T("CopyFailed", ex.Message),
                    L.T("ErrorCaption"),
                    MessageBoxButton.OK,
                    MessageBoxImage.Error);
            }
        }

        private void DeleteEntry(ClipboardCacheEntry entry)
        {
            if (!ClipboardCache.Delete(entry.FilePath, out string? error))
            {
                MessageBox.Show(
                    this,
                    L.T("ClipboardDeleteFailed", error ?? string.Empty),
                    L.T("ErrorCaption"),
                    MessageBoxButton.OK,
                    MessageBoxImage.Error);
            }

            Refresh();
        }

        private void ClearBuffer()
        {
            if (_entries.Count == 0)
            {
                return;
            }

            MessageBoxResult result = MessageBox.Show(
                this,
                L.T("ClipboardClearConfirm"),
                L.T("ClipboardClearCaption"),
                MessageBoxButton.YesNo,
                MessageBoxImage.Warning,
                MessageBoxResult.No);

            if (result != MessageBoxResult.Yes)
            {
                return;
            }

            if (!ClipboardCache.Clear(out string? error))
            {
                MessageBox.Show(
                    this,
                    L.T("ClipboardClearFailed", error ?? string.Empty),
                    L.T("ErrorCaption"),
                    MessageBoxButton.OK,
                    MessageBoxImage.Error);
            }

            Refresh();
        }

        private void Refresh()
        {
            double offset = _scroll.VerticalOffset;

            _entries = ClipboardCache.LoadEntries();
            _itemsPanel.Children.Clear();
            foreach (ClipboardCacheEntry entry in _entries)
            {
                _itemsPanel.Children.Add(CreateCard(entry));
            }

            bool hasEntries = _entries.Count > 0;
            _emptyState.Visibility = hasEntries ? Visibility.Collapsed : Visibility.Visible;
            _scroll.Visibility = hasEntries ? Visibility.Visible : Visibility.Collapsed;
            _sizeText.Text = L.T("ClipboardBufferSize", ClipboardCache.FormatBufferSize(ClipboardCache.GetTotalBytes()));

            _clearButton.IsEnabled = hasEntries;
            _clearButton.Opacity = hasEntries ? 1.0 : 0.45;
            _clearButton.Cursor = hasEntries ? Cursors.Hand : Cursors.Arrow;

            if (hasEntries && offset > 0)
            {
                _scroll.ScrollToVerticalOffset(offset);
            }
        }

        private void ShowStatus(string text)
        {
            _statusText.Text = text;
            _statusTimer.Stop();
            _statusTimer.Start();
        }

        private void OnStatusTimerTick(object? sender, EventArgs e)
        {
            _statusTimer.Stop();
            _statusText.Text = string.Empty;
        }

        private void OnPreviewKeyDown(object sender, KeyEventArgs e)
        {
            if (e.Key == Key.Escape)
            {
                e.Handled = true;
                Close();
            }
        }

        private static SolidColorBrush CreateBrush(byte alpha, byte red, byte green, byte blue)
        {
            var brush = new SolidColorBrush(Color.FromArgb(alpha, red, green, blue));
            brush.Freeze();
            return brush;
        }
    }
}
