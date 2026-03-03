//! 会话管理模块 - 实现持久化的 PTY 会话

use anyhow::{Context, Result};
use dashmap::DashMap;
use log::{info, warn, error};
use portable_pty::{CommandBuilder, MasterPty, Child, PtySize, PtySystem, NativePtySystem, SlavePty};
use std::collections::VecDeque;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex, RwLock};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use axum::extract::ws::Message;

/// PTY 写入器包装类型
pub type PtyWriter = Arc<Mutex<Box<dyn Write + Send>>>;

/// 会话配置
#[derive(Clone)]
pub struct SessionConfig {
    pub max_history_lines: usize,
    pub session_timeout: Duration,
    pub max_sessions: usize,
    pub default_rows: u16,
    pub default_cols: u16,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            max_history_lines: 10000,
            session_timeout: Duration::from_secs(3600), // 1小时超时
            max_sessions: 10,
            default_rows: 24,
            default_cols: 80,
        }
    }
}

/// WebSocket 客户端连接信息
#[derive(Debug)]
struct WsClient {
    id: String,
    tx: mpsc::Sender<Message>,
}

/// 持久会话
pub struct Session {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub last_activity: Arc<RwLock<DateTime<Utc>>>,

    // PTY - Windows 上必须保持 slave 存活！
    pub pty_master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    #[allow(dead_code)]
    pub pty_slave: Arc<Mutex<Box<dyn SlavePty + Send>>>,  // 必须保持存活，Windows 上不能提前 drop
    pub pty_writer: PtyWriter,  // PTY 写入器（复用）
    pub child: Arc<Mutex<Box<dyn Child + Send>>>,

    // WebSocket 客户端列表（支持多客户端同时连接）
    pub ws_clients: Arc<RwLock<Vec<WsClient>>>,

    // 输出历史
    pub output_history: Arc<Mutex<VecDeque<String>>>,
    pub max_history_lines: usize,

    // 元数据
    pub command: String,
    pub cwd: String,
    
    // 进程状态
    pub is_process_running: Arc<RwLock<bool>>,
}

