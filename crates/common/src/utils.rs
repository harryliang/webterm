//! 网络工具模块
//!
//! 提供 IP 地址获取、端口查找等网络相关功能。
//! 配置项从配置文件读取。

use crate::config::NetworkConfig;
use anyhow::{Context, Result};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use log::{debug, info, warn};

/// 获取本机优先的 IP 地址
///
/// 根据配置中的 `preferred_ip_prefixes` 按优先级查找匹配的 IP。
/// 如果没有配置或找不到匹配的 IP，则返回任意可用 IP。
pub fn get_preferred_local_ip(config: &NetworkConfig) -> Result<Ipv4Addr> {
    // 获取所有本机 IP 地址
    let all_ips = get_all_local_ips()?;
    
    if all_ips.is_empty() {
        anyhow::bail!("未找到任何本机 IP 地址");
    }
    
    // 如果配置了优先前缀，按优先级查找
    if !config.preferred_ip_prefixes.is_empty() {
        for prefix in &config.preferred_ip_prefixes {
            if let Some(ip) = all_ips.iter().find(|ip| {
                ip.to_string().starts_with(prefix)
            }) {
                debug!("找到优先 IP 地址: {} (匹配前缀: {})", ip, prefix);
                return Ok(*ip);
            }
        }
        warn!("未找到匹配配置前缀的 IP，使用任意可用 IP");
    }
    
    // 返回第一个可用 IP（通常是路由优先级最高的）
    let selected = all_ips[0];
    info!("使用本机 IP 地址: {}", selected);
    Ok(selected)
}

/// 获取所有本机 IP 地址（按路由优先级排序）
fn get_all_local_ips() -> Result<Vec<Ipv4Addr>> {
    let mut ips = Vec::new();
    
    // 方法1: 使用 UDP socket 获取首选 IP（路由优先级最高的）
    if let Ok(ip) = get_default_route_ip() {
        if !ips.contains(&ip) {
            ips.push(ip);
        }
    }
    
    // 方法2: Windows 平台使用 ipconfig
    #[cfg(windows)]
    {
        if let Ok(win_ips) = get_windows_ips() {
            for ip in win_ips {
                if !ips.contains(&ip) {
                    ips.push(ip);
                }
            }
        }
    }
    
    // 方法3: Linux/macOS 使用 ifconfig 或 ip 命令
    #[cfg(not(windows))]
    {
        if let Ok(unix_ips) = get_unix_ips() {
            for ip in unix_ips {
                if !ips.contains(&ip) {
                    ips.push(ip);
                }
            }
        }
    }
    
    if ips.is_empty() {
        anyhow::bail!("未找到任何本机 IP 地址");
    }
    
    Ok(ips)
}

/// 获取默认路由 IP（路由优先级最高的）
fn get_default_route_ip() -> Result<Ipv4Addr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0")
        .context("无法创建 UDP socket")?;
    
    // 连接到一个公共地址（不会实际发送数据）
    socket.connect("8.8.8.8:80")
        .context("无法连接外部地址")?;
    
    let local_addr = socket.local_addr()
        .context("无法获取本地地址")?;
    
    if let SocketAddr::V4(addr) = local_addr {
        Ok(*addr.ip())
    } else {
        anyhow::bail!("获取到 IPv6 地址")
    }
}

/// Windows 平台获取 IP 地址列表
#[cfg(windows)]
fn get_windows_ips() -> Result<Vec<Ipv4Addr>> {
    let output = std::process::Command::new("powershell")
        .args(&[
            "-Command",
            "Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.IPAddress -notlike '127.*' } | Select-Object -ExpandProperty IPAddress"
        ])
        .output()
        .context("执行 PowerShell 命令失败")?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut ips = Vec::new();
    
    for line in stdout.lines() {
        let ip_str = line.trim();
        if !ip_str.is_empty() {
            if let Ok(ip) = ip_str.parse::<Ipv4Addr>() {
                if !ip.is_loopback() {
                    ips.push(ip);
                }
            }
        }
    }
    
    Ok(ips)
}

