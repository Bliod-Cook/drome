# drome

Tauri rewrite of Cherry Studio.

## 与 Cherry Studio 的差异（当前暂不支持）

> 对比来源：`cherry-studio/src/main/ipc.ts` 与 `src-tauri/src/commands/ipc.rs` 的 stub/no-op 通道（更新于 2026-02-12）。

| 模块 | 当前状态（drome） |
| --- | --- |
| 桌面系统集成 | `app:proxy`、`app:check-for-update`、`app:quit-and-install`、开机启动/托盘/拼写检查相关通道、`app:handle-zoom-factor`、`app:is-binary-exist`、`app:get-binary-path`、`app:install-uv-binary`、`app:install-bun-binary`、`app:install-ovms-binary`、`app:mac-is-process-trusted`、`app:mac-request-process-trust`、`app:quote-to-main`、`app:set-disable-hardware-acceleration` 目前未对齐。 |
| 云备份 | `backup:*` 里 WebDAV 与 S3 相关接口（备份/恢复/列表/连接检测/删除）目前均为 stub。 |
| 知识与记忆 | `knowledge-base:*` 与 `memory:*` 当前为 stub（创建、检索、更新、删除等未实现）。 |
| 第三方账号与生态 | `copilot:*`、`cherryin:*`、`nutstore:*`、`vertexai:*`、`ovms:*`、`anthropic:*`、`external-apps:*`、`code-tools:*`、`ocr:*`、`api-server:*`、`claudeCodePlugin:*`、`local-transfer:*`、`openclaw:*`、`analytics:*`、`provider:*` 目前未对齐。 |
| Selection / 小程序 / 通知 | `selection:*`、`minapp`、`notification:send`、`notification:on-click`、`search-window:*` 当前为 stub。 |
| Webview 与窗口扩展 | `webview:set-open-link-external`、`webview:set-spell-check-enabled`、`webview:print-to-pdf`、`webview:save-as-html`、`webview:search-hotkey`、`window:resize`、`window:maximized-changed`、`window:navigate-to-about` 当前未实现。 |
| 文件服务扩展 | `file-service:*` 与 `gemini:*`（文件上传/查询/删除）当前为 stub。 |
| 其他未对齐接口 | `shortcuts:update`、`obsidian:*`、`store-sync:subscribe`、`store-sync:unsubscribe`、`store-sync:broadcast-sync`、`agent-message:*`、`agent-tool-permission:*` 目前未实现。`export:word` IPC 在 Tauri 侧为 no-op（当前由 renderer 侧导出流程承担）。 |

## Dev

- Install: `pnpm install`
- Run: `pnpm tauri:dev`

## Build (with DevTools)

- Debug bundle (DevTools enabled): `pnpm tauri build --debug`
- Release bundle + DevTools: `pnpm tauri build --features devtools`
