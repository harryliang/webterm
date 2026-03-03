//! 配置文件管理模块
//!
//! 配置文件搜索路径（按优先级）：
//! 1. 命令行指定的路径 (--config)
//! 2. 当前目录: ./webterm.toml
//! 3. 用户配置目录:
//!    - Windows: %APPDATA%/webterm/config.toml
//!    - Linux/macOS: ~/.config/webterm/config.toml
//! 4. 系统配置目录:
//!    - Windows: %PROGRAMDATA%/webterm/config.toml
//!    - Linux: /etc/webterm/config.toml
//!    - macOS: /Library/Application Support/webterm/config.toml

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 默认配置文件名（webterm）
pub const DEFAULT_CONFIG_FILE_WEBTERM: &str = "webterm.toml";

/// 主配置结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// MQTT 配置
    #[serde(default)]
    pub mqtt: MqttConfig,
    
    /// Hub 配置
    #[serde(default)]
    pub hub: HubConfig,
    
    /// WebTerm 配置
    #[serde(default)]
    pub webterm: WebTermConfig,
    
    /// Session 配置
    #[serde(default)]
    pub session: SessionConfig,
    
    /// 网络配置
    #[serde(default)]
    pub network: NetworkConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mqtt: MqttConfig::default(),
            hub: HubConfig::default(),
            webterm: WebTermConfig::default(),
            session: SessionConfig::default(),
            network: NetworkConfig::default(),
        }
    }
}

impl Config {
    /// 从文件加载配置
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("无法读取配置文件: {}", path.as_ref().display()))?;
        
        let config: Config = toml::from_str(&content)
            .with_context(|| format!("解析配置文件失败: {}", path.as_ref().display()))?;
        
        Ok(config)
    }
    
    /// 保存配置到文件
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .context("序列化配置失败")?;
        
        std::fs::write(&path, content)
            .with_context(|| format!("无法写入配置文件: {}", path.as_ref().display()))?;
        
        Ok(())
    }
    
    /// 搜索并加载配置文件
    pub fn load() -> Result<Self> {
        // 1. 尝试从环境变量 PORTMAP_CONFIG 指定的路径加载
        if let Ok(config_path) = std::env::var("PORTMAP_CONFIG") {
            if Path::new(&config_path).exists() {
                log::info!("从环境变量 PORTMAP_CONFIG 加载配置: {}", config_path);
                return Self::from_file(&config_path);
            }
        }
        
        // 2. 尝试当前目录（webterm.toml）
        let current_dir = std::env::current_dir()?;
        let current_config = current_dir.join(DEFAULT_CONFIG_FILE_WEBTERM);
        if current_config.exists() {
            log::info!("从当前目录加载配置: {}", current_config.display());
            return Self::from_file(&current_config);
        }
        let current_config = current_dir.join(DEFAULT_CONFIG_FILE_WEBTERM);
        if current_config.exists() {
            log::info!("从当前目录加载配置: {}", current_config.display());
            return Self::from_file(&current_config);
        }
        
        // 3. 尝试用户配置目录
        if let Some(user_config) = get_user_config_path() {
            if user_config.exists() {
                log::info!("从用户目录加载配置: {}", user_config.display());
                return Self::from_file(&user_config);
            }
        }
        
        // 4. 尝试系统配置目录
        if let Some(system_config) = get_system_config_path() {
            if system_config.exists() {
                log::info!("从系统目录加载配置: {}", system_config.display());
                return Self::from_file(&system_config);
            }
        }
        
        // 没有找到配置文件，返回默认配置
        log::info!("未找到配置文件，使用默认配置");
        Ok(Self::default())
    }
    
    /// 创建默认配置文件到用户目录
    pub fn create_default_config() -> Result<PathBuf> {
        let config_path = get_user_config_path()
            .context("无法确定用户配置目录")?;
        
        // 确保目录存在
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let config = Self::default();
        config.save_to_file(&config_path)?;
        
        Ok(config_path)
    }
}

