//! Tauri-оболочка Dub Studio. Ничего тяжёлого сама не делает: поднимает нативный `dub-server`
//! (axum, тот же REST/SSE-контракт, что бэкенд-питон) на 127.0.0.1:<свободный порт> и открывает
//! окно на этот URL. Сервер сам раздаёт SPA (frontend/dist) и API на одном origin — фронт работает
//! с относительными путями без правок.
//!
//! Где лежат ресурсы, данные, профиль WebView2 и временные файлы — решает модуль `layout`.

mod layout;

use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use tauri::{WebviewUrl, WebviewWindowBuilder};

/// Занять свободный TCP-порт на 127.0.0.1 (ядро выдаёт порт 0 -> читаем реальный, отпускаем).
fn pick_free_port() -> std::io::Result<u16> {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
    Ok(listener.local_addr()?.port())
}

/// Дождаться, пока сервер начнёт принимать соединения (или таймаут).
fn wait_until_ready(port: u16, timeout: Duration) -> bool {
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    let start = Instant::now();
    while start.elapsed() < timeout {
        if TcpStream::connect_timeout(&addr.into(), Duration::from_millis(200)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(120));
    }
    false
}

/// Прописать ORT_DYLIB_PATH (onnxruntime 1.28) в окружение ПРОЦЕССА до старта сервера (dub-asr трогает ort
/// лениво; PATH под движок/инструменты добавит augment_path_for_tools внутри serve_blocking).
fn setup_server_env(repo_root: &PathBuf) {
    if std::env::var_os("ORT_DYLIB_PATH").is_none() {
        let rt = repo_root.join("models").join("runtime");
        for cand in [
            // GPU-сборка (cuda13) приоритетнее — суперсет CPU+CUDA; переключение backend без рестарта.
            rt.join("onnxruntime-win-x64-gpu_cuda13-1.28.2").join("lib").join("onnxruntime.dll"),
            rt.join("onnxruntime-win-x64-1.28.2").join("lib").join("onnxruntime.dll"),
            layout::executable_directory().join("onnxruntime.dll"),
            rt.join("onnxruntime-1.28.dll"),
            rt.join("onnxruntime.dll"),
        ] {
            if cand.is_file() {
                std::env::set_var("ORT_DYLIB_PATH", cand);
                break;
            }
        }
    }
}

/// Проверка обновления на GitHub-релизе и (по согласию юзера) установка. Драйвится из Rust: фронт
/// грузится с внешнего http-URL встроенного сервера, где Tauri JS-IPC ненадёжен, а Rust-апдейтер
/// работает независимо от webview. Тихо выходит при отсутствии апдейта/сети. На лету ставится только
/// копия из NSIS-установщика; портатив (нельзя перезаписать запущенный ~489-МБ каталог), MSI
/// (msiexec не принимает /D=, а Program Files без повышения прав недоступен) и сборка без типа
/// бандла получают предложение открыть страницу релиза.
const RELEASES_URL: &str = "https://github.com/timoncool/dub-studio/releases/latest";
fn spawn_update_check(app: tauri::AppHandle, portable: bool) {
    use tauri::utils::config::BundleType;
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    use tauri_plugin_updater::UpdaterExt;
    let install_in_place =
        !portable && tauri::utils::platform::bundle_type() == Some(BundleType::Nsis);
    tauri::async_runtime::spawn(async move {
        let cleanup = app.clone();
        let mut builder = app
            .updater_builder()
            .on_before_exit(move || cleanup.cleanup_before_exit());
        if install_in_place {
            // Без /D= установщик, запущенный из студии, ставит копию в папку по умолчанию, а не в
            // текущую; NSIS требует его последним аргументом и без кавычек.
            builder = builder.installer_arg(format!("/D={}", layout::executable_directory().display()));
        }
        let updater = match builder.build() {
            Ok(u) => u,
            Err(_) => return,
        };
        let update = match updater.check().await {
            Ok(Some(u)) => u,
            _ => return, // нет апдейта или ошибка сети -> тихо
        };
        let ver = update.version.clone();
        if !install_in_place {
            let open = app
                .dialog()
                .message(format!(
                    "Доступна новая версия {ver}. Открыть страницу загрузки?"
                ))
                .title("Обновление Dub Studio")
                .kind(MessageDialogKind::Info)
                .buttons(MessageDialogButtons::OkCancelCustom(
                    "Открыть".into(),
                    "Позже".into(),
                ))
                .blocking_show();
            if open {
                use tauri_plugin_opener::OpenerExt;
                let _ = app.opener().open_url(RELEASES_URL, None::<&str>);
            }
            return;
        }
        let yes = app
            .dialog()
            .message(format!(
                "Доступна новая версия {ver}. Обновить сейчас? Приложение перезапустится."
            ))
            .title("Обновление Dub Studio")
            .kind(MessageDialogKind::Info)
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Обновить".into(),
                "Позже".into(),
            ))
            .blocking_show();
        if !yes {
            return;
        }
        match update.download_and_install(|_, _| {}, || {}).await {
            Ok(_) => {
                app.restart(); // Windows: инсталлятор сам закроет приложение; на прочих ОС перезапустим
            }
            Err(e) => {
                app.dialog()
                    .message(format!("Не удалось обновить: {e}"))
                    .title("Обновление Dub Studio")
                    .kind(MessageDialogKind::Error)
                    .blocking_show();
            }
        }
    });
}

