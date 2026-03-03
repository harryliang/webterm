@echo off
chcp 65001 >nul
echo ==========================================
echo      调试工具 - 绕过加密日志
echo ==========================================
echo.

set ADB=F:\android-sdk\platform-tools\adb

echo [1] 检查设备...
%ADB% devices
echo.

echo [2] 重启应用并获取进程 ID...
%ADB% shell am force-stop com.example.hello 2>nul
timeout /t 1 /nobreak >nul
%ADB% shell am start -n com.example.hello/.MainActivity
timeout /t 2 /nobreak >nul
for /f "tokens=2" %%a in ('%ADB% shell pidof com.example.hello') do set PID=%%a
echo    应用 PID: %PID%
echo.

echo [3] 使用 pid 过滤日志（绕过加密）...
echo    正在收集 PID=%PID% 的日志...
echo    按 Ctrl+C 停止
echo ==========================================
%ADB% logcat --pid=%PID%
