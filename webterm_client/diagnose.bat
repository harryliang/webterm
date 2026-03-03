@echo off
chcp 65001 >nul
echo ==========================================
echo      Portmap Android 诊断工具
echo ==========================================
echo.

set ADB=F:\android-sdk\platform-tools\adb

echo [1/5] 检查设备连接...
%ADB% devices
echo.

echo [2/5] 清空旧日志...
%ADB% logcat -c 2>nul
echo    完成
echo.

echo [3/5] 启动应用...
%ADB% shell am start -n com.example.hello/.MainActivity 2>nul
if errorlevel 1 (
    echo    [警告] 应用启动失败，可能未安装
    echo    尝试安装应用...
    %ADB% install -r app\build\hello.apk
    %ADB% shell am start -n com.example.hello/.MainActivity
)
echo    完成
echo.

echo [4/5] 等待 5 秒让应用初始化...
timeout /t 5 /nobreak >nul
echo    完成
echo.

echo [5/5] 收集日志（显示最近 50 条，按 Ctrl+C 停止实时查看）...
echo ==========================================
%ADB% logcat -d -s MainActivity:MqttService | findstr /i "===\|MQTT\|URL\|收到\|打开\|错误\|失败" | tail -50
echo.
echo ==========================================
echo.

echo 诊断完成！
echo.
echo 如果看到 "收到 MQTT 消息" 但没有 "打开 URL"，
echo 请检查 WebView 是否正常。
echo.
echo 如果没有看到 "收到 MQTT 消息"，
echo 请在 PC 端运行: python test_mqtt_send.py
echo 然后再次运行此脚本查看日志。
echo.

pause
