package com.example.hello;

import android.app.Activity;
import android.content.res.Configuration;
import android.graphics.Rect;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;
import android.view.View;
import android.view.ViewTreeObserver;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.LinearLayout;

import org.json.JSONObject;

import java.util.Locale;

public class MainActivity extends Activity {
    private static final String TAG = "MainActivity";

    private WebView webView;
    private MqttManager mqttManager;
    private Handler mainHandler;
    
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        // 强制使用英文 locale（带美国区域），避免 Paho MQTT 资源缺失问题
        Locale locale = new Locale("en", "US");
        Locale.setDefault(locale);
        Configuration config = getResources().getConfiguration();
        config.setLocale(locale);
        getResources().updateConfiguration(config, getResources().getDisplayMetrics());
        
        // 设置全屏 - 隐藏状态栏和导航栏
        requestWindowFeature(android.view.Window.FEATURE_NO_TITLE);
        getWindow().setFlags(
            android.view.WindowManager.LayoutParams.FLAG_FULLSCREEN,
            android.view.WindowManager.LayoutParams.FLAG_FULLSCREEN
        );
        
        super.onCreate(savedInstanceState);
        mainHandler = new Handler(Looper.getMainLooper());
        
        // 检查是否直接传入 URL
        String directUrl = getIntent().getStringExtra("url");
        
        createUI();
        setupWebView();
        
        if (directUrl != null && !directUrl.isEmpty()) {
            // 直接打开指定 URL
            webView.loadUrl(directUrl);
            Log.i(TAG, "已连接: " + directUrl);
        } else {
            showWaitingPage();

            // 创建并连接 MQTT（旧模式）
            mqttManager = new MqttManager();
            mqttManager.setStatusListener(status -> {
                Log.i(TAG, "MQTT状态: " + status);
            });
            mqttManager.setMessageListener(payload -> {
                handleMqttMessage(payload);
            });

            mainHandler.postDelayed(() -> {
                mqttManager.connect();
            }, 500);
        }
    }
    
    private void createUI() {
        LinearLayout mainLayout = new LinearLayout(this);
        mainLayout.setOrientation(LinearLayout.VERTICAL);
        // 全屏模式，不适应系统窗口（避免顶部白色边距）
        mainLayout.setFitsSystemWindows(false);
        mainLayout.setBackgroundColor(0xFF1e1e1e); // 与终端背景色一致

        // WebView - 全屏显示终端（移除状态栏以节省空间）
        webView = new WebView(this);
        webView.setBackgroundColor(0xFF1e1e1e); // 设置与终端一致的背景色，避免白边
        LinearLayout.LayoutParams webParams = new LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.MATCH_PARENT);
        mainLayout.addView(webView, webParams);

        setContentView(mainLayout);

        // 监听键盘弹出/收起，动态调整WebView高度
        setupKeyboardListener(mainLayout, webView);
    }
    
    private void setupKeyboardListener(final View rootView, final WebView webView) {
        rootView.getViewTreeObserver().addOnGlobalLayoutListener(new ViewTreeObserver.OnGlobalLayoutListener() {
            private int lastVisibleHeight = 0;
            private final int MIN_KEYBOARD_HEIGHT = 200; // 键盘最小高度（dp）
            
            @Override
            public void onGlobalLayout() {
                Rect rect = new Rect();
                rootView.getWindowVisibleDisplayFrame(rect);
                int visibleHeight = rect.height();
                int rootHeight = rootView.getHeight();
                int keyboardHeight = rootHeight - visibleHeight;
                
                if (lastVisibleHeight == 0) {
                    lastVisibleHeight = visibleHeight;
                    return;
                }
                
                // 键盘状态变化时
                if (visibleHeight != lastVisibleHeight) {
                    lastVisibleHeight = visibleHeight;
                    
                    boolean isKeyboardVisible = keyboardHeight > MIN_KEYBOARD_HEIGHT;
                    
                    // 调整 WebView 布局参数
                    LinearLayout.LayoutParams params = (LinearLayout.LayoutParams) webView.getLayoutParams();
                    if (isKeyboardVisible) {
                        // 键盘弹出：设置 WebView 高度为可见区域
                        params.height = visibleHeight;
                        params.weight = 0;
                    } else {
                        // 键盘收起：恢复全屏
                        params.height = LinearLayout.LayoutParams.MATCH_PARENT;
                        params.weight = 0;
                    }
                    webView.setLayoutParams(params);
                    
                    // 通知 WebView 内容变化
                    webView.post(() -> {
                        webView.evaluateJavascript(
                            "if (window.visualViewport) {" +
                            "  window.dispatchEvent(new Event('resize'));" +
                            "  if (window.handleResize) window.handleResize();" +
                            "} else {" +
                            "  window.dispatchEvent(new Event('resize'));" +
                            "}", 
                            null
                        );
                    });
                }
            }
        });
    }
    
    private void setupWebView() {
        WebSettings settings = webView.getSettings();
        settings.setJavaScriptEnabled(true);
        settings.setLoadWithOverviewMode(true);
        settings.setUseWideViewPort(true);
        settings.setDomStorageEnabled(true);
        settings.setCacheMode(WebSettings.LOAD_DEFAULT);
        settings.setSupportZoom(true);
        settings.setBuiltInZoomControls(true);
        settings.setDisplayZoomControls(false);
        // 软键盘相关设置
        settings.setJavaScriptCanOpenWindowsAutomatically(true);
        
        webView.setWebViewClient(new WebViewClient() {
            @Override
            public boolean shouldOverrideUrlLoading(WebView view, String url) {
                view.loadUrl(url);
                return true;
            }
        });
        
        // 确保 WebView 可以获取焦点，支持软键盘输入
        webView.setFocusable(true);
        webView.setFocusableInTouchMode(true);
        webView.requestFocus();
    }
    
    private void showWaitingPage() {
        String html = "<html><body style='background:#1e1e1e;color:#fff;" +
            "display:flex;justify-content:center;align-items:center;height:100vh;margin:0;'>" +
            "<div style='text-align:center;'>" +
            "<h2>🖥️ Portmap WebTerm</h2>" +
            "<p>等待PC端启动...</p>" +
            "<p style='color:#888;font-size:14px;'>请在PC运行: portmap web-term</p>" +
            "</div></body></html>";
        webView.loadData(html, "text/html", "UTF-8");
    }
    
    private void handleMqttMessage(String payload) {
        try {
            JSONObject json = new JSONObject(payload);
            String event = json.optString("event", "");

            if ("webterm_started".equals(event)) {
                String url = json.optString("url", "");
                String ip = json.optString("ip", "");
                int port = json.optInt("port", 0);

                if (!url.isEmpty()) {
                    webView.loadUrl(url);
                    Log.i(TAG, "已连接: " + ip + ":" + port);
                }
            }
        } catch (Exception e) {
            Log.e(TAG, "解析失败: " + e.getMessage());
        }
    }
    
    @Override
    protected void onDestroy() {
        super.onDestroy();
        if (mqttManager != null) {
            mqttManager.disconnect();
        }
        if (webView != null) {
            webView.destroy();
        }
    }
}
