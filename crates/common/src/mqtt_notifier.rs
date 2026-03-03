//! MQTT 通知模块
//!
//! 用于发送 WebTerm 启动通知到 MQTT Broker。
//! 所有配置优先从配置文件读取，其次从环境变量读取。

use crate::config::MqttConfig;
use log::{info, warn, error};
use serde_json::json;
use std::net::SocketAddr;
use std::time::Duration;
use rumqttc::{MqttOptions, Client, QoS, Event, Packet, ConnectionError};
use std::thread;
use std::sync::{Arc, Mutex};

/// MQTT 通知器
pub struct MqttNotifier {
    config: MqttConfig,
}

impl MqttNotifier {
    /// 从配置创建 MQTT 通知器
    pub fn new(config: MqttConfig) -> Self {
        Self { config }
    }
    
    /// 从环境变量创建（向后兼容）
    pub fn from_env() -> Option<Self> {
        let host = std::env::var("MQTT_BROKER")
            .or_else(|_| std::env::var("MQTT_HOST"))
            .ok()?;
        let port = std::env::var("MQTT_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(1883);
        let topic = std::env::var("MQTT_TOPIC").unwrap_or_else(|_| "webterm/notifications".to_string());
        let username = std::env::var("MQTT_USERNAME").ok();
        let password = std::env::var("MQTT_PASSWORD").ok();
        let secret_key = std::env::var("MQTT_SECRET_KEY").ok();
        let keep_alive = std::env::var("MQTT_KEEP_ALIVE")
            .ok()
            .and_then(|k| k.parse().ok())
            .unwrap_or(5);
        
        Some(Self::new(MqttConfig {
            host,
            port,
            username,
            password,
            topic,
            secret_key,
            keep_alive,
        }))
    }
    
    /// 对 topic 进行编码（如果配置了 secret_key）
    fn encode_topic(&self, topic: &str) -> String {
        if let Some(ref secret) = self.config.secret_key {
            let s = format!("{}{}", topic, secret);
            format!("{:x}", md5::compute(s.as_bytes()))
        } else {
            topic.to_string()
        }
    }
    
    /// 发送消息到 MQTT
    pub fn send(&self, topic: &str, msg: &str) -> anyhow::Result<()> {
        let encoded_topic = self.encode_topic(topic);
        
        info!("连接 MQTT Broker: {}:{}", self.config.host, self.config.port);
        info!("发送消息到 topic: {} (编码后: {})", topic, encoded_topic);
        
        // 配置 MQTT 选项
        let mut mqttoptions = MqttOptions::new(
            format!("webterm_{}", uuid::Uuid::new_v4()),
            &self.config.host,
            self.config.port.into()
        );
        
        // 设置认证信息（如果配置了）
        if let (Some(ref user), Some(ref pass)) = (&self.config.username, &self.config.password) {
            mqttoptions.set_credentials(user, pass);
        }
        
        mqttoptions.set_keep_alive(Duration::from_secs(self.config.keep_alive));
        mqttoptions.set_clean_session(true);
        
        // 创建客户端
        let (client, mut connection) = Client::new(mqttoptions, 10);
        
        // 使用共享状态来跟踪连接状态
        let connected = Arc::new(Mutex::new(false));
        let connected_clone = connected.clone();
        
        // 在后台线程中处理连接事件
        let handle = thread::spawn(move || {
            for notification in connection.iter() {
                match notification {
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        info!("MQTT 连接已确认");
                        *connected_clone.lock().unwrap() = true;
                    }
                    Ok(Event::Incoming(Packet::Publish(p))) => {
                        info!("收到消息: {:?}", p);
                    }
                    Err(ConnectionError::Io(e)) => {
                        error!("MQTT IO 错误: {:?}", e);
                        return Err(ConnectionError::Io(e));
                    }
                    Err(e) => {
                        error!("MQTT 连接错误: {:?}", e);
                        return Err(e);
                    }
                    _ => {}
                }
            }
            Ok(())
        });
        
        // 等待连接建立（最多 3 秒）
        let start = std::time::Instant::now();
        loop {
            if *connected.lock().unwrap() {
                break;
            }
            if start.elapsed() > Duration::from_secs(3) {
                warn!("等待连接超时，尝试直接发送...");
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        
        // 发布消息
        let result = client.publish(&encoded_topic, QoS::AtMostOnce, false, msg.as_bytes());
        
        // 等待消息发送完成
        thread::sleep(Duration::from_millis(300));
        
        // 断开连接
        let _ = client.disconnect();
        
        // 检查发布结果
        match result {
            Ok(_) => {
                info!("MQTT 消息发送成功");
                Ok(())
            }
            Err(e) => {
                error!("发布失败: {:?}", e);
                // 等待后台线程完成，检查是否有连接错误
                drop(client);
                match handle.join() {
                    Ok(Err(ConnectionError::Io(io_err))) => {
                        Err(anyhow::anyhow!("MQTT 连接失败: {}，请检查 broker 地址、端口、用户名密码是否正确", io_err))
                    }
                    Ok(Err(other)) => {
                        Err(anyhow::anyhow!("MQTT 错误: {:?}", other))
                    }
                    _ => Err(e.into())
                }
            }
        }
    }
    
    /// 发送 WebTerm 启动通知
    pub fn notify_webterm_started(&self, bind_addr: &SocketAddr, command: &str) -> anyhow::Result<()> {
        let url = format!("http://{}", bind_addr);
        let payload = json!({
            "event": "webterm_started",
            "ip": bind_addr.ip().to_string(),
            "port": bind_addr.port(),
            "url": url,
            "command": command,
            "timestamp": chrono::Local::now().to_rfc3339(),
        });
        
        info!("发送 MQTT 通知到 {}:{}/{}", 
            self.config.host, self.config.port, self.config.topic);
        info!("消息内容: {}", payload.to_string());
        
        self.send(&self.config.topic, &payload.to_string())
    }
}

/// 发送 WebTerm 启动通知（使用配置，向后兼容的简化接口）
/// 
/// 如果提供了配置则使用配置，否则尝试从环境变量读取
pub fn notify_webterm_started(
    bind_addr: &SocketAddr, 
    command: &str,
    config: Option<&MqttConfig>,
) {
    // 尝试获取 MQTT 配置
    let notifier = if let Some(cfg) = config {
        Some(MqttNotifier::new(cfg.clone()))
    } else {
        MqttNotifier::from_env()
    };
    
    let Some(notifier) = notifier else {
        warn!("未配置 MQTT，跳过通知");
        return;
    };
    
    // 尝试使用 Python paho-mqtt 发送
    if let Err(e) = try_python_paho(&notifier, bind_addr, command) {
        warn!("Python paho-mqtt 失败: {}，尝试 Rust 客户端...", e);
        
        // 回退到 Rust MQTT 客户端
        if let Err(e) = notifier.notify_webterm_started(bind_addr, command) {
            warn!("MQTT 通知失败: {}", e);
        }
    }
}

/// 使用 Python paho-mqtt 发送消息（与参考实现 mqtt_utils.py 一致）
fn try_python_paho(
    notifier: &MqttNotifier,
    bind_addr: &SocketAddr,
    command: &str,
) -> anyhow::Result<()> {
    use std::process::Command;
    
    let url = format!("http://{}", bind_addr);
    let payload = json!({
        "event": "webterm_started",
        "ip": bind_addr.ip().to_string(),
        "port": bind_addr.port(),
        "url": url,
        "command": command,
        "timestamp": chrono::Local::now().to_rfc3339(),
    });
    
    // 构建 Python 脚本
    let secret_key_part = if let Some(ref secret) = notifier.config.secret_key {
        format!("key='{}';", secret)
    } else {
        "key='';".to_string()
    };
    
    let auth_part = if let (Some(ref user), Some(ref pass)) = (&notifier.config.username, &notifier.config.password) {
        format!(", auth={{'username':'{}','password':'{}'}}", user, pass)
    } else {
        String::new()
    };
    
    let python_script = format!(
        r#"import paho.mqtt.publish as publish; import hashlib; {} topic='{}'; encoded=hashlib.md5((topic+key).encode()).hexdigest() if key else topic; publish.single(encoded, '''{}''', hostname='{}', port={}{})"#,
        secret_key_part,
        notifier.config.topic,
        payload.to_string().replace("'", "\\'"),
        notifier.config.host,
        notifier.config.port,
        auth_part
    );
    
    info!("尝试使用 Python paho-mqtt 发送...");
    
    // 尝试 python
    let output = Command::new("python")
        .args(&["-c", &python_script])
        .output();
    
    if let Ok(result) = output {
        if result.status.success() {
            info!("MQTT 消息发送成功 (Python paho-mqtt)");
            return Ok(());
        } else {
            let stderr = String::from_utf8_lossy(&result.stderr);
            warn!("Python 发送失败: {}", stderr);
        }
    }
    
    // 尝试 python3
    let output = Command::new("python3")
        .args(&["-c", &python_script])
        .output();
    
    match output {
        Ok(result) if result.status.success() => {
            info!("MQTT 消息发送成功 (Python3 paho-mqtt)");
            Ok(())
        }
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr);
            warn!("Python3 发送失败: {}", stderr);
            Err(anyhow::anyhow!("Python paho-mqtt 失败: {}", stderr))
        }
        Err(e) => {
            warn!("无法启动 Python: {}", e);
            Err(anyhow::anyhow!("无法启动 Python: {}", e))
        }
    }
}

/// 向后兼容的接口（使用环境变量）
pub fn notify_webterm_started_env(bind_addr: &SocketAddr, command: &str) {
    notify_webterm_started(bind_addr, command, None);
}

/// 使用 TCP 直接发送简单的通知消息
pub fn notify_via_tcp(bind_addr: &SocketAddr, target: &str) -> anyhow::Result<()> {
    use std::io::Write;
    use std::net::TcpStream;
    
    let message = format!(
        "WEBTERM_STARTED|{}|{}|http://{}\n",
        bind_addr.ip(),
        bind_addr.port(),
        bind_addr
    );
    
    let parts: Vec<&str> = target.split(':').collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!("目标地址格式错误，应为 IP:PORT"));
    }
    
    let ip = parts[0];
    let port: u16 = parts[1].parse()?;
    
    match TcpStream::connect((ip, port)) {
        Ok(mut stream) => {
            stream.write_all(message.as_bytes())?;
            stream.flush()?;
            info!("TCP 通知已发送到 {}", target);
            Ok(())
        }
        Err(e) => {
            Err(anyhow::anyhow!("TCP 连接失败: {}", e))
        }
    }
}