/// MQTT 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttConfig {
    /// MQTT Broker 地址
    #[serde(default = "default_mqtt_host")]
    pub host: String,
    
    /// MQTT Broker 端口
    #[serde(default = "default_mqtt_port")]
    pub port: u16,
    
    /// MQTT 用户名（可选）
    pub username: Option<String>,
    
    /// MQTT 密码（可选）
    pub password: Option<String>,
    
    /// 默认 Topic
    #[serde(default = "default_mqtt_topic")]
    pub topic: String,
    
    /// 用于 Topic 编码的密钥（可选）
    pub secret_key: Option<String>,
    
    /// 保持连接间隔（秒）
    #[serde(default = "default_keep_alive")]
    pub keep_alive: u64,
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            host: default_mqtt_host(),
            port: default_mqtt_port(),
            username: None,
            password: None,
            topic: default_mqtt_topic(),
            secret_key: None,
            keep_alive: default_keep_alive(),
        }
    }
}

fn default_mqtt_host() -> String {
    std::env::var("MQTT_BROKER")
        .or_else(|_| std::env::var("MQTT_HOST"))
        .unwrap_or_else(|_| "localhost".to_string())
}

fn default_mqtt_port() -> u16 {
    std::env::var("MQTT_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(1883)
}

fn default_mqtt_topic() -> String {
    std::env::var("MQTT_TOPIC")
        .unwrap_or_else(|_| "webterm".to_string())
}

fn default_keep_alive() -> u64 {
    5
}

/// Hub 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubConfig {
    /// Hub HTTP API 监听地址
    #[serde(default = "default_hub_bind")]
    pub bind: SocketAddr,
    
    /// 心跳超时时间（秒）
    #[serde(default = "default_heartbeat_timeout")]
    pub heartbeat_timeout: i64,
    
    /// 心跳发送间隔（秒）
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval: u64,
    
    /// 清理任务间隔（秒）
    #[serde(default = "default_cleanup_interval")]
    pub cleanup_interval: u64,
}

impl Default for HubConfig {
    fn default() -> Self {
        Self {
            bind: default_hub_bind(),
            heartbeat_timeout: default_heartbeat_timeout(),
            heartbeat_interval: default_heartbeat_interval(),
            cleanup_interval: default_cleanup_interval(),
        }
    }
}

fn default_hub_bind() -> SocketAddr {
    "0.0.0.0:8080".parse().unwrap()
}

fn default_heartbeat_timeout() -> i64 {
    90
}

fn default_heartbeat_interval() -> u64 {
    30
}

fn default_cleanup_interval() -> u64 {
    30
}

/// WebTerm 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebTermConfig {
    /// 默认命令（Windows）
    #[serde(default = "default_windows_cmd")]
    pub windows_cmd: String,
    
    /// 默认命令（Linux/Mac）
    #[serde(default = "default_unix_cmd")]
    pub unix_cmd: String,
    
    /// 自动重连间隔（毫秒）
    #[serde(default = "default_reconnect_interval")]
    pub reconnect_interval: u64,
    
    /// 心跳检测间隔（毫秒）
    #[serde(default = "default_ws_heartbeat_interval")]
    pub ws_heartbeat_interval: u64,
    
    /// 心跳超时时间（毫秒）
    #[serde(default = "default_ws_heartbeat_timeout")]
    pub ws_heartbeat_timeout: u64,
}

impl Default for WebTermConfig {
    fn default() -> Self {
        Self {
            windows_cmd: default_windows_cmd(),
            unix_cmd: default_unix_cmd(),
            reconnect_interval: default_reconnect_interval(),
            ws_heartbeat_interval: default_ws_heartbeat_interval(),
            ws_heartbeat_timeout: default_ws_heartbeat_timeout(),
        }
    }
}

fn default_windows_cmd() -> String {
    "cmd.exe".to_string()
}

fn default_unix_cmd() -> String {
    "/bin/bash".to_string()
}

fn default_reconnect_interval() -> u64 {
    5000
}

