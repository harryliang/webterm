//! Webterm Hub - 中心管理服务
//!
//! 用于统一管理多台 PC 上的 Web 终端。
//! 所有配置优先从环境变量读取，便于容器化部署。

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, Json},
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use log::{info, warn};
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use serde::{Deserialize, Serialize};
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval, sleep};
use tower_http::cors::{Any, CorsLayer};

/// Server 信息
#[derive(Debug, Clone, Serialize)]
struct ServerInfo {
    id: String,
    name: String,
    user: String,
    hostname: String,
    #[serde(skip_serializing)]
    last_heartbeat: DateTime<Utc>,
    webterms: Vec<WebTermInfo>,
}

/// WebTerm 会话信息
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WebTermInfo {
    id: String,
    url: String,
    command: String,
    #[serde(rename = "cwd")]
    work_dir: String,
    #[serde(skip, default = "Utc::now")]
    created_at: DateTime<Utc>,
    #[serde(skip)]
    last_heartbeat: DateTime<Utc>,
}

/// MQTT 注册消息
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum HubMessage {
    #[serde(rename = "register")]
    Register {
        server_id: String,
        server_name: String,
        user: String,
        hostname: String,
        webterm: WebTermRegister,
    },
    #[serde(rename = "heartbeat")]
    Heartbeat {
        server_id: String,
        #[serde(default)]
        server_name: String,
        #[serde(default)]
        user: String,
        #[serde(default)]
        hostname: String,
        active_webterms: Vec<String>,
        #[serde(default)]
        webterms_info: Vec<WebTermInfo>,
    },
    #[serde(rename = "unregister")]
    Unregister {
        server_id: String,
        webterm_id: String,
    },
}

