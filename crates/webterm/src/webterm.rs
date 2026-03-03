use anyhow::{Context, Result};
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{State, Query, Path},
    response::{Html, IntoResponse, Json, Response},
    routing::get,
    Router,
    body::Body,
};
use futures::stream::StreamExt;
use futures::SinkExt;
use log::{error, info, warn};

use serde::{Deserialize, Serialize};

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};

use webterm_common::config::Config;
use webterm_common::session::{SessionManager, SessionInfo};
use webterm_common::hub_client;
use webterm_common::mqtt_notifier;

// 嵌入静态文件到可执行文件中
static XTERM_JS: &str = include_str!("../../../static/xterm.min.js");
static XTERM_ADDON_FIT_JS: &str = include_str!("../../../static/xterm-addon-fit.min.js");
static XTERM_CSS: &str = include_str!("../../../static/xterm.css");

/// 服务器信息
#[derive(Serialize, Clone, Debug)]
struct ServerInfo {
    server_id: String,
    server_name: String,
    user: String,
    hostname: String,
    url: String,
    command: String,
}

#[derive(Clone)]
struct AppState {
    command: String,
    args: Vec<String>,
    session_manager: Arc<SessionManager>,
    default_session_id: Option<String>, // 用于 Hub 模式的 webterm_id
    server_info: Arc<RwLock<Option<ServerInfo>>>, // 服务器信息（Hub 模式，可动态更新）
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
enum WsMessage {
    #[serde(rename = "input")]
    Input { data: String },
    #[serde(rename = "output")]
    Output { data: String },
    #[serde(rename = "resize")]
    Resize { cols: u16, rows: u16 },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "session_info")]
    SessionInfo { session_id: String },
}

#[derive(Deserialize)]
struct WsQuery {
    session: Option<String>,
}

// 嵌入的前端 HTML
const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
    <title>Web Terminal</title>
    <script src="/static/xterm.min.js"></script>
    <script src="/static/xterm-addon-fit.min.js"></script>
    <link rel="stylesheet" href="/static/xterm.css">
    <style>
        * {
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }
        html, body {
            background: #1e1e1e;
            width: 100%;
            height: 100%;
            overflow: hidden;
        }
        body {
            display: flex;
            flex-direction: column;
            font-family: 'Courier New', monospace;
        }
        #command-bar {
            background: #333;
            color: #4ec9b0;
            font-size: 12px;
            padding: 4px 10px;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
            flex-shrink: 0;
        }
        #terminal-container {
            flex: 1;
            min-height: 0;
            overflow: hidden;
            position: relative;
        }
        #terminal {
            width: 100%;
            height: 100%;
        }
        @supports (height: 100dvh) {
            html, body {
                height: 100dvh;
            }
        }
    </style>
