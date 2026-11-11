use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE,
    REG_OPTION_NON_VOLATILE, REG_SAM_FLAGS, REG_SZ, REG_VALUE_TYPE,
};
use windows::Win32::UI::Shell::{
    SHGetKnownFolderPath, FOLDERID_Desktop, FOLDERID_Pictures, FOLDERID_PublicDesktop,
    KF_FLAG_DEFAULT,
};

use crate::hotkeys;
use crate::win::wide;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct AppSettings {
    pub language: String,
    pub show_save_notification: bool,
    pub save_folder: String,
    pub run_at_startup: bool,
    pub region_hotkey: String,
    pub fullscreen_hotkey: String,
    pub clipboard_hotkey: String,
    pub image_format: String,
    pub quality: i32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "ru".to_string(),
            show_save_notification: true,
            save_folder: default_save_folder(),
            run_at_startup: false,
            region_hotkey: hotkeys::DEFAULT_REGION.to_string(),
            fullscreen_hotkey: hotkeys::DEFAULT_FULLSCREEN.to_string(),
            clipboard_hotkey: hotkeys::DEFAULT_CLIPBOARD.to_string(),
            image_format: "png".to_string(),
            quality: 95,
        }
    }
}

impl AppSettings {
    pub fn normalized(mut self) -> Self {
        self.normalize();
        self
    }

    pub fn normalize(&mut self) {
        self.language = if self.language.eq_ignore_ascii_case("en") {
            "en".to_string()
        } else {
            "ru".to_string()
        };

        if self.save_folder.trim().is_empty() {
            self.save_folder = default_save_folder();
        } else {
            let expanded = expand_environment_variables(self.save_folder.trim().trim_matches('"'));
            self.save_folder = expanded;
        }

        let format = self.image_format.trim().to_ascii_lowercase();
        self.image_format = if format.starts_with("jp") {
            "jpeg".to_string()
        } else {
            "png".to_string()
        };

        self.quality = self.quality.clamp(1, 100);

        self.region_hotkey = hotkeys::normalize(&self.region_hotkey, hotkeys::DEFAULT_REGION);
        self.fullscreen_hotkey =
            hotkeys::normalize(&self.fullscreen_hotkey, hotkeys::DEFAULT_FULLSCREEN);
        self.clipboard_hotkey =
            hotkeys::normalize(&self.clipboard_hotkey, hotkeys::DEFAULT_CLIPBOARD);

        if hotkeys::same(&self.region_hotkey, &self.fullscreen_hotkey) {
            self.fullscreen_hotkey = hotkeys::normalize(
                hotkeys::DEFAULT_FULLSCREEN,
                hotkeys::DEFAULT_FULLSCREEN,
            );
            if hotkeys::same(&self.region_hotkey, &self.fullscreen_hotkey) {
                self.region_hotkey =
                    hotkeys::normalize(hotkeys::DEFAULT_REGION, hotkeys::DEFAULT_REGION);
            }
        }

        if hotkeys::same(&self.clipboard_hotkey, &self.region_hotkey)
            || hotkeys::same(&self.clipboard_hotkey, &self.fullscreen_hotkey)
        {
            self.clipboard_hotkey =
                hotkeys::normalize(hotkeys::DEFAULT_CLIPBOARD, hotkeys::DEFAULT_CLIPBOARD);
            if hotkeys::same(&self.clipboard_hotkey, &self.region_hotkey)
                || hotkeys::same(&self.clipboard_hotkey, &self.fullscreen_hotkey)
            {
                self.clipboard_hotkey = "Alt+C".to_string();
            }
            if hotkeys::same(&self.clipboard_hotkey, &self.region_hotkey)
                || hotkeys::same(&self.clipboard_hotkey, &self.fullscreen_hotkey)
            {
                self.clipboard_hotkey = "Ctrl+Alt+C".to_string();
            }
        }
    }
}

pub fn exe_directory() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn local_app_data() -> PathBuf {
    if let Some(value) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(value);
    }
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        return PathBuf::from(profile).join("AppData").join("Local");
    }
    PathBuf::from(".")
}

pub fn known_folder(folder: &windows::core::GUID) -> Option<PathBuf> {
    unsafe {
        let path: PWSTR = SHGetKnownFolderPath(folder, KF_FLAG_DEFAULT, None).ok()?;
        if path.is_null() {
            return None;
        }
        let mut length = 0usize;
        while *path.0.add(length) != 0 {
            length += 1;
        }
        let slice = std::slice::from_raw_parts(path.0, length);
        let value = OsString::from_wide(slice);
        CoTaskMemFree(Some(path.0 as *const _));
        Some(PathBuf::from(value))
    }
}

pub fn pictures_folder() -> PathBuf {
    known_folder(&FOLDERID_Pictures)
        .or_else(|| std::env::var_os("USERPROFILE").map(|profile| PathBuf::from(profile).join("Pictures")))
        .unwrap_or_else(|| exe_directory())
}

pub fn desktop_folder() -> Option<PathBuf> {
    known_folder(&FOLDERID_Desktop)
}

pub fn public_desktop_folder() -> Option<PathBuf> {
    known_folder(&FOLDERID_PublicDesktop)
}

pub fn default_save_folder() -> String {
    pictures_folder().join("Lshot").to_string_lossy().to_string()
}

pub fn expand_environment_variables(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0usize;
    while index < chars.len() {
        if chars[index] == '%' {
            if let Some(end) = chars[index + 1..].iter().position(|character| *character == '%') {
                let name: String = chars[index + 1..index + 1 + end].iter().collect();
                if !name.is_empty() {
                    if let Ok(value) = std::env::var(&name) {
                        result.push_str(&value);
                        index = index + end + 2;
                        continue;
                    }
                }
            }
        }
        result.push(chars[index]);
        index += 1;
    }
    result
}

