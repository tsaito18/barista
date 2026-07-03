//! バックグラウンドで指定時間スリープするだけのタイマー。
//! `SetThreadExecutionState` はスレッドアフィンなためここでは呼び出さず、
//! メインスレッドへメッセージを投げて処理を委譲する。

use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP};

/// タイマー満了を通知するスレッドメッセージ。hwnd を持たないスレッドメッセージとして送るため、
/// 通常のウィンドウメッセージと衝突しない `WM_APP` 領域を使う。
pub const WM_APP_EXPIRE: u32 = WM_APP + 1;

/// `minutes` 分後に `main_thread_id` へ満了メッセージを送るスレッドを起動する。
///
/// `generation` をそのまま `wParam` に載せて運ぶことで、送信後に別の選択で状態が
/// 変わっていた場合（古い世代のタイマーの発火）をメインスレッド側で判別・無視できるようにする。
pub fn start(minutes: u32, generation: u64, main_thread_id: u32) {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(u64::from(minutes) * 60));
        unsafe {
            let _ = PostThreadMessageW(
                main_thread_id,
                WM_APP_EXPIRE,
                WPARAM(generation as usize),
                LPARAM(0),
            );
        }
    });
}