</head>
<body>
    <div id="command-bar">Loading...</div>
    <div id="terminal-container">
        <div id="terminal"></div>
    </div>

    <script>
        const term = new Terminal({
            cursorBlink: true,
            fontSize: 14,
            fontFamily: '"Courier New", "DejaVu Sans Mono", monospace',
            theme: {
                background: '#1e1e1e',
                foreground: '#d4d4d4',
                cursor: '#d4d4d4',
                selection: '#264f78',
                black: '#1e1e1e',
                red: '#f48771',
                green: '#89d185',
                yellow: '#dcdcaa',
                blue: '#569cd6',
                magenta: '#c586c0',
                cyan: '#4ec9b0',
                white: '#d4d4d4',
                brightBlack: '#808080',
                brightRed: '#f48771',
                brightGreen: '#89d185',
                brightYellow: '#dcdcaa',
                brightBlue: '#569cd6',
                brightMagenta: '#c586c0',
                brightCyan: '#4ec9b0',
                brightWhite: '#ffffff'
            },
            scrollback: 10000,
            allowProposedApi: true
        });

        const fitAddon = new FitAddon.FitAddon();
        term.loadAddon(fitAddon);
        term.open(document.getElementById('terminal'));

        // 获取并显示命令信息
        fetch('/api/servers')
            .then(res => res.json())
            .then(servers => {
                if (servers && servers.length > 0) {
                    document.getElementById('command-bar').textContent = servers[0].command;
                } else {
                    document.getElementById('command-bar').textContent = 'Terminal';
                }
            })
            .catch(() => {
                document.getElementById('command-bar').textContent = 'Terminal';
            });

        let ws;
        let reconnectTimer;
        let heartbeatTimer;
        let lastPongTime = Date.now();
        let isManualClose = false;

        // 从 URL 或 localStorage 获取 session_id
        const urlParams = new URLSearchParams(window.location.search);
        let sessionId = urlParams.get('session') || localStorage.getItem('webterm_session_id');

        function fitTerminal() {
            const container = document.getElementById('terminal-container');
            const rect = container.getBoundingClientRect();

            // 确保容器有有效尺寸
            if (rect.width <= 0 || rect.height <= 0) {
                console.warn('Container has no size, retrying...');
                requestAnimationFrame(fitTerminal);
                return;
            }

            console.log('fitTerminal: container size', rect.width, 'x', rect.height);

            // 使用 fitAddon 自动计算并应用最佳尺寸
            try {
                fitAddon.fit();
            } catch (e) {
                console.error('fitAddon.fit() failed:', e);
                return;
            }

            const dims = fitAddon.proposeDimensions();
            console.log('fitTerminal: dims', dims);
            if (dims && ws && ws.readyState === WebSocket.OPEN) {
                sendResize(dims.cols, dims.rows);
            }
            // 确保 fit 完成后滚动到底部，让输入行可见
            requestAnimationFrame(() => {
                term.scrollToBottom();
            });
        }

        // 延迟初始化，确保 DOM 完全渲染
        requestAnimationFrame(fitTerminal);

        function connect() {
            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            const wsUrl = sessionId
                ? `${protocol}//${window.location.host}/ws?session=${sessionId}`
                : `${protocol}//${window.location.host}/ws`;

            console.log('Connecting...');
            ws = new WebSocket(wsUrl);
            ws.binaryType = 'arraybuffer';

            ws.onopen = () => {
                console.log('WebSocket connected');
                lastPongTime = Date.now();

                // 发送初始终端大小
                fitTerminal();

                // 启动心跳检测
                startHeartbeat();
            };

            ws.onmessage = (event) => {
                // 处理二进制 ping 消息
                if (event.data instanceof ArrayBuffer) {
                    const view = new Uint8Array(event.data);
                    if (view.length === 1 && view[0] === 0x09) {
                        // 收到 ping，回复 pong
                        lastPongTime = Date.now();
                        if (ws.readyState === WebSocket.OPEN) {
                            ws.send(new Uint8Array([0x0A]));
                        }
                        return;
                    }
                    return;
                }

                try {
                    const msg = JSON.parse(event.data);
                    if (msg.type === 'output') {
                        term.write(msg.data);
                        // 有新输出时滚动到底部
                        requestAnimationFrame(() => term.scrollToBottom());
                    } else if (msg.type === 'error') {
                        term.write(`\r\n[错误: ${msg.message}]\r\n`);
                    } else if (msg.type === 'session_info') {
                        sessionId = msg.session_id;
                        localStorage.setItem('webterm_session_id', sessionId);
                        console.log('Session established:', sessionId.substring(0, 12) + '...');
                        // 会话建立后滚动到底部
                        requestAnimationFrame(() => term.scrollToBottom());
                    } else if (msg.type === 'process_exit') {
                        // 进程退出
                        console.log('Process exited');
                        term.write(`\r\n\n[${msg.message}]\r\n`);
                        // 清除 session_id，避免重连时连接到已退出的会话
                        localStorage.removeItem('webterm_session_id');
                        sessionId = null;
                        // 延迟后自动重新连接（创建新会话）
                        setTimeout(() => {
                            console.log('Auto reconnecting after process exit...');
                            isManualClose = false;
                            connect();
                        }, 1500);
                    }
                } catch (e) {
                    console.error('解析消息失败:', e);
                }
            };

            ws.onclose = () => {
                console.log('WebSocket disconnected');
                clearInterval(heartbeatTimer);
                // 只有在不是主动关闭的情况下才显示重连提示
                if (!isManualClose) {
                    term.write('\r\n\n[连接已关闭，5秒后尝试重连...]\r\n');
                    reconnectTimer = setTimeout(connect, 5000);
                } else {
                    // 进程退出后，提示用户刷新页面
                    term.write('\r\n[按 Enter 键或刷新页面重新开始]\r\n');
                }
            };

            ws.onerror = (error) => {
                console.error('WebSocket error:', error);
            };
        }
        
        // 监听页面可见性变化，页面重新可见时检查连接
        if (document.visibilityState) {
            document.addEventListener('visibilitychange', () => {
                if (document.visibilityState === 'visible') {
                    console.log('Page visible, checking connection...');
                    // 如果连接已断开，立即重连
                    if (!ws || ws.readyState !== WebSocket.OPEN) {
                        console.log('Connection lost, reconnecting...');
                        clearTimeout(reconnectTimer);
                        connect();
                    }
                }
            });
        }
        
        // 监听页面重新获得焦点（备用方案）
        window.addEventListener('focus', () => {
            if (!ws || ws.readyState !== WebSocket.OPEN) {
                console.log('Window focused, reconnecting...');
                clearTimeout(reconnectTimer);
                connect();
            }
        });
        
        function startHeartbeat() {
            // 每 30 秒检查一次连接状态
            heartbeatTimer = setInterval(() => {
                const now = Date.now();
                // 如果超过 90 秒没有收到 ping，认为连接断开
                if (now - lastPongTime > 90000) {
                    console.log('Heartbeat timeout, closing connection');
                    ws.close();
                    return;
                }
            }, 30000);
        }

        function sendInput(data) {
            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({ type: 'input', data }));
            }
        }

        function sendResize(cols, rows) {
            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({ type: 'resize', cols, rows }));
            }
        }

        // 处理终端输入
        term.onData((data) => {
            console.log('term.onData 触发, 数据:', JSON.stringify(data), '字节:', data.split('').map(c => c.charCodeAt(0)));
            
            // 如果 WebSocket 已关闭且用户按下回车，尝试重新连接
            if ((!ws || ws.readyState !== WebSocket.OPEN) && isManualClose && data === '\r') {
                console.log('进程退出后用户按回车，重新连接...');
                isManualClose = false; // 重置标志
                term.write('\r\n[正在重新连接...]\r\n');
                connect();
                return;
            }
            
            sendInput(data);
        });

        // 额外：监听键盘事件用于调试
        term.onKey((e) => {
            console.log('term.onKey 触发:', e.key, 'domEvent:', e.domEvent ? e.domEvent.key : 'none');
        });

        // 处理窗口大小变化（包括键盘弹出/收起）
        let resizeTimer;
        function handleResize() {
            clearTimeout(resizeTimer);
            resizeTimer = setTimeout(() => {
                console.log('handleResize triggered');
                fitTerminal();
            }, 100);
        }

        window.addEventListener('resize', handleResize);

        // 监听屏幕旋转
        window.addEventListener('orientationchange', () => {
            console.log('orientationchange triggered');
            // 延迟执行，等待屏幕旋转完成
            setTimeout(() => {
                fitTerminal();
            }, 200);
        });

        // 使用 Visual Viewport API 监听键盘变化（如果支持）
        if (window.visualViewport) {
            window.visualViewport.addEventListener('resize', handleResize);
            window.visualViewport.addEventListener('scroll', handleResize);
        }

        // 初始连接
        connect();

        // 页面关闭时清理
        window.addEventListener('beforeunload', () => {
            isManualClose = true;
            if (reconnectTimer) clearTimeout(reconnectTimer);
            if (heartbeatTimer) clearInterval(heartbeatTimer);
            if (ws) ws.close();
        });
    </script>
