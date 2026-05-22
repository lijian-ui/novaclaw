use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState},
    Manager, WindowEvent,
};

mod cmd;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
                let _ = window.unminimize();
            }
        }))
        .setup(|app| {
            let window = app.get_webview_window("main").expect("窗口不存在");
            let window_clone = window.clone();

            window.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window_clone.hide();
                }
            });

            let app_handle = app.handle();
            
            let quit_i = MenuItemBuilder::with_id("quit", "退出").build(app_handle)?;
            let show_i = MenuItemBuilder::with_id("show", "显示主窗口").build(app_handle)?;
            let maximize_i = MenuItemBuilder::with_id("maximize", "最大化").build(app_handle)?;
            
            let tray_menu = MenuBuilder::new(app_handle)
                .item(&show_i)
                .item(&maximize_i)
                .separator()
                .item(&quit_i)
                .build()?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&tray_menu)
                .tooltip("NovaClaw")
                .on_menu_event(|app, event| {
                    match event.id().as_ref() {
                        "quit" => {
                            app.exit(0);
                        }
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                                let _ = window.unminimize();
                            }
                        }
                        "maximize" => {
                            if let Some(window) = app.get_webview_window("main") {
                                if window.is_maximized().unwrap_or(false) {
                                    let _ = window.unmaximize();
                                } else {
                                    let _ = window.maximize();
                                }
                            }
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                            let _ = window.unminimize();
                        }
                    }
                })
                .build(app)?;

            // 先初始化后端（确保 Soul/智能体等目录创建），再启动 HTTP 服务
            tauri::async_runtime::spawn(async move {
                novaclaw_backend::initialize().await;
                novaclaw_backend::start_server().await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            cmd::get_config_json,
            cmd::save_config_json,
            cmd::get_models_json,
            cmd::save_models_json,
            cmd::read_file,
            cmd::write_file,
            cmd::create_directory,
            cmd::delete_path,
            cmd::rename_path,
            cmd::copy_path,
            cmd::list_directory,
            cmd::list_directory_detailed,
            cmd::get_data_dir,
            cmd::set_data_dir,
            cmd::get_config_dir,
            cmd::get_workspace_dir,
            cmd::get_skills_dir,
            cmd::get_memories_dir,
            cmd::get_sessions_dir,
            cmd::get_system_info,
            cmd::show_window,
            cmd::hide_window,
            cmd::minimize_window,
            cmd::maximize_window,
            cmd::close_window,
            cmd::terminal_spawn,
            cmd::terminal_exec,
            cmd::terminal_write,
            cmd::terminal_kill,
            cmd::terminal_resize,
        ])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用时发生错误");
}
