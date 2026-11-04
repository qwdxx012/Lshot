using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Microsoft.Win32;
using WinForms = System.Windows.Forms;
using IconPath = System.Windows.Shapes.Path;
using Rectangle = System.Windows.Shapes.Rectangle;

namespace Lshot
{
    internal enum AnnotationKind
    {
        Pen,
        Line,
        Arrow,
        Rectangle,
        Highlighter,
        Text
    }

    internal sealed class Annotation
    {
        private static readonly Typeface TextTypeface = new Typeface("Segoe UI");

        public AnnotationKind Kind { get; init; }
        public Color Color { get; init; } = Colors.Red;
        public double Thickness { get; init; } = 3;
        public List<Point> Points { get; } = new List<Point>();
        public string Text { get; init; } = string.Empty;
        public double FontSize { get; init; } = 20;
        public Point TextPosition { get; init; }

        public void Draw(DrawingContext dc, double pixelsPerDip)
        {
            var colorBrush = new SolidColorBrush(Color);
            colorBrush.Freeze();
            var pen = new Pen(colorBrush, Math.Max(1, Thickness))
            {
                StartLineCap = PenLineCap.Round,
                EndLineCap = PenLineCap.Round,
                LineJoin = PenLineJoin.Round
            };

            switch (Kind)
            {
                case AnnotationKind.Pen:
                    DrawFreehand(dc, colorBrush, pen);
                    break;

                case AnnotationKind.Highlighter:
                    {
                        var highlightColor = Color.FromArgb(82, Color.R, Color.G, Color.B);
                        var highlightBrush = new SolidColorBrush(highlightColor);
                        highlightBrush.Freeze();
                        var highlightPen = new Pen(highlightBrush, Math.Max(10, Thickness))
                        {
                            StartLineCap = PenLineCap.Square,
                            EndLineCap = PenLineCap.Square,
                            LineJoin = PenLineJoin.Round
                        };
                        DrawFreehand(dc, highlightBrush, highlightPen);
                        break;
                    }

                case AnnotationKind.Line:
                    if (Points.Count >= 2)
                    {
                        dc.DrawLine(pen, Points[0], Points[1]);
                    }
                    break;

                case AnnotationKind.Arrow:
                    DrawArrow(dc, colorBrush, pen);
                    break;

                case AnnotationKind.Rectangle:
                    if (Points.Count >= 2)
                    {
                        double left = Math.Min(Points[0].X, Points[1].X);
                        double top = Math.Min(Points[0].Y, Points[1].Y);
                        double width = Math.Abs(Points[1].X - Points[0].X);
                        double height = Math.Abs(Points[1].Y - Points[0].Y);
                        dc.DrawRectangle(null, pen, new Rect(left, top, width, height));
                    }
                    break;

                case AnnotationKind.Text:
                    DrawText(dc, colorBrush, pixelsPerDip);
                    break;
            }
        }

        private void DrawFreehand(DrawingContext dc, Brush brush, Pen pen)
        {
            if (Points.Count == 0)
            {
                return;
            }

            if (Points.Count == 1)
            {
                double radius = Math.Max(1, pen.Thickness / 2);
                dc.DrawEllipse(brush, null, Points[0], radius, radius);
                return;
            }

            var geometry = new StreamGeometry();
            using (StreamGeometryContext context = geometry.Open())
            {
                context.BeginFigure(Points[0], false, false);
                for (int i = 1; i < Points.Count; i++)
                {
                    context.LineTo(Points[i], true, false);
                }
            }
            geometry.Freeze();
            dc.DrawGeometry(null, pen, geometry);
        }

        private void DrawArrow(DrawingContext dc, Brush brush, Pen pen)
        {
            if (Points.Count < 2)
            {
                return;
            }

            Point start = Points[0];
            Point end = Points[1];
            Vector direction = end - start;
            if (direction.Length < 0.5)
            {
                return;
            }

            direction.Normalize();
            Vector perpendicular = new Vector(-direction.Y, direction.X);
            double headLength = Math.Max(11, pen.Thickness * 4);
            Point basePoint = end - direction * headLength;

            dc.DrawLine(pen, start, basePoint);

            var head = new StreamGeometry();
            using (StreamGeometryContext context = head.Open())
            {
                context.BeginFigure(end, true, true);
                context.LineTo(basePoint + perpendicular * (headLength * 0.48), true, false);
                context.LineTo(basePoint - perpendicular * (headLength * 0.48), true, false);
            }
            head.Freeze();
            dc.DrawGeometry(brush, null, head);
        }

        private void DrawText(DrawingContext dc, Brush brush, double pixelsPerDip)
        {
            if (string.IsNullOrEmpty(Text))
            {
                return;
            }

            var formatted = new FormattedText(
                Text,
                CultureInfo.CurrentCulture,
                FlowDirection.LeftToRight,
                TextTypeface,
                Math.Max(8, FontSize),
                brush,
                pixelsPerDip);

            var shadowBrush = new SolidColorBrush(Color.FromArgb(150, 0, 0, 0));
            shadowBrush.Freeze();
            var shadow = new FormattedText(
                Text,
                CultureInfo.CurrentCulture,
                FlowDirection.LeftToRight,
                TextTypeface,
                Math.Max(8, FontSize),
                shadowBrush,
                pixelsPerDip);

            dc.DrawText(shadow, new Point(TextPosition.X + 1.5, TextPosition.Y + 1.5));
            dc.DrawText(formatted, TextPosition);
        }
    }

    internal sealed class AnnotationLayer : FrameworkElement
    {
        public List<Annotation> Items { get; } = new List<Annotation>();
        public Annotation? Preview { get; set; }
        public Rect Selection { get; set; }
        public bool HasSelection { get; set; }

        public void Refresh()
        {
            InvalidateVisual();
        }

        protected override void OnRender(DrawingContext drawingContext)
        {
            base.OnRender(drawingContext);

            bool clipped = HasSelection && Selection.Width > 0 && Selection.Height > 0;
            if (clipped)
            {
                drawingContext.PushClip(new RectangleGeometry(Selection));
            }

            double pixelsPerDip = VisualTreeHelper.GetDpi(this).PixelsPerDip;
            foreach (Annotation annotation in Items)
            {
                annotation.Draw(drawingContext, pixelsPerDip);
            }
            Preview?.Draw(drawingContext, pixelsPerDip);

            if (clipped)
            {
                drawingContext.Pop();
            }
        }

        public void DrawForExport(DrawingContext drawingContext)
        {
            foreach (Annotation annotation in Items)
            {
                annotation.Draw(drawingContext, 1.0);
            }
        }
    }

    internal static class ImageExport
    {
        public static void Save(BitmapSource image, string path, int quality)
        {
            string extension = Path.GetExtension(path).ToLowerInvariant();
            bool jpeg = extension == ".jpg" || extension == ".jpeg";

            BitmapEncoder encoder = jpeg
                ? new JpegBitmapEncoder { QualityLevel = Math.Clamp(quality, 1, 100) }
                : new PngBitmapEncoder();

            BitmapSource source = image;
            if (jpeg && image.Format != PixelFormats.Bgr24)
            {
                source = new FormatConvertedBitmap(image, PixelFormats.Bgr24, null, 0);
            }

            encoder.Frames.Add(BitmapFrame.Create(source));

            string? directory = Path.GetDirectoryName(path);
            if (!string.IsNullOrEmpty(directory))
            {
                Directory.CreateDirectory(directory);
            }

            using (var stream = new FileStream(path, FileMode.Create, FileAccess.Write, FileShare.None))
            {
                encoder.Save(stream);
            }
        }
    }