</body>
</html>"#;

async fn index_handler() -> impl IntoResponse {
    Html(INDEX_HTML)
}

async fn static_handler(Path(filename): Path<String>) -> impl IntoResponse {
    let (content, content_type) = match filename.as_str() {
        "xterm.min.js" => (XTERM_JS, "application/javascript"),
        "xterm-addon-fit.min.js" => (XTERM_ADDON_FIT_JS, "application/javascript"),
        "xterm.css" => (XTERM_CSS, "text/css"),
        _ => return Response::builder().status(404).body(Body::empty()).unwrap(),
    };
    
    Response::builder()
        .header("Content-Type", content_type)
        .body(Body::from(content))
        .unwrap()
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(query): Query<WsQuery>,
) -> impl IntoResponse {
    let session_id = query.session;
    ws.on_upgrade(move |socket| handle_socket(socket, state, session_id))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>, session_id: Option<String>) {
    // 尝试获取或创建会话
    let session = if let Some(ref id) = session_id {
        // 尝试连接到现有会话
        if let Some(existing) = state.session_manager.get(id) {
            // 检查进程是否仍在运行
            if existing.is_process_running().await {
                Some(existing)
            } else {
                // 进程已退出，销毁旧会话并创建新会话
                info!("会话 {} 进程已退出，创建新会话", id);
                state.session_manager.destroy(id).await;
                create_new_session(&state).await
            }
        } else {
            // 会话不存在，自动创建新会话（使用新的 session_id）
            create_new_session(&state).await
        }
    } else {
        // 没有指定 session_id，创建新会话
        create_new_session(&state).await
    };

    match session {
        Some(session) => {
            // 发送新的 session_id 给客户端
            let (mut socket_tx, socket_rx) = socket.split();
            let msg = serde_json::to_string(&WsMessage::SessionInfo { 
                session_id: session.id.clone() 
            }).unwrap();
            
            if socket_tx.send(Message::Text(msg)).await.is_ok() {
                // 使用新的 handler 处理已 split 的 socket
                let session_manager = state.session_manager.clone();
                handle_split_socket(socket_tx, socket_rx, session, session_manager).await;
            }
        }
        None => {
            let (mut socket_tx, _) = socket.split();
            let msg = serde_json::to_string(&WsMessage::Error {
                message: "无法创建会话".to_string(),
            }).unwrap();
            let _ = socket_tx.send(Message::Text(msg)).await;
        }
    }
}