impl Session {
    /// 创建新会话
    /// 
    /// # 参数
    /// - `command`: 要运行的命令
    /// - `args`: 命令参数
    /// - `cwd`: 工作目录
    /// - `session_id`: 可选的会话 ID，如果为 None 则自动生成
    /// - `rows`: 终端行数
    /// - `cols`: 终端列数
    pub async fn new(
        command: String, 
        args: Vec<String>, 
        cwd: String, 
        session_id: Option<String>,
        rows: u16,
        cols: u16,
    ) -> Result<(Arc<Self>, String)> {
        let session_id = session_id.unwrap_or_else(|| format!("sess-{}", Uuid::new_v4().simple()));
        info!("创建会话: {}", session_id);

        // 使用指定的终端大小创建 PTY
        let pty_system = NativePtySystem::default();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        }).context("无法打开 PTY")?;

        // 启动命令
        let mut cmd_builder = CommandBuilder::new(&command);
        if !args.is_empty() {
            cmd_builder.args(&args);
        }
        cmd_builder.cwd(&cwd);
        
        // Windows 上必须显式设置 PATH 环境变量
        if let Ok(path) = std::env::var("PATH") {
            cmd_builder.env("PATH", path);
        }
        
        // Windows cmd.exe 需要特殊环境变量
        if command.to_lowercase().contains("cmd.exe") {
            cmd_builder.env("TERM", "xterm-256color");
            cmd_builder.env("ConEmuANSI", "ON");
            cmd_builder.env("PROMPT", "$P$G");
        }

        let child = pair.slave.spawn_command(cmd_builder)
            .context("无法启动命令")?;

        // ⚠️ Windows 关键：不能 drop pair.slave！必须保持存活直到会话结束
        // 先获取 writer，再 clone reader（顺序很重要！）
        // 获取 PTY 写入器（只获取一次，后续复用）
        let pty_writer = pair.master.take_writer()
            .context("无法获取 PTY writer")?;
        
        // 再 clone reader
        let reader = pair.master.try_clone_reader()
            .context("无法克隆 PTY reader")?;

        let pty_writer = Arc::new(Mutex::new(pty_writer));
        
        let session = Arc::new(Session {
            id: session_id.clone(),
            created_at: Utc::now(),
            last_activity: Arc::new(RwLock::new(Utc::now())),
            pty_master: Arc::new(Mutex::new(pair.master)),
            pty_slave: Arc::new(Mutex::new(pair.slave)),
            pty_writer: pty_writer.clone(),
            child: Arc::new(Mutex::new(child)),
            ws_clients: Arc::new(RwLock::new(Vec::new())),
            output_history: Arc::new(Mutex::new(VecDeque::new())),
            max_history_lines: 10000,
            command: format!("{} {}", command, args.join(" ")),
            cwd,
            is_process_running: Arc::new(RwLock::new(true)),
        });

        // Windows ConPTY 需要一点时间初始化
        #[cfg(windows)]
        {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        
        // 启动后台读取任务（传入已克隆的 reader）
        session.start_reader_task_with_reader(reader).await;

        Ok((session, session_id))
    }

    /// 启动后台读取任务，将 PTY 输出发送到 WebSocket 并保存到历史
    async fn start_reader_task_with_reader(self: &Arc<Self>, reader: Box<dyn std::io::Read + Send>) {
        let session = self.clone();

        // 获取当前运行时的 Handle，用于派生异步任务
        let rt_handle = match tokio::runtime::Handle::try_current() {
            Ok(h) => h,
            Err(e) => {
                error!("无法获取当前运行时: {}", e);
                return;
            }
        };

        // 使用标准线程读取 PTY（阻塞操作）
        std::thread::spawn(move || {
            use std::io::Read;
            let mut reader = reader;
            let mut buf = [0u8; 4096];
            let mut _process_exited = false;
            
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        // PTY 关闭，进程已退出
                        _process_exited = true;
                        info!("会话 {} 的 PTY 已关闭（进程退出）", session.id);
                        break;
                    }
                    Ok(n) => {
                        let data = String::from_utf8_lossy(&buf[..n]).to_string();

                        // 派生异步任务处理数据
                        let session = session.clone();
                        let data_clone = data.clone();
                        rt_handle.spawn(async move {
                            // 保存到历史
                            {
                                let mut history = session.output_history.lock().await;
                                history.push_back(data_clone);
                                while history.len() > session.max_history_lines {
                                    history.pop_front();
                                }
                            }

                            // 广播到所有 WebSocket 客户端
                            let msg = serde_json::json!({
                                "type": "output",
                                "data": data
                            });
                            session.broadcast(Message::Text(msg.to_string())).await;

                            // 更新活动时间
                            *session.last_activity.write().await = Utc::now();
                        });
                    }
                    Err(e) => {
                        warn!("读取 PTY 失败: {}", e);
                        _process_exited = true;
                        break;
                    }
                }
            }
            
            // 进程退出后的处理
            // 标记进程已停止并发送退出通知
            let session_clone = session.clone();
            rt_handle.spawn(async move {
                // 标记进程已停止
                *session_clone.is_process_running.write().await = false;
                
                // 发送进程退出通知给客户端
                let exit_msg = serde_json::json!({
                    "type": "process_exit",
                    "message": "进程已退出，请刷新页面重新连接"
                });
                session_clone.broadcast(Message::Text(exit_msg.to_string())).await;
                
                info!("会话 {} 已广播进程退出消息", session_clone.id);
                
                // 延迟后清理客户端连接
                tokio::time::sleep(Duration::from_secs(3)).await;
                
                // 关闭所有 WebSocket 客户端连接
                let clients = session_clone.ws_clients.read().await;
                for client in clients.iter() {
                    let close_msg = Message::Close(None);
                    let _ = client.tx.send(close_msg).await;
                }
            });
        });
    }

    /// 绑定 WebSocket，返回客户端 ID
    pub async fn attach(&self, ws_tx: mpsc::Sender<Message>) -> String {
        let client_id = format!("client-{}", Uuid::new_v4().simple());
        
        {
            let mut clients = self.ws_clients.write().await;
            clients.push(WsClient {
                id: client_id.clone(),
                tx: ws_tx,
            });
            info!("会话 {} 新增客户端 {}，当前客户端数: {}", self.id, client_id, clients.len());
        }

        // 发送历史输出给新连接
        self.send_history_to(&client_id).await;
        
        client_id
    }

    /// 解绑指定的 WebSocket 客户端
    pub async fn detach(&self, client_id: &str) {
        let mut clients = self.ws_clients.write().await;
        let before_len = clients.len();
        clients.retain(|c| c.id != client_id);
        let after_len = clients.len();
        info!("会话 {} 客户端 {} 已断开，剩余客户端数: {}", self.id, client_id, after_len);
    }

    /// 发送历史输出给指定客户端
    async fn send_history_to(&self, client_id: &str) {
        let history = self.output_history.lock().await;
        let clients = self.ws_clients.read().await;

        if let Some(client) = clients.iter().find(|c| c.id == client_id) {
            let all_output: String = history.iter().cloned().collect();
            if !all_output.is_empty() {
                let msg = serde_json::json!({
                    "type": "output",
                    "data": all_output
                });
                let _ = client.tx.send(Message::Text(msg.to_string())).await;
            }
        }
    }
    
    /// 广播消息到所有连接的客户端
    async fn broadcast(&self, msg: Message) {
        let clients = self.ws_clients.read().await;
        for client in clients.iter() {
            let _ = client.tx.send(msg.clone()).await;
        }
    }

    /// 写入输入到 PTY
    pub async fn write_input(&self, data: &str) -> Result<()> {
        // 对于 Windows，需要确保换行符是 \r\n
        #[cfg(windows)]
        let data = if data == "\n" {
            "\r\n".to_string()
        } else if data.contains('\n') && !data.contains('\r') {
            data.replace('\n', "\r\n")
        } else {
            data.to_string()
        };
        
        #[cfg(not(windows))]
        let data = data.to_string();
        
        let mut writer = self.pty_writer.lock().await;
        writer.write_all(data.as_bytes())?;
        writer.flush()?;
        drop(writer); // 显式释放锁
        
        *self.last_activity.write().await = Utc::now();
        Ok(())
    }

    /// 调整终端大小
    pub async fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let master = self.pty_master.lock().await;
        master.resize(PtySize {
            cols,
            rows,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    /// 检查是否活跃（最近有活动）
    pub async fn is_active_recently(&self, timeout: Duration) -> bool {
        let last = *self.last_activity.read().await;
        Utc::now().signed_duration_since(last).to_std().unwrap_or(Duration::MAX) < timeout
    }

    /// 检查进程是否仍在运行
    pub async fn is_process_running(&self) -> bool {
        *self.is_process_running.read().await
    }

    /// 获取当前连接的客户端数量
    pub async fn client_count(&self) -> usize {
        self.ws_clients.read().await.len()
    }

    /// 终止会话
    pub async fn terminate(&self) -> Result<()> {
        let mut child = self.child.lock().await;
        let _ = child.kill();
        info!("会话 {} 已终止", self.id);
        Ok(())
    }

    /// 获取会话信息
    pub fn get_info(&self) -> SessionInfo {
        SessionInfo {
            id: self.id.clone(),
            command: self.command.clone(),
            cwd: self.cwd.clone(),
            created_at: self.created_at,
        }
    }
}

/// 会话信息（用于 API 返回）
#[derive(serde::Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub command: String,
    pub cwd: String,
    pub created_at: DateTime<Utc>,
}

