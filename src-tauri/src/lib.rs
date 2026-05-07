use tauri::{
    menu::{Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    tray::TrayIconBuilder,
    Manager, WindowEvent,
};

mod cmd;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "novaclaw_desktop=info".into()),
        )
        .init();

    tracing::info!("NovaClaw Desktop 启动中...");

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
            tracing::info!("Tauri 应用初始化中...");

            let window = app.get_webview_window("main").expect("窗口不存在");
            let window_clone = window.clone();

            let menu = build_app_menu(app)?;

            if let Err(e) = app.set_menu(menu) {
                tracing::error!("设置菜单失败: {}", e);
            }

            let window_clone2 = window_clone.clone();
            window.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window_clone2.hide();
                }
            });

            let _tray = TrayIconBuilder::new()
                .tooltip("NovaClaw - 点击显示窗口")
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click { button, .. } = event {
                        if button == tauri::tray::MouseButton::Left {
                            if let Some(window) = tray.app_handle().get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                                let _ = window.unminimize();
                            }
                        }
                    }
                })
                .build(app)?;

            tauri::async_runtime::spawn(async move {
                tracing::info!("启动 NovaClaw 后端服务...");
                novaclaw_backend::start_server().await;
            });

            tracing::info!("NovaClaw Desktop 初始化完成");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            cmd::get_config_json,
            cmd::save_config_json,
            cmd::get_models_json,
            cmd::save_models_json,
            cmd::read_file,
            cmd::write_file,
            cmd::list_directory,
            cmd::get_data_dir,
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
        ])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用时发生错误");
}

fn build_app_menu(app: &tauri::App) -> Result<Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    let app_handle = app.handle();

    let about_item = MenuItemBuilder::with_id("about", "关于 NovaClaw").build(app_handle)?;

    let edit_menu = SubmenuBuilder::new(app_handle, "编辑")
        .item(&MenuItemBuilder::with_id("undo", "撤销").accelerator("CmdOrCtrl+Z").build(app_handle)?)
        .item(&MenuItemBuilder::with_id("redo", "重做").accelerator("CmdOrCtrl+Shift+Z").build(app_handle)?)
        .separator()
        .item(&MenuItemBuilder::with_id("cut", "剪切").accelerator("CmdOrCtrl+X").build(app_handle)?)
        .item(&MenuItemBuilder::with_id("copy", "复制").accelerator("CmdOrCtrl+C").build(app_handle)?)
        .item(&MenuItemBuilder::with_id("paste", "粘贴").accelerator("CmdOrCtrl+V").build(app_handle)?)
        .item(&MenuItemBuilder::with_id("select_all", "全选").accelerator("CmdOrCtrl+A").build(app_handle)?)
        .build()?;

    let novaclaw_submenu = SubmenuBuilder::new(app_handle, "NovaClaw")
        .item(&edit_menu)
        .build()?;

    let view_menu = SubmenuBuilder::new(app_handle, "视图")
        .item(&MenuItemBuilder::with_id("reload", "刷新").accelerator("CmdOrCtrl+R").build(app_handle)?)
        .item(&MenuItemBuilder::with_id("toggle_fullscreen", "切换全屏").accelerator("F11").build(app_handle)?)
        .separator()
        .item(&MenuItemBuilder::with_id("zoom_in", "放大").accelerator("CmdOrCtrl+=").build(app_handle)?)
        .item(&MenuItemBuilder::with_id("zoom_out", "缩小").accelerator("CmdOrCtrl+-").build(app_handle)?)
        .item(&MenuItemBuilder::with_id("reset_zoom", "重置缩放").accelerator("CmdOrCtrl+0").build(app_handle)?)
        .build()?;

    let window_menu = SubmenuBuilder::new(app_handle, "窗口")
        .item(&MenuItemBuilder::with_id("minimize", "最小化").accelerator("CmdOrCtrl+M").build(app_handle)?)
        .item(&MenuItemBuilder::with_id("maximize", "最大化/还原").build(app_handle)?)
        .separator()
        .item(&MenuItemBuilder::with_id("close", "关闭到托盘").build(app_handle)?)
        .build()?;

    let help_menu = SubmenuBuilder::new(app_handle, "帮助")
        .item(&MenuItemBuilder::with_id("docs", "开发文档").build(app_handle)?)
        .item(&MenuItemBuilder::with_id("report_issue", "报告问题").build(app_handle)?)
        .separator()
        .item(&MenuItemBuilder::with_id("check_updates", "检查更新").build(app_handle)?)
        .build()?;

    let menu = MenuBuilder::new(app_handle)
        .item(&novaclaw_submenu)
        .item(&view_menu)
        .item(&window_menu)
        .item(&help_menu)
        .separator()
        .item(&about_item)
        .build()?;

    Ok(menu)
}