    internal enum EditorIcon
    {
        Pen,
        Line,
        Arrow,
        Rectangle,
        Highlighter,
        Text,
        Undo,
        Copy,
        Save,
        Close
    }

    internal static class EditorIcons
    {
        public static Canvas Create(EditorIcon icon)
        {
            var canvas = new Canvas
            {
                Width = 14,
                Height = 14,
                HorizontalAlignment = HorizontalAlignment.Center,
                VerticalAlignment = VerticalAlignment.Center,
                IsHitTestVisible = false
            };

            switch (icon)
            {
                case EditorIcon.Pen:
                    canvas.Children.Add(CreatePath("M2.8,11.2 L4.0,8.4 L10.0,2.4 L11.6,4.0 L5.6,10.0 Z", 1.25));
                    canvas.Children.Add(CreatePath("M9.2,3.2 L10.8,4.8", 1.0));
                    canvas.Children.Add(CreateFilledPath("M2.8,11.2 L4.0,8.4 L5.6,10.0 Z"));
                    break;

                case EditorIcon.Line:
                    canvas.Children.Add(CreatePath("M2.6,11.4 L11.4,2.6", 1.7));
                    break;

                case EditorIcon.Arrow:
                    canvas.Children.Add(CreatePath(
                        "M2.6,11.4 L10.4,3.6 M6.6,3.2 L11.0,3.0 L10.8,7.4",
                        1.5));
                    break;

                case EditorIcon.Rectangle:
                    canvas.Children.Add(CreatePath("M2.5,3.3 H11.5 V10.7 H2.5 Z", 1.5));
                    break;

                case EditorIcon.Highlighter:
                    canvas.Children.Add(CreatePath("M5.4,6.9 L9.4,3.0 L11.9,5.5 L8.2,9.7 Z", 1.2));
                    canvas.Children.Add(CreateFilledPath("M9.4,3.0 L11.9,5.5 L10.9,6.5 L8.4,4.0 Z"));
                    canvas.Children.Add(CreateFilledPath("M2.6,11.4 L5.4,6.9 L8.2,9.7 L4.9,11.6 Z"));
                    break;

                case EditorIcon.Text:
                    canvas.Children.Add(CreatePath(
                        "M2.8,3.4 H11.2 M2.8,3.4 V4.8 M11.2,3.4 V4.8 M7.0,3.4 V11.4",
                        1.5));
                    break;

                case EditorIcon.Undo:
                    canvas.Children.Add(CreatePath(
                        "M10.8,10.8 A4.2,4.2 0 1 0 3.2,5.6 M3.2,2.2 L3.2,5.6 L6.6,5.6",
                        1.4));
                    break;

                case EditorIcon.Copy:
                    canvas.Children.Add(CreatePath(
                        "M2.8,7.8 V2.8 H7.8 V4.6 M5.2,5.2 H11.2 V11.2 H5.2 Z",
                        1.2));
                    break;

                case EditorIcon.Save:
                    canvas.Children.Add(CreatePath(
                        "M2.7,2.7 H9.7 L11.3,4.3 V11.3 H2.7 Z M4.7,2.7 V5.9 H8.7 V2.7 M4.7,8.9 H9.3 V11.3 H4.7 Z",
                        1.2));
                    break;

                case EditorIcon.Close:
                    canvas.Children.Add(CreatePath(
                        "M3.2,3.2 L10.8,10.8 M10.8,3.2 L3.2,10.8",
                        1.6));
                    break;
            }

            return canvas;
        }

        private static IconPath CreatePath(string data, double thickness)
        {
            var path = new IconPath
            {
                Data = Geometry.Parse(data),
                Stroke = Brushes.White,
                StrokeThickness = thickness,
                StrokeStartLineCap = PenLineCap.Round,
                StrokeEndLineCap = PenLineCap.Round,
                StrokeLineJoin = PenLineJoin.Round,
                Fill = null,
                IsHitTestVisible = false
            };
            return path;
        }

        private static IconPath CreateFilledPath(string data)
        {
            var path = new IconPath
            {
                Data = Geometry.Parse(data),
                Fill = Brushes.White,
                Stroke = null,
                IsHitTestVisible = false
            };
            return path;
        }
    }

    internal sealed class RegionEditorWindow : Window
    {
        private enum HandleKind
        {
            None,
            NorthWest,
            North,
            NorthEast,
            East,
            SouthEast,
            South,
            SouthWest,
            West
        }

        private readonly BitmapSource _screen;
        private readonly Native.RECT _screenBounds;
        private readonly AppSettings _settings;
        private readonly bool _fullSelection;
        private readonly double _initialDpiScale;

        private readonly Image _screenImage;
        private readonly Canvas _annotationCanvas;
        private readonly AnnotationLayer _annotationLayer;
        private readonly Rectangle _dimTop;
        private readonly Rectangle _dimBottom;
        private readonly Rectangle _dimLeft;
        private readonly Rectangle _dimRight;
        private readonly Canvas _selectionCanvas;
        private readonly Rectangle _selectionOutline;
        private readonly Rectangle[] _handles = new Rectangle[8];
        private readonly Border _sizeBadge;
        private readonly TextBlock _sizeText;
        private readonly Canvas _chromeCanvas;
        private readonly Border _toolBar;
        private readonly Border _actionBar;
        private readonly Border _hint;
        private readonly Border _colorSwatch;
        private Border _undoButton = null!;
        private readonly Dictionary<AnnotationKind, Border> _toolButtons = new Dictionary<AnnotationKind, Border>();
        private readonly List<Annotation> _annotations = new List<Annotation>();

        private Rect _selection;
        private bool _hasSelection;
        private bool _selectionFinalized;
        private bool _draggingNewSelection;
        private bool _movingSelection;
        private bool _resizingSelection;
        private HandleKind _activeHandle = HandleKind.None;
        private Point _dragStartPoint;
        private Rect _dragStartSelection;
        private AnnotationKind? _activeTool;
        private Annotation? _previewAnnotation;
        private Color _currentColor = Colors.Red;
        private TextBox? _textEditor;
        private Point _textPosition;
        private bool _committingText;

        public event Action<string>? Saved;

