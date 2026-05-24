# Tauri 桌面端 API 请求问题修复（解决问题版0.1）

## 问题现象

在 Tauri 桌面端应用中，配置文件（模型配置、Agent 配置等）无法保存。刷新页面后配置丢失。

## 根本原因

### 原代码

```javascript
// 后端地址：Tauri 桌面端直连 3000 端口，浏览器开发环境用 Vite proxy
const isTauri = (): boolean =>
  typeof window !== 'undefined' && !!(window as any).__TAURI__

const API_BASE = isTauri() ? 'http://127.0.0.1:3000/api' : ''
```

**问题**：在 Tauri WebView 中，`window.__TAURI__` 的初始化时机**晚于 JavaScript 模块加载**，导致 `isTauri()` 返回 `false`，使得 `API_BASE` 被设置为空字符串。

**结果**：前端请求发送到 `/api/...` 而不是 `http://127.0.0.1:3000/api/...`，在 Tauri 桌面端（没有 Vite proxy）请求全部失败。

## 修复方案

改用 Vite 的 `import.meta.env.DEV` 检测开发环境，因为它在**编译时**确定，而不是运行时。

### 修改的文件

**文件**: `src/hooks/useApi.ts`

### 修改后的代码

```javascript
// 后端地址：Tauri 桌面端直连 3000 端口，浏览器开发环境用 Vite proxy
export const getApiBase = (): string => {
  // Vite 的 import.meta.env.DEV 在开发模式下为 true，生产构建为 false
  // Tauri 桌面端使用生产构建，所以 DEV=false，走直连后端
  // 浏览器开发模式 DEV=true，走 Vite proxy
  const isDevelopment = typeof import.meta !== 'undefined' && (import.meta as any).env?.DEV === true

  if (isDevelopment) {
    return '/api'
  }
  // Tauri 桌面端或生产环境：直连后端
  return 'http://127.0.0.1:3000/api'
}
```

## 关键区别

| 环境 | `import.meta.env.DEV` | API_BASE | 说明 |
|------|----------------------|----------|------|
| 浏览器开发 (`npm run dev`) | `true` | `/api` | 走 Vite proxy |
| Tauri 桌面端 (`npm run tauri build`) | `false` | `http://127.0.0.1:3000/api` | 直连后端 |

## 技术原理

`import.meta.env.DEV` 是 Vite 提供的环境变量，在**编译时**就已经确定值：

- 开发模式 (`npm run dev`)：Vite 会将 `import.meta.env.DEV` 替换为 `true`
- 生产构建 (`npm run build` 或 `npm run tauri build`)：Vite 会将 `import.meta.env.DEV` 替换为 `false`

这与 `window.__TAURI__` 的运行时检测不同。`window.__TAURI__` 需要 Tauri WebView 运行时初始化，而模块加载时可能还未准备好。

## 其他相关修改

在修复过程中，还修改了以下文件，将所有 `API_BASE` 引用改为 `getApiBase()` 函数调用：

- `src/components/FileExplorer.tsx`
- `src/components/TreeBrowser.tsx`
- `src/hooks/useFileEditor.ts`
- `src/pages/AgentSettings.tsx`
- `src/pages/IMSettings.tsx`
- `src/pages/SettingsPage.tsx`
- `src/pages/LogsPage.tsx`

这些修改确保所有 API 请求都使用动态获取的 baseURL。

## 总结

这是一个典型的**环境检测时机**问题。使用编译时环境变量 `import.meta.env.DEV` 替代运行时检测 `window.__TAURI__`，解决了 Tauri 桌面端 API 请求无法到达后端的问题。
