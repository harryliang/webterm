//! Hub 客户端模块
//!
//! 用于将 WebTerm 注册到 Hub 中心服务，实现多 Server 管理。

use crate::config::MqttConfig;
use anyhow::Result;
use log::{info, warn};
use rumqttc::{AsyncClient, MqttOptions, QoS};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::interval;
use uuid::Uuid;

/// 控制命令（从 Hub 接收）
#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum ControlCommand {
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

/// 控制命令处理器
type ControlCommandHandler = Box<dyn Fn(ControlCommand) + Send + Sync>;

/// 控制命令发送器
type ControlCommandSender = tokio::sync::mpsc::Sender<ControlCommand>;

/// Hub 客户端
pub struct HubClient {
    client: AsyncClient,
    server_id: String,
    server_name: String,
    user: String,
    hostname: String,
    work_dir: String,
    mqtt_config: MqttConfig,
    control_tx: Option<ControlCommandSender>,
    active_webterms: Arc<Mutex<HashSet<String>>>,
    base_url: String,
}

#[derive(Serialize)]
struct RegisterMessage {
    #[serde(rename = "type")]
    msg_type: String,
    server_id: String,
    server_name: String,
    user: String,
    hostname: String,
    webterm: WebTermInfo,
}

#[derive(Serialize)]
struct WebTermInfo {
    id: String,
    url: String,
    command: String,
    #[serde(rename = "cwd")]
    work_dir: String,
}

#[derive(Serialize)]
struct HeartbeatMessage {
    #[serde(rename = "type")]
    msg_type: String,
    server_id: String,
    active_webterms: Vec<String>,
}

#[derive(Serialize)]
struct UnregisterMessage {
    #[serde(rename = "type")]
    msg_type: String,
    server_id: String,
    webterm_id: String,
}

/// 检测 MQTT 连接
async fn check_mqtt_connection(host: &str, port: u16, mqtt_config: Option<&MqttConfig>) -> Result<()> {
    let check_client_id = format!("webterm-check-{}", Uuid::new_v4().simple());
    let mut mqttoptions = MqttOptions::new(&check_client_id, host, port);
    mqttoptions.set_keep_alive(Duration::from_secs(5));
    
    // 设置 MQTT 认证信息
    if let Some(config) = mqtt_config {
        if let (Some(ref username), Some(ref password)) = (&config.username, &config.password) {
            mqttoptions.set_credentials(username, password);
        }
    } else if let (Ok(user), Ok(pass)) = (
        std::env::var("HUB_MQTT_USER"),
        std::env::var("HUB_MQTT_PASS")
    ) {
        mqttoptions.set_credentials(user, pass);
    }

    let (_client, mut eventloop) = AsyncClient::new(mqttoptions, 10);
    
    // 等待连接确认，超时 5 秒
    let timeout = tokio::time::Duration::from_secs(5);
    let check_future = async {
        loop {
            match eventloop.poll().await {
                Ok(rumqttc::Event::Incoming(rumqttc::Packet::ConnAck(_))) => {
                    return Ok(());
                }
                Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(_))) => {}
                Ok(_) => {}
                Err(e) => {
                    return Err(anyhow::anyhow!("MQTT 连接错误: {}", e));
                }
            }
        }
    };
    
    match tokio::time::timeout(timeout, check_future).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(anyhow::anyhow!("连接 MQTT Broker 超时（5秒）")),
    }
}