        public RegionEditorWindow(BitmapSource screen, Native.RECT screenBounds, double dpiScale, AppSettings settings, bool fullSelection)
        {
            _screen = screen;
            _screenBounds = screenBounds;
            _settings = settings.Clone();
            _fullSelection = fullSelection;
            _initialDpiScale = dpiScale <= 0 ? 1.0 : dpiScale;

            WindowStyle = WindowStyle.None;
            ResizeMode = ResizeMode.NoResize;
            ShowInTaskbar = false;
            Topmost = true;
            WindowStartupLocation = WindowStartupLocation.Manual;
            Background = Brushes.Black;
            Cursor = Cursors.Cross;
            Focusable = true;
            SnapsToDevicePixels = true;
            UseLayoutRounding = true;
            FontFamily = new FontFamily("Segoe UI");

            Left = screenBounds.Left / _initialDpiScale;
            Top = screenBounds.Top / _initialDpiScale;
            Width = Math.Max(1, screenBounds.Width / _initialDpiScale);
            Height = Math.Max(1, screenBounds.Height / _initialDpiScale);

            _screenImage = new Image
            {
                Source = screen,
                Stretch = Stretch.Fill,
                SnapsToDevicePixels = true
            };
            RenderOptions.SetBitmapScalingMode(_screenImage, BitmapScalingMode.HighQuality);

            _annotationLayer = new AnnotationLayer { IsHitTestVisible = false };
            _annotationCanvas = new Canvas { IsHitTestVisible = false };

            var dimBrush = new SolidColorBrush(Color.FromArgb(0x80, 0x00, 0x00, 0x00));
            _dimTop = CreateDimRectangle(dimBrush);
            _dimBottom = CreateDimRectangle(dimBrush);
            _dimLeft = CreateDimRectangle(dimBrush);
            _dimRight = CreateDimRectangle(dimBrush);

            _selectionCanvas = new Canvas { IsHitTestVisible = false };

            _selectionOutline = new Rectangle
            {
                Stroke = Brushes.White,
                StrokeThickness = 1.5,
                Fill = Brushes.Transparent,
                StrokeDashArray = new DoubleCollection(new[] { 4.0, 3.0 }),
                Visibility = Visibility.Collapsed,
                IsHitTestVisible = false
            };

            _sizeText = new TextBlock
            {
                FontFamily = new FontFamily("Segoe UI"),
                FontSize = 12,
                FontWeight = FontWeights.SemiBold,
                Foreground = Brushes.White
            };
            _sizeBadge = new Border
            {
                Background = new SolidColorBrush(Color.FromArgb(0xE6, 0x10, 0x11, 0x14)),
                CornerRadius = new CornerRadius(8),
                Padding = new Thickness(8, 3, 8, 3),
                Child = _sizeText,
                Visibility = Visibility.Collapsed,
                IsHitTestVisible = false
            };

            _selectionCanvas.Children.Add(_selectionOutline);

            for (int i = 0; i < _handles.Length; i++)
            {
                var handle = new Rectangle
                {
                    Width = 10,
                    Height = 10,
                    Fill = Brushes.White,
                    Stroke = Brushes.Black,
                    StrokeThickness = 1,
                    Visibility = Visibility.Collapsed,
                    IsHitTestVisible = false
                };
                _handles[i] = handle;
                _selectionCanvas.Children.Add(handle);
            }

            _selectionCanvas.Children.Add(_sizeBadge);

            _chromeCanvas = new Canvas();

            _toolBar = new Border
            {
                Background = new SolidColorBrush(Color.FromArgb(0xF2, 0x16, 0x17, 0x1A)),
                BorderBrush = new SolidColorBrush(Color.FromArgb(0x44, 0xFF, 0xFF, 0xFF)),
                BorderThickness = new Thickness(1),
                CornerRadius = new CornerRadius(4),
                Padding = new Thickness(2),
                Visibility = Visibility.Collapsed,
                Child = new StackPanel { Orientation = Orientation.Vertical }
            };

            _actionBar = new Border
            {
                Background = new SolidColorBrush(Color.FromArgb(0xF2, 0x16, 0x17, 0x1A)),
                BorderBrush = new SolidColorBrush(Color.FromArgb(0x44, 0xFF, 0xFF, 0xFF)),
                BorderThickness = new Thickness(1),
                CornerRadius = new CornerRadius(4),
                Padding = new Thickness(2),
                Visibility = Visibility.Collapsed,
                Child = new StackPanel { Orientation = Orientation.Horizontal }
            };

            _colorSwatch = new Border
            {
                Width = 14,
                Height = 14,
                CornerRadius = new CornerRadius(3),
                BorderBrush = Brushes.White,
                BorderThickness = new Thickness(1),
                Background = new SolidColorBrush(_currentColor),
                HorizontalAlignment = HorizontalAlignment.Center,
                VerticalAlignment = VerticalAlignment.Center
            };

            BuildToolBars();

            _hint = new Border
            {
                Background = new SolidColorBrush(Color.FromArgb(0xE6, 0x10, 0x11, 0x14)),
                BorderBrush = new SolidColorBrush(Color.FromArgb(0x44, 0xFF, 0xFF, 0xFF)),
                BorderThickness = new Thickness(1),
                CornerRadius = new CornerRadius(7),
                Padding = new Thickness(16, 8, 16, 8),
                HorizontalAlignment = HorizontalAlignment.Center,
                VerticalAlignment = VerticalAlignment.Top,
                Margin = new Thickness(0, 28, 0, 0),
                IsHitTestVisible = false,
                Visibility = _fullSelection ? Visibility.Collapsed : Visibility.Visible,
                Child = new TextBlock
                {
                    Text = L.T("DragHint"),
                    FontFamily = new FontFamily("Segoe UI"),
                    FontSize = 13,
                    Foreground = Brushes.White
                }
            };

            _annotationCanvas.Children.Add(_dimTop);
            _annotationCanvas.Children.Add(_dimBottom);
            _annotationCanvas.Children.Add(_dimLeft);
            _annotationCanvas.Children.Add(_dimRight);

            _dimTop.IsHitTestVisible = false;
            _dimBottom.IsHitTestVisible = false;
            _dimLeft.IsHitTestVisible = false;
            _dimRight.IsHitTestVisible = false;

            var root = new Grid();
            root.Children.Add(_screenImage);
            root.Children.Add(_annotationCanvas);
            root.Children.Add(_annotationLayer);
            root.Children.Add(_selectionCanvas);
            root.Children.Add(_chromeCanvas);
            root.Children.Add(_hint);
            Content = root;

            _chromeCanvas.Children.Add(_toolBar);
            _chromeCanvas.Children.Add(_actionBar);

            SourceInitialized += OnSourceInitialized;
            Loaded += OnLoaded;
            PreviewMouseLeftButtonDown += OnMouseLeftButtonDown;
            PreviewMouseMove += OnMouseMove;
            PreviewMouseLeftButtonUp += OnMouseLeftButtonUp;
            PreviewKeyDown += OnPreviewKeyDown;
            SizeChanged += (_, _) =>
            {
                if (_fullSelection && !_hasSelection)
                {
                    InitializeFullSelection();
                }
                else
                {
                    UpdateVisuals();
                }
            };
        }

        private static Rectangle CreateDimRectangle(Brush brush)
        {
            return new Rectangle
            {
                Fill = brush,
                IsHitTestVisible = false
            };
        }

