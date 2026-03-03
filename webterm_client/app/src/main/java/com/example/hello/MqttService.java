package com.example.hello;

import android.app.Service;
import android.content.Intent;
import android.os.Binder;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.util.Log;

import org.eclipse.paho.client.mqttv3.*;
import org.eclipse.paho.client.mqttv3.persist.MemoryPersistence;

import java.security.MessageDigest;
import java.util.Locale;

public class MqttService extends Service {
    private static final String TAG = "MqttService";
    
    private static final String MQTT_HOST = "tcp://mqtt.iyanjiu.com:1883";
    private static final String MQTT_USERNAME = "idiaoyan";
    private static final String MQTT_PASSWORD = "Idy@3984#24039";
    private static final String TOPIC = "portmap/webterm";
    private static final String SECRET_KEY = "3444462b-0f6f-4523-b382-92a1288345ef";
    
    private MqttClient mqttClient;
    private final IBinder binder = new LocalBinder();
    private MqttCallback messageCallback;
    private Handler mainHandler;
    
    // 回调接口，用于通知MainActivity
    public interface StatusCallback {
        void onStatusUpdate(String status);
    }
    private StatusCallback statusCallback;
    
    public void setStatusCallback(StatusCallback cb) {
        this.statusCallback = cb;
    }
    
    public class LocalBinder extends Binder {
        MqttService getService() {
            return MqttService.this;
        }
    }
    
    @Override
    public void onCreate() {
        super.onCreate();
        mainHandler = new Handler(Looper.getMainLooper());
        Log.i(TAG, "服务创建");
        notifyStatus("服务创建");
        
        // 延迟连接，确保binder已准备好
        mainHandler.postDelayed(() -> connect(), 100);
    }
    
    @Override
    public IBinder onBind(Intent intent) {
        Log.i(TAG, "服务绑定");
        notifyStatus("服务绑定");
        return binder;
    }
    
    private void notifyStatus(String msg) {
        Log.i(TAG, msg);
        if (statusCallback != null) {
            mainHandler.post(() -> statusCallback.onStatusUpdate(msg));
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
            return sb.toString();
        } catch (Exception e) {
            notifyStatus("MD5错误: " + e.getMessage());
            return topic;
        }
    }
    
    private void connect() {
        // 设置英文 locale，避免 Paho MQTT 资源缺失问题
        Locale.setDefault(Locale.ENGLISH);
        
        notifyStatus("开始连接MQTT...");
        
        try {
            String clientId = "android_" + System.currentTimeMillis();
            notifyStatus("ClientID: " + clientId.substring(0, 20) + "...");
            
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
                    notifyStatus("连接丢失: " + cause.getMessage());
                }
                
                @Override
                public void messageArrived(String topic, MqttMessage message) throws Exception {
                    final String payload = new String(message.getPayload());
                    notifyStatus("收到消息!");
                    
                    if (messageCallback != null) {
                        mainHandler.post(() -> {
                            try {
                                messageCallback.messageArrived(topic, message);
                            } catch (Exception e) {
                                notifyStatus("回调错误: " + e.getMessage());
                            }
                        });
                    }
                }
                
                @Override
                public void deliveryComplete(IMqttDeliveryToken token) {
                }
            });
            
            notifyStatus("正在连接broker...");
            mqttClient.connect(options);
            notifyStatus("MQTT连接成功!");
            
            // 订阅
            String encodedTopic = topicEncode(TOPIC);
            notifyStatus("订阅: " + encodedTopic.substring(0, 20) + "...");
            mqttClient.subscribe(encodedTopic);
            notifyStatus("订阅成功! 等待消息...");
            
        } catch (Exception e) {
            notifyStatus("连接失败: " + e.getClass().getSimpleName() + " - " + e.getMessage());
            Log.e(TAG, "连接失败", e);
        }
    }
    
    public void setMessageCallback(MqttCallback callback) {
        this.messageCallback = callback;
    }
    
    public boolean isConnected() {
        return mqttClient != null && mqttClient.isConnected();
    }
    
    @Override
    public void onDestroy() {
        try {
            if (mqttClient != null) mqttClient.disconnect();
        } catch (Exception e) {}
        super.onDestroy();
    }
}