/// 控制命令请求（来自 Android 客户端）
#[derive(Debug, Deserialize)]
struct ControlRequest {
    server_id: String,
    command: ControlCommand,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action")]
enum ControlCommand {
    #[serde(rename = "start")]
    Start {
        #[serde(default)]
        cmd: Option<String>,
        #[serde(default)]
        args: Vec<String>,
    },
    #[serde(rename = "stop")]
    Stop { webterm_id: String },
}

/// 控制命令响应
#[derive(Debug, Serialize)]
struct ControlResponse {
    success: bool,
    message: String,
}

#[derive(Debug, Deserialize)]
struct WebTermRegister {
    id: String,
    url: String,
    command: String,
    #[serde(rename = "cwd")]
    work_dir: String,
}

/// 应用状态
#[derive(Clone)]
struct AppState {
    servers: Arc<DashMap<String, ServerInfo>>,
    webterms: Arc<DashMap<String, WebTermInfo>>,
    heartbeat_timeout: i64,
    mqtt_client: Arc<tokio::sync::Mutex<Option<AsyncClient>>>,
}

impl AppState {
    fn new(heartbeat_timeout: i64) -> Self {
        Self {
            servers: Arc::new(DashMap::new()),
            webterms: Arc::new(DashMap::new()),
            heartbeat_timeout,
            mqtt_client: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    async fn set_mqtt_client(&self, client: AsyncClient) {
        let mut guard = self.mqtt_client.lock().await;
        *guard = Some(client);
    }

    async fn publish_control(&self, server_id: &str, command: &ControlCommand) -> Result<(), String> {
        let client_opt = self.mqtt_client.lock().await.clone();
        if let Some(client) = client_opt {
            let topic = format!("webterm/server/{}/control", server_id);
            let payload = serde_json::to_string(command).map_err(|e| e.to_string())?;
            client
                .publish(topic, QoS::AtLeastOnce, false, payload)
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        } else {
            Err("MQTT 客户端未初始化".to_string())
        }
    }

    fn register_server(&self, msg: HubMessage) {
        if let HubMessage::Register {
            server_id,
            server_name,
            user,
            hostname,
            webterm,
        } = msg
        {
            let mut server = self.servers.entry(server_id.clone()).or_insert_with(|| {
                info!("新 Server 注册: {}", server_name);
                ServerInfo {
                    id: server_id.clone(),
                    name: server_name.clone(),
                    user: user.clone(),
                    hostname: hostname.clone(),
                    last_heartbeat: Utc::now(),
                    webterms: vec![],
                }
            });

            // 添加或更新 webterm
            let now = Utc::now();
            let wt_info = WebTermInfo {
                id: webterm.id.clone(),
                url: webterm.url,
                command: webterm.command,
                work_dir: webterm.work_dir,
                created_at: now,
                last_heartbeat: now,
            };
            
            self.webterms.insert(webterm.id.clone(), wt_info.clone());
            
            // 检查是否已存在该 webterm
            if !server.webterms.iter().any(|wt| wt.id == webterm.id) {
                server.webterms.push(wt_info);
            }

            server.last_heartbeat = Utc::now();
            info!("Server {} 注册/更新完成, WebTerm: {}", server_id, webterm.id);
        }
    }

    fn update_heartbeat(&self, msg: HubMessage) {
        if let HubMessage::Heartbeat {
            server_id,
            server_name,
            user,
            hostname,
            active_webterms,
            webterms_info,
        } = msg
        {
            let now = Utc::now();
            
            // 如果 Server 不存在（Hub 重启后），从心跳信息中恢复
            if !self.servers.contains_key(&server_id) {
                info!("收到心跳但 Server {} 不存在，从心跳恢复完整信息", server_id);
                
                // 使用心跳中的 Server 信息，如果不存在则使用默认值
                let name = if server_name.is_empty() { 
                    format!("恢复中... ({})", &server_id[..8.min(server_id.len())])
                } else { 
                    server_name 
                };
                let user = if user.is_empty() { "unknown".to_string() } else { user };
                let hostname = if hostname.is_empty() { "unknown".to_string() } else { hostname };
                
                let placeholder = ServerInfo {
                    id: server_id.clone(),
                    name,
                    user,
                    hostname,
                    last_heartbeat: now,
                    webterms: vec![],
                };
                self.servers.insert(server_id.clone(), placeholder);
            }
            
            if let Some(mut server) = self.servers.get_mut(&server_id) {
                server.last_heartbeat = now;
                
                // 从心跳中恢复缺失的 webterm 信息
                for wt_info in webterms_info {
                    let wt_id = &wt_info.id;
                    
                    // 检查是否已存在该 webterm
                    if let Some(existing) = server.webterms.iter_mut().find(|wt| wt.id == *wt_id) {
                        // 更新现有 term 的心跳时间
                        existing.last_heartbeat = now;
                    } else {
                        // 恢复缺失的 webterm
                        info!("从心跳恢复 WebTerm: {} ({})", wt_id, wt_info.command);
                        let mut new_wt = wt_info.clone();
                        new_wt.last_heartbeat = now;
                        server.webterms.push(new_wt);
                        self.webterms.insert(wt_id.clone(), wt_info.clone());
                    }
                }
                
                info!("Server {} 心跳更新, 活跃 WebTerms: {:?}", server_id, active_webterms);
            }
        }
    }

    fn unregister_webterm(&self, msg: HubMessage) {
        if let HubMessage::Unregister {
            server_id,
            webterm_id,
        } = msg
        {
            if let Some(mut server) = self.servers.get_mut(&server_id) {
                server.webterms.retain(|wt| wt.id != webterm_id);
                self.webterms.remove(&webterm_id);
                info!("WebTerm {} 从 Server {} 注销", webterm_id, server_id);
            }
        }
    }

    fn get_all_servers(&self) -> Vec<ServerInfo> {
        self.servers
            .iter()
            .filter(|entry| {
                // 只返回有心跳超时时间内的 Server
                let elapsed = Utc::now() - entry.last_heartbeat;
                elapsed.num_seconds() < self.heartbeat_timeout
            })
            .map(|entry| entry.clone())
            .collect()
    }

    fn get_server(&self, id: &str) -> Option<ServerInfo> {
        self.servers.get(id).map(|entry| entry.clone())
    }
}

/// Hub 配置
struct HubConfig {
    http_bind: SocketAddr,
    mqtt_host: String,
    mqtt_port: u16,
    mqtt_username: Option<String>,
    mqtt_password: Option<String>,
    heartbeat_timeout: i64,
    cleanup_interval: u64,
}

/// 获取本地 IP 地址，优先返回 10.126 开头的地址
/// 使用系统命令获取（Windows: ipconfig, Linux/Mac: ip addr）
fn get_local_ip() -> Option<String> {
    #[cfg(windows)]
    {
        get_ip_from_ipconfig()
    }
    #[cfg(unix)]
    {
        get_ip_from_ip_command().or_else(get_ip_from_ifconfig)
    }
    #[cfg(not(any(windows, unix)))]
    {
        None
    }
}

#[cfg(windows)]
fn get_ip_from_ipconfig() -> Option<String> {
    use std::process::Command;
    
    let output = Command::new("ipconfig").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    for line in stdout.lines() {
        // 查找包含 "IPv4" 和 "10.126." 的行
        if line.contains("IPv4") && line.contains("10.126.") {
            // 找到冒号后的地址
            if let Some(pos) = line.rfind(':') {
                let ip_part = line[pos+1..].trim();
                // 提取 IP 地址（只保留数字和点）
                let ip: String = ip_part.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
                if !ip.is_empty() && ip.starts_with("10.126.") {
                    return Some(ip);
                }
            }
        }
    }
    
    None
}

#[cfg(unix)]
fn get_ip_from_ip_command() -> Option<String> {
    use std::process::Command;
    
    let output = Command::new("ip").args(["-4", "addr", "show"]).output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    for line in stdout.lines() {
        if line.contains("10.126.") {
            // 提取 inet 后的地址
            if let Some(pos) = line.find("inet ") {
                let rest = &line[pos+5..];
                let ip_part = rest.split_whitespace().next()?;
                // 去掉 CIDR 后缀
                let ip = ip_part.split('/').next()?;
                if ip.starts_with("10.126.") {
                    return Some(ip.to_string());
                }
            }
        }
    }
    
    None
}

#[cfg(unix)]
fn get_ip_from_ifconfig() -> Option<String> {
    use std::process::Command;
    
    let output = Command::new("ifconfig").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    for line in stdout.lines() {
        if line.contains("inet ") && line.contains("10.126.") {
            // 提取 inet 后的地址
            if let Some(pos) = line.find("inet ") {
                let rest = &line[pos+5..];
                let ip = rest.split_whitespace().next()?;
                if ip.starts_with("10.126.") {
                    return Some(ip.to_string());
                }
            }
        }
    }
    
    None
}

impl HubConfig {
    fn from_env() -> Self {
        // HTTP 监听地址
        let http_bind = std::env::var("HUB_HTTP_BIND")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| {
                // 优先使用 10.126 开头的地址
                let default_bind = get_local_ip()
                    .map(|ip| format!("{}:8080", ip))
                    .unwrap_or_else(|| "0.0.0.0:8080".to_string());
                default_bind.parse().unwrap()
            });
        
        // MQTT 配置
        let mqtt_host = std::env::var("HUB_MQTT_HOST")
            .unwrap_or_else(|_| "localhost".to_string());
        let mqtt_port = std::env::var("HUB_MQTT_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(1883);
        let mqtt_username = std::env::var("HUB_MQTT_USER").ok();
        let mqtt_password = std::env::var("HUB_MQTT_PASS").ok();
        
        // 心跳超时（秒）
        let heartbeat_timeout = std::env::var("HUB_HEARTBEAT_TIMEOUT")
            .ok()
            .and_then(|t| t.parse().ok())
            .unwrap_or(90);
        
        // 清理间隔（秒）
        let cleanup_interval = std::env::var("HUB_CLEANUP_INTERVAL")
            .ok()
            .and_then(|i| i.parse().ok())
            .unwrap_or(30);
        
        Self {
            http_bind,
            mqtt_host,
            mqtt_port,
            mqtt_username,
            mqtt_password,
            heartbeat_timeout,
            cleanup_interval,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    let config = HubConfig::from_env();
    
    // 先检测 MQTT 连接
    print!("正在检测 MQTT Broker 连接 ({}:{})...", config.mqtt_host, config.mqtt_port);
    io::Write::flush(&mut io::stdout()).unwrap();
    
    match check_mqtt_connection(&config).await {
        Ok(_) => {
            println!(" 成功");
        }
        Err(e) => {
            println!(" 失败");
            eprintln!("");
            eprintln!("========================================");
            eprintln!("错误: 无法连接到 MQTT Broker!");
            eprintln!("========================================");
            eprintln!("  地址: {}:{}", config.mqtt_host, config.mqtt_port);
            eprintln!("  原因: {}", e);
            eprintln!("");
            eprintln!("请确保 MQTT Broker 已启动:");
            eprintln!("  - Windows (choco):  choco install mosquitto && mosquitto");
            eprintln!("  - Docker:           docker run -d -p 1883:1883 eclipse-mosquitto");
            eprintln!("  - Linux:            apt install mosquitto && mosquitto -d");
            eprintln!("========================================");
            std::process::exit(1);
        }
    }
    
    // 打印启动信息（使用 println! 确保始终显示）
    println!("");
    println!("========================================");
    println!("    Webterm Hub 中心服务已启动");
    println!("========================================");
    println!("  HTTP API 地址: http://{}", config.http_bind);
    println!("  MQTT Broker:   {}:{}", config.mqtt_host, config.mqtt_port);
    println!("  心跳超时:      {} 秒", config.heartbeat_timeout);
    println!("----------------------------------------");
    println!("  可用接口:");
    println!("    GET http://{}            (Web 控制台)", config.http_bind);
    println!("    GET http://{}/api/servers  (Server列表API)", config.http_bind);
    println!("    GET http://{}/api/health   (健康检查)", config.http_bind);
    println!("========================================");
    println!("");
    
    info!("启动 Webterm Hub...");
    info!("HTTP API 监听: http://{}", config.http_bind);
    info!("MQTT Broker: {}:{}", config.mqtt_host, config.mqtt_port);
    info!("心跳超时: {} 秒", config.heartbeat_timeout);

    let state = AppState::new(config.heartbeat_timeout);

    // 启动 MQTT 服务
    let mqtt_state = state.clone();
    let mqtt_state_for_client = state.clone();
    let mqtt_config_clone = HubConfig {
        http_bind: config.http_bind,
        mqtt_host: config.mqtt_host.clone(),
        mqtt_port: config.mqtt_port,
        mqtt_username: config.mqtt_username.clone(),
        mqtt_password: config.mqtt_password.clone(),
        heartbeat_timeout: config.heartbeat_timeout,
        cleanup_interval: config.cleanup_interval,
    };
    
    tokio::spawn(async move {
        if let Err(e) = run_mqtt(mqtt_state, &mqtt_config_clone, mqtt_state_for_client).await {
            warn!("MQTT 服务错误: {}", e);
        }
    });

    // 启动心跳清理任务
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        run_cleanup_task(cleanup_state, config.heartbeat_timeout, config.cleanup_interval).await;
    });

    // 启动 HTTP API
    let app = create_router(state);
    
    let listener = tokio::net::TcpListener::bind(config.http_bind).await
        .with_context(|| format!("无法绑定到地址: {}", config.http_bind))?;
    
    axum::serve(listener, app).await?;

    Ok(())
}

/// 创建 HTTP 路由
fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/", get(index_page))
        .route("/api/servers", get(list_servers))
        .route("/api/servers/:id", get(get_server))
        .route("/api/servers/:id/control", post(control_server))
        .route("/api/health", get(health_check))
        .layer(cors)
        .with_state(state)
}

/// 首页 - Server 列表页面
async fn index_page() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

/// 获取所有 Server 列表
async fn list_servers(State(state): State<AppState>) -> Json<Vec<ServerInfo>> {
    let servers = state.get_all_servers();
    Json(servers)
}

/// 获取单个 Server 详情
async fn get_server(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<ServerInfo>, StatusCode> {
    state
        .get_server(&id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// 健康检查
async fn health_check() -> &'static str {
    "OK"
}

/// 控制 Server（启动/停止 WebTerm）
async fn control_server(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::extract::Json(request): axum::extract::Json<ControlRequest>,
) -> Result<Json<ControlResponse>, StatusCode> {
    // 验证 server 是否存在
    if state.servers.get(&id).is_none() {
        return Ok(Json(ControlResponse {
            success: false,
            message: format!("Server {} 不存在或已离线", id),
        }));
    }

    info!("收到控制请求: server={}, command={:?}", id, request.command);

    // 通过 MQTT 发布控制命令到对应的 server
    match state.publish_control(&id, &request.command).await {
        Ok(_) => {
            let message = match &request.command {
                ControlCommand::Start { cmd, args } => {
                    format!("启动命令已发送到 server {}: cmd={:?}, args={:?}", id, cmd, args)
                }
                ControlCommand::Stop { webterm_id } => {
                    format!("停止命令已发送到 server {}: webterm_id={}", id, webterm_id)
                }
            };
            Ok(Json(ControlResponse {
                success: true,
                message,
            }))
        }
        Err(e) => Ok(Json(ControlResponse {
            success: false,
            message: format!("发送命令失败: {}", e),
        })),
    }
}

/// 检测 MQTT 连接
async fn check_mqtt_connection(config: &HubConfig) -> Result<(AsyncClient, EventLoop)> {
    let mut mqttoptions = MqttOptions::new("webterm-hub-check", &config.mqtt_host, config.mqtt_port);
    mqttoptions.set_keep_alive(Duration::from_secs(5));
    
    // 设置 MQTT 认证（如果配置了）
    if let (Some(ref user), Some(ref pass)) = (&config.mqtt_username, &config.mqtt_password) {
        mqttoptions.set_credentials(user, pass);
    }

    let (_client, mut eventloop) = AsyncClient::new(mqttoptions, 10);
    
    // 等待连接确认，超时 5 秒
    let timeout = tokio::time::Duration::from_secs(5);
    let check_future = async {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    return Ok(());
                }
                Ok(Event::Incoming(Packet::Publish(_))) => {}
                Ok(_) => {}
                Err(e) => {
                    return Err(anyhow::anyhow!("MQTT 连接错误: {}", e));
                }
            }
        }
    };
    
