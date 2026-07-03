#![windows_subsystem = "windows"]

mod awake;
mod config;
mod timer;

use awake::{apply, Mode};

use std::time::{Duration, Instant};

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, TranslateMessage, MSG};

/// 「6択の時間オプション」1件を構成するメニュー項目とモードの組。
struct TimeOption {
    item: CheckMenuItem,
    mode: Mode,
}

/// メインスレッドが単独で保持するアプリ状態。
///
/// `SetThreadExecutionState` はスレッドアフィンなので、この状態とその反映（`apply`）は
/// 常にメインスレッド上でのみ変更・実行される。メニュー選択はメインスレッドのメッセージループで
/// 処理され、タイマーはメインスレッドへメッセージを投げるだけなので、Mutex などのロックは不要。
struct AppState {
    mode: Mode,
    keep_display: bool,
    /// モード変更のたびに増加する世代カウンタ。選択し直された後に古いタイマーが
    /// 発火しても、この値との不一致で無視できるようにする。
    generation: u64,
    /// Timed モードの満了予定時刻（ツールチップの残り時間表示用）。それ以外は None。
    deadline: Option<Instant>,
    main_thread_id: u32,
    tray: TrayIcon,
    time_options: Vec<TimeOption>,
    item_keep_display: CheckMenuItem,
    item_quit: MenuItem,
}

fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Off => "無効",
        Mode::Indefinite => "無制限",
        Mode::Timed(15) => "15分",
        Mode::Timed(30) => "30分",
        Mode::Timed(60) => "1時間",
        Mode::Timed(120) => "2時間",
        Mode::Timed(_) => "タイマー",
    }
}

/// 残り時間を「残り H:MM:SS」（1時間未満は「残り M:SS」）で表す。
fn fmt_remaining(d: Duration) -> String {
    let secs = d.as_secs();
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 {
        format!("残り {}:{:02}:{:02}", h, m, s)
    } else {
        format!("残り {}:{:02}", m, s)
    }
}

fn update_tooltip(state: &AppState) {
    let mut text = format!("Barista — {}", mode_label(state.mode));
    if let (Mode::Timed(_), Some(deadline)) = (state.mode, state.deadline) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        text.push_str(&format!(" {}", fmt_remaining(remaining)));
    }
    if state.keep_display && state.mode != Mode::Off {
        text.push_str(" (画面オン)");
    }
    let _ = state.tray.set_tooltip(Some(text));
}

/// モードに応じてトレイアイコンを切り替える。無効時はグレー(オーディナル2)、
/// それ以外は通常のカラー(オーディナル1)。build.rs の set_icon_with_id と対応。
fn update_icon(state: &AppState) {
    let ordinal = if state.mode == Mode::Off { 2 } else { 1 };
    if let Ok(icon) =
        Icon::from_resource(ordinal, Some((32, 32))).or_else(|_| Icon::from_resource(ordinal, None))
    {
        let _ = state.tray.set_icon(Some(icon));
    }
}

/// 時間オプション（6択）のいずれかが選択されたときの処理。
fn select_mode(state: &mut AppState, mode: Mode) {
    state.mode = mode;
    // muda はクリックされたチェック項目のチェック状態を自動でトグルするだけなので、
    // ラジオボタン的な排他性はここで明示的に作り直す（クリックされた項目自身の再チェックも含む）。
    for opt in &state.time_options {
        opt.item.set_checked(opt.mode == mode);
    }

    state.generation += 1;
    if let Mode::Timed(minutes) = mode {
        timer::start(minutes, state.generation, state.main_thread_id);
    }
    state.deadline = match mode {
        Mode::Timed(m) => Some(Instant::now() + Duration::from_secs(u64::from(m) * 60)),
        _ => None,
    };

    apply(state.mode, state.keep_display);
    update_tooltip(state);
    update_icon(state);
}

/// タイマー満了メッセージの処理。世代が一致する場合のみ正当な満了として扱う。
fn handle_expiry(state: &mut AppState, generation: u64) {
    if generation != state.generation {
        return; // 選択し直された後の古いタイマー。無視する
    }

    state.mode = Mode::Off;
    state.deadline = None;
    for opt in &state.time_options {
        opt.item.set_checked(opt.mode == Mode::Off);
    }
    apply(state.mode, state.keep_display);
    update_tooltip(state);
    update_icon(state);
}