async fn create_new_session(state: &Arc<AppState>) -> Option<Arc<webterm_common::session::Session>> {
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());
    
    // 使用 default_session_id（Hub 模式下的 webterm_id）
    let session_id = state.default_session_id.clone();
    
    match state.session_manager.create(
        state.command.clone(), 
        state.args.clone(), 
        cwd, 
        session_id
    ).await {
        Ok(id) => state.session_manager.get(&id),
        Err(e) => {
            error!("创建会话失败: {}", e);
            None
        }
    }
}

async fn handle_split_socket(
    mut socket_tx: futures::stream::SplitSink<WebSocket, Message>,
    mut socket_rx: futures::stream::SplitStream<WebSocket>,
    session: Arc<webterm_common::session::Session>,
    session_manager: Arc<webterm_common::session::SessionManager>,
) {
    info!("WebSocket 连接到会话: {}", session.id);

    // 创建通道用于从会话发送数据到 WebSocket
    let (tx, mut rx) = mpsc::channel::<Message>(100);

    // 绑定会话，获取客户端 ID
    let client_id = session.attach(tx).await;
    info!("会话 {} 客户端 {} 已连接", session.id, client_id);

    // 转发会话输出到 WebSocket
    let session_for_exit = session.clone();
    let client_id_for_exit = client_id.clone();
    let forward_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            // 检查是否是进程退出消息
            if let Message::Text(ref text) = msg {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
                    if json.get("type").and_then(|t| t.as_str()) == Some("process_exit") {
                        info!("会话 {} 客户端 {} 收到进程退出消息", session_for_exit.id, client_id_for_exit);
                    }
                }
            }
            if socket_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    // 处理 WebSocket 输入
    let session_for_input = session.clone();
    let client_id_for_input = client_id.clone();
    let input_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = socket_rx.next().await {
            match msg {
                Message::Text(text) => {
                    log::debug!("客户端 {} 收到文本消息: {}", client_id_for_input, text);
                    match serde_json::from_str::<WsMessage>(&text) {
                        Ok(WsMessage::Input { data }) => {
                            log::debug!("客户端 {} 解析为 Input 消息，数据长度: {}", client_id_for_input, data.len());
                            if let Err(e) = session_for_input.write_input(&data).await {
                                warn!("客户端 {} 写入 PTY 失败: {}", client_id_for_input, e);
                                break;
                            } else {
                                log::debug!("客户端 {} 成功写入 PTY: {:?}", client_id_for_input, data.as_bytes());
                            }
                        }
                        Ok(WsMessage::Resize { cols, rows }) => {
                            log::debug!("客户端 {} 解析为 Resize 消息: {}x{}", client_id_for_input, cols, rows);
                            if let Err(e) = session_for_input.resize(cols, rows).await {
                                warn!("客户端 {} 调整终端大小失败: {}", client_id_for_input, e);
                            }
                        }
                        Err(e) => {
                            log::warn!("客户端 {} 解析消息失败: {}, 原始消息: {}", client_id_for_input, e, text);
                        }
                        _ => {}
                    }
                }
                Message::Close(_) => break,
                Message::Binary(bin) => {
                    log::debug!("收到二进制消息: {:?}", bin);
                }
                _ => {
                    log::debug!("收到其他类型消息");
                }
            }
        }
        log::info!("输入处理任务结束");
    });

    // 等待任一任务结束
    tokio::select! {
        _ = forward_task => {}
        _ = input_task => {}
    }

    // 解绑当前客户端（但不销毁会话）
    session.detach(&client_id).await;
    info!("WebSocket 断开，会话 {} 客户端 {} 已移除", session.id, client_id);
    
    // 检查进程是否已退出且没有更多客户端，如果是则销毁会话
    let is_running = session.is_process_running().await;
    let client_count = session.client_count().await;
    if !is_running && client_count == 0 {
        info!("会话 {} 进程已退出且无客户端连接，销毁会话", session.id);
        session_manager.destroy(&session.id).await;
    }
}