        private void BuildToolBars()
        {
            var toolPanel = (StackPanel)_toolBar.Child;
            toolPanel.Children.Add(CreateAnnotationTool(EditorIcon.Pen, L.T("ToolPen"), AnnotationKind.Pen));
            toolPanel.Children.Add(CreateAnnotationTool(EditorIcon.Line, L.T("ToolLine"), AnnotationKind.Line));
            toolPanel.Children.Add(CreateAnnotationTool(EditorIcon.Arrow, L.T("ToolArrow"), AnnotationKind.Arrow));
            toolPanel.Children.Add(CreateAnnotationTool(EditorIcon.Rectangle, L.T("ToolRectangle"), AnnotationKind.Rectangle));
            toolPanel.Children.Add(CreateAnnotationTool(EditorIcon.Highlighter, L.T("ToolHighlighter"), AnnotationKind.Highlighter));
            toolPanel.Children.Add(CreateAnnotationTool(EditorIcon.Text, L.T("ToolText"), AnnotationKind.Text));
            toolPanel.Children.Add(CreateToolItem(_colorSwatch, L.T("ToolColor"), ShowColorPicker));
            _undoButton = CreateToolItem(EditorIcons.Create(EditorIcon.Undo), L.T("ToolUndo"), Undo);
            toolPanel.Children.Add(_undoButton);

            var actionPanel = (StackPanel)_actionBar.Child;
            actionPanel.Children.Add(CreateActionItem(EditorIcon.Copy, L.T("ActionCopy"), CopyToClipboard));
            actionPanel.Children.Add(CreateActionItem(EditorIcon.Save, L.T("ActionSave"), SaveToFile));
            actionPanel.Children.Add(CreateActionItem(EditorIcon.Close, L.T("ActionClose"), Close));
        }

        private Border CreateAnnotationTool(EditorIcon icon, string tooltip, AnnotationKind kind)
        {
            Border border = CreateToolItem(EditorIcons.Create(icon), tooltip, () => ToggleTool(kind));
            _toolButtons[kind] = border;
            return border;
        }

        private Border CreateToolItem(UIElement content, string tooltip, Action action)
        {
            var border = new Border
            {
                Width = 24,
                Height = 24,
                Margin = new Thickness(0, 1, 0, 1),
                CornerRadius = new CornerRadius(3),
                Background = Brushes.Transparent,
                Cursor = Cursors.Hand,
                ToolTip = tooltip,
                Child = content
            };
            border.MouseEnter += (_, _) =>
            {
                if (!IsActiveToolButton(border))
                {
                    border.Background = new SolidColorBrush(Color.FromArgb(0x24, 0xFF, 0xFF, 0xFF));
                }
            };
            border.MouseLeave += (_, _) => UpdateToolHighlights();
            border.MouseLeftButtonUp += (_, e) =>
            {
                e.Handled = true;
                action();
            };
            return border;
        }

        private Border CreateActionItem(EditorIcon icon, string tooltip, Action action)
        {
            var border = new Border
            {
                Width = 24,
                Height = 24,
                Margin = new Thickness(1, 0, 1, 0),
                CornerRadius = new CornerRadius(3),
                Background = Brushes.Transparent,
                Cursor = Cursors.Hand,
                ToolTip = tooltip,
                Child = EditorIcons.Create(icon)
            };
            border.MouseEnter += (_, _) =>
                border.Background = new SolidColorBrush(Color.FromArgb(0x24, 0xFF, 0xFF, 0xFF));
            border.MouseLeave += (_, _) => border.Background = Brushes.Transparent;
            border.MouseLeftButtonUp += (_, e) =>
            {
                e.Handled = true;
                action();
            };
            return border;
        }

        private bool IsActiveToolButton(Border border)
        {
            if (_activeTool == null)
            {
                return false;
            }
            return _toolButtons.TryGetValue(_activeTool.Value, out Border? active) && ReferenceEquals(active, border);
        }

        private void UpdateToolHighlights()
        {
            foreach (KeyValuePair<AnnotationKind, Border> pair in _toolButtons)
            {
                pair.Value.Background = _activeTool == pair.Key
                    ? new SolidColorBrush(Color.FromRgb(0x3B, 0x82, 0xF6))
                    : Brushes.Transparent;
            }
        }

        private void UpdateUndoState()
        {
            bool enabled = _annotations.Count > 0;
            _undoButton.Opacity = enabled ? 1.0 : 0.38;
            _undoButton.IsHitTestVisible = enabled;
        }

        private void OnSourceInitialized(object? sender, EventArgs e)
        {
            IntPtr hwnd = new WindowInteropHelper(this).Handle;
            Native.MakeRegionOverlay(hwnd);
            ApplyPhysicalBounds(hwnd);
        }

        private void OnLoaded(object sender, RoutedEventArgs e)
        {
            IntPtr hwnd = new WindowInteropHelper(this).Handle;
            ApplyPhysicalBounds(hwnd);
            Native.ActivateWindow(hwnd);
            Activate();
            Focus();
            Keyboard.Focus(this);

            if (_fullSelection)
            {
                InitializeFullSelection();
            }
            else
            {
                UpdateVisuals();
            }
        }

        private void ApplyPhysicalBounds(IntPtr hwnd)
        {
            Native.SetWindowPhysicalBounds(
                hwnd,
                _screenBounds.Left,
                _screenBounds.Top,
                Math.Max(1, _screenBounds.Width),
                Math.Max(1, _screenBounds.Height),
                true);
        }

        private void InitializeFullSelection()
        {
            if (ActualWidth <= 0 || ActualHeight <= 0)
            {
                return;
            }

            _selection = new Rect(0, 0, ActualWidth, ActualHeight);
            _hasSelection = true;
            _selectionFinalized = true;
            _hint.Visibility = Visibility.Collapsed;
            UpdateVisuals();
        }

        private void OnPreviewKeyDown(object sender, KeyEventArgs e)
        {
            if (_textEditor != null && _textEditor.IsKeyboardFocusWithin)
            {
                if (e.Key == Key.Escape)
                {
                    CancelTextEditor();
                    e.Handled = true;
                }
                else if (e.Key == Key.Enter)
                {
                    CommitTextEditor();
                    e.Handled = true;
                }
                return;
            }

            if (e.Key == Key.Escape)
            {
                if (_activeTool != null)
                {
                    SetActiveTool(null);
                    e.Handled = true;
                }
                else
                {
                    Close();
                    e.Handled = true;
                }
            }
            else if (e.Key == Key.C && Keyboard.Modifiers == ModifierKeys.Control)
            {
                if (_hasSelection && _selectionFinalized)
                {
                    CopyToClipboard();
                }
                e.Handled = true;
            }
            else if (e.Key == Key.Z && Keyboard.Modifiers == ModifierKeys.Control)
            {
                Undo();
                e.Handled = true;
            }
        }

