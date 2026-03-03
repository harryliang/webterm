use anyhow::Result;
use clap::Parser;
use webterm_common::config::Config;
use webterm_common::utils;

mod webterm;
use webterm::run_webterm;

#[derive(Parser, Debug)]
#[command(name = "webterm")]
#[command(about = "Web 终端工具 - 通过浏览器访问命令行程序")]
#[command(version)]
struct Cli {
    /// 配置文件路径
    #[arg(long, value_name = "PATH", help = "指定配置文件路径")]
    config: Option<String>,
    
    /// 生成默认配置文件
    #[arg(long, help = "生成默认配置文件到用户配置目录")]
    init_config: bool,
    
    /// Web 服务监听地址，例如: 192.168.1.100:30000
    /// 如果不指定，自动查找可用 IP 和端口
    #[arg(short, long, value_name = "ADDR")]
    bind: Option<String>,

    /// 要运行的命令（默认根据系统: Windows=cmd.exe, Unix=/bin/bash）
    #[arg(short, long, value_name = "CMD")]
    command: Option<String>,

    /// 命令参数
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,

    /// 启用 MQTT 通知（需要配置 MQTT）
    #[arg(long, help = "启用 MQTT 通知")]
    mqtt: bool,

    /// MQTT Broker 地址（覆盖配置文件）
    #[arg(long, value_name = "HOST", help = "MQTT Broker 地址")]
    mqtt_broker: Option<String>,

    /// MQTT Broker 端口（覆盖配置文件）
    #[arg(long, value_name = "PORT", help = "MQTT Broker 端口")]
    mqtt_port: Option<u16>,

    /// MQTT 主题（覆盖配置文件）
    #[arg(long, value_name = "TOPIC", help = "MQTT 主题")]
    mqtt_topic: Option<String>,

    /// Hub 中心服务地址（启用多 Server 管理）
    #[arg(long, value_name = "ADDR", help = "Hub MQTT 地址，如 192.168.1.100:1883")]
    hub: Option<String>,

    /// Server 显示名称
    #[arg(long, value_name = "NAME", help = "在 Hub 中显示的 Server 名称")]
    server_name: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let cli = Cli::parse();
    
    // 处理生成配置文件命令
    if cli.init_config {
        match Config::create_default_config() {
            Ok(path) => {
                println!("默认配置文件已生成: {}", path.display());
                println!("您可以编辑此文件来自定义配置。");
                return Ok(());
            }
            Err(e) => {
                eprintln!("生成配置文件失败: {}", e);
                std::process::exit(1);
            }
        }
    }
    
    // 加载配置
    let config = if let Some(config_path) = &cli.config {
        Config::from_file(config_path)?
    } else {
        Config::load()?
    };
    
    // 构建最终配置（命令行参数覆盖配置文件）
    let mut final_config = config.clone();
    
    if let Some(broker) = cli.mqtt_broker {
        final_config.mqtt.host = broker;
    }
    if let Some(port) = cli.mqtt_port {
        final_config.mqtt.port = port;
    }
    if let Some(topic) = cli.mqtt_topic {
        final_config.mqtt.topic = topic;
    }
    
    // 如果没有指定 bind，自动获取推荐地址
    let bind_addr = match cli.bind {
        Some(addr) => addr,
        None => {
            log::info!("未指定绑定地址，自动查找可用 IP 和端口...");
            let addr = utils::get_recommended_bind_addr(&final_config.network)?;
            log::info!("推荐使用地址: {}", addr);
            addr.to_string()
        }
    };
    
    run_webterm(
        bind_addr, 
        cli.command, 
        cli.args, 
        cli.mqtt, 
        cli.hub, 
        cli.server_name,
        &final_config
    ).await?;
    
    Ok(())
}