/// Unix 平台获取 IP 地址列表
#[cfg(not(windows))]
fn get_unix_ips() -> Result<Vec<Ipv4Addr>> {
    // 优先尝试 ip 命令
    let output = std::process::Command::new("sh")
        .args(&[
            "-c",
            "ip -4 addr show | grep -oP '(?<=inet\\s)\\d+(\\.\\d+){3}' | grep -v '^127\\.'"
        ])
        .output();
    
    let output = match output {
        Ok(out) if out.status.success() => out,
        _ => {
            // 回退到 ifconfig
            std::process::Command::new("sh")
                .args(&[
                    "-c",
                    "ifconfig | grep -oP '(?<=inet\\s)\\d+(\\.\\d+){3}' | grep -v '^127\\.'"
                ])
                .output()
                .context("无法获取 IP 地址")?
        }
    };
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut ips = Vec::new();
    
    for line in stdout.lines() {
        let ip_str = line.trim();
        if !ip_str.is_empty() {
            if let Ok(ip) = ip_str.parse::<Ipv4Addr>() {
                if !ip.is_loopback() {
                    ips.push(ip);
                }
            }
        }
    }
    
    Ok(ips)
}

/// 查找一个可用的端口（在指定 IP 上）
pub fn find_available_port_on_ip(ip: Ipv4Addr, start: u16, end: u16) -> Result<u16> {
    for port in start..=end {
        if let Ok(listener) = TcpListener::bind((ip, port)) {
            drop(listener);
            return Ok(port);
        }
    }
    anyhow::bail!("在范围 {}-{} 内未找到可用端口", start, end)
}

/// 查找一个可用的端口（在本地回环上，用于兼容性）
pub fn find_available_port(start: u16, end: u16) -> Result<u16> {
    for port in start..=end {
        if let Ok(listener) = TcpListener::bind((Ipv4Addr::new(127, 0, 0, 1), port)) {
            drop(listener);
            return Ok(port);
        }
    }
    anyhow::bail!("在范围 {}-{} 内未找到可用端口", start, end)
}

/// 检查端口是否可用
pub fn is_port_available(addr: &SocketAddr) -> bool {
    TcpListener::bind(addr).is_ok()
}

/// 获取推荐绑定地址（根据配置自动选择 IP + 可用端口）
pub fn get_recommended_bind_addr(config: &NetworkConfig) -> Result<SocketAddr> {
    let ip = get_preferred_local_ip(config)?;
    let port = find_available_port_on_ip(ip, config.port_start, config.port_end)?;
    Ok(SocketAddr::new(IpAddr::V4(ip), port))
}

/// 向后兼容的接口（使用默认配置）
/// 
/// # 警告
/// 此函数已弃用，请使用 `get_recommended_bind_addr(config)` 以获得更可控的行为。
#[deprecated(since = "0.2.0", note = "请使用 `get_recommended_bind_addr(config)` 替代")]
pub fn get_recommended_bind_addr_legacy() -> Result<SocketAddr> {
    let config = NetworkConfig::default();
    get_recommended_bind_addr(&config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_available_port() {
        let port = find_available_port(40000, 41000).unwrap();
        println!("Found available port: {}", port);
        assert!(port >= 40000 && port <= 41000);
    }
    
    #[test]
    fn test_get_all_local_ips() {
        let ips = get_all_local_ips().unwrap();
        println!("Local IPs: {:?}", ips);
        assert!(!ips.is_empty());
    }
    
    #[test]
    fn test_preferred_ip_selection() {
        let config = NetworkConfig {
            preferred_ip_prefixes: vec!["192.168.".to_string()],
            port_start: 30000,
            port_end: 40000,
            buffer_size: 8192,
        };
        
        // 如果存在 192.168.x.x 的 IP，应该返回它
        if let Ok(ip) = get_preferred_local_ip(&config) {
            println!("Preferred IP: {}", ip);
        }
    }
}
