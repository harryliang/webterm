import zipfile
import shutil
import sys

apk_path = sys.argv[1]
jar_path = sys.argv[2]

# 读取jar包中的服务文件
with zipfile.ZipFile(jar_path, 'r') as jar:
    service_content = jar.read('META-INF/services/org.eclipse.paho.client.mqttv3.spi.NetworkModuleFactory')

# 资源文件内容
logcat_content = '''# Paho MQTT Log Messages
0={0}
1={0}: {1}
2={0}: {1} {2}
3={0}: {1} {2} {3}
4={0}: {1} {2} {3} {4}
'''

# 重建APK
with zipfile.ZipFile(apk_path, 'r') as zf_in:
    with zipfile.ZipFile(apk_path + '.tmp', 'w') as zf_out:
        for item in zf_in.infolist():
            data = zf_in.read(item.filename)
            zf_out.writestr(item, data)
        
        # 添加SPI服务文件
        zf_out.writestr('META-INF/services/org.eclipse.paho.client.mqttv3.spi.NetworkModuleFactory', service_content)
        
        # 添加logcat资源文件
        nls_path = 'org/eclipse/paho/client/mqttv3/internal/nls/'
        for f in ['logcat.properties', 'logcat_en.properties', 'logcat_en_US.properties']:
            zf_out.writestr(nls_path + f, logcat_content)

shutil.move(apk_path + '.tmp', apk_path)
print('Paho MQTT资源添加完成')
