# services-manager-tray

用 Web 界面管理后台服务：启动 / 停止 / 重启、查看状态与实时日志、随管理器自动启动。
后端 Rust（axum + tokio），前端 React SPA（Vite + TypeScript + Ant Design），构建产物直接内嵌进单个二进制。

## 功能

- 服务管理：在界面里添加 / 编辑 / 删除服务（命令、参数、工作目录、环境变量）
- 生命周期：启动、停止（SIGTERM 优雅停止 → 超时 SIGKILL）、重启
- 状态：运行中 / 已停止 / 异常退出 / 等待重启，含 PID、运行时长、退出码
- 日志：实时 SSE 流式推送（stdout / stderr 分色），保留最近 5000 行
- 随管理器自启：服务开启 autostart 后，管理器启动时自动拉起
- 异常自动重启：进程非零退出后自动重启（可选）
- 单二进制部署：`frontend/dist` 通过 rust-embed 打进可执行文件

> 安全提示：服务未做鉴权，默认只监听 `127.0.0.1`。请勿直接暴露到公网。

## 快速开始

需要：Rust (stable)、Node.js ≥ 18、pnpm。

```sh
# 安装前端依赖并构建 SPA
pnpm --dir frontend install
pnpm --dir frontend build

# 构建并运行（默认 http://127.0.0.1:8730）
cargo run --release -p services-manager-server
```

或使用 [Task](https://taskfile.dev)：

```sh
task build      # 前端 + 后端一起构建
task dev:backend   # 只跑后端 (8730)
task dev:frontend  # 只跑前端 dev server (5173，/api 已代理)
```

## 命令行参数

```
services-manager [--host HOST] [--port PORT] [--config PATH]

--host HOST    监听地址，默认 127.0.0.1（环境变量 SM_HOST）
--port PORT    监听端口，默认 8730（环境变量 SM_PORT）
--config PATH  配置文件路径（环境变量 SM_CONFIG）
```

默认配置文件位置（由界面自动维护，格式见 `config.example.toml`）：

| 系统 | 路径 |
| --- | --- |
| Linux | `~/.config/services-manager/config.toml` |
| macOS | `~/Library/Application Support/services-manager/config.toml` |
| Windows | `%APPDATA%\services-manager\config.toml` |

## 开机自启

服务的「随管理器自启」指管理器启动时自动拉起。要让管理器本身开机启动，
把它注册成 systemd 用户服务即可，例如 `~/.config/systemd/user/services-manager.service`：

```ini
[Unit]
Description=Services Manager

[Service]
ExecStart=%h/.local/bin/services-manager

[Install]
WantedBy=default.target
```

```sh
systemctl --user enable --now services-manager
```

## API 一览

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/services` | 服务列表（含状态） |
| POST | `/api/services` | 创建服务 |
| PUT | `/api/services/{id}` | 更新服务（运行中不能改启动配置） |
| DELETE | `/api/services/{id}` | 删除（运行中先停止） |
| PUT | `/api/services/{id}/autostart` | `{"enabled": bool}` 切换自启 |
| POST | `/api/services/{id}/start` | 启动 |
| POST | `/api/services/{id}/stop` | 停止 |
| POST | `/api/services/{id}/restart` | 重启 |
| GET | `/api/services/{id}/logs?tail=N&after=SEQ` | 拉取日志 |
| GET | `/api/services/{id}/logs/stream?tail=N` | SSE 实时日志（支持 Last-Event-ID 断点续传） |

## 目录结构

```
crates/server/     Rust 后端：supervisor（进程管理）+ axum API + 内嵌前端
frontend/          React SPA（Vite + TS + Ant Design）
```

后端核心以 library 形式组织（`build_app()`），后续 macOS / Windows 托盘程序
可以直接复用同一个 supervisor 和 API，再叠加 tray 交互。

## Roadmap

- [ ] macOS / Windows 系统托盘外壳（复用 `crates/server`）
- [ ] Windows 进程组 / Job Object 支持（当前 Windows 下停止仅终止直接子进程）
- [ ] 手动按行写入 stdin、Web Terminal
