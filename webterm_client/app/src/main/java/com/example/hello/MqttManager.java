package com.example.hello;

import android.os.Handler;
import android.os.Looper;
import android.util.Log;

import org.eclipse.paho.client.mqttv3.*;
import org.eclipse.paho.client.mqttv3.persist.MemoryPersistence;
import org.eclipse.paho.client.mqttv3.logging.LoggerFactory;

import java.security.MessageDigest;
import java.util.Locale;

/**
 * MQTT 管理类（非服务，直接在主线程运行）
 */
public class MqttManager {
    private static final String TAG = "MqttManager";
    
    private static final String MQTT_HOST = "tcp://mqtt.iyanjiu.com:1883";
    private static final String MQTT_USERNAME = "idiaoyan";
    private static final String MQTT_PASSWORD = "Idy@3984#24039";
    private static final String TOPIC = "portmap/webterm";
    private static final String SECRET_KEY = "3444462b-0f6f-4523-b382-92a1288345ef";
    
    private MqttClient mqttClient;
    private MessageListener listener;
    private StatusListener statusListener;
    private Handler mainHandler;
    
    public interface MessageListener {
        void onMessage(String payload);
    }
    
    public interface StatusListener {
        void onStatus(String status);
    }
    
    public MqttManager() {
        mainHandler = new Handler(Looper.getMainLooper());
    }
    
    public void setMessageListener(MessageListener l) {
        this.listener = l;
    }
    
    public void setStatusListener(StatusListener l) {
        this.statusListener = l;
    }
    
    private void notifyStatus(String msg) {
        Log.i(TAG, msg);
        if (statusListener != null) {
            mainHandler.post(() -> statusListener.onStatus(msg));
        }
    }
    
    private String topicEncode(String topic) {
        try {
            String s = topic + SECRET_KEY;
            MessageDigest md = MessageDigest.getInstance("MD5");
            byte[] digest = md.digest(s.getBytes());
            StringBuilder sb = new StringBuilder();
            for (byte b : digest) {
                sb.append(String.format("%02x", b));
            }
            String result = sb.toString();
            notifyStatus("主题编码: " + result);
            return result;
        } catch (Exception e) {
            notifyStatus("MD5错误!");
            return topic;
        }
    }
    
    public void connect() {
        // 在后台线程连接
        new Thread(() -> {
            // 设置英文 locale（带美国区域），避免 Paho MQTT 资源缺失问题
            Locale.setDefault(new Locale("en", "US"));
            try {
                doConnect();
            } catch (Exception e) {
                notifyStatus("连接异常: " + e.getMessage());
            }
        }).start();
    }
    
    private void doConnect() {
        notifyStatus("开始连接...");
        
        try {
            String clientId = "android_" + System.currentTimeMillis();
            notifyStatus("Client: " + clientId.substring(8));
            notifyStatus("Locale: " + Locale.getDefault());
            
            mqttClient = new MqttClient(MQTT_HOST, clientId, new MemoryPersistence());
            
            MqttConnectOptions options = new MqttConnectOptions();
            options.setUserName(MQTT_USERNAME);
            options.setPassword(MQTT_PASSWORD.toCharArray());
            options.setCleanSession(true);
            options.setAutomaticReconnect(true);
            options.setConnectionTimeout(10);
            options.setKeepAliveInterval(20);
            
            mqttClient.setCallback(new MqttCallback() {
                @Override
                public void connectionLost(Throwable cause) {
                    notifyStatus("连接丢失!");
                }
                
                @Override
                public void messageArrived(String topic, MqttMessage message) throws Exception {
                    final String payload = new String(message.getPayload());
                    notifyStatus("收到消息!");
                    if (listener != null) {
                        mainHandler.post(() -> listener.onMessage(payload));
                    }
                }
                
                @Override
                public void deliveryComplete(IMqttDeliveryToken token) {
                }
            });
            
            notifyStatus("正在连接服务器...");
            mqttClient.connect(options);
            notifyStatus("连接成功!");
            
            // 订阅
            String encodedTopic = topicEncode(TOPIC);
            mqttClient.subscribe(encodedTopic);
            notifyStatus("已订阅，等待PC连接...");
            
        } catch (Exception e) {
            notifyStatus("失败: " + e.getClass().getSimpleName() + " - " + e.getMessage());
            // 打印堆栈到字符串
            StringBuilder sb = new StringBuilder();
            for (StackTraceElement ste : e.getStackTrace()) {
                sb.append(ste.toString()).append("\n");
            }
            notifyStatus("堆栈: " + sb.toString().substring(0, Math.min(200, sb.length())));
            Log.e(TAG, "连接失败", e);
        }
    }
    
    public boolean isConnected() {
        return mqttClient != null && mqttClient.isConnected();
    }
    
    public void disconnect() {
        try {
            if (mqttClient != null) mqttClient.disconnect();
        } catch (Exception e) {}
    }
}