        private void OnMouseLeftButtonDown(object sender, MouseButtonEventArgs e)
        {
            Point point = e.GetPosition(this);
            DependencyObject? source = e.OriginalSource as DependencyObject;

            if (_textEditor != null && IsWithin(source, _textEditor))
            {
                return;
            }

            if (IsWithin(source, _toolBar) || IsWithin(source, _actionBar))
            {
                CommitTextEditor();
                return;
            }

            CommitTextEditor();

            if (_selectionFinalized)
            {
                HandleKind handle = GetHandleAt(point);
                if (handle != HandleKind.None)
                {
                    _activeHandle = handle;
                    _resizingSelection = true;
                    _dragStartPoint = point;
                    _dragStartSelection = _selection;
                    CaptureMouse();
                    e.Handled = true;
                    return;
                }

                if (_selection.Contains(point))
                {
                    if (_activeTool == AnnotationKind.Text)
                    {
                        BeginTextEditor(point);
                        e.Handled = true;
                        return;
                    }

                    if (_activeTool != null)
                    {
                        BeginAnnotation(point);
                        e.Handled = true;
                        return;
                    }

                    _movingSelection = true;
                    _dragStartPoint = point;
                    _dragStartSelection = _selection;
                    CaptureMouse();
                    e.Handled = true;
                    return;
                }
            }

            BeginNewSelection(point);
            e.Handled = true;
        }

        private void OnMouseMove(object sender, MouseEventArgs e)
        {
            Point point = ClampPoint(e.GetPosition(this));

            if (_draggingNewSelection)
            {
                UpdateNewSelection(point);
                return;
            }

            if (_resizingSelection)
            {
                UpdateResize(point);
                return;
            }

            if (_movingSelection)
            {
                UpdateMove(point);
                return;
            }

            if (_previewAnnotation != null)
            {
                UpdateAnnotation(point);
                return;
            }

            UpdateCursor(point);
        }

        private void OnMouseLeftButtonUp(object sender, MouseButtonEventArgs e)
        {
            if (_draggingNewSelection)
            {
                _draggingNewSelection = false;
                ReleaseMouseCapture();

                if (_selection.Width < 6 || _selection.Height < 6)
                {
                    ResetSelection();
                }
                else
                {
                    _selectionFinalized = true;
                    _hint.Visibility = Visibility.Collapsed;
                    UpdateVisuals();
                }
                e.Handled = true;
                return;
            }

            if (_resizingSelection || _movingSelection)
            {
                _resizingSelection = false;
                _movingSelection = false;
                _activeHandle = HandleKind.None;
                ReleaseMouseCapture();
                UpdateVisuals();
                e.Handled = true;
                return;
            }

            if (_previewAnnotation != null)
            {
                CommitAnnotation();
                ReleaseMouseCapture();
                e.Handled = true;
            }
        }

        private void BeginNewSelection(Point point)
        {
            CommitTextEditor();
            ClearAnnotations();
            SetActiveTool(null);
            _hasSelection = true;
            _selectionFinalized = false;
            _draggingNewSelection = true;
            _dragStartPoint = ClampPoint(point);
            _selection = new Rect(_dragStartPoint, new Size(0, 0));
            _hint.Visibility = Visibility.Collapsed;
            _toolBar.Visibility = Visibility.Collapsed;
            _actionBar.Visibility = Visibility.Collapsed;
            _selectionOutline.Visibility = Visibility.Visible;
            _sizeBadge.Visibility = Visibility.Visible;
            foreach (Rectangle handle in _handles)
            {
                handle.Visibility = Visibility.Collapsed;
            }
            UpdateVisuals();
            CaptureMouse();
        }

        private void UpdateNewSelection(Point point)
        {
            double left = Math.Min(_dragStartPoint.X, point.X);
            double top = Math.Min(_dragStartPoint.Y, point.Y);
            double width = Math.Abs(point.X - _dragStartPoint.X);
            double height = Math.Abs(point.Y - _dragStartPoint.Y);
            _selection = ClampRect(new Rect(left, top, width, height));
            UpdateVisuals();
        }

        private void UpdateMove(Point point)
        {
            double dx = point.X - _dragStartPoint.X;
            double dy = point.Y - _dragStartPoint.Y;
            var moved = new Rect(
                _dragStartSelection.X + dx,
                _dragStartSelection.Y + dy,
                _dragStartSelection.Width,
                _dragStartSelection.Height);

            _selection = ClampRect(moved);
            UpdateVisuals();
        }

        private void UpdateResize(Point point)
        {
            Rect original = _dragStartSelection;
            double left = original.Left;
            double top = original.Top;
            double right = original.Right;
            double bottom = original.Bottom;

            switch (_activeHandle)
            {
                case HandleKind.NorthWest:
                    left = point.X;
                    top = point.Y;
                    break;
                case HandleKind.North:
                    top = point.Y;
                    break;
                case HandleKind.NorthEast:
                    right = point.X;
                    top = point.Y;
                    break;
                case HandleKind.East:
                    right = point.X;
                    break;
                case HandleKind.SouthEast:
                    right = point.X;
                    bottom = point.Y;
                    break;
                case HandleKind.South:
                    bottom = point.Y;
                    break;
                case HandleKind.SouthWest:
                    left = point.X;
                    bottom = point.Y;
                    break;
                case HandleKind.West:
                    left = point.X;
                    break;
            }

            const double minimum = 20;
            double maxWidth = Math.Max(1, ActualWidth);
            double maxHeight = Math.Max(1, ActualHeight);

            left = Math.Clamp(left, 0, maxWidth);
            top = Math.Clamp(top, 0, maxHeight);
            right = Math.Clamp(right, 0, maxWidth);
            bottom = Math.Clamp(bottom, 0, maxHeight);

            if (right - left < minimum)
            {
                if (_activeHandle == HandleKind.West ||
                    _activeHandle == HandleKind.NorthWest ||
                    _activeHandle == HandleKind.SouthWest)
                {
                    left = Math.Max(0, right - minimum);
                }
                else
                {
                    right = Math.Min(maxWidth, left + minimum);
                }
            }

            if (bottom - top < minimum)
            {
                if (_activeHandle == HandleKind.North ||
                    _activeHandle == HandleKind.NorthWest ||
                    _activeHandle == HandleKind.NorthEast)
                {
                    top = Math.Max(0, bottom - minimum);
                }
                else
                {
                    bottom = Math.Min(maxHeight, top + minimum);
                }
            }

            _selection = new Rect(left, top, right - left, bottom - top);
            UpdateVisuals();
        }

        private void ResetSelection()
        {
            _hasSelection = false;
            _selectionFinalized = false;
            _draggingNewSelection = false;
            _selection = Rect.Empty;
            _selectionOutline.Visibility = Visibility.Collapsed;
            _sizeBadge.Visibility = Visibility.Collapsed;
            foreach (Rectangle handle in _handles)
            {
                handle.Visibility = Visibility.Collapsed;
            }
            _toolBar.Visibility = Visibility.Collapsed;
            _actionBar.Visibility = Visibility.Collapsed;
            _hint.Visibility = Visibility.Visible;
            _annotationLayer.HasSelection = false;
            _annotationLayer.Selection = Rect.Empty;
            _annotationLayer.Refresh();
            UpdateVisuals();
        }

        private void BeginAnnotation(Point point)
        {
            if (_activeTool == null)
            {
                return;
            }

            Point clamped = ClampPointToSelection(point);
            var annotation = new Annotation
            {
                Kind = _activeTool.Value,
                Color = _currentColor,
                Thickness = _activeTool.Value == AnnotationKind.Highlighter ? 16 : 3
            };
            annotation.Points.Add(clamped);
            _previewAnnotation = annotation;
            _annotationLayer.Preview = annotation;
            _annotationLayer.Refresh();
            CaptureMouse();
        }