impl HubClient {
    /// 创建并连接到 Hub
    /// 
    /// # 参数
    /// - `hub_addr`: Hub MQTT 地址，格式 "host:port"
    /// - `server_name`: Server 显示名称
    /// - `base_url`: 基础 URL 地址
    /// - `mqtt_config`: MQTT 配置（包含认证信息）
    /// 
    /// # 返回
    /// (HubClient, webterm_id, server_id, server_name, user, hostname, control_rx)
    pub async fn new(
        hub_addr: &str,
        server_name: Option<String>,
        base_url: &str,
        mqtt_config: Option<&MqttConfig>,
    ) -> Result<(Self, String, String, String, String, String, tokio::sync::mpsc::Receiver<ControlCommand>)> {
        let server_name = server_name.unwrap_or_else(|| {
            format!("{}的电脑", whoami::username())
        });

        let user = whoami::username();
        let hostname = gethostname::gethostname().to_string_lossy().to_string();
        
        // 获取当前工作目录
        let cwd = std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        
        // 生成确定性 Server ID（基于 hostname + username + server_name）
        // 这样同一个机器的多个 webterm 会合并到同一个 server 下
        let server_id_input = format!("{}:{}:{}", hostname, user, server_name);
        let server_id = format!("{:x}", md5::compute(&server_id_input));
        
        // WebTerm ID 仍然需要唯一，用于区分不同端口
        let webterm_id = format!("wt-{}", Uuid::new_v4().simple());

        // 解析 Hub 地址
        let (host, port) = if hub_addr.contains(':') {
            let parts: Vec<&str> = hub_addr.split(':').collect();
            (parts[0].to_string(), parts[1].parse::<u16>()?)
        } else {
            (hub_addr.to_string(), 1883u16)
        };

        // 先检测 MQTT 连接
        check_mqtt_connection(&host, port, mqtt_config).await.map_err(|e| {
            eprintln!("");
            eprintln!("========================================");
            eprintln!("错误: 无法连接到 Hub 的 MQTT Broker!");
            eprintln!("========================================");
            eprintln!("  Hub 地址: {}:{}", host, port);
            eprintln!("  原因: {}", e);
            eprintln!("");
            eprintln!("请检查:");
            eprintln!("  1. Hub 服务是否已启动 (webterm-hub.exe)");
            eprintln!("  2. MQTT Broker 是否在运行");
            eprintln!("  3. 地址和端口是否正确");
            eprintln!("");
            eprintln!("启动 MQTT Broker:");
            eprintln!("  - Docker: docker run -d -p 1883:1883 eclipse-mosquitto");
            eprintln!("========================================");
            e
        })?;

        info!("连接到 Hub: {}:{}", host, port);
        info!("Server ID: {}, WebTerm ID: {}", server_id, webterm_id);

        // MQTT Client ID 必须是唯一的，否则会发生连接冲突
        // 使用 webterm_id 作为 client ID，但注册消息中仍使用 server_id
        let mqtt_client_id = format!("{}-{}", server_id, webterm_id);
        
        // 创建 MQTT 客户端选项
        let mut mqttoptions = MqttOptions::new(&mqtt_client_id, host, port);
        mqttoptions.set_keep_alive(Duration::from_secs(
            mqtt_config.map(|c| c.keep_alive).unwrap_or(5)
        ));
        
        // 设置 MQTT 认证信息（优先从 mqtt_config，其次从单独的环境变量）
        if let Some(config) = mqtt_config {
            if let (Some(ref username), Some(ref password)) = (&config.username, &config.password) {
                mqttoptions.set_credentials(username, password);
            }
        } else if let (Ok(user), Ok(pass)) = (
            std::env::var("HUB_MQTT_USER"),
            std::env::var("HUB_MQTT_PASS")
        ) {
            mqttoptions.set_credentials(user, pass);
        }

        let (client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

        let mqtt_config_clone = mqtt_config.cloned().unwrap_or_default();

        // 创建控制命令通道
        let (control_tx, control_rx) = tokio::sync::mpsc::channel::<ControlCommand>(10);

        let client_wrapper = Self {
            client: client.clone(),
            server_id: server_id.clone(),
            server_name: server_name.clone(),
            user: user.clone(),
            hostname: hostname.clone(),
            work_dir: cwd,
            mqtt_config: mqtt_config_clone,
            control_tx: Some(control_tx.clone()),
            active_webterms: Arc::new(Mutex::new(HashSet::new())),
            base_url: base_url.to_string(),
        };

        // 订阅控制命令主题
        let control_topic = format!("webterm/server/{}/control", server_id);
        client.subscribe(&control_topic, QoS::AtLeastOnce).await?;
        info!("已订阅控制命令主题: {}", control_topic);

        // 在后台运行事件循环，处理控制命令
        let server_id_clone = server_id.clone();
        tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(publish))) => {
                        let topic = publish.topic.clone();
                        let payload = String::from_utf8_lossy(&publish.payload);
                        
                        // 检查是否是控制命令
                        if topic == format!("webterm/server/{}/control", server_id_clone) {
                            match serde_json::from_str::<ControlCommand>(&payload) {
                                Ok(cmd) => {
                                    info!("收到控制命令: {:?}", cmd);
                                    if let Err(e) = control_tx.send(cmd).await {
                                        warn!("发送控制命令到通道失败: {}", e);
                                    }
                                }
                                Err(e) => {
                                    warn!("解析控制命令失败: {}, payload: {}", e, payload);
                                }
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        warn!("MQTT 事件循环错误: {}", e);
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        });

        Ok((client_wrapper, webterm_id, server_id, server_name, user, hostname, control_rx))
    }

