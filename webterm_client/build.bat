@echo off
chcp 65001 >nul
setlocal EnableDelayedExpansion

:: ==================== 配置 ====================
set ANDROID_SDK=F:\android-sdk
set BUILD_TOOLS=%ANDROID_SDK%\build-tools\35.0.0
set PLATFORM=%ANDROID_SDK%\platforms\android-35

set PROJECT_DIR=%~dp0
set SRC_DIR=%PROJECT_DIR%app\src\main
set BUILD_DIR=%PROJECT_DIR%app\build
set CLASSES_DIR=%BUILD_DIR%\classes
set LIBS_DIR=%PROJECT_DIR%app\libs

set APP_NAME=hello.apk
set PACKAGE_NAME=com.example.hello
set MAIN_ACTIVITY=.MainActivity

echo ============================================
echo      Portmap Android 客户端构建脚本
echo ============================================
echo.

:: ==================== 清理 ====================
echo [1/7] 清理旧文件...
if exist "%BUILD_DIR%" rmdir /s /q "%BUILD_DIR%"
mkdir "%BUILD_DIR%"
mkdir "%CLASSES_DIR%"
echo      完成
echo.

:: ==================== 编译资源 ====================
echo [2/7] 编译资源...
"%BUILD_TOOLS%\aapt2" compile -o "%BUILD_DIR%\res.zip" --dir "%SRC_DIR%\res"
if errorlevel 1 goto error
echo      完成
echo.

:: ==================== 链接资源 ====================
echo [3/7] 链接资源...
"%BUILD_TOOLS%\aapt2" link -o "%BUILD_DIR%\linked.apk" -I "%PLATFORM%\android.jar" --java "%BUILD_DIR%" --manifest "%SRC_DIR%\AndroidManifest.xml" "%BUILD_DIR%\res.zip"
if errorlevel 1 goto error
echo      完成
echo.

:: ==================== 编译 Java ====================
echo [4/7] 编译 Java 代码...
set CLASSPATH=%PLATFORM%\android.jar
for %%f in ("%LIBS_DIR%\*.jar") do set CLASSPATH=!CLASSPATH!;%%f

set JAVA_FILES=
for %%f in ("%SRC_DIR%\java\com\example\hello\*.java") do set JAVA_FILES=!JAVA_FILES! "%%f"
set JAVA_FILES=!JAVA_FILES! "%BUILD_DIR%\com\example\hello\R.java"

javac -encoding UTF-8 -cp "!CLASSPATH!" -d "%CLASSES_DIR%" !JAVA_FILES!
if errorlevel 1 goto error
echo      完成
echo.

:: ==================== 转换为 DEX ====================
echo [5/7] 转换为 DEX 格式...
cd /d "%CLASSES_DIR%"
set JAR_FILES=
for %%f in ("%LIBS_DIR%\*.jar") do set JAR_FILES=!JAR_FILES! "%%f"
powershell -Command "$classFiles = Get-ChildItem -Recurse -Filter '*.class' | ForEach-Object { $_.FullName }; & '%BUILD_TOOLS%\d8' --output '%BUILD_DIR%' --lib '%PLATFORM%\android.jar' $classFiles !JAR_FILES!"
cd /d "%PROJECT_DIR%"
if not exist "%BUILD_DIR%\classes.dex" goto error
echo      完成
echo.

:: ==================== 打包 APK ====================
echo [6/7] 打包 APK...
copy /y "%BUILD_DIR%\linked.apk" "%BUILD_DIR%\app.zip" >nul
cd /d "%BUILD_DIR%"
powershell -Command "Compress-Archive -Path classes.dex -Update -DestinationPath app.zip" >nul

:: 添加resources目录中的文件到APK（保持目录结构）
if exist "%SRC_DIR%\resources" (
    echo      添加资源文件...
    cd /d "%SRC_DIR%\resources"
    for /f "delims=" %%i in ('dir /s /b /a-d') do (
        set "filePath=%%i"
        set "relPath=!filePath:%SRC_DIR%\resources\=!"
        set "dirPath=!relPath:\=/!"
        echo         !dirPath!
    )
    :: 使用PowerShell保持目录结构添加文件
    powershell -Command "$basePath = '%SRC_DIR%\resources'; Get-ChildItem -Path $basePath -Recurse -File | ForEach-Object { $relPath = $_.FullName.Substring($basePath.Length + 1).Replace('\', '/'); $entryName = $relPath; try { $zip = [System.IO.Compression.ZipFile]::Open('app.zip', [System.IO.Compression.ZipArchiveMode]::Update); $existing = $zip.GetEntry($entryName); if ($existing) { $existing.Delete() }; [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, $_.FullName, $entryName); $zip.Dispose() } catch {} }" >nul 2>&1
    cd /d "%BUILD_DIR%"
)

move /y app.zip app.apk >nul
cd /d "%PROJECT_DIR%"
echo      完成
echo.

:: ==================== 添加 Paho MQTT 必要资源 ====================
echo [7/8] 添加 Paho MQTT 资源...
python "%PROJECT_DIR%fix_apk.py" "%BUILD_DIR%\app.apk" "%LIBS_DIR%\org.eclipse.paho.client.mqttv3-1.2.5.jar"
if errorlevel 1 goto error
echo      完成
echo.

:: ==================== 签名 APK ====================
echo [8/8] 签名 APK...
if not exist "%BUILD_DIR%\debug.keystore" (
    echo      生成调试密钥...
    echo y | keytool -genkey -v -keystore "%BUILD_DIR%\debug.keystore" -storepass android -alias androiddebugkey -keypass android -keyalg RSA -validity 10000 -dname "CN=Android Debug,O=Android,C=US" >nul 2>&1
)
"%BUILD_TOOLS%\apksigner" sign --ks "%BUILD_DIR%\debug.keystore" --ks-pass pass:android --key-pass pass:android --out "%BUILD_DIR%\%APP_NAME%" "%BUILD_DIR%\app.apk"
if errorlevel 1 goto error
echo      完成
echo.

echo ============================================
echo      构建成功！APK: %BUILD_DIR%\%APP_NAME%
echo ============================================
echo.
echo 下一步操作:
echo   1. 确保手机通过 USB 连接并开启调试模式
echo   2. 运行 install.bat 安装应用
echo   3. 或在手机上手动安装 APK
echo.

goto end

:error
echo.
echo [错误] 构建失败！
exit /b 1

:end
pause
