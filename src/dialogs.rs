use std::path::{Path, PathBuf};
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
use windows::Win32::Foundation::SYSTEMTIME;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::Shell::{
    FileOpenDialog, FileSaveDialog, IFileOpenDialog, IFileSaveDialog, IShellItem,
    SHCreateItemFromParsingName, Common::COMDLG_FILTERSPEC, FOS_FILEMUSTEXIST,
    FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS,
    SIGDN_FILESYSPATH,
};

use crate::win::{self, MB_DEFBUTTON2, MB_ICONERROR, MB_ICONWARNING, MB_OK, MB_YESNO};

pub fn pick_folder(owner: HWND, description: &str, initial: Option<&Path>) -> Option<PathBuf> {
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let options = dialog.GetOptions().unwrap_or_default();
        dialog
            .SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST)
            .ok()?;
        let title = win::wide(description);
        let _ = dialog.SetTitle(PCWSTR(title.as_ptr()));
        if let Some(folder) = initial {
            if folder.is_dir() {
                let wide_path = win::wide(&folder.to_string_lossy());
                if let Ok(item) =
                    SHCreateItemFromParsingName::<_, Option<&windows::Win32::System::Com::IBindCtx>, IShellItem>(
                        PCWSTR(wide_path.as_ptr()),
                        None,
                    )
                {
                    let _ = dialog.SetFolder(&item);
                }
            }
        }
        if dialog.Show(owner).is_err() {
            return None;
        }
        let item = dialog.GetResult().ok()?;
        shell_item_path(&item)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn save_file(
    owner: HWND,
    title: &str,
    initial_directory: &Path,
    file_name: &str,
    jpeg_default: bool,
) -> Option<PathBuf> {
    unsafe {
        let dialog: IFileSaveDialog =
            CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let options = dialog.GetOptions().unwrap_or_default();
        dialog
            .SetOptions(
                options | FOS_OVERWRITEPROMPT | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST,
            )
            .ok()?;

        let title_wide = win::wide(title);
        let _ = dialog.SetTitle(PCWSTR(title_wide.as_ptr()));

        let png_name = win::wide(crate::i18n::t("FilterPng"));
        let png_spec = win::wide("*.png");
        let jpeg_name = win::wide(crate::i18n::t("FilterJpeg"));
        let jpeg_spec = win::wide("*.jpg;*.jpeg");
        let filters = [
            COMDLG_FILTERSPEC {
                pszName: PCWSTR(png_name.as_ptr()),
                pszSpec: PCWSTR(png_spec.as_ptr()),
            },
            COMDLG_FILTERSPEC {
                pszName: PCWSTR(jpeg_name.as_ptr()),
                pszSpec: PCWSTR(jpeg_spec.as_ptr()),
            },
        ];
        let _ = dialog.SetFileTypes(&filters);
        let _ = dialog.SetFileTypeIndex(if jpeg_default { 2 } else { 1 });

        let extension = win::wide(if jpeg_default { ".jpg" } else { ".png" });
        let _ = dialog.SetDefaultExtension(PCWSTR(extension.as_ptr()));

        let name_wide = win::wide(file_name);
        let _ = dialog.SetFileName(PCWSTR(name_wide.as_ptr()));

        if initial_directory.is_dir() {
            let folder = win::wide(&initial_directory.to_string_lossy());
            if let Ok(item) =
                SHCreateItemFromParsingName::<_, Option<&windows::Win32::System::Com::IBindCtx>, IShellItem>(
                    PCWSTR(folder.as_ptr()),
                    None,
                )
            {
                let _ = dialog.SetFolder(&item);
            }
        }

        if dialog.Show(owner).is_err() {
            return None;
        }
        let item = dialog.GetResult().ok()?;
        let mut path = shell_item_path(&item)?;

        let has_extension = path
            .extension()
            .map(|value| !value.is_empty())
            .unwrap_or(false);
        if !has_extension {
            let extension = if jpeg_default { "jpg" } else { "png" };
            path.set_extension(extension);
        }
        Some(path)
    }
}

fn shell_item_path(item: &IShellItem) -> Option<PathBuf> {
    unsafe {
        let pointer = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        if pointer.is_null() {
            return None;
        }
        let value = win::pwstr_to_string(pointer);
        CoTaskMemFree(Some(pointer.0 as *const _));
        value.map(PathBuf::from)
    }
}

pub fn default_file_name() -> String {
    unsafe {
        let time: SYSTEMTIME = GetLocalTime();
        format!(
            "Lshot_{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
            time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond
        )
    }
}

pub fn timestamp_name() -> String {
    unsafe {
        let time: SYSTEMTIME = GetLocalTime();
        let mut counter = 0u32;
        let base = format!(
            "{:04}{:02}{:02}_{:02}{:02}{:02}_{:03}",
            time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond,
            time.wMilliseconds
        );
        let mut candidate = format!("Lshot_{}.png", base);
        while crate::settings::local_app_data().join("Lshot").join("ClipboardCache").join(&candidate).exists()
            && counter < 1000
        {
            counter += 1;
            candidate = format!("Lshot_{}_{}.png", base, counter);
        }
        candidate
    }
}

pub fn show_error(owner: HWND, message: &str) {
    win::message_box(
        owner,
        message,
        crate::i18n::t("ErrorCaption"),
        MB_OK | MB_ICONERROR,
    );
}

pub fn show_warning(owner: HWND, message: &str) {
    win::message_box(
        owner,
        message,
        crate::i18n::t("ErrorCaption"),
        MB_OK | MB_ICONWARNING,
    );
}

pub fn confirm_yes_no(owner: HWND, message: &str, caption: &str) -> bool {
    win::message_box(
        owner,
        message,
        caption,
        MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
    )
    .0
        == win::IDYES_VALUE
}