        private void UpdateAnnotation(Point point)
        {
            if (_previewAnnotation == null)
            {
                return;
            }

            Point clamped = ClampPointToSelection(point);
            if (_previewAnnotation.Kind == AnnotationKind.Pen ||
                _previewAnnotation.Kind == AnnotationKind.Highlighter)
            {
                Point last = _previewAnnotation.Points[_previewAnnotation.Points.Count - 1];
                if (Math.Abs(last.X - clamped.X) >= 1 || Math.Abs(last.Y - clamped.Y) >= 1)
                {
                    _previewAnnotation.Points.Add(clamped);
                }
            }
            else
            {
                if (_previewAnnotation.Points.Count < 2)
                {
                    _previewAnnotation.Points.Add(clamped);
                }
                else
                {
                    _previewAnnotation.Points[1] = clamped;
                }
            }

            _annotationLayer.Refresh();
        }

        private void CommitAnnotation()
        {
            if (_previewAnnotation == null)
            {
                return;
            }

            Annotation annotation = _previewAnnotation;
            _previewAnnotation = null;
            _annotationLayer.Preview = null;

            bool keep = true;
            if (annotation.Kind != AnnotationKind.Pen && annotation.Kind != AnnotationKind.Highlighter)
            {
                if (annotation.Points.Count < 2)
                {
                    keep = false;
                }
                else
                {
                    double dx = annotation.Points[1].X - annotation.Points[0].X;
                    double dy = annotation.Points[1].Y - annotation.Points[0].Y;
                    keep = Math.Abs(dx) >= 2 || Math.Abs(dy) >= 2;
                }
            }

            if (keep)
            {
                _annotations.Add(annotation);
                _annotationLayer.Items.Add(annotation);
                UpdateUndoState();
            }

            _annotationLayer.Refresh();
        }

        private void BeginTextEditor(Point point)
        {
            CommitTextEditor();
            Point position = ClampPointToSelection(point);
            _textPosition = position;

            var editor = new TextBox
            {
                FontFamily = new FontFamily("Segoe UI"),
                FontSize = 20,
                Foreground = new SolidColorBrush(_currentColor),
                CaretBrush = new SolidColorBrush(_currentColor),
                Background = new SolidColorBrush(Color.FromArgb(0x66, 0x00, 0x00, 0x00)),
                BorderBrush = new SolidColorBrush(Color.FromArgb(0x88, 0xFF, 0xFF, 0xFF)),
                BorderThickness = new Thickness(1),
                Padding = new Thickness(2),
                MinWidth = 120,
                MaxWidth = Math.Max(120, _selection.Right - position.X - 6),
                AcceptsReturn = false,
                TextWrapping = TextWrapping.NoWrap
            };

            editor.LostKeyboardFocus += (_, _) => CommitTextEditor();

            Canvas.SetLeft(editor, position.X);
            Canvas.SetTop(editor, position.Y);
            _chromeCanvas.Children.Add(editor);
            _textEditor = editor;

            Dispatcher.BeginInvoke(new Action(() =>
            {
                if (_textEditor != null)
                {
                    _textEditor.Focus();
                    Keyboard.Focus(_textEditor);
                }
            }), System.Windows.Threading.DispatcherPriority.Input);
        }

        private void CommitTextEditor()
        {
            if (_textEditor == null || _committingText)
            {
                return;
            }

            _committingText = true;
            TextBox editor = _textEditor;
            _textEditor = null;
            string text = editor.Text.Trim();

            if (!string.IsNullOrEmpty(text))
            {
                var annotation = new Annotation
                {
                    Kind = AnnotationKind.Text,
                    Color = _currentColor,
                    Text = text,
                    FontSize = 20,
                    TextPosition = _textPosition
                };
                _annotations.Add(annotation);
                _annotationLayer.Items.Add(annotation);
                _annotationLayer.Refresh();
                UpdateUndoState();
            }

            _chromeCanvas.Children.Remove(editor);
            _committingText = false;
            Focus();
        }

        private void CancelTextEditor()
        {
            if (_textEditor == null)
            {
                return;
            }

            TextBox editor = _textEditor;
            _textEditor = null;
            _chromeCanvas.Children.Remove(editor);
            Focus();
        }

        private void ToggleTool(AnnotationKind kind)
        {
            if (_activeTool == kind)
            {
                SetActiveTool(null);
            }
            else
            {
                SetActiveTool(kind);
            }
        }

        private void SetActiveTool(AnnotationKind? tool)
        {
            _activeTool = tool;
            UpdateToolHighlights();
            if (tool == null)
            {
                Cursor = _selectionFinalized ? Cursors.SizeAll : Cursors.Cross;
            }
            else if (tool == AnnotationKind.Text)
            {
                Cursor = Cursors.IBeam;
            }
            else
            {
                Cursor = Cursors.Cross;
            }
        }

        private sealed class WindowHandleOwner : WinForms.IWin32Window
        {
            public WindowHandleOwner(IntPtr handle)
            {
                Handle = handle;
            }

            public IntPtr Handle { get; }
        }

        private void ShowColorPicker()
        {
            var owner = new WindowHandleOwner(new WindowInteropHelper(this).Handle);
            using (var dialog = new WinForms.ColorDialog
            {
                FullOpen = true,
                AnyColor = true,
                Color = System.Drawing.Color.FromArgb(_currentColor.A, _currentColor.R, _currentColor.G, _currentColor.B)
            })
            {
                if (dialog.ShowDialog(owner) == WinForms.DialogResult.OK)
                {
                    _currentColor = Color.FromArgb(dialog.Color.A, dialog.Color.R, dialog.Color.G, dialog.Color.B);
                    _colorSwatch.Background = new SolidColorBrush(_currentColor);
                }
            }

            Activate();
            Focus();
        }

        private void Undo()
        {
            if (_textEditor != null)
            {
                CancelTextEditor();
                return;
            }

            if (_previewAnnotation != null)
            {
                _previewAnnotation = null;
                _annotationLayer.Preview = null;
                _annotationLayer.Refresh();
                ReleaseMouseCapture();
                return;
            }

            if (_annotations.Count == 0)
            {
                return;
            }

            _annotations.RemoveAt(_annotations.Count - 1);
            _annotationLayer.Items.RemoveAt(_annotationLayer.Items.Count - 1);
            _annotationLayer.Refresh();
            UpdateUndoState();
        }

        private void ClearAnnotations()
        {
            _annotations.Clear();
            _annotationLayer.Items.Clear();
            _annotationLayer.Preview = null;
            _previewAnnotation = null;
            _annotationLayer.Refresh();
            UpdateUndoState();
        }