pub struct SettingsStore;

impl SettingsStore {
    pub fn primary_path() -> PathBuf {
        exe_directory().join("settings.json")
    }

    pub fn fallback_path() -> PathBuf {
        local_app_data().join("Lshot").join("settings.json")
    }

    pub fn current_path() -> PathBuf {
        let primary = Self::primary_path();
        if primary.exists() {
            return primary;
        }
        let fallback = Self::fallback_path();
        if fallback.exists() {
            return fallback;
        }
        primary
    }

    pub fn load() -> AppSettings {
        let path = Self::current_path();
        if path.exists() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(settings) = serde_json::from_str::<AppSettings>(&text) {
                    return settings.normalized();
                }
            }
        }
        AppSettings::default().normalized()
    }

    pub fn save(settings: &mut AppSettings) -> Result<(), String> {
        settings.normalize();
        let primary = Self::primary_path();
        match Self::write_to(settings, &primary) {
            Ok(()) => Ok(()),
            Err(primary_error) => {
                let fallback = Self::fallback_path();
                if let Some(parent) = fallback.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                Self::write_to(settings, &fallback).map_err(|fallback_error| {
                    format!("{}\n{}", primary_error, fallback_error)
                })
            }
        }
    }

    fn write_to(settings: &AppSettings, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let json = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, json).map_err(|error| error.to_string())?;
        std::fs::rename(&temp, path).or_else(|_| {
            std::fs::copy(&temp, path)
                .map(|_| ())
                .map_err(|error| error.to_string())
                .and_then(|_| {
                    let _ = std::fs::remove_file(&temp);
                    Ok(())
                })
        })
    }
}

pub struct StartupManager;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "Lshot";
const LEGACY_RUN_VALUE: &str = "HoverShot";

unsafe fn read_run_value(name: &str) -> Option<String> {
    let key_path = wide(RUN_KEY);
    let mut key = HKEY::default();
    if RegOpenKeyExW(
        HKEY_CURRENT_USER,
        PCWSTR(key_path.as_ptr()),
        0,
        KEY_QUERY_VALUE,
        &mut key,
    ) != ERROR_SUCCESS
    {
        return None;
    }
    let value_name = wide(name);
    let mut value_type = REG_VALUE_TYPE(0);
    let mut size = 0u32;
    let status = RegQueryValueExW(
        key,
        PCWSTR(value_name.as_ptr()),
        None,
        Some(&mut value_type),
        None,
        Some(&mut size),
    );
    if status != ERROR_SUCCESS || size == 0 {
        let _ = RegCloseKey(key);
        return None;
    }
    let mut buffer = vec![0u8; size as usize];
    let status = RegQueryValueExW(
        key,
        PCWSTR(value_name.as_ptr()),
        None,
        Some(&mut value_type),
        Some(buffer.as_mut_ptr()),
        Some(&mut size),
    );
    let _ = RegCloseKey(key);
    if status != ERROR_SUCCESS {
        return None;
    }
    buffer.truncate(size as usize);
    if buffer.len() >= 2 && buffer.last() == Some(&0) {
        buffer.truncate(buffer.len() - 2);
    }
    let wide_chars: Vec<u16> = buffer
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    Some(OsString::from_wide(&wide_chars).to_string_lossy().to_string())
}

pub fn create_run_key(access: REG_SAM_FLAGS) -> Result<HKEY, String> {
    unsafe {
        let key_path = wide(RUN_KEY);
        let mut key = HKEY::default();
        let status = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key_path.as_ptr()),
            0,
            None,
            REG_OPTION_NON_VOLATILE,
            access,
            None,
            &mut key,
            None,
        );
        if status != ERROR_SUCCESS {
            return Err(format!("Could not open the Windows startup registry key ({}).", status.0));
        }
        Ok(key)
    }
}

impl StartupManager {
    pub fn is_enabled() -> bool {
        unsafe { read_run_value(RUN_VALUE).is_some() }
    }

    pub fn cleanup_legacy_entry() {
        unsafe {
            let key_path = wide(RUN_KEY);
            let mut key = HKEY::default();
            if RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(key_path.as_ptr()),
                0,
                KEY_SET_VALUE,
                &mut key,
            ) == ERROR_SUCCESS
            {
                let value_name = wide(LEGACY_RUN_VALUE);
                let _ = RegDeleteValueW(key, PCWSTR(value_name.as_ptr()));
                let _ = RegCloseKey(key);
            }
        }
    }

    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        unsafe {
            let key = create_run_key(KEY_SET_VALUE)?;
            let value_name = wide(RUN_VALUE);
            if !enabled {
                let _ = RegDeleteValueW(key, PCWSTR(value_name.as_ptr()));
                let _ = RegCloseKey(key);
                return Ok(());
            }
            let executable = std::env::current_exe()
                .map_err(|error| error.to_string())?
                .to_string_lossy()
                .to_string();
            let quoted = format!("\"{}\"", executable);
            let mut bytes: Vec<u8> = Vec::new();
            for unit in quoted.encode_utf16() {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
            bytes.extend_from_slice(&[0, 0]);
            let status = RegSetValueExW(
                key,
                PCWSTR(value_name.as_ptr()),
                0,
                REG_SZ,
                Some(&bytes),
            );
            let _ = RegCloseKey(key);
            if status != ERROR_SUCCESS {
                return Err(format!("The startup value could not be written ({}).", status.0));
            }
            Ok(())
        }
    }
}
