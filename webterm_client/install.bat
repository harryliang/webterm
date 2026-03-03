@echo off
chcp 65001 >nul
echo ==========================================
echo      Portmap Android 安装脚本
echo ==========================================
echo.

set ADB=F:\android-sdk\platform-tools\adb
set APK=app\build\hello.apk
set PACKAGE_NAME=com.example.hello
set MAIN_ACTIVITY=.MainActivity

:: 检查 APK 是否存在
if not exist "%APK%" (
    echo [错误] 找不到 APK 文件: %APK%
    echo 请先运行 build.bat 构建应用
    pause
    exit /b 1
)

echo [1/4] 检查设备连接...
%ADB% devices
echo.

echo [2/4] 卸载旧版本（避免签名冲突）...
%ADB% uninstall %PACKAGE_NAME% 2>nul
echo    完成
echo.

echo [3/4] 安装应用...
%ADB% install -r "%APK%"
if errorlevel 1 (
    echo [错误] 安装失败
    pause
    exit /b 1
)
echo    安装成功
echo.

echo [4/4] 启动应用...
%ADB% shell am start -n %PACKAGE_NAME%/%MAIN_ACTIVITY%
if errorlevel 1 (
    echo [警告] 启动失败，请手动打开应用
) else (
    echo    应用已启动
echo.

echo ==========================================
echo      安装完成！
echo ==========================================
echo.
echo 现在可以：
echo   1. 在手机上查看应用界面
echo   2. 点击"显示日志"按钮查看状态
echo   3. 在 PC 端运行: portmap web-term -c cmd.exe
echo   4. 观察手机是否自动打开终端
echo.
echo 如果未自动打开，请运行 diagnose.bat 查看日志
echo.

pause
