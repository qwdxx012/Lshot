use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
};

pub const MOD_ALT_VALUE: u32 = MOD_ALT.0;
pub const MOD_CONTROL_VALUE: u32 = MOD_CONTROL.0;
pub const MOD_SHIFT_VALUE: u32 = MOD_SHIFT.0;
pub const MOD_WIN_VALUE: u32 = MOD_WIN.0;
pub const MOD_NOREPEAT_VALUE: u32 = MOD_NOREPEAT.0;

pub const VK_SNAPSHOT: u32 = 0x2C;
pub const VK_SHIFT: u32 = 0x10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gesture {
    pub modifiers: u32,
    pub vk: u32,
    pub display: String,
}

pub const DEFAULT_REGION: &str = "Ctrl+Shift+S";
pub const DEFAULT_FULLSCREEN: &str = "Alt+PrintScreen";
pub const DEFAULT_CLIPBOARD: &str = "Ctrl+Shift+V";

fn key_display(vk: u32) -> Option<&'static str> {
    let name = match vk {
        0x41..=0x5A => {
            const LETTERS: [&str; 26] = [
                "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P",
                "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
            ];
            return Some(LETTERS[(vk - 0x41) as usize]);
        }
        0x30..=0x39 => {
            const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
            return Some(DIGITS[(vk - 0x30) as usize]);
        }
        0x70..=0x87 => {
            const FKEYS: [&str; 24] = [
                "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
                "F13", "F14", "F15", "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23",
                "F24",
            ];
            return Some(FKEYS[(vk - 0x70) as usize]);
        }
        0x2C => "PrintScreen",
        0x2D => "Insert",
        0x2E => "Delete",
        0x24 => "Home",
        0x23 => "End",
        0x21 => "PageUp",
        0x22 => "PageDown",
        0x20 => "Space",
        0x0D => "Enter",
        0x09 => "Tab",
        0x08 => "Backspace",
        0x25 => "Left",
        0x27 => "Right",
        0x26 => "Up",
        0x28 => "Down",
        0x1B => "Esc",
        0x14 => "CapsLock",
        0x13 => "Pause",
        0x91 => "ScrollLock",
        _ => return None,
    };
    Some(name)
}

fn parse_key(token: &str) -> Option<u32> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.len() == 1 {
        let character = trimmed.chars().next()?.to_ascii_uppercase();
        if character.is_ascii_alphabetic() {
            return Some(character as u32);
        }
        if character.is_ascii_digit() {
            return Some(character as u32);
        }
    }
    if trimmed.len() >= 2 {
        let upper = trimmed.to_ascii_uppercase();
        if let Some(number) = upper.strip_prefix('F') {
            if let Ok(index) = number.parse::<u32>() {
                if (1..=24).contains(&index) {
                    return Some(0x6F + index);
                }
            }
        }
    }
    let normalized: String = trimmed
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    let value = match normalized.as_str() {
        "printscreen" | "prtsc" | "snapshot" => VK_SNAPSHOT,
        "insert" | "ins" => 0x2D,
        "delete" | "del" => 0x2E,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" | "pgup" => 0x21,
        "pagedown" | "pgdn" => 0x22,
        "space" => 0x20,
        "enter" | "return" => 0x0D,
        "tab" => 0x09,
        "backspace" | "back" => 0x08,
        "left" => 0x25,
        "right" => 0x27,
        "up" => 0x26,
        "down" => 0x28,
        "esc" | "escape" => 0x1B,
        "capslock" => 0x14,
        "pause" => 0x13,
        "scrolllock" | "scroll" => 0x91,
        _ => return None,
    };
    Some(value)
}

pub fn build_display(modifiers: u32, vk: u32) -> Option<String> {
    let key = key_display(vk)?;
    let mut parts: Vec<&str> = Vec::with_capacity(5);
    if modifiers & MOD_CONTROL_VALUE != 0 {
        parts.push("Ctrl");
    }
    if modifiers & MOD_ALT_VALUE != 0 {
        parts.push("Alt");
    }
    if modifiers & MOD_SHIFT_VALUE != 0 {
        parts.push("Shift");
    }
    if modifiers & MOD_WIN_VALUE != 0 {
        parts.push("Win");
    }
    parts.push(key);
    Some(parts.join("+"))
}

pub fn parse(text: &str) -> Option<Gesture> {
    if text.trim().is_empty() {
        return None;
    }
    let mut modifiers = 0u32;
    let mut key_token: Option<&str> = None;
    for raw in text.split('+') {
        let part = raw.trim();
        if part.is_empty() {
            continue;
        }
        if part.eq_ignore_ascii_case("ctrl") || part.eq_ignore_ascii_case("control") {
            modifiers |= MOD_CONTROL_VALUE;
        } else if part.eq_ignore_ascii_case("alt") {
            modifiers |= MOD_ALT_VALUE;
        } else if part.eq_ignore_ascii_case("shift") {
            modifiers |= MOD_SHIFT_VALUE;
        } else if part.eq_ignore_ascii_case("win") || part.eq_ignore_ascii_case("windows") {
            modifiers |= MOD_WIN_VALUE;
        } else {
            if key_token.is_some() {
                return None;
            }
            key_token = Some(part);
        }
    }
    let token = key_token?;
    let vk = parse_key(token)?;
    if modifiers == 0 && vk != VK_SNAPSHOT {
        return None;
    }
    let display = build_display(modifiers, vk)?;
    Some(Gesture {
        modifiers,
        vk,
        display,
    })
}

pub fn from_key(vk: u32, modifiers: u32) -> Option<Gesture> {
    if key_display(vk).is_none() {
        return None;
    }
    if modifiers == 0 && vk != VK_SNAPSHOT {
        return None;
    }
    let display = build_display(modifiers, vk)?;
    Some(Gesture {
        modifiers,
        vk,
        display,
    })
}

pub fn normalize(text: &str, fallback: &str) -> String {
    parse(text)
        .map(|gesture| gesture.display)
        .unwrap_or_else(|| fallback.to_string())
}

pub fn same(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}