/// Спрятать ОКНО консоли. Exe — console-subsystem (чтобы дети ffmpeg/llama/roformer наследовали ОДНУ
/// консоль и НЕ плодили своих окон на каждый спавн). Сама консоль остаётся выделенной (дети её наследуют),
/// прячем только её ОКНО -> исчезает чёрное окно и его пустая запись в ALT+TAB/панели задач; в переключателе
/// остаётся лишь GUI-окно (с иконкой, см. .icon() ниже). Дети по-прежнему не открывают окон.
#[cfg(windows)]
fn hide_console_window() {
    extern "system" {
        fn GetConsoleWindow() -> isize;
        fn ShowWindow(hwnd: isize, n_cmd_show: i32) -> i32;
        fn GetConsoleProcessList(lpdw_process_list: *mut u32, dw_process_count: u32) -> u32;
    }
    unsafe {
        // Прячем ТОЛЬКО собственную консоль. Если exe запущен ИЗ существующего терминала, наш процесс
        // делит его консоль (к ней привязано >1 процесса) — это ЧУЖОЕ окно терминала пользователя, трогать
        // нельзя. При двойном клике из Проводника загрузчик создаёт нам отдельную консоль (count==1).
        let mut pids = [0u32; 4];
        let n = GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32);
        if n != 1 {
            return;
        }
        let hwnd = GetConsoleWindow();
        if hwnd != 0 {
            ShowWindow(hwnd, 0); // SW_HIDE
        }
    }
}

pub fn run() {
    #[cfg(windows)]
    hide_console_window();
    let placed = match layout::resolve().and_then(|l| layout::apply_environment(&l).map(|_| l)) {
        Ok(l) => l,
        Err(e) => layout::fatal(&e),
    };
    let repo_root = placed.server_root;
    let port = pick_free_port().unwrap_or(8765);
    setup_server_env(&repo_root);

    // axum-бэкенд поднимается В ЭТОМ ЖЕ процессе на фоновом потоке — ОДИН exe, без dub-server.exe-сайдкара.
    // Поток-демон: живёт до выхода процесса, отдельно убивать не нужно (нет дочернего процесса).
    let root = repo_root.clone();
    std::thread::spawn(move || {
        if let Err(e) = dub_server::serve_blocking(&root, port) {
            eprintln!("встроенный dub-server упал: {e}");
        }
    });

    // Ждём готовности сервера, чтобы окно не открылось на пустоту.
    if !wait_until_ready(port, Duration::from_secs(30)) {
        eprintln!("встроенный dub-server не поднялся на 127.0.0.1:{port} за 30с");
    }

    let url = format!("http://127.0.0.1:{port}/");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            // Иконка бандла для GUI-окна: без явной установки окно оставалось пустым в ALT+TAB/панели задач
            // (иконка висела на консольном окне). Ставим её на само GUI-окно.
            let icon = app.default_window_icon().cloned();
            let win = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External(url.parse().expect("валидный URL")),
            )
            .title(format!("Dub Studio {}", app.package_info().version))
            .inner_size(1600.0, 1040.0)
            .min_inner_size(1280.0, 800.0)
            .resizable(true)
            // Tauri v2 по умолчанию перехватывает OS-drop файлов -> HTML5 onDrop в дропзоне НЕ срабатывает
            // (юзеры жаловались «перетаскивание не работает»). Отключаем перехват -> webview сам ловит drop.
            .disable_drag_drop_handler()
            .build()?;
            // Иконка окна (ALT+TAB/таскбар) — ПОСЛЕ создания: не паникуем, если не выйдет, окно рабочее.
            if let Some(ic) = icon {
                let _ = win.set_icon(ic);
            }
            // авто-обновление: проверка на GitHub-релизе в фоне, установка по согласию (см. spawn_update_check)
            spawn_update_check(app.handle().clone(), layout::is_portable());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("ошибка запуска Tauri");
}
