# Lshot

Lshot is a lightweight Windows screenshot tool with a Lightshot-style region editor, instant image previews, and a local clipboard history. It works fully offline.

Platform: Windows 10 and Windows 11, 64-bit.

---

## Русский

### О программе

Lshot - быстрый офлайн-инструмент для создания скриншотов, добавления аннотаций и предпросмотра изображений. Приложение работает в системном трее и не отправляет данные в интернет.

### Возможности

- Редактор выделенной области в стиле Lightshot: изменение размера, перемещение, перо, линия, стрелка, прямоугольник, маркер, текст, выбор цвета и отмена действий.
- Копирование или сохранение готового снимка с аннотациями в PNG или JPEG.
- История буфера обмена: каждый скопированный снимок автоматически сохраняется локально, открывается по горячей клавише и может быть скопирован повторно.
- Предпросмотр изображений: удерживайте Shift и наведите курсор на файл в Проводнике.
- Настраиваемые глобальные горячие клавиши.
- Уведомления о сохранении и переход к файлу в Проводнике.
- Полностью офлайн: без телеметрии, без сетевых запросов, без облака.

### Горячие клавиши по умолчанию

| Действие | Сочетание |
| --- | --- |
| Выделение области | `Ctrl+Shift+S` |
| Скриншот всего экрана | `Alt+PrintScreen` |
| История буфера обмена | `Ctrl+Shift+V` |

Все сочетания можно изменить в разделе **Настройки > Горячие клавиши**.

### Скачивание и установка

1. Откройте вкладку **Releases** на странице этого репозитория.
2. Выберите последний выпуск.
3. В блоке **Assets** скачайте файл `Lshot-Setup.exe`.
4. Запустите установщик. Права администратора не требуются.

Установщик:

- устанавливает приложение для текущего пользователя в `%LocalAppData%\Programs\Lshot`;
- создаёт ярлык в меню Пуск и предлагает дополнительный ярлык на рабочем столе;
- предлагает включить запуск Lshot при входе в Windows;
- закрывает запущенный экземпляр Lshot перед обновлением или удалением;
- запускает Lshot после завершения установки.

Для запуска без установки скачайте из **Assets** файл `Lshot-Portable.exe`. Портативная версия не создаёт ярлыков и записей в реестре.

### Приватность

Lshot не использует сеть и не собирает статистику. Настройки хранятся локально. История буфера обмена находится в `%LocalAppData%\Lshot\ClipboardCache` и удаляется только через кнопку **Очистить буфер** или вручную.

### Сборка из исходников

Требования: Rust (stable), Inno Setup 6 для установщика.

```powershell
cargo build --release
New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item target\release\lshot.exe dist\Lshot-Portable.exe
iscc installer.iss
```

Результаты:

- `target\release\lshot.exe`: исполняемый файл приложения.
- `dist\Lshot-Portable.exe`: портативная версия.
- `dist\Lshot-Setup.exe`: установщик.

Иконка и сведения о версии встраиваются в EXE при сборке из `logo.ico`. Если компилятор ресурсов (`windres`, `llvm-rc` или `rc.exe`) недоступен, сборка проходит без них — иконку можно добавить после сборки:

```powershell
python tools\embed_icon.py target\release\lshot.exe logo.ico
```

`logo.ico` построен из `installer.png` (иконка установщика, файла и ярлыков), `logo.png` — иконка в трее. Оба PNG встроены в бинарь, поэтому портативная версия остаётся одним файлом.

### Автоматический выпуск

Workflow `.github/workflows/release.yml` собирает портативную версию и установщик при отправке тега вида `v1.0.0`, а затем публикует оба файла в GitHub Releases. Запуск можно выполнить и вручную через **Actions > Release > Run workflow**.

---

## English

### About

Lshot is a fast offline tool for screenshots, annotations, and image previews. It lives in the system tray and never sends data to the internet.

### Features

- Lightshot-style region editor: resize, move, pen, line, arrow, rectangle, highlighter, text, color picker, and undo.
- Copy or save the annotated result as PNG or JPEG.
- Clipboard history: every copied screenshot is saved locally, can be opened with a hotkey, and can be copied again at any time.
- Image preview: hold Shift and hover a file in File Explorer.
- Customizable global hotkeys.
- Save notifications with a shortcut to reveal the file in Explorer.
- Fully offline: no telemetry, no network calls, no cloud.

### Default hotkeys

| Action | Shortcut |
| --- | --- |
| Region capture | `Ctrl+Shift+S` |
| Fullscreen screenshot | `Alt+PrintScreen` |
| Clipboard history | `Ctrl+Shift+V` |

All shortcuts can be changed in **Settings > Hotkeys**.

### Download and install

1. Open the **Releases** tab of this repository.
2. Select the latest release.
3. In the **Assets** section, download `Lshot-Setup.exe`.
4. Run the installer. Administrator rights are not required.

The installer:

- installs per user into `%LocalAppData%\Programs\Lshot`;
- creates a Start Menu shortcut and offers an optional Desktop shortcut;
- offers to run Lshot at Windows startup;
- closes any running Lshot instance before an update or uninstall;
- launches Lshot when setup finishes.

To run without installing, download `Lshot-Portable.exe` from **Assets**. The portable build does not create shortcuts or registry entries.

### Privacy

Lshot does not use the network and does not collect analytics. Settings are stored locally. Clipboard history is stored in `%LocalAppData%\Lshot\ClipboardCache` and is removed only with the **Clear Clipboard** button or manually.

### Build from source

Requirements: Rust (stable), Inno Setup 6 for the installer.

```powershell
cargo build --release
New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item target\release\lshot.exe dist\Lshot-Portable.exe
iscc installer.iss
```

Outputs:

- `target\release\lshot.exe`: the application executable.
- `dist\Lshot-Portable.exe`: portable build.
- `dist\Lshot-Setup.exe`: installer.

The icon and version information are embedded into the EXE at build time from `logo.ico`. Without a resource compiler (`windres`, `llvm-rc`, or `rc.exe`) the build succeeds without them; the icon can be added afterwards:

```powershell
python tools\embed_icon.py target\release\lshot.exe logo.ico
```

`logo.ico` is generated from `installer.png` (installer, file and shortcut icon), `logo.png` is the tray icon. Both PNGs are compiled into the binary, so the portable build stays a single file.

### Automated release

The `.github/workflows/release.yml` workflow builds the portable executable and the installer when a tag such as `v1.0.0` is pushed, then uploads both files to GitHub Releases. It can also be started manually from **Actions > Release > Run workflow**.