/// メニュー項目クリックの処理。「終了」がクリックされた場合のみ `true` を返す。
fn handle_menu_event(state: &mut AppState, ev: &MenuEvent) -> bool {
    if ev.id == state.item_quit.id() {
        apply(Mode::Off, state.keep_display);
        return true;
    }

    if ev.id == state.item_keep_display.id() {
        // muda が既にチェック状態をトグル済みなので、新しい状態をそのまま読み取る。
        state.keep_display = state.item_keep_display.is_checked();
        config::save_keep_display(state.keep_display);
        apply(state.mode, state.keep_display);
        update_tooltip(state);
        return false;
    }

    let matched_mode = state
        .time_options
        .iter()
        .find(|opt| ev.id == opt.item.id())
        .map(|opt| opt.mode);
    if let Some(mode) = matched_mode {
        select_mode(state, mode);
    }

    false
}

fn main() {
    let main_thread_id = unsafe { GetCurrentThreadId() };

    // 画面オン維持のみ前回終了時の設定を復元する（モードは常に無効から始める仕様）。
    let keep_display = config::load_keep_display();

    let item_off = CheckMenuItem::new("無効", true, true, None);
    let item_indefinite = CheckMenuItem::new("無制限", true, false, None);
    let item_15 = CheckMenuItem::new("15分", true, false, None);
    let item_30 = CheckMenuItem::new("30分", true, false, None);
    let item_60 = CheckMenuItem::new("1時間", true, false, None);
    let item_120 = CheckMenuItem::new("2時間", true, false, None);
    let item_keep_display = CheckMenuItem::new("画面オンを維持", true, keep_display, None);
    let item_quit = MenuItem::new("終了", true, None);

    let menu = Menu::new();
    menu.append_items(&[
        &item_off,
        &item_indefinite,
        &item_15,
        &item_30,
        &item_60,
        &item_120,
        &PredefinedMenuItem::separator(),
        &item_keep_display,
        &PredefinedMenuItem::separator(),
        &item_quit,
    ])
    .expect("failed to build tray menu");

    // build.rs の set_icon_with_id(..., "1") で埋め込んだアイコンをオーディナル 1 で読み込む。
    // 固定サイズでの読み込みに失敗する環境向けに、既定サイズへのフォールバックを用意する。
    let icon = Icon::from_resource(1, Some((32, 32)))
        .or_else(|_| Icon::from_resource(1, None))
        .expect("failed to load embedded tray icon");

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_icon(icon)
        .with_tooltip(format!("Barista — {}", mode_label(Mode::Off)))
        .with_menu_on_left_click(true)
        .build()
        .expect("failed to build tray icon");

    let time_options = vec![
        TimeOption {
            item: item_off,
            mode: Mode::Off,
        },
        TimeOption {
            item: item_indefinite,
            mode: Mode::Indefinite,
        },
        TimeOption {
            item: item_15,
            mode: Mode::Timed(15),
        },
        TimeOption {
            item: item_30,
            mode: Mode::Timed(30),
        },
        TimeOption {
            item: item_60,
            mode: Mode::Timed(60),
        },
        TimeOption {
            item: item_120,
            mode: Mode::Timed(120),
        },
    ];

    let mut state = AppState {
        mode: Mode::Off,
        keep_display,
        generation: 0,
        deadline: None,
        main_thread_id,
        tray,
        time_options,
        item_keep_display,
        item_quit,
    };

    // 起動時は何も保持しない（オプトイン）。ES_CONTINUOUS のみを渡して基準状態を明示しておく。
    apply(state.mode, state.keep_display);
    update_icon(&state);

    let mut msg = MSG::default();
    'main: loop {
        // GetMessageW はメッセージが来るまでブロックするため、アイドル時の CPU 使用率が
        // ほぼ 0% になる（Sleep によるポーリングを避ける）。
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if r.0 <= 0 {
            break; // 0 = WM_QUIT, 負値 = エラー
        }

        if msg.hwnd.is_invalid() && msg.message == timer::WM_APP_EXPIRE {
            handle_expiry(&mut state, msg.wParam.0 as u64);
            continue;
        }

        let _ = unsafe { TranslateMessage(&msg) };
        unsafe {
            DispatchMessageW(&msg);
        }

        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if handle_menu_event(&mut state, &ev) {
                break 'main;
            }
        }

        // ホバーなどのトレイ操作イベントを検知した瞬間にツールチップの残り時間を更新する
        // （中身は使わず、イベントの発生自体を「表示中かもしれない」トリガーとして使う）。
        while TrayIconEvent::receiver().try_recv().is_ok() {
            update_tooltip(&state);
        }
    }
}
