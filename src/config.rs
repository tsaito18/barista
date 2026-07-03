//! `keep_display` の永続化。追加クレートなしでバイナリサイズを増やさないため、
//! per-user 設定として HKCU（管理者権限不要・他ユーザーに影響しない）を使う。
//! モードは意図的に対象外（起動時は常に無効から始める仕様）。

use core::ffi::c_void;
use core::mem::size_of;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegGetValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_SET_VALUE, REG_DWORD, REG_OPTION_NON_VOLATILE, RRF_RT_REG_DWORD,
};

/// キー/値が存在しない場合や読み取り失敗時は既定値 `false`（オプトイン）を返す。
pub fn load_keep_display() -> bool {
    let mut data: u32 = 0;
    let mut size = size_of::<u32>() as u32;

    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Barista"),
            w!("keep_display"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };

    status == ERROR_SUCCESS && data != 0
}

/// 常駐トレイアプリ（コンソールなし）のため失敗時はログせず無視する
/// （保存に失敗しても常駐継続を優先するベストエフォート仕様）。
pub fn save_keep_display(value: bool) {
    let mut hkey = HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Barista"),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut hkey,
            None,
        )
    };
    if status != ERROR_SUCCESS {
        return;
    }

    let data = (value as u32).to_le_bytes();
    unsafe {
        let _ = RegSetValueExW(hkey, w!("keep_display"), None, REG_DWORD, Some(&data));
        let _ = RegCloseKey(hkey);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 実レジストリを往復させて確認する。最後に false へ戻すため残留状態を作らない。
    #[test]
    fn roundtrip() {
        save_keep_display(true);
        assert!(load_keep_display());
        save_keep_display(false);
        assert!(!load_keep_display());
    }
}
