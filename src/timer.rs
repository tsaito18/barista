//! バックグラウンドで指定時間後にメインスレッドへ満了を通知するタイマー。
//! `SetThreadExecutionState` はスレッドアフィンなためここでは呼び出さず、
//! メインスレッドへメッセージを投げて処理を委譲する。
//!
//! 待機には相対時間の `thread::sleep` ではなく、絶対時刻を設定した待機可能タイマーを使う。
//! 相対待機はシステムサスペンド中カウントが止まるため、スリープ抑止中に手動スリープすると
//! 復帰後もタイマーが満了せず「残り 0:00 のまま抑止が有効」な状態が残る。絶対時刻タイマーなら
//! 期限がサスペンド中に過ぎても復帰時にシグナル化され、正しく満了を通知できる。

use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, LPARAM, WPARAM};
use windows::Win32::System::Threading::{
    CreateWaitableTimerW, SetWaitableTimer, WaitForSingleObject, INFINITE,
};
use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP};

/// タイマー満了を通知するスレッドメッセージ。hwnd を持たないスレッドメッセージとして送るため、
/// 通常のウィンドウメッセージと衝突しない `WM_APP` 領域を使う。
pub const WM_APP_EXPIRE: u32 = WM_APP + 1;

/// FILETIME(1601-01-01 起点)と Unix 時刻(1970-01-01 起点)の差。100ns 単位。
const UNIX_TO_FILETIME_100NS: i64 = 116_444_736_000_000_000;

/// `minutes` 分後に `main_thread_id` へ満了メッセージを送るスレッドを起動する。
///
/// `generation` をそのまま `wParam` に載せて運ぶことで、送信後に別の選択で状態が
/// 変わっていた場合（古い世代のタイマーの発火）をメインスレッド側で判別・無視できるようにする。
pub fn start(minutes: u32, generation: u64, main_thread_id: u32) {
    thread::spawn(move || {
        wait_until_due(minutes);
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

/// `minutes` 分後の絶対時刻まで待機する。待機可能タイマーの生成・設定に失敗した場合のみ、
/// サスペンドに弱い相対待機へフォールバックし、最低限の満了通知だけは維持する。
fn wait_until_due(minutes: u32) {
    if wait_with_timer(minutes).is_none() {
        thread::sleep(Duration::from_secs(u64::from(minutes) * 60));
    }
}

/// 絶対時刻を設定した待機可能タイマーで待機する。成功時 `Some(())`、Win32 呼び出し失敗時 `None`。
fn wait_with_timer(minutes: u32) -> Option<()> {
    unsafe {
        // 単発待機なので手動リセット不要。プロセス内専用のため無名タイマーで足りる。
        let timer = CreateWaitableTimerW(None, false, PCWSTR::null()).ok()?;

        let due = absolute_due_100ns(minutes);
        // fResume=FALSE: 期限到来でシステムを起こさない（既に起きているときだけシグナル化する）。
        if SetWaitableTimer(timer, &due, 0, None, None, false).is_err() {
            let _ = CloseHandle(timer);
            return None;
        }

        let _ = WaitForSingleObject(timer, INFINITE);
        let _ = CloseHandle(timer);
        Some(())
    }
}

/// 現在時刻 + `minutes` 分を FILETIME 系（100ns 単位・正値=絶対時刻）で返す。
/// 壁時計(`SystemTime`)基準のため、サスペンドで経過した実時間も期限に反映される。
fn absolute_due_100ns(minutes: u32) -> i64 {
    let unix_100ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_nanos() / 100) as i64)
        .unwrap_or(0);
    UNIX_TO_FILETIME_100NS + unix_100ns + i64::from(minutes) * 60 * 10_000_000
}