    /// 注册 WebTerm
    pub async fn register(&self, webterm_id: &str, command: &str) -> Result<()> {
        // 添加到活跃 webterms 列表
        {
            let mut webterms = self.active_webterms.lock().unwrap();
            webterms.insert(webterm_id.to_string());
        }
        
        // 构建带 session 参数的 URL，让手机端可以直接访问特定会话
        let url = format!("{}?session={}", self.base_url, webterm_id);
        
        let msg = RegisterMessage {
            msg_type: "register".to_string(),
            server_id: self.server_id.clone(),
            server_name: self.server_name.clone(),
            user: self.user.clone(),
            hostname: self.hostname.clone(),
            webterm: WebTermInfo {
                id: webterm_id.to_string(),
                url,
                command: command.to_string(),
                work_dir: self.work_dir.clone(),
            },
        };

        let payload = serde_json::to_string(&msg)?;
        self.client
            .publish("webterm/hub/register", QoS::AtLeastOnce, false, payload)
            .await?;

        info!("已向 Hub 注册 WebTerm: {}", webterm_id);
        Ok(())
    }
    
    /// 注销 WebTerm
    pub async fn unregister(&self, webterm_id: &str) -> Result<()> {
        // 从活跃 webterms 列表中移除
        {
            let mut webterms = self.active_webterms.lock().unwrap();
            webterms.remove(webterm_id);
        }
        
        let msg = UnregisterMessage {
            msg_type: "unregister".to_string(),
            server_id: self.server_id.clone(),
            webterm_id: webterm_id.to_string(),
        };

        let payload = serde_json::to_string(&msg)?;
        self.client
            .publish("webterm/hub/unregister", QoS::AtLeastOnce, false, payload)
            .await?;

        info!("已向 Hub 注销 WebTerm: {}", webterm_id);
        Ok(())
    }

    /// 启动心跳任务
    pub fn start_heartbeat(&self) {
        let client = self.client.clone();
        let server_id = self.server_id.clone();
        let heartbeat_interval = self.mqtt_config.keep_alive.max(30);
        let active_webterms = self.active_webterms.clone();
        
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(heartbeat_interval));

            loop {
                ticker.tick().await;
                
                // 获取所有活跃的 webterms
                let webterms: Vec<String> = {
                    let guard = active_webterms.lock().unwrap();
                    guard.iter().cloned().collect()
                };

                let msg = HeartbeatMessage {
                    msg_type: "heartbeat".to_string(),
                    server_id: server_id.clone(),
                    active_webterms: webterms,
                };

                match serde_json::to_string(&msg) {
                    Ok(payload) => {
                        if let Err(e) = client
                            .publish("webterm/hub/heartbeat", QoS::AtLeastOnce, false, payload)
                            .await
                        {
                            warn!("发送心跳失败: {}", e);
                        }
                    }
                    Err(e) => warn!("序列化心跳消息失败: {}", e),
                }
            }
        });
    }
}