    match tokio::time::timeout(timeout, check_future).await {
        Ok(Ok(())) => {
            // 连接成功，重新创建客户端（因为 eventloop 已经被消费）
            let mut mqttoptions = MqttOptions::new("webterm-hub", &config.mqtt_host, config.mqtt_port);
            mqttoptions.set_keep_alive(Duration::from_secs(5));
            if let (Some(ref user), Some(ref pass)) = (&config.mqtt_username, &config.mqtt_password) {
                mqttoptions.set_credentials(user, pass);
            }
            let (client, eventloop) = AsyncClient::new(mqttoptions, 10);
            Ok((client, eventloop))
        }
        Ok(Err(e)) => Err(e),
        Err(_) => Err(anyhow::anyhow!("连接 MQTT Broker 超时（5秒）")),
    }
}

/// 运行 MQTT 服务
async fn run_mqtt(state: AppState, config: &HubConfig, app_state: AppState) -> Result<()> {
    info!("连接到 MQTT Broker: {}:{}", config.mqtt_host, config.mqtt_port);
    
    let mut mqttoptions = MqttOptions::new("webterm-hub", &config.mqtt_host, config.mqtt_port);
    mqttoptions.set_keep_alive(Duration::from_secs(5));
    
    // 设置 MQTT 认证（如果配置了）
    if let (Some(ref user), Some(ref pass)) = (&config.mqtt_username, &config.mqtt_password) {
        mqttoptions.set_credentials(user, pass);
    }

    let (client, mut eventloop) = AsyncClient::new(mqttoptions, 10);
    
    // 保存 MQTT 客户端到 AppState
    app_state.set_mqtt_client(client.clone()).await;

    // 订阅主题
    client
        .subscribe("webterm/hub/register", QoS::AtLeastOnce)
        .await?;
    client
        .subscribe("webterm/hub/heartbeat", QoS::AtLeastOnce)
        .await?;
    client
        .subscribe("webterm/hub/unregister", QoS::AtLeastOnce)
        .await?;

    info!("MQTT 已连接到 broker, 等待消息...");

    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Packet::Publish(publish))) => {
                let topic = publish.topic.clone();
                let payload = String::from_utf8_lossy(&publish.payload);
                info!("收到 MQTT 消息: topic={}, payload={}", topic, payload);

                match serde_json::from_str::<HubMessage>(&payload) {
                    Ok(msg) => {
                        info!("解析消息成功: {:?}", msg);
                        if topic == "webterm/hub/register" {
                            state.register_server(msg);
                        } else if topic == "webterm/hub/heartbeat" {
                            state.update_heartbeat(msg);
                        } else if topic == "webterm/hub/unregister" {
                            state.unregister_webterm(msg);
                        }
                    }
                    Err(e) => {
                        warn!("解析 MQTT 消息失败: {}, payload: {}", e, payload);
                    }
                }
            }
            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                info!("MQTT 连接确认");
            }
            Ok(_) => {}
            Err(e) => {
                warn!("MQTT 错误: {}, 5秒后重连...", e);
                sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

/// 清理离线 Server 的任务
async fn run_cleanup_task(state: AppState, heartbeat_timeout: i64, cleanup_interval: u64) {
    let mut ticker = interval(Duration::from_secs(cleanup_interval));

    loop {
        ticker.tick().await;

        let now = Utc::now();
        let mut offline_servers = Vec::new();
        
        // 清理每个 server 下离线的 webterms，并标记超时的 Server
        for mut entry in state.servers.iter_mut() {
            let server = entry.value_mut();
            let original_count = server.webterms.len();
            
            // 检查 Server 本身是否已超时
            let server_elapsed = now - server.last_heartbeat;
            let server_offline = server_elapsed.num_seconds() >= heartbeat_timeout;
            
            // 保留心跳超时时间内的 webterms
            server.webterms.retain(|wt| {
                let elapsed = now - wt.last_heartbeat;
                let keep = elapsed.num_seconds() < heartbeat_timeout;
                if !keep {
                    info!("清理离线 WebTerm: {} (Server: {})", wt.id, server.name);
                    state.webterms.remove(&wt.id);
                }
                keep
            });
            
            // 如果 server 超时了，或者没有 webterm 了，标记为待清理
            if server_offline || (server.webterms.is_empty() && original_count > 0) {
                offline_servers.push(server.id.clone());
            }
        }
        
        // 清理离线的 server
        for id in offline_servers {
            if let Some((_, server)) = state.servers.remove(&id) {
                info!("清理离线 Server: {} ({})", id, server.name);
            }
        }
    }
}
