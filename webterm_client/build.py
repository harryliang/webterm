#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Android Hello World 自动构建脚本
支持 Windows/Linux/Mac
"""

import os
import sys
import subprocess
import shutil
import zipfile
from pathlib import Path

# ==================== 配置 ====================
# Android SDK 路径（根据实际情况修改）
ANDROID_SDK = r"F:\android-sdk"  # Windows 示例
# ANDROID_SDK = "/home/user/Android/Sdk"  # Linux/Mac 示例

BUILD_TOOLS_VERSION = "35.0.0"
PLATFORM_VERSION = "android-35"

# APK 配置
APP_NAME = "hello.apk"
PACKAGE_NAME = "com.example.hello"
MAIN_ACTIVITY = ".MainActivity"

# ==================== 路径计算 ====================
BUILD_TOOLS = Path(ANDROID_SDK) / "build-tools" / BUILD_TOOLS_VERSION
PLATFORM = Path(ANDROID_SDK) / "platforms" / PLATFORM_VERSION
ADB = Path(ANDROID_SDK) / "platform-tools" / ("adb.exe" if os.name == "nt" else "adb")

AAPT2 = BUILD_TOOLS / ("aapt2.exe" if os.name == "nt" else "aapt2")
D8 = BUILD_TOOLS / ("d8.bat" if os.name == "nt" else "d8")
APKSIGNER = BUILD_TOOLS / ("apksigner.bat" if os.name == "nt" else "apksigner")
KEYTOOL = "keytool"
JAVAC = "javac"

# ==================== 项目路径 ====================
SCRIPT_DIR = Path(__file__).parent.resolve()
SRC_DIR = SCRIPT_DIR / "app" / "src" / "main"
BUILD_DIR = SCRIPT_DIR / "app" / "build"
CLASSES_DIR = BUILD_DIR / "classes"
JAVA_SRC = SRC_DIR / "java" / "com" / "example" / "hello" / "MainActivity.java"
MANIFEST = SRC_DIR / "AndroidManifest.xml"
RES_DIR = SRC_DIR / "res"
ANDROID_JAR = PLATFORM / "android.jar"


def run(cmd, check=True, **kwargs):
    """运行命令"""
    cmd_str = " ".join(str(c) for c in cmd) if isinstance(cmd, list) else str(cmd)
    print(f"    > {cmd_str[:80]}...")
    result = subprocess.run(cmd, shell=isinstance(cmd, str), capture_output=True, text=True, **kwargs)
    if result.returncode != 0 and check:
        print(f"[错误] 命令失败: {result.stderr}")
        sys.exit(1)
    return result


def step(num, total, desc):
    """打印步骤信息"""
    print(f"\n[{num}/{total}] {desc}...")


def check_tools():
    """检查必要工具"""
    print("检查构建工具...")
    required = [
        (AAPT2, "aapt2"),
        (D8, "d8"),
        (APKSIGNER, "apksigner"),
        (ANDROID_JAR, "android.jar"),
        (ADB, "adb"),
    ]
    
    for path, name in required:
        if not path.exists():
            print(f"[错误] 找不到 {name}: {path}")
            print(f"请检查 ANDROID_SDK 路径是否正确: {ANDROID_SDK}")
            sys.exit(1)
    
    # 检查 Java
    try:
        subprocess.run([JAVAC, "-version"], capture_output=True, check=True)
    except FileNotFoundError:
        print("[错误] 找不到 javac，请确保已安装 JDK 并添加到 PATH")
        sys.exit(1)
    
    print("  所有工具就绪！")


def clean():
    """清理旧文件"""
    if BUILD_DIR.exists():
        shutil.rmtree(BUILD_DIR)
    BUILD_DIR.mkdir(parents=True)
    CLASSES_DIR.mkdir(parents=True)


def compile_resources():
    """编译资源"""
    res_zip = BUILD_DIR / "res.zip"
    run([AAPT2, "compile", "-o", res_zip, "--dir", RES_DIR])
    return res_zip


def link_resources(res_zip):
    """链接资源"""
    linked_apk = BUILD_DIR / "linked.apk"
    run([
        AAPT2, "link",
        "-o", linked_apk,
        "-I", ANDROID_JAR,
        "--java", BUILD_DIR,
        "--manifest", MANIFEST,
        res_zip
    ])
    return linked_apk


def compile_java():
    """编译 Java 代码"""
    r_java = BUILD_DIR / "com" / "example" / "hello" / "R.java"
    sources = [JAVA_SRC, r_java]
    run([JAVAC, "-cp", ANDROID_JAR, "-d", CLASSES_DIR] + sources)


def dex():
    """转换为 DEX"""
    classes = list(CLASSES_DIR.glob("**/*.class"))
    if not classes:
        print("[错误] 找不到编译后的 class 文件")
        sys.exit(1)
    run([D8, classes, "--output", BUILD_DIR])


def package_apk(linked_apk):
    """打包 APK"""
    temp_apk = BUILD_DIR / "temp.apk"
    shutil.copy(linked_apk, temp_apk)
    
    # 将 classes.dex 添加到 APK
    with zipfile.ZipFile(temp_apk, 'a', zipfile.ZIP_DEFLATED) as zf:
        dex_file = BUILD_DIR / "classes.dex"
        zf.write(dex_file, "classes.dex")
    
    return temp_apk


def sign_apk(unsigned_apk):
    """签名 APK"""
    keystore = BUILD_DIR / "debug.keystore"
    
    # 生成调试密钥（如果不存在）
    if not keystore.exists():
        print("    生成调试密钥...")
        run([
            KEYTOOL, "-genkey", "-v",
            "-keystore", keystore,
            "-storepass", "android",
            "-alias", "androiddebugkey",
            "-keypass", "android",
            "-keyalg", "RSA",
            "-validity", "10000",
            "-dname", "CN=Android Debug,O=Android,C=US"
        ])
    
    signed_apk = BUILD_DIR / APP_NAME
    run([
        APKSIGNER, "sign",
        "--ks", keystore,
        "--ks-pass", "pass:android",
        "--key-pass", "pass:android",
        "--out", signed_apk,
        unsigned_apk
    ])
    return signed_apk


def install_and_run(apk_path):
    """安装并运行应用"""
    print("\n安装 APK...")
    result = run([ADB, "install", "-r", apk_path], check=False)
    
    if result.returncode != 0:
        print("  安装失败，尝试先卸载再安装...")
        run([ADB, "uninstall", PACKAGE_NAME], check=False)
        run([ADB, "install", apk_path])
    
    print("  安装成功！")
    
    print("\n启动应用...")
    run([ADB, "shell", "am", "start", "-n", f"{PACKAGE_NAME}/{MAIN_ACTIVITY}"])


def main():
    print("=" * 50)
    print("     Android Hello World 构建脚本")
    print("=" * 50)
    
    # 检查工具
    check_tools()
    
    # 构建步骤
    steps = [
        ("清理旧文件", clean),
        ("编译资源", compile_resources),
        ("链接资源", link_resources),
        ("编译 Java", compile_java),
        ("转换为 DEX", dex),
        ("打包 APK", None),  # 特殊处理
        ("签名 APK", None),  # 特殊处理
    ]
    
    result = None
    for i, (desc, func) in enumerate(steps, 1):
        if func is None:
            continue
        step(i, len(steps), desc)
        result = func() if func.__code__.co_argcount > 0 else func() or result
    
    # 特殊步骤：打包和签名
    step(6, 7, "打包 APK")
    linked_apk = BUILD_DIR / "linked.apk"
    temp_apk = package_apk(linked_apk)
    
    step(7, 7, "签名 APK")
    final_apk = sign_apk(temp_apk)
    
    # 安装和运行
    install_and_run(final_apk)
    
    print("\n" + "=" * 50)
    print(f"构建完成！APK: {final_apk}")
    print(f"包名: {PACKAGE_NAME}")
    print("=" * 50)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\n\n[中断] 用户取消构建")
        sys.exit(1)
