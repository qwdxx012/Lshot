using System;
using System.Collections.Generic;
using System.Globalization;

namespace Lshot
{
    internal static class L
    {
        private static string _language = "ru";

        private static readonly Dictionary<string, (string Ru, string En)> Strings =
            new Dictionary<string, (string Ru, string En)>(StringComparer.Ordinal)
            {
                ["AppName"] = ("Lshot", "Lshot"),
                ["AppTagline"] = (
                    "Быстрые скриншоты с аннотациями и мгновенным предпросмотром изображений.",
                    "Fast annotated screenshots with instant image previews."),
                ["SettingsTitle"] = ("Настройки Lshot", "Lshot Settings"),
                ["TraySettings"] = ("Настройки...", "Settings..."),
                ["TrayAbout"] = ("О программе...", "About..."),
                ["TrayExit"] = ("Выход", "Exit"),
                ["TrayTooltip"] = (
                    "Lshot - скриншоты и предпросмотр изображений",
                    "Lshot - screenshots and image previews"),
                ["TabGeneral"] = ("Основные", "General"),
                ["TabHotkeys"] = ("Горячие клавиши", "Hotkeys"),
                ["TabFormat"] = ("Формат", "Format"),
                ["Language"] = ("Язык", "Language"),
                ["LanguageRussian"] = ("Русский", "Русский"),
                ["LanguageEnglish"] = ("English", "English"),
                ["ShowNotification"] = (
                    "Показывать уведомление при сохранении скриншота",
                    "Show a notification when a screenshot is saved"),
                ["SaveFolder"] = ("Папка сохранения", "Save folder"),
                ["Browse"] = ("Обзор...", "Browse..."),
                ["Autostart"] = ("Автозапуск с Windows", "Start with Windows"),
                ["HotkeyRegion"] = ("Основная горячая клавиша", "Main region capture hotkey"),
                ["HotkeyFullscreen"] = ("Скриншот всего экрана", "Fullscreen screenshot hotkey"),
                ["HotkeyClipboard"] = ("Открыть буфер обмена", "Open Clipboard History"),
                ["HotkeysHint"] = (
                    "Щёлкните по полю и нажмите сочетание клавиш. Backspace: сбросить.",
                    "Click a field and press a key combination. Backspace resets it."),
                ["HotkeysInfo"] = (
                    "Основная клавиша открывает редактор выделенной области. Вторая клавиша сразу выделяет весь экран. Третья клавиша открывает историю буфера обмена.",
                    "The main hotkey opens the region editor. The second hotkey selects the entire screen. The third hotkey opens clipboard history."),
                ["ImageFormat"] = ("Формат изображения", "Image format"),
                ["Quality"] = ("Качество", "Quality"),
                ["QualityHint"] = ("Применяется к JPEG.", "Applies to JPEG."),
                ["Save"] = ("Сохранить", "Save"),
                ["Cancel"] = ("Отмена", "Cancel"),
                ["AboutCaption"] = ("О программе", "About"),
                ["AboutText"] = (
                    "Lshot {0}\n\nЛёгкий офлайн-инструмент для создания скриншотов с аннотациями и предпросмотра изображений по Shift + наведению.\n\nВсе данные остаются на вашем компьютере.",
                    "Lshot {0}\n\nA lightweight offline tool for annotated screenshots and Shift + hover image previews.\n\nAll data stays on your computer."),
                ["AboutVersion"] = ("Версия {0}", "Version {0}"),
                ["NotificationSavedTitle"] = ("Скриншот сохранён", "Screenshot saved"),
                ["NotificationSavedText"] = (
                    "Нажмите, чтобы показать файл в Проводнике:\n{0}",
                    "Click to reveal the file in Explorer:\n{0}"),
                ["HotkeyPressPrompt"] = ("Нажмите сочетание...", "Press keys..."),
                ["HotkeyInvalid"] = ("Некорректное сочетание клавиш.", "Invalid key combination."),
                ["HotkeyModifierRequired"] = (
                    "Добавьте Ctrl, Alt, Shift или Win к сочетанию.",
                    "Add Ctrl, Alt, Shift, or Win to the shortcut."),
                ["HotkeyDuplicate"] = (
                    "Горячие клавиши не должны повторяться.",
                    "Hotkeys must be unique."),
                ["HotkeyRegisterError"] = (
                    "Не удалось зарегистрировать горячие клавиши: {0}",
                    "Could not register hotkeys: {0}"),
                ["FolderRequired"] = ("Выберите папку сохранения.", "Please choose a save folder."),
                ["FolderCreateError"] = (
                    "Не удалось создать папку:\n\n{0}",
                    "The folder could not be created:\n\n{0}"),
                ["StartupError"] = (
                    "Не удалось изменить автозапуск:\n\n{0}",
                    "The Windows startup setting could not be changed:\n\n{0}"),
                ["SettingsSaveError"] = (
                    "Не удалось сохранить настройки:\n\n{0}",
                    "Settings could not be saved:\n\n{0}"),
                ["FolderBrowseDescription"] = (
                    "Выберите папку для сохранения скриншотов",
                    "Choose the folder for saved screenshots"),
                ["DragHint"] = (
                    "Выделите область   •   Esc: отмена",
                    "Drag to select   •   Esc: cancel"),
                ["ToolPen"] = ("Перо", "Pen"),
                ["ToolLine"] = ("Линия", "Line"),
                ["ToolArrow"] = ("Стрелка", "Arrow"),
                ["ToolRectangle"] = ("Прямоугольник", "Rectangle"),
                ["ToolHighlighter"] = ("Маркер", "Highlighter"),
                ["ToolText"] = ("Текст", "Text"),
                ["ToolColor"] = ("Цвет", "Color"),
                ["ToolUndo"] = ("Отменить", "Undo"),
                ["ActionCopy"] = ("Копировать", "Copy to clipboard"),
                ["ActionSave"] = ("Сохранить", "Save"),
                ["ActionClose"] = ("Закрыть", "Close"),
                ["SaveDialogTitle"] = ("Сохранить скриншот", "Save screenshot"),
                ["FilterPng"] = ("PNG изображение (*.png)|*.png", "PNG image (*.png)|*.png"),
                ["FilterJpeg"] = (
                    "JPEG изображение (*.jpg;*.jpeg)|*.jpg;*.jpeg",
                    "JPEG image (*.jpg;*.jpeg)|*.jpg;*.jpeg"),
                ["CopyFailed"] = (
                    "Не удалось скопировать изображение в буфер обмена.\n\n{0}",
                    "The image could not be copied to the clipboard.\n\n{0}"),
                ["SaveFailed"] = (
                    "Не удалось сохранить изображение.\n\n{0}",
                    "The image could not be saved.\n\n{0}"),
                ["ExportFailed"] = (
                    "Не удалось подготовить изображение.\n\n{0}",
                    "The image could not be prepared.\n\n{0}"),
                ["ErrorCaption"] = ("Ошибка Lshot", "Lshot error"),
                ["ClipboardTitle"] = ("Буфер обмена Lshot", "Lshot Clipboard"),
                ["ClipboardBufferSize"] = ("Размер буфера: {0}", "Buffer size: {0}"),
                ["ClipboardClear"] = ("Очистить буфер", "Clear Clipboard"),
                ["ClipboardClearConfirm"] = (
                    "Вы уверены, что хотите полностью очистить буфер обмена? Все сохраненные в нем снимки будут удалены.",
                    "Are you sure you want to clear the entire clipboard history? All saved snapshots in it will be deleted."),
                ["ClipboardClearCaption"] = ("Очистка буфера", "Clear clipboard history"),
                ["ClipboardClearFailed"] = (
                    "Не удалось полностью очистить буфер обмена.\n\n{0}",
                    "The clipboard history could not be fully cleared.\n\n{0}"),
                ["ClipboardEmptyTitle"] = ("Буфер обмена пуст", "Clipboard history is empty"),
                ["ClipboardEmptyHint"] = (
                    "Скопируйте снимок в редакторе, и он появится здесь.",
                    "Copy a snapshot in the editor and it will appear here."),
                ["ClipboardCopied"] = ("Скопировано!", "Copied!"),
                ["ClipboardCopy"] = ("Скопировать", "Copy"),
                ["ClipboardDelete"] = ("Удалить", "Delete"),
                ["ClipboardDeleteFailed"] = (
                    "Не удалось удалить снимок.\n\n{0}",
                    "The snapshot could not be deleted.\n\n{0}"),
                ["ClipboardHint"] = (
                    "Нажмите на снимок, чтобы скопировать его в буфер обмена.",
                    "Click a snapshot to copy it to the clipboard."),
                ["UnitBytes"] = ("Б", "B"),
                ["UnitKb"] = ("КБ", "KB"),
                ["UnitMb"] = ("МБ", "MB"),
                ["UnitGb"] = ("ГБ", "GB")
            };

        public static string Language => _language;

        public static bool IsEnglish => string.Equals(_language, "en", StringComparison.OrdinalIgnoreCase);

        public static void SetLanguage(string? language)
        {
            _language = string.Equals(language, "en", StringComparison.OrdinalIgnoreCase) ? "en" : "ru";
        }

        public static string T(string key, params object?[] arguments)
        {
            if (!Strings.TryGetValue(key, out (string Ru, string En) entry))
            {
                return key;
            }

            string template = IsEnglish ? entry.En : entry.Ru;
            return arguments.Length == 0
                ? template
                : string.Format(CultureInfo.CurrentCulture, template, arguments);
        }
    }
}