// 会话列表 API
async fn list_sessions(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let sessions: Vec<SessionInfo> = state.session_manager.list();
    Json(sessions)
}

// 服务器列表 API - 返回当前服务器信息
async fn list_servers(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let servers: Vec<ServerInfo> = match *state.server_info.read().await {
        Some(ref info) => vec![info.clone()],
        None => vec![],
    };
    Json(servers)
}

// 删除会话 API
async fn delete_session(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    state.session_manager.destroy(&id).await;
    Json(serde_json::json!({ "success": true }))
}

pub async fn run_webterm(
    bind: String, 
    command: Option<String>, 
    args: Vec<String>,
    enable_mqtt: bool,
    hub_addr: Option<String>,
    server_name: Option<String>,
    config: &Config,
) -> Result<()> {
    let bind_addr: SocketAddr = bind
        .parse()
        .context("解析绑定地址失败，请使用格式: IP:PORT")?;

    // 确定默认命令（使用配置文件中的值）
    let default_cmd = if cfg!(windows) {
        &config.webterm.windows_cmd
    } else {
        &config.webterm.unix_cmd
    };

    let cmd = command.unwrap_or_else(|| default_cmd.to_string());
    let full_command = format!("{} {}", cmd, args.join(" "));

    info!("启动 Web 终端服务...");
    info!("监听地址: http://{}", bind_addr);
    info!("运行命令: {} {:?}", cmd, args);

    // 创建会话管理器（使用配置文件中的会话配置）
    let session_config = webterm_common::session::SessionConfig {
        max_history_lines: config.session.max_history_lines,
        session_timeout: std::time::Duration::from_secs(config.session.timeout_secs),
        max_sessions: config.session.max_sessions,
        default_rows: config.session.default_rows,
        default_cols: config.session.default_cols,
    };
    let session_manager = SessionManager::new(session_config);

    // 发送 MQTT 通知（如果启用且有配置）
    if enable_mqtt {
        mqtt_notifier::notify_webterm_started(&bind_addr, &cmd, Some(&config.mqtt));
    }

    // 连接到 Hub（支持重连）
    let mut default_session_id: Option<String> = None;
    let server_info: Arc<RwLock<Option<ServerInfo>>> = Arc::new(RwLock::new(None));
    let mut control_rx = None;
    let hub_url = format!("http://{}", bind_addr);
    let hub_client: Option<Arc<webterm_common::hub_client::HubClient>> = if let Some(ref hub) = hub_addr {
        match hub_client::HubClient::new(hub, server_name.clone(), &hub_url, Some(&config.mqtt)).await {
            Ok((client, webterm_id, server_id, srv_name, user, hostname, rx)) => {
                if let Err(e) = client.register(&webterm_id, &full_command).await {
                    warn!("Hub 注册失败: {}", e);
                    None
                } else {
                    client.start_heartbeat();
                    info!("已连接到 Hub: {}", hub);
                    // 保存 webterm_id 作为默认 session_id
                    default_session_id = Some(webterm_id.clone());
                    
                    // 创建服务器信息
                    let info = ServerInfo {
                        server_id: server_id.clone(),
                        server_name: srv_name.clone(),
                        user: user.clone(),
                        hostname: hostname.clone(),
                        url: hub_url.clone(),
                        command: full_command.clone(),
                    };
                    *server_info.write().await = Some(info);
                    
                    control_rx = Some((rx, server_id.clone()));
                    
                    Some(Arc::new(client))
                }
            }
            Err(e) => {
                warn!("连接 Hub 失败: {}，将启动重连机制", e);
                // 启动后台重连任务
                let hub = hub.clone();
                let server_name = server_name.clone();
                let hub_url = hub_url.clone();
                let mqtt_config = config.mqtt.clone();
                let full_command = full_command.clone();
                let server_info = server_info.clone();
                tokio::spawn(async move {
                    let mut retry_count = 0;
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                        retry_count += 1;
                        info!("尝试重连 Hub... (第 {} 次)", retry_count);
                        
                        match hub_client::HubClient::new(&hub, server_name.clone(), &hub_url, Some(&mqtt_config)).await {
                            Ok((client, webterm_id, server_id, srv_name, user, hostname, rx)) => {
                                if let Err(e) = client.register(&webterm_id, &full_command).await {
                                    warn!("Hub 重连后注册失败: {}", e);
                                    continue;
                                }
                                client.start_heartbeat();
                                info!("Hub 重连成功！");
                                
                                // 更新服务器信息
                                let info = ServerInfo {
                                    server_id: server_id.clone(),
                                    server_name: srv_name.clone(),
                                    user: user.clone(),
                                    hostname: hostname.clone(),
                                    url: hub_url.clone(),
                                    command: full_command.clone(),
                                };
                                *server_info.write().await = Some(info);
                                
                                // 注意：控制命令处理只在初始连接成功时启动
                                // 重连后如需支持控制命令，需要更复杂的架构
                                let _ = rx; // 抑制未使用变量警告
                                
                                info!("Hub 重连完成，server_id: {}", server_id);
                                break;
                            }
                            Err(e) => {
                                warn!("Hub 重连失败: {}", e);
                            }
                        }
                    }
                });
                None
            }
        }
    } else {
        None
    };

    let state = Arc::new(AppState {
        command: cmd.clone(),
        args: args.clone(),
        session_manager: session_manager.clone(),
        default_session_id,
        server_info,
    });
    
    // 启动控制命令处理任务
    if let Some((mut rx, server_id)) = control_rx {
        let state_for_control = state.clone();
        let hub_client_for_control = hub_client.clone();
        tokio::spawn(async move {
            info!("启动控制命令处理任务，server_id: {}", server_id);
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    webterm_common::hub_client::ControlCommand::Start { cmd: custom_cmd, args: custom_args } => {
                        info!("收到启动命令: cmd={:?}, args={:?}", custom_cmd, custom_args);
                        // 使用自定义命令或默认命令
                        let command = custom_cmd.unwrap_or_else(|| state_for_control.command.clone());
                        let arguments = if custom_args.is_empty() {
                            state_for_control.args.clone()
                        } else {
                            custom_args
                        };
                        
                        // 创建新会话
                        let cwd = std::env::current_dir()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|_| ".".to_string());
                        
                        match state_for_control.session_manager.create(
                            command.clone(),
                            arguments.clone(),
                            cwd,
                            None, // 让系统自动生成 session_id
                        ).await {
                            Ok(session_id) => {
                                info!("控制命令: 成功创建新会话 {}", session_id);
                                // 向 Hub 注册这个新会话
                                if let Some(ref hc) = hub_client_for_control {
                                    let cmd_str = format!("{} {}", command, arguments.join(" "));
                                    if let Err(e) = hc.register(&session_id, &cmd_str).await {
                                        warn!("向 Hub 注册新会话失败: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("控制命令: 创建会话失败: {}", e);
                            }
                        }
                    }
                    webterm_common::hub_client::ControlCommand::Stop { webterm_id } => {
                        info!("收到停止命令: webterm_id={}", webterm_id);
                        // 停止指定的会话
                        state_for_control.session_manager.destroy(&webterm_id).await;
                        info!("控制命令: 已停止会话 {}", webterm_id);
                        // 向 Hub 注销这个会话
                        if let Some(ref hc) = hub_client_for_control {
                            if let Err(e) = hc.unregister(&webterm_id).await {
                                warn!("向 Hub 注销会话失败: {}", e);
                            }
                        }
                    }
                }
            }
            warn!("控制命令处理任务结束");
        });
    }

    info!("[run_webterm] 注册路由...");
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/static/:filename", get(static_handler))
        .route("/ws", get(ws_handler))
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/:id", axum::routing::delete(delete_session))
        .route("/api/servers", get(list_servers))
        .with_state(state);
    info!("[run_webterm] 路由注册完成，包含 /api/servers");

    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("无法绑定到地址: {}", bind_addr))?;

    info!("Web 终端已启动，请在浏览器中打开 http://{}", bind_addr);
    
    // 打印 QR 码或 URL，方便手机扫描
    println!("\n========================================");
    println!("WebTerm 已启动!");
    println!("URL: http://{}", bind_addr);
    println!("========================================\n");

    axum::serve(listener, app)
        .await
        .context("Web 服务运行失败")?;

    Ok(())
}
