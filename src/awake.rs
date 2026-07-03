//! `SetThreadExecutionState` はスレッドアフィンな Win32 API で、呼び出したスレッドの
//! 実行状態にのみ反映され、そのスレッドが終了すると自動的に解除される。
//! そのため呼び出しは必ずメインスレッドから行うこと（他スレッドから呼んでも無効化される）。

use windows::Win32::System::Power::{
    SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
};

/// スリープ抑止のモード。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// 抑止なし。通常どおりスリープ・画面オフを許可する。
    Off,
    /// 無期限に抑止する。
    Indefinite,
    /// 指定分数だけ抑止する。実際の解除はタイマー経由で `Mode::Off` への遷移として行われる。
    Timed(u32),
}

/// 現在のモードと画面維持設定を Win32 の実行状態フラグへ反映する。
///
/// `Off` のときは `ES_CONTINUOUS` のみを渡し、既存の抑止をすべて解除して通常のスリープを許可する
/// （このとき `keep_display` の値は無視される）。それ以外のモードでは `ES_SYSTEM_REQUIRED` を常に立て、
/// `keep_display` が真のときのみ `ES_DISPLAY_REQUIRED` を追加する。
pub fn apply(mode: Mode, keep_display: bool) {
    let flags = match mode {
        Mode::Off => ES_CONTINUOUS,
        Mode::Indefinite | Mode::Timed(_) => {
            let mut flags = ES_CONTINUOUS | ES_SYSTEM_REQUIRED;
            if keep_display {
                flags |= ES_DISPLAY_REQUIRED;
            }
            flags
        }
    };
    unsafe {
        SetThreadExecutionState(flags);
    }
}
