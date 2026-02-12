use serde_json::Value;
use tauri::{AppHandle, Emitter, State, WebviewWindow};

use crate::commands;
use crate::error::{DromeError, Result};
use crate::state::AppState;

fn arg<T: serde::de::DeserializeOwned>(args: &[Value], idx: usize) -> Result<T> {
  let value = args
    .get(idx)
    .ok_or_else(|| DromeError::Message(format!("Missing arg at index {idx}")))?
    .clone();
  serde_json::from_value(value).map_err(|e| DromeError::Message(format!("Invalid arg at index {idx}: {e}")))
}

fn opt_arg<T: serde::de::DeserializeOwned>(args: &[Value], idx: usize) -> Result<Option<T>> {
  let Some(value) = args.get(idx) else { return Ok(None) };
  if value.is_null() {
    return Ok(None);
  }
  Ok(Some(
    serde_json::from_value(value.clone()).map_err(|e| DromeError::Message(format!("Invalid arg at index {idx}: {e}")))?,
  ))
}

fn to_value<T: serde::Serialize>(value: T) -> Result<Value> {
  Ok(serde_json::to_value(value)?)
}

#[tauri::command]
pub async fn ipc_invoke(
  app: AppHandle,
  window: WebviewWindow,
  state: State<'_, AppState>,
  channel: String,
  args: Vec<Value>,
) -> std::result::Result<Value, String> {
  (|| -> Result<Value> {
    match channel.as_str() {
      // App
      "app:info" => to_value(commands::app::app_info(&app, &state)?),
      "app:get-cache-size" => to_value(0u64),
      "app:clear-cache" => Ok(Value::Null),
      "app:reload" => to_value(commands::app::app_reload(&window)?),
      "app:quit" => to_value(commands::app::app_quit(&app)?),
      "open:website" => to_value(commands::app::open_website(&app, arg::<String>(&args, 0)?)?),
      "open:path" => to_value(commands::app::open_path(&app, arg::<String>(&args, 0)?)?),
      "app:log-to-main" => to_value(commands::app::app_log_to_main(args)?),
      "app:get-disk-info" => to_value(commands::app::app_get_disk_info(arg::<String>(&args, 0)?)?),
      "app:get-data-path-from-args" => to_value(commands::app::app_get_data_path_from_args()?),
      "app:select" => to_value(commands::app::app_select(&app, &state, args.get(0).cloned())?),
      "app:is-not-empty-dir" => to_value(commands::app::app_is_not_empty_dir(arg::<String>(&args, 0)?)?),
      "app:copy" => to_value(commands::app::app_copy(
        &window,
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
        args.get(2)
          .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
          .unwrap_or_default(),
      )?),
      "app:set-stop-quit-app" => to_value(commands::app::app_set_stop_quit_app(
        &state,
        arg::<bool>(&args, 0)?,
        arg::<String>(&args, 1)?,
      )?),
      "app:flush-app-data" => to_value(commands::app::app_flush_app_data(&app)?),
      "app:set-app-data-path" => to_value(commands::app::app_set_app_data_path(&state, arg::<String>(&args, 0)?)?),
      "app:relaunch-app" => to_value(commands::app::app_relaunch_app(&app, args.get(0).cloned())?),
      "app:set-full-screen" => to_value(commands::app::app_set_full_screen(&window, arg::<bool>(&args, 0)?)?),
      "app:is-full-screen" => to_value(commands::app::app_is_full_screen(&window)?),
      // App stubs (Electron-only)
      "app:proxy" => Ok(Value::Null),
      "app:check-for-update" => Ok(Value::Null),
      "app:quit-and-install" => Ok(Value::Null),
      "app:set-launch-on-boot" => Ok(Value::Null),
      "app:set-language" => Ok(Value::Null),
      "app:set-enable-spell-check" => Ok(Value::Null),
      "app:set-spell-check-languages" => Ok(Value::Null),
      "app:set-launch-to-tray" => Ok(Value::Null),
      "app:set-tray" => Ok(Value::Null),
      "app:set-tray-on-close" => Ok(Value::Null),
      "app:set-theme" => Ok(Value::Null),
      "app:set-auto-update" => Ok(Value::Null),
      "app:set-test-plan" => Ok(Value::Null),
      "app:set-test-channel" => Ok(Value::Null),
      "app:handle-zoom-factor" => Ok(Value::Null),
      "app:is-binary-exist" => to_value(false),
      "app:get-binary-path" => Ok(Value::Null),
      "app:install-uv-binary" => to_value(serde_json::json!({ "success": false })),
      "app:install-bun-binary" => to_value(serde_json::json!({ "success": false })),
      "app:install-ovms-binary" => to_value(serde_json::json!({ "success": false })),
      "app:mac-is-process-trusted" => to_value(false),
      "app:mac-request-process-trust" => to_value(false),
      "app:quote-to-main" => Ok(Value::Null),
      "app:set-disable-hardware-acceleration" => Ok(Value::Null),
      "app:set-use-system-title-bar" => Ok(Value::Null),
      "app:get-system-fonts" => to_value(Vec::<String>::new()),
      "app:crash-render-process" => Ok(Value::Null),
      "redux-store-ready" => Ok(Value::Null),

      // Store Sync
      "store-sync:subscribe" => Ok(Value::Null),
      "store-sync:unsubscribe" => Ok(Value::Null),
      "store-sync:on-update" => {
        commands::store_sync::store_sync_on_update(&app, &window, arg::<Value>(&args, 0)?)?;
        Ok(Value::Null)
      }

      // Window controls
      "window:minimize" => to_value(commands::window::window_minimize(&window)?),
      "window:maximize" => to_value(commands::window::window_maximize(&window)?),
      "window:unmaximize" => to_value(commands::window::window_unmaximize(&window)?),
      "window:close" => to_value(commands::window::window_close(&window)?),
      "window:is-maximized" => to_value(commands::window::window_is_maximized(&window)?),
      "window:set-minimum-size" => to_value(commands::window::window_set_minimum_size(
        &window,
        arg::<f64>(&args, 0)?,
        arg::<f64>(&args, 1)?,
      )?),
      "window:reset-minimum-size" => to_value(commands::window::window_reset_minimum_size(&window)?),
      "window:get-size" => to_value(commands::window::window_get_size(&window)?),

      // Config
      "config:get" => to_value(commands::config::config_get(&state, arg::<String>(&args, 0)?)?),
      "config:set" => to_value(commands::config::config_set(
        &state,
        &app,
        arg::<String>(&args, 0)?,
        args.get(1).cloned().unwrap_or(Value::Null),
        args.get(2).and_then(|v| v.as_bool()).unwrap_or(false),
      )?),

      // File dialogs and fs
      "file:open" => to_value(commands::file::file_open(&app, &state, args.get(0).cloned())?),
      "file:openPath" => to_value(commands::file::file_open_path(&app, arg::<String>(&args, 0)?)?),
      "file:select" => to_value(commands::file::file_select(&app, &state, args.get(0).cloned())?),
      "file:selectFolder" => to_value(commands::file::file_select_folder(&app, &state, args.get(0).cloned())?),
      "file:save" => to_value(commands::file::file_save(&app, &state, args)?),
      "file:clear" => to_value(commands::file::file_clear(&state)?),
      "file:read" => to_value(commands::file::file_read(
        &state,
        arg::<String>(&args, 0)?,
        args.get(1).and_then(|v| v.as_bool()).unwrap_or(false),
      )?),
      "file:readExternal" => to_value(commands::file::file_read_external(
        &state,
        arg::<String>(&args, 0)?,
        args.get(1).and_then(|v| v.as_bool()).unwrap_or(false),
      )?),
      "file:write" => to_value(commands::file::file_write(
        &state,
        arg::<String>(&args, 0)?,
        arg::<Value>(&args, 1)?,
      )?),
      "file:writeWithId" => to_value(commands::file::file_write_with_id(
        &state,
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
      )?),
      "file:mkdir" => to_value(commands::file::file_mkdir(&state, arg::<String>(&args, 0)?)?),
      "file:upload" => to_value(commands::file::file_upload(&state, arg::<commands::file::StoredFileMetadata>(&args, 0)?)?),
      "file:get" => to_value(commands::file::file_get(&state, arg::<String>(&args, 0)?)?),
      "file:createTempFile" => to_value(commands::file::file_create_temp_file(&state, arg::<String>(&args, 0)?)?),
      "file:delete" => to_value(commands::file::file_delete(&state, arg::<String>(&args, 0)?)?),
      "file:deleteDir" => to_value(commands::file::file_delete_dir(&state, arg::<String>(&args, 0)?)?),
      "file:deleteExternalFile" => to_value(commands::file::file_delete_external_file(&state, arg::<String>(&args, 0)?)?),
      "file:deleteExternalDir" => to_value(commands::file::file_delete_external_dir(&state, arg::<String>(&args, 0)?)?),
      "file:move" => to_value(commands::file::file_move(
        &state,
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
      )?),
      "file:moveDir" => to_value(commands::file::file_move_dir(
        &state,
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
      )?),
      "file:rename" => to_value(commands::file::file_rename(
        &state,
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
      )?),
      "file:renameDir" => to_value(commands::file::file_rename_dir(
        &state,
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
      )?),
      "file:saveImage" => to_value(commands::file::file_save_image(
        &app,
        &state,
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
      )?),
      "file:base64Image" => to_value(commands::file::file_base64_image(&state, arg::<String>(&args, 0)?)?),
      "file:saveBase64Image" => to_value(commands::file::file_save_base64_image(&state, arg::<String>(&args, 0)?)?),
      "file:savePastedImage" => to_value(commands::file::file_save_pasted_image(
        &state,
        arg::<Vec<u8>>(&args, 0)?,
        opt_arg::<String>(&args, 1)?,
      )?),
      "file:download" => to_value(commands::file::file_download(
        &state,
        arg::<String>(&args, 0)?,
        opt_arg::<bool>(&args, 1)?,
      )?),
      "file:copy" => to_value(commands::file::file_copy(
        &state,
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
      )?),
      "file:base64File" => to_value(commands::file::file_base64_file(&state, arg::<String>(&args, 0)?)?),
      "file:getPdfInfo" => to_value(commands::file::file_pdf_page_count(&state, arg::<String>(&args, 0)?)?),
      "file:binaryImage" => to_value(commands::file::file_binary_image(&state, arg::<String>(&args, 0)?)?),
      "file:openWithRelativePath" => to_value(commands::file::file_open_with_relative_path(
        &state,
        arg::<commands::file::StoredFileMetadata>(&args, 0)?,
      )?),
      "file:isTextFile" => to_value(commands::file::file_is_text_file(&state, arg::<String>(&args, 0)?)?),
      "file:isDirectory" => to_value(commands::file::file_is_directory(&state, arg::<String>(&args, 0)?)?),
      "file:listDirectory" => to_value(commands::file::file_list_directory(
        &state,
        arg::<String>(&args, 0)?,
        args.get(1).cloned(),
      )?),
      "file:getDirectoryStructure" => to_value(commands::file::file_get_directory_structure(&state, arg::<String>(&args, 0)?)?),
      "file:checkFileName" => to_value(commands::file::file_check_file_name(
        &state,
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
        arg::<bool>(&args, 2)?,
      )?),
      "file:validateNotesDirectory" => to_value(commands::file::file_validate_notes_directory(&state, arg::<String>(&args, 0)?)?),
      "file:startWatcher" => to_value(commands::file::file_start_watcher(
        &app,
        &window,
        &state,
        arg::<String>(&args, 0)?,
        args.get(1).cloned(),
      )?),
      "file:stopWatcher" => to_value(commands::file::file_stop_watcher()?),
      "file:pauseWatcher" => to_value(commands::file::file_pause_watcher()?),
      "file:resumeWatcher" => to_value(commands::file::file_resume_watcher()?),
      "file:batchUploadMarkdown" => to_value(commands::file::file_batch_upload_markdown(
        &state,
        arg::<Vec<String>>(&args, 0)?,
        arg::<String>(&args, 1)?,
      )?),
      "file:showInFolder" => to_value(commands::file::file_show_in_folder(&app, arg::<String>(&args, 0)?)?),

      // FS (read-only convenience)
      "fs:read" => to_value(commands::fs::fs_read(
        &app,
        &state,
        arg::<String>(&args, 0)?,
        args.get(1).and_then(|v| v.as_str()).map(|s| s.to_string()),
      )?),
      "fs:readText" => to_value(commands::fs::fs_read_text(&app, &state, arg::<String>(&args, 0)?)?),
      "app:resolve-path" => to_value(commands::file::resolve_path(arg::<String>(&args, 0)?)?),
      "app:is-path-inside" => to_value(commands::file::is_path_inside(
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
      )?),
      "app:has-write-permission" => to_value(commands::file::has_write_permission(arg::<String>(&args, 0)?)?),

      // Zip (gzip)
      "zip:compress" => to_value(commands::zip::zip_compress(arg::<String>(&args, 0)?)?),
      "zip:decompress" => to_value(commands::zip::zip_decompress(arg::<Vec<u8>>(&args, 0)?)?),

      // Backup (zip)
      "backup:backup" => to_value(commands::backup::backup_backup(&app, &window, &state, args)?),
      "backup:restore" => to_value(commands::backup::backup_restore(
        &app,
        &window,
        &state,
        arg::<String>(&args, 0)?,
      )?),
      "backup:backupToLocalDir" => to_value(commands::backup::backup_to_local_dir(
        &app,
        &window,
        &state,
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
        arg::<Value>(&args, 2)?,
      )?),
      "backup:restoreFromLocalBackup" => to_value(commands::backup::restore_from_local_backup(
        &app,
        &window,
        &state,
        arg::<String>(&args, 0)?,
        opt_arg::<String>(&args, 1)?,
      )?),
      "backup:listLocalBackupFiles" => to_value(commands::backup::list_local_backup_files(opt_arg::<String>(&args, 0)?)?),
      "backup:deleteLocalBackupFile" => to_value(commands::backup::delete_local_backup_file(
        arg::<String>(&args, 0)?,
        opt_arg::<String>(&args, 1)?,
      )?),
      "backup:createLanTransferBackup" => to_value(commands::backup::create_lan_transfer_backup(
        &app,
        &window,
        &state,
        arg::<String>(&args, 0)?,
      )?),
      "backup:deleteTempBackup" => to_value(commands::backup::delete_temp_backup(&state, arg::<String>(&args, 0)?)?),
      // Backup stubs (not supported yet)
      "backup:backupToWebdav" => to_value(false),
      "backup:restoreFromWebdav" => to_value(String::new()),
      "backup:listWebdavFiles" => to_value(Vec::<Value>::new()),
      "backup:checkConnection" => to_value(false),
      "backup:createDirectory" => to_value(false),
      "backup:deleteWebdavFile" => to_value(false),
      "backup:backupToS3" => to_value(false),
      "backup:restoreFromS3" => to_value(String::new()),
      "backup:listS3Files" => to_value(Vec::<Value>::new()),
      "backup:deleteS3File" => to_value(false),
      "backup:checkS3Connection" => to_value(false),

      // System
      "system:getDeviceType" => to_value(commands::system::system_get_device_type()?),
      "system:getHostname" => to_value(commands::system::system_get_hostname()?),
      "system:getCpuName" => to_value(commands::system::system_get_cpu_name()?),
      "system:checkGitBash" => to_value(commands::system::system_check_git_bash(&state)?),
      "system:getGitBashPath" => to_value(commands::system::system_get_git_bash_path(&state)?),
      "system:getGitBashPathInfo" => to_value(commands::system::system_get_git_bash_path_info(&state)?),
      "system:setGitBashPath" => to_value(commands::system::system_set_git_bash_path(&state, opt_arg::<String>(&args, 0)?)?),
      "system:toggleDevTools" => to_value(commands::system::system_toggle_devtools(&window)?),

      // MiniWindow
      "miniwindow:show" => to_value(commands::mini_window::mini_window_show(&app)?),
      "miniwindow:hide" => to_value(commands::mini_window::mini_window_hide(&app)?),
      "miniwindow:close" => to_value(commands::mini_window::mini_window_close(&app)?),
      "miniwindow:toggle" => to_value(commands::mini_window::mini_window_toggle(&app)?),
      "miniwindow:set-pin" => to_value(commands::mini_window::mini_window_set_pin(&app, arg::<bool>(&args, 0)?)?),

      // AES
      "aes:encrypt" => to_value(commands::aes::aes_encrypt(
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
        arg::<String>(&args, 2)?,
      )?),
      "aes:decrypt" => to_value(commands::aes::aes_decrypt(
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
        arg::<String>(&args, 2)?,
      )?),

      // Trace
      "trace:saveData" => to_value(commands::trace::trace_save_data(&state, arg::<String>(&args, 0)?)?),
      "trace:getData" => to_value(commands::trace::trace_get_data(
        &state,
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
        opt_arg::<String>(&args, 2)?,
      )?),
      "trace:saveEntity" => to_value(commands::trace::trace_save_entity(&state, arg::<Value>(&args, 0)?)?),
      "trace:getEntity" => to_value(commands::trace::trace_get_entity(&state, arg::<String>(&args, 0)?)?),
      "trace:bindTopic" => to_value(commands::trace::trace_bind_topic(
        &state,
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
      )?),
      "trace:tokenUsage" => to_value(commands::trace::trace_token_usage(
        &state,
        arg::<String>(&args, 0)?,
        arg::<Value>(&args, 1)?,
      )?),
      "trace:cleanHistory" => to_value(commands::trace::trace_clean_history(
        &state,
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
        opt_arg::<String>(&args, 2)?,
      )?),
      "trace:cleanTopic" => to_value(commands::trace::trace_clean_topic(
        &state,
        arg::<String>(&args, 0)?,
        opt_arg::<String>(&args, 1)?,
      )?),
      "trace:openWindow" => to_value(commands::trace::trace_open_window(
        &app,
        &window,
        &state,
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
        opt_arg::<bool>(&args, 2)?,
        opt_arg::<String>(&args, 3)?,
      )?),
      "trace:setTitle" => to_value(commands::trace::trace_set_title(&window, arg::<String>(&args, 0)?)?),
      "trace:addEndMessage" => to_value(commands::trace::trace_add_end_message(
        &state,
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
        arg::<String>(&args, 2)?,
      )?),
      "trace:cleanLocalData" => to_value(commands::trace::trace_clean_local_data(&state)?),
      "trace:addStreamMessage" => to_value(commands::trace::trace_add_stream_message(
        &state,
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
        arg::<String>(&args, 2)?,
        arg::<Value>(&args, 3)?,
      )?),

      // KnowledgeBase (stub)
      "knowledge-base:create" => Ok(Value::Null),
      "knowledge-base:reset" => Ok(Value::Null),
      "knowledge-base:delete" => Ok(Value::Null),
      "knowledge-base:add" => Ok(Value::Null),
      "knowledge-base:remove" => Ok(Value::Null),
      "knowledge-base:search" => to_value(Vec::<Value>::new()),
      "knowledge-base:rerank" => to_value(Vec::<Value>::new()),

      // Memory (stub)
      "memory:add" => Ok(Value::Null),
      "memory:search" => Ok(Value::Null),
      "memory:list" => Ok(Value::Null),
      "memory:delete" => Ok(Value::Null),
      "memory:update" => Ok(Value::Null),
      "memory:get" => Ok(Value::Null),
      "memory:set-config" => Ok(Value::Null),
      "memory:delete-user" => Ok(Value::Null),
      "memory:delete-all-memories-for-user" => Ok(Value::Null),
      "memory:get-users-list" => to_value(Vec::<Value>::new()),
      "memory:migrate-memory-db" => Ok(Value::Null),

      // Shortcuts (stub)
      "shortcuts:update" => Ok(Value::Null),

      // Export (unused in Tauri; renderer generates docx)
      "export:word" => Ok(Value::Null),

      // Obsidian (stub)
      "obsidian:get-vaults" => to_value(Vec::<Value>::new()),
      "obsidian:get-files" => to_value(Vec::<Value>::new()),

      // Search Window (stub)
      "search-window:open" => Ok(Value::Null),
      "search-window:close" => Ok(Value::Null),
      "search-window:open-url" => to_value(String::new()),

      // Webview (stub)
      "webview:set-open-link-external" => Ok(Value::Null),
      "webview:set-spell-check-enabled" => Ok(Value::Null),
      "webview:print-to-pdf" => Ok(Value::Null),
      "webview:save-as-html" => Ok(Value::Null),
      "webview:search-hotkey" => Ok(Value::Null),

      // File service (stub)
      "file-service:upload" => Ok(Value::Null),
      "file-service:list" => Ok(Value::Null),
      "file-service:delete" => Ok(Value::Null),
      "file-service:retrieve" => Ok(Value::Null),

      // HTTP (native fetch proxy for Tauri to bypass CORS)
      "http:fetch" => to_value(commands::http::http_fetch(arg::<commands::http::HttpFetchRequest>(&args, 0)?)?),

      // CherryAI
      "cherryai:get-signature" => to_value(commands::cherryai::cherryai_get_signature(
        arg::<commands::cherryai::CherryAiSignatureParams>(&args, 0)?,
      )?),

      // MCP
      "mcp:get-install-info" => to_value(commands::mcp::mcp_get_install_info()?),

      // Misc stubs for optional integrations
      c if c.starts_with("mcp:") => Ok(Value::Null),
      c if c.starts_with("python:") => Ok(Value::Null),
      c if c.starts_with("copilot:") => Ok(Value::Null),
      c if c.starts_with("cherryin:") => Ok(Value::Null),
      c if c.starts_with("nutstore:") => Ok(Value::Null),
      c if c.starts_with("vertexai:") => Ok(Value::Null),
      c if c.starts_with("ovms:") => Ok(Value::Null),
      c if c.starts_with("anthropic:") => Ok(Value::Null),
      c if c.starts_with("external-apps:") => Ok(Value::Null),
      c if c.starts_with("code-tools:") => Ok(Value::Null),
      c if c.starts_with("ocr:") => Ok(Value::Null),
      c if c.starts_with("cherryai:") => Ok(Value::Null),
      c if c.starts_with("api-server:") => Ok(Value::Null),
      c if c.starts_with("claudeCodePlugin:") => Ok(Value::Null),
      c if c.starts_with("local-transfer:") => Ok(Value::Null),
      c if c.starts_with("openclaw:") => Ok(Value::Null),
      c if c.starts_with("analytics:") => Ok(Value::Null),
      c if c.starts_with("selection:") => Ok(Value::Null),
      c if c.starts_with("provider:") => Ok(Value::Null),
      "notification:send" => Ok(Value::Null),
      "notification:on-click" => Ok(Value::Null),
      "minapp" => Ok(Value::Null),
      "store-sync:broadcast-sync" => Ok(Value::Null),
      "agent-message:persist-exchange" => Ok(Value::Null),
      "agent-message:get-history" => Ok(Value::Null),
      "agent-tool-permission:request" => Ok(Value::Null),
      "agent-tool-permission:response" => Ok(Value::Null),
      "agent-tool-permission:result" => Ok(Value::Null),
      "gemini:upload-file" => Ok(Value::Null),
      "gemini:base64-file" => Ok(Value::Null),
      "gemini:retrieve-file" => Ok(Value::Null),
      "gemini:list-files" => Ok(Value::Null),
      "gemini:delete-file" => Ok(Value::Null),
      "window:resize" => Ok(Value::Null),
      "window:maximized-changed" => Ok(Value::Null),
      "window:navigate-to-about" => Ok(Value::Null),

      // Migration
      "drome:migration-detect" => to_value(commands::migration::migration_detect(&state)?),
      "drome:migration-copy-data" => to_value(commands::migration::migration_copy_data_dir(
        &app,
        &window,
        &state,
        arg::<String>(&args, 0)?,
      )?),

      // Unknown channel: return a neutral value (do not use "Not implemented" wording).
      _ => Ok(Value::Null),
    }
  })()
  .map_err(String::from)
}

#[tauri::command]
pub async fn ipc_emit(window: WebviewWindow, channel: String, payload: Value) -> std::result::Result<(), String> {
  window.emit(&channel, payload).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ipc_emit_global(app: AppHandle, channel: String, payload: Value) -> std::result::Result<(), String> {
  app.emit(&channel, payload).map_err(|e| e.to_string())
}