fn default_ws_heartbeat_interval() -> u64 {
    30000
}

fn default_ws_heartbeat_timeout() -> u64 {
    90000
}

/// Session 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// 最大历史行数
    #[serde(default = "default_max_history_lines")]
    pub max_history_lines: usize,
    
    /// 会话超时时间（秒）
    #[serde(default = "default_session_timeout")]
    pub timeout_secs: u64,
    
    /// 最大会话数
    #[serde(default = "default_max_sessions")]
    pub max_sessions: usize,
    
    /// 终端默认行数
    #[serde(default = "default_terminal_rows")]
    pub default_rows: u16,
    
    /// 终端默认列数
    #[serde(default = "default_terminal_cols")]
    pub default_cols: u16,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            max_history_lines: default_max_history_lines(),
            timeout_secs: default_session_timeout(),
            max_sessions: default_max_sessions(),
            default_rows: default_terminal_rows(),
            default_cols: default_terminal_cols(),
        }
    }
}

fn default_max_history_lines() -> usize {
    10000
}

fn default_session_timeout() -> u64 {
    3600
}

fn default_max_sessions() -> usize {
    10
}

fn default_terminal_rows() -> u16 {
    24
}

fn default_terminal_cols() -> u16 {
    80
}

/// 网络配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// 自动绑定时优先使用的 IP 前缀列表（按优先级排序）
    /// 例如: ["10.126.", "192.168.", "10.0."]
    /// 如果为空或找不到匹配的 IP，则使用任意可用 IP
    #[serde(default)]
    pub preferred_ip_prefixes: Vec<String>,
    
    /// 自动绑定时的起始端口
    #[serde(default = "default_port_start")]
    pub port_start: u16,
    
    /// 自动绑定时的结束端口
    #[serde(default = "default_port_end")]
    pub port_end: u16,
    
    /// 端口转发缓冲区大小（字节）
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            preferred_ip_prefixes: vec!["10.126.".to_string()],
            port_start: default_port_start(),
            port_end: default_port_end(),
            buffer_size: default_buffer_size(),
        }
    }
}

fn default_port_start() -> u16 {
    30000
}

fn default_port_end() -> u16 {
    40000
}

fn default_buffer_size() -> usize {
    8192
}

/// 获取用户配置目录
#[cfg(target_os = "windows")]
fn get_user_config_path() -> Option<PathBuf> {
    std::env::var("APPDATA")
        .ok()
        .map(|app_data| PathBuf::from(app_data).join("webterm").join("config.toml"))
}

#[cfg(target_os = "macos")]
fn get_user_config_path() -> Option<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".config").join("webterm").join("config.toml"))
}

#[cfg(target_os = "linux")]
fn get_user_config_path() -> Option<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".config").join("webterm").join("config.toml"))
}

/// 获取系统配置目录
#[cfg(target_os = "windows")]
fn get_system_config_path() -> Option<PathBuf> {
    std::env::var("PROGRAMDATA")
        .ok()
        .map(|pd| PathBuf::from(pd).join("webterm").join("config.toml"))
}

#[cfg(target_os = "macos")]
fn get_system_config_path() -> Option<PathBuf> {
    Some(PathBuf::from("/Library/Application Support/webterm/config.toml"))
}

#[cfg(target_os = "linux")]
fn get_system_config_path() -> Option<PathBuf> {
    Some(PathBuf::from("/etc/webterm/config.toml"))
}

// 对于其他平台，使用 Linux 默认
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn get_user_config_path() -> Option<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".config").join("webterm").join("config.toml"))
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn get_system_config_path() -> Option<PathBuf> {
    Some(PathBuf::from("/etc/webterm/config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.mqtt.port, 1883);
        assert_eq!(config.hub.heartbeat_timeout, 90);
        assert_eq!(config.session.max_sessions, 10);
    }
    
    #[test]
    fn test_config_serialize() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        println!("{}", toml_str);
        
        // 验证可以反序列化
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.mqtt.port, config.mqtt.port);
    }
}
