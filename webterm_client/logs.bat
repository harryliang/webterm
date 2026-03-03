@echo off
chcp 65001 >nul
echo ==========================================
echo      查看应用日志
echo ==========================================
echo.

echo [1] 清空旧日志...
adb logcat -c
echo    完成
echo.

echo [2] 启动应用（如果未运行）...
adb shell am start -n com.example.hello/.MainActivity 2>nul
echo.

echo [3] 正在收集日志（按 Ctrl+C 停止）...
echo ==========================================
echo.
adb logcat MainActivity:D MqttService:D *:S