        private BitmapSource ComposeImage()
        {
            if (!_hasSelection || _selection.Width < 1 || _selection.Height < 1)
            {
                throw new InvalidOperationException("No selection.");
            }

            double canvasWidth = Math.Max(1, ActualWidth);
            double canvasHeight = Math.Max(1, ActualHeight);
            Rect selection = _selection;

            int left = (int)Math.Round(selection.Left / canvasWidth * _screen.PixelWidth);
            int top = (int)Math.Round(selection.Top / canvasHeight * _screen.PixelHeight);
            int right = (int)Math.Round(selection.Right / canvasWidth * _screen.PixelWidth);
            int bottom = (int)Math.Round(selection.Bottom / canvasHeight * _screen.PixelHeight);

            left = Math.Clamp(left, 0, Math.Max(0, _screen.PixelWidth - 1));
            top = Math.Clamp(top, 0, Math.Max(0, _screen.PixelHeight - 1));
            right = Math.Clamp(right, left + 1, _screen.PixelWidth);
            bottom = Math.Clamp(bottom, top + 1, _screen.PixelHeight);

            int pixelWidth = right - left;
            int pixelHeight = bottom - top;

            var cropped = new CroppedBitmap(_screen, new Int32Rect(left, top, pixelWidth, pixelHeight));
            cropped.Freeze();

            double scaleX = (double)_screen.PixelWidth / canvasWidth;
            double scaleY = (double)_screen.PixelHeight / canvasHeight;

            var visual = new DrawingVisual();
            using (DrawingContext dc = visual.RenderOpen())
            {
                dc.DrawImage(cropped, new Rect(0, 0, pixelWidth, pixelHeight));
                dc.PushTransform(new MatrixTransform(
                    scaleX, 0,
                    0, scaleY,
                    -selection.Left * scaleX,
                    -selection.Top * scaleY));
                _annotationLayer.DrawForExport(dc);
                dc.Pop();
            }

            var bitmap = new RenderTargetBitmap(pixelWidth, pixelHeight, 96, 96, PixelFormats.Pbgra32);
            bitmap.Render(visual);
            bitmap.Freeze();
            return bitmap;
        }

