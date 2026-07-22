# Lan Chat

终端多人聊天室。同一个局域网内的多台电脑通过 TCP 直连聊天，一人启动服务端，其余人连接加入。

## 快速开始

```bash
# 编译
cargo build --release

# 启动服务端（本机）
cargo run --release -- --server

# 好友连接（对方机器）
cargo run --release -- --connect 192.168.1.100:9876
```

不加参数启动会显示菜单界面，在屏幕上选择操作：

```
┌─ Lan Chat ───────────────────────┐
│                                    │
│            Lan Chat                │
│                                    │
│    [1] Start Server                │
│                                    │
│    [2] Connect to Server           │
│                                    │
│   Esc/q: Quit | 1: Start Server   │
│   2: Connect | Enter: Confirm      │
└────────────────────────────────────┘
```

## 使用教程

### 场景：你和朋友在同一个局域网

**你（作为主机）：**

```bash
# 方式 A：菜单启动
cargo run --release
# 按 1 → 回车，服务端启动，进入聊天

# 方式 B：命令行启动（跳过菜单）
cargo run --release -- --server
```

告诉朋友你的局域网 IP（Linux 下用 `ip a` 或 `hostname -I` 查看）。

**朋友（连接者）：**

```bash
# 方式 A：菜单启动
cargo run --release
# 按 2 → 输入你的 IP:端口（如 192.168.1.100:9876）→ 回车

# 方式 B：命令行启动（跳过菜单）
cargo run --release -- --connect 192.168.1.100:9876
```

连接成功后所有人进入聊天界面，可以互相发消息。

### 聊天操作

| 按键 | 作用 |
|------|------|
| 字母/数字/空格 | 输入文字 |
| Backspace | 删除上一个字 |
| Enter | 发送 |
| Ctrl+C / Esc / q | 退出聊天 |

### 自定义端口

```bash
# 服务端指定端口
cargo run --release -- --server --port 5678

# 客户端连接到指定端口
cargo run --release -- --connect 192.168.1.100:5678
```

默认端口 `9876`，服务端和客户端必须使用相同端口。

### 查看本机 IP

```bash
# Linux
hostname -I
# 或
ip addr show

# Windows
ipconfig
```

## Windows 版

在 Linux 上交叉编译：

```bash
sudo apt install gcc-mingw-w64-x86-64
cargo build --release --target x86_64-pc-windows-gnu
```

产物：`target/x86_64-pc-windows-gnu/release/lan-chat.exe`

将 `.exe` 发给朋友直接运行（弹出终端窗口 → 选择 Connect → 输入你的 IP）。

## 启动选项

| 参数 | 作用 |
|------|------|
| `--server` 或 `-s` | 跳过菜单，直接启动服务端 |
| `--connect <ip:port>` 或 `-c` | 跳过菜单，直接连接指定地址 |
| `--port <number>` 或 `-p` | 指定端口（默认 9876） |

参数可以组合使用：

```bash
# 服务端：指定端口，跳过菜单
cargo run --release -- --server -p 5678

# 客户端：指定地址和端口，跳过菜单
cargo run --release -- --connect 10.0.0.5:5678
```

## 服务端与客户端

- **服务端**：本机启动服务端后，自己也作为聊天参与者，可以正常发消息。其他客户端连接后，所有人消息互通。
- **客户端**：连接到服务端的地址和端口，加入聊天。
- 服务端最多支持同时连接多个客户端，所有人消息互通。

## 项目结构

```
src/
├── main.rs          # 入口、事件循环、CLI 参数
├── app.rs           # App 状态机（Menu/Chat 模式切换）
├── ui.rs            # UI 渲染（菜单 + 聊天界面）
├── config.rs        # 全局配置（端口、模式）
├── protocol.rs      # 消息协议（Message 枚举、JSON 序列化）
└── network/
    ├── mod.rs       # Network trait（抽象网络层）
    ├── server.rs    # TCP 服务端实现
    └── client.rs    # TCP 客户端实现
```

协议：TCP + 换行符分割的 JSON 消息。每条消息一行 JSON，`\n` 分隔。
