@echo off
chcp 65001 >nul
echo ==========================================
echo      截取手机屏幕
echo ==========================================
echo.

set ADB=F:\android-sdk\platform-tools\adb
set SCREENSHOT=%~dp0screenshot.png

echo [1] 正在截图...
%ADB% shell screencap -p /sdcard/screenshot.png
echo    完成
echo.

echo [2] 复制到电脑...
%ADB% pull /sdcard/screenshot.png "%SCREENSHOT%"
echo    完成
echo.

echo [3] 删除手机上的临时文件...
%ADB% shell rm /sdcard/screenshot.png
echo    完成
echo.

echo ==========================================
echo      截图已保存: %SCREENSHOT%
echo ==========================================
echo.

:: 尝试用默认程序打开截图
start "" "%SCREENSHOT%"

echo 请把截图路径发给我，例如:
echo @E:\Study\portmap\webterm_client\screenshot.png
echo.

pause