        private void CopyToClipboard()
        {
            CommitTextEditor();

            BitmapSource image;
            try
            {
                image = ComposeImage();
            }
            catch (Exception ex)
            {
                MessageBox.Show(this, L.T("ExportFailed", ex.Message), L.T("ErrorCaption"), MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            try
            {
                ClipboardCache.CopyToClipboard(image);
            }
            catch (Exception ex)
            {
                MessageBox.Show(this, L.T("CopyFailed", ex.Message), L.T("ErrorCaption"), MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            ClipboardCache.Save(image);
            Close();
        }

        private void SaveToFile()
        {
            CommitTextEditor();

            BitmapSource image;
            try
            {
                image = ComposeImage();
            }
            catch (Exception ex)
            {
                MessageBox.Show(this, L.T("ExportFailed", ex.Message), L.T("ErrorCaption"), MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            bool jpegDefault = string.Equals(_settings.ImageFormat, "jpeg", StringComparison.OrdinalIgnoreCase);
            var dialog = new SaveFileDialog
            {
                Title = L.T("SaveDialogTitle"),
                Filter = L.T("FilterPng") + "|" + L.T("FilterJpeg"),
                FilterIndex = jpegDefault ? 2 : 1,
                DefaultExt = jpegDefault ? ".jpg" : ".png",
                AddExtension = true,
                OverwritePrompt = true,
                ValidateNames = true,
                FileName = "Lshot_" + DateTime.Now.ToString("yyyy-MM-dd_HH-mm-ss"),
                InitialDirectory = Directory.Exists(_settings.SaveFolder)
                    ? _settings.SaveFolder
                    : Environment.GetFolderPath(Environment.SpecialFolder.MyPictures)
            };

            if (dialog.ShowDialog(this) != true)
            {
                return;
            }

            try
            {
                ImageExport.Save(image, dialog.FileName, _settings.Quality);
            }
            catch (Exception ex)
            {
                MessageBox.Show(this, L.T("SaveFailed", ex.Message), L.T("ErrorCaption"), MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            Close();
            Saved?.Invoke(dialog.FileName);
        }

        private void UpdateVisuals()
        {
            double canvasWidth = Math.Max(1, ActualWidth);
            double canvasHeight = Math.Max(1, ActualHeight);

            if (!_hasSelection)
            {
                SetRectangle(_dimTop, 0, 0, canvasWidth, canvasHeight);
                SetRectangle(_dimBottom, 0, 0, 0, 0);
                SetRectangle(_dimLeft, 0, 0, 0, 0);
                SetRectangle(_dimRight, 0, 0, 0, 0);
                _selectionOutline.Visibility = Visibility.Collapsed;
                _sizeBadge.Visibility = Visibility.Collapsed;
                _toolBar.Visibility = Visibility.Collapsed;
                _actionBar.Visibility = Visibility.Collapsed;
                foreach (Rectangle handle in _handles)
                {
                    handle.Visibility = Visibility.Collapsed;
                }
                _annotationLayer.HasSelection = false;
                _annotationLayer.Selection = Rect.Empty;
                _annotationLayer.Refresh();
                return;
            }

            Rect selection = _selection;
            SetRectangle(_dimTop, 0, 0, canvasWidth, Math.Max(0, selection.Top));
            SetRectangle(_dimBottom, 0, selection.Bottom, canvasWidth, Math.Max(0, canvasHeight - selection.Bottom));
            SetRectangle(_dimLeft, 0, selection.Top, Math.Max(0, selection.Left), Math.Max(0, selection.Height));
            SetRectangle(_dimRight, selection.Right, selection.Top, Math.Max(0, canvasWidth - selection.Right), Math.Max(0, selection.Height));

            _selectionOutline.Visibility = Visibility.Visible;
            Canvas.SetLeft(_selectionOutline, selection.Left);
            Canvas.SetTop(_selectionOutline, selection.Top);
            _selectionOutline.Width = Math.Max(0, selection.Width);
            _selectionOutline.Height = Math.Max(0, selection.Height);

            int pixelWidth = (int)Math.Round(selection.Width / canvasWidth * _screen.PixelWidth);
            int pixelHeight = (int)Math.Round(selection.Height / canvasHeight * _screen.PixelHeight);
            _sizeText.Text = pixelWidth + "x" + pixelHeight;
            _sizeBadge.Visibility = Visibility.Visible;
            _sizeBadge.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
            Size badgeSize = _sizeBadge.DesiredSize;
            double badgeX = Math.Clamp(selection.Left, 4, Math.Max(4, canvasWidth - badgeSize.Width - 4));
            double badgeY = selection.Top - badgeSize.Height - 6;
            if (badgeY < 4)
            {
                badgeY = Math.Min(canvasHeight - badgeSize.Height - 4, selection.Top + 6);
            }
            badgeY = Math.Max(4, badgeY);
            Canvas.SetLeft(_sizeBadge, badgeX);
            Canvas.SetTop(_sizeBadge, badgeY);

            PositionHandles(selection);
            bool showHandles = _selectionFinalized;
            foreach (Rectangle handle in _handles)
            {
                handle.Visibility = showHandles ? Visibility.Visible : Visibility.Collapsed;
            }

            _annotationLayer.HasSelection = true;
            _annotationLayer.Selection = selection;
            _annotationLayer.Refresh();

            _toolBar.Visibility = showHandles ? Visibility.Visible : Visibility.Collapsed;
            _actionBar.Visibility = showHandles ? Visibility.Visible : Visibility.Collapsed;
            if (showHandles)
            {
                PositionChrome();
            }
            UpdateUndoState();
        }

        private void PositionHandles(Rect selection)
        {
            double cx = selection.Left + selection.Width / 2;
            double cy = selection.Top + selection.Height / 2;
            double right = selection.Right;
            double bottom = selection.Bottom;

            SetHandle(_handles[0], selection.Left, selection.Top);
            SetHandle(_handles[1], cx, selection.Top);
            SetHandle(_handles[2], right, selection.Top);
            SetHandle(_handles[3], right, cy);
            SetHandle(_handles[4], right, bottom);
            SetHandle(_handles[5], cx, bottom);
            SetHandle(_handles[6], selection.Left, bottom);
            SetHandle(_handles[7], selection.Left, cy);
        }

        private static void SetHandle(Rectangle handle, double centerX, double centerY)
        {
            Canvas.SetLeft(handle, centerX - handle.Width / 2);
            Canvas.SetTop(handle, centerY - handle.Height / 2);
        }

        private void PositionChrome()
        {
            double canvasWidth = Math.Max(1, ActualWidth);
            double canvasHeight = Math.Max(1, ActualHeight);
            Rect selection = _selection;
            const double gap = 6;
            const double margin = 4;

            _toolBar.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
            Size toolSize = _toolBar.DesiredSize;

            double toolX = selection.Right + gap;
            if (toolX + toolSize.Width > canvasWidth - margin)
            {
                toolX = canvasWidth - toolSize.Width - margin;
            }
            toolX = Math.Clamp(toolX, margin, Math.Max(margin, canvasWidth - toolSize.Width - margin));

            double toolY = selection.Bottom - toolSize.Height;
            toolY = Math.Clamp(toolY, margin, Math.Max(margin, canvasHeight - toolSize.Height - margin));
            Canvas.SetLeft(_toolBar, toolX);
            Canvas.SetTop(_toolBar, toolY);

            _actionBar.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
            Size actionSize = _actionBar.DesiredSize;

            double actionX = selection.Right - actionSize.Width;
            actionX = Math.Clamp(actionX, margin, Math.Max(margin, canvasWidth - actionSize.Width - margin));

            double actionY = selection.Bottom + gap;
            if (actionY + actionSize.Height > canvasHeight - margin)
            {
                double above = selection.Top - actionSize.Height - gap;
                actionY = above >= margin ? above : canvasHeight - actionSize.Height - margin;
            }
            actionY = Math.Clamp(actionY, margin, Math.Max(margin, canvasHeight - actionSize.Height - margin));
            Canvas.SetLeft(_actionBar, actionX);
            Canvas.SetTop(_actionBar, actionY);
        }

        private void UpdateCursor(Point point)
        {
            if (_selectionFinalized)
            {
                HandleKind handle = GetHandleAt(point);
                if (handle != HandleKind.None)
                {
                    Cursor = CursorForHandle(handle);
                    return;
                }

                if (_selection.Contains(point))
                {
                    if (_activeTool == AnnotationKind.Text)
                    {
                        Cursor = Cursors.IBeam;
                    }
                    else if (_activeTool != null)
                    {
                        Cursor = Cursors.Cross;
                    }
                    else
                    {
                        Cursor = Cursors.SizeAll;
                    }
                    return;
                }
            }

            Cursor = Cursors.Cross;
        }

        private HandleKind GetHandleAt(Point point)
        {
            if (!_selectionFinalized)
            {
                return HandleKind.None;
            }

            Rect selection = _selection;
            double cx = selection.Left + selection.Width / 2;
            double cy = selection.Top + selection.Height / 2;
            Point[] centers =
            {
                new Point(selection.Left, selection.Top),
                new Point(cx, selection.Top),
                new Point(selection.Right, selection.Top),
                new Point(selection.Right, cy),
                new Point(selection.Right, selection.Bottom),
                new Point(cx, selection.Bottom),
                new Point(selection.Left, selection.Bottom),
                new Point(selection.Left, cy)
            };

            for (int i = 0; i < centers.Length; i++)
            {
                if (Math.Abs(point.X - centers[i].X) <= 7 && Math.Abs(point.Y - centers[i].Y) <= 7)
                {
                    return (HandleKind)(i + 1);
                }
            }

            return HandleKind.None;
        }

        private static Cursor CursorForHandle(HandleKind handle)
        {
            return handle switch
            {
                HandleKind.NorthWest => Cursors.SizeNWSE,
                HandleKind.NorthEast => Cursors.SizeNESW,
                HandleKind.SouthEast => Cursors.SizeNWSE,
                HandleKind.SouthWest => Cursors.SizeNESW,
                HandleKind.North => Cursors.SizeNS,
                HandleKind.South => Cursors.SizeNS,
                HandleKind.East => Cursors.SizeWE,
                HandleKind.West => Cursors.SizeWE,
                _ => Cursors.Cross
            };
        }

        private Point ClampPoint(Point point)
        {
            double width = Math.Max(1, ActualWidth);
            double height = Math.Max(1, ActualHeight);
            return new Point(
                Math.Clamp(point.X, 0, width),
                Math.Clamp(point.Y, 0, height));
        }

        private Point ClampPointToSelection(Point point)
        {
            double left = _selection.Left;
            double top = _selection.Top;
            double right = _selection.Right;
            double bottom = _selection.Bottom;
            return new Point(
                Math.Clamp(point.X, left, right),
                Math.Clamp(point.Y, top, bottom));
        }

        private Rect ClampRect(Rect rect)
        {
            double width = Math.Max(1, ActualWidth);
            double height = Math.Max(1, ActualHeight);
            double left = Math.Clamp(rect.Left, 0, width);
            double top = Math.Clamp(rect.Top, 0, height);
            double right = Math.Clamp(rect.Right, 0, width);
            double bottom = Math.Clamp(rect.Bottom, 0, height);
            return new Rect(left, top, Math.Max(0, right - left), Math.Max(0, bottom - top));
        }

        private static void SetRectangle(Rectangle rectangle, double x, double y, double width, double height)
        {
            Canvas.SetLeft(rectangle, x);
            Canvas.SetTop(rectangle, y);
            rectangle.Width = Math.Max(0, width);
            rectangle.Height = Math.Max(0, height);
        }

        private static bool IsWithin(DependencyObject? source, DependencyObject target)
        {
            while (source != null)
            {
                if (ReferenceEquals(source, target))
                {
                    return true;
                }

                try
                {
                    source = VisualTreeHelper.GetParent(source);
                }
                catch
                {
                    return false;
                }
            }
            return false;
        }

        protected override void OnClosed(EventArgs e)
        {
            if (_textEditor != null)
            {
                _chromeCanvas.Children.Remove(_textEditor);
                _textEditor = null;
            }
            ReleaseMouseCapture();
            base.OnClosed(e);
        }
    }
}