/// 会话管理器
pub struct SessionManager {
    sessions: Arc<DashMap<String, Arc<Session>>>,
    config: SessionConfig,
}

impl SessionManager {
    pub fn new(config: SessionConfig) -> Arc<Self> {
        let manager = Arc::new(Self {
            sessions: Arc::new(DashMap::new()),
            config,
        });

        // 启动清理任务
        manager.start_cleanup_task();

        manager
    }

    /// 创建会话
    pub async fn create(&self, command: String, args: Vec<String>, cwd: String, session_id: Option<String>) -> Result<String> {
        // 如果指定了 session_id 且已存在，直接返回现有会话
        if let Some(ref id) = session_id {
            if self.sessions.get(id).is_some() {
                info!("使用现有会话: {}", id);
                return Ok(id.clone());
            }
        }

        // 检查会话数量限制
        if self.sessions.len() >= self.config.max_sessions {
            // 清理最老的会话
            if let Some(oldest) = self.find_oldest_session().await {
                self.destroy(&oldest).await;
            }
        }

        let (session, id) = Session::new(
            command, 
            args, 
            cwd, 
            session_id,
            self.config.default_rows,
            self.config.default_cols,
        ).await?;
        self.sessions.insert(id.clone(), session);
        info!("创建会话: {}", id);

        Ok(id)
    }

    /// 获取会话
    pub fn get(&self, id: &str) -> Option<Arc<Session>> {
        self.sessions.get(id).map(|e| e.clone())
    }

    /// 销毁会话
    pub async fn destroy(&self, id: &str) {
        if let Some((_, session)) = self.sessions.remove(id) {
            let _ = session.terminate().await;
            info!("销毁会话: {}", id);
        }
    }

    /// 获取所有会话列表
    pub fn list(&self) -> Vec<SessionInfo> {
        self.sessions
            .iter()
            .map(|e| {
                let session = e.value();
                session.get_info()
            })
            .collect()
    }

    /// 查找最老的会话
    async fn find_oldest_session(&self) -> Option<String> {
        let mut oldest: Option<(String, DateTime<Utc>)> = None;

        for entry in self.sessions.iter() {
            let session = entry.value();
            let last_activity = *session.last_activity.read().await;

            match &oldest {
                None => oldest = Some((session.id.clone(), last_activity)),
                Some((_, oldest_time)) if last_activity < *oldest_time => {
                    oldest = Some((session.id.clone(), last_activity));
                }
                _ => {}
            }
        }

        oldest.map(|(id, _)| id)
    }

    /// 启动清理任务
    fn start_cleanup_task(&self) {
        let sessions = self.sessions.clone();
        let timeout = self.config.session_timeout;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));

            loop {
                interval.tick().await;

                let mut to_remove = Vec::new();

                for entry in sessions.iter() {
                    let session = entry.value();
                    if !session.is_active_recently(timeout).await {
                        to_remove.push(session.id.clone());
                    }
                }

                for id in to_remove {
                    if let Some((_, session)) = sessions.remove(&id) {
                        let _ = session.terminate().await;
                        info!("清理超时会话: {}", id);
                    }
                }
            }
        });
    }
}
