package com.example.hello;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.Intent;
import android.os.Bundle;
import android.view.LayoutInflater;
import android.view.View;
import android.view.ViewGroup;
import android.widget.BaseAdapter;
import android.widget.Button;
import android.widget.ListView;
import android.widget.TextView;
import android.widget.Toast;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public class WebTermListActivity extends Activity {
    
    private static final String DEFAULT_HUB_URL = "http://10.126.126.6:8080";
    
    private ListView listView;
    private WebTermAdapter adapter;
    private List<WebTermItem> webTermList = new ArrayList<>();
    private String serverId;
    private ExecutorService executor = Executors.newSingleThreadExecutor();
    
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setContentView(R.layout.activity_webterm_list);
        
        serverId = getIntent().getStringExtra("server_id");
        String serverName = getIntent().getStringExtra("server_name");
        String webtermsJson = getIntent().getStringExtra("webterms");
        
        TextView tvTitle = findViewById(R.id.tv_title);
        tvTitle.setText(serverName + " - 选择会话");
        
        listView = findViewById(R.id.list_webterms);
        adapter = new WebTermAdapter();
        listView.setAdapter(adapter);
        
        parseWebTerms(webtermsJson);
        
        listView.setOnItemClickListener((parent, view, position, id) -> {
            WebTermItem item = webTermList.get(position);
            openWebView(item.url);
        });
        
        // 长按停止终端
        listView.setOnItemLongClickListener((parent, view, position, id) -> {
            WebTermItem item = webTermList.get(position);
            showStopConfirmDialog(item);
            return true;
        });
        
        findViewById(R.id.btn_back).setOnClickListener(v -> finish());
    }
    
    private void showStopConfirmDialog(WebTermItem item) {
        new AlertDialog.Builder(this)
            .setTitle("停止终端")
            .setMessage("确定要停止这个终端吗？\n" + item.command)
            .setPositiveButton("停止", (dialog, which) -> {
                sendStopCommand(item.id);
            })
            .setNegativeButton("取消", null)
            .show();
    }
    
    private void sendStopCommand(String webtermId) {
        if (serverId == null) {
            Toast.makeText(this, "Server ID 未知", Toast.LENGTH_SHORT).show();
            return;
        }
        
        executor.execute(() -> {
            HttpURLConnection conn = null;
            try {
                String urlString = DEFAULT_HUB_URL + "/api/servers/" + serverId + "/control";
                URL url = new URL(urlString);
                conn = (HttpURLConnection) url.openConnection();
                conn.setRequestMethod("POST");
                conn.setRequestProperty("Content-Type", "application/json");
                conn.setConnectTimeout(5000);
                conn.setReadTimeout(5000);
                conn.setDoOutput(true);
                
                JSONObject requestBody = new JSONObject();
                requestBody.put("server_id", serverId);
                
                JSONObject command = new JSONObject();
                command.put("action", "stop");
                command.put("webterm_id", webtermId);
                
                requestBody.put("command", command);
                
                OutputStream os = conn.getOutputStream();
                os.write(requestBody.toString().getBytes("UTF-8"));
                os.close();
                
                int responseCode = conn.getResponseCode();
                if (responseCode == 200) {
                    BufferedReader reader = new BufferedReader(
                        new InputStreamReader(conn.getInputStream())
                    );
                    StringBuilder sb = new StringBuilder();
                    String line;
                    while ((line = reader.readLine()) != null) {
                        sb.append(line);
                    }
                    reader.close();
                    
                    JSONObject response = new JSONObject(sb.toString());
                    boolean success = response.getBoolean("success");
                    String message = response.getString("message");
                    
                    runOnUiThread(() -> {
                        Toast.makeText(this, message, Toast.LENGTH_LONG).show();
                        if (success) {
                            // 从列表中移除并刷新
                            removeWebTerm(webtermId);
                        }
                    });
                } else {
                    runOnUiThread(() -> {
                        Toast.makeText(this, "停止失败: " + responseCode, Toast.LENGTH_SHORT).show();
                    });
                }
            } catch (Exception e) {
                runOnUiThread(() -> {
                    Toast.makeText(this, "错误: " + e.getMessage(), Toast.LENGTH_SHORT).show();
                });
            } finally {
                if (conn != null) {
                    conn.disconnect();
                }
            }
        });
    }
    
    private void removeWebTerm(String webtermId) {
        for (int i = 0; i < webTermList.size(); i++) {
            if (webTermList.get(i).id.equals(webtermId)) {
                webTermList.remove(i);
                adapter.notifyDataSetChanged();
                break;
            }
        }
    }
    
    @Override
    protected void onDestroy() {
        super.onDestroy();
        executor.shutdown();
    }
    
    private void parseWebTerms(String json) {
        try {
            JSONArray array = new JSONArray(json);
            for (int i = 0; i < array.length(); i++) {
                JSONObject obj = array.getJSONObject(i);
                WebTermItem item = new WebTermItem();
                item.id = obj.getString("id");
                item.url = obj.getString("url");
                item.command = obj.getString("command");
                item.cwd = obj.optString("cwd", "");
                webTermList.add(item);
            }
            adapter.notifyDataSetChanged();
        } catch (JSONException e) {
            e.printStackTrace();
        }
    }
    
    private void openWebView(String url) {
        Intent intent = new Intent(this, MainActivity.class);
        intent.putExtra("url", url);
        startActivity(intent);
    }
    
    static class WebTermItem {
        String id;
        String url;
        String command;
        String cwd;
    }
    
    static class ViewHolder {
        TextView tvCommand;
        TextView tvUrl;
        Button btnStop;
    }
    
    class WebTermAdapter extends BaseAdapter {
        @Override
        public int getCount() {
            return webTermList.size();
        }
        
        @Override
        public Object getItem(int position) {
            return webTermList.get(position);
        }
        
        @Override
        public long getItemId(int position) {
            return position;
        }
        
        @Override
        public View getView(int position, View convertView, ViewGroup parent) {
            ViewHolder holder;
            
            if (convertView == null) {
                convertView = LayoutInflater.from(WebTermListActivity.this)
                    .inflate(R.layout.item_webterm, parent, false);
                holder = new ViewHolder();
                holder.tvCommand = convertView.findViewById(R.id.tv_command);
                holder.tvUrl = convertView.findViewById(R.id.tv_url);
                holder.btnStop = convertView.findViewById(R.id.btn_stop);
                convertView.setTag(holder);
            } else {
                holder = (ViewHolder) convertView.getTag();
            }
            
            WebTermItem item = webTermList.get(position);
            // 格式: C:\path\to > command args
            String displayText = item.cwd.isEmpty() 
                ? item.command 
                : item.cwd + " > " + item.command;
            holder.tvCommand.setText(displayText);
            holder.tvUrl.setText(item.url);
            
            // 设置停止按钮点击事件（按钮会拦截事件，不会触发列表项点击）
            if (holder.btnStop != null) {
                holder.btnStop.setOnClickListener(v -> {
                    showStopConfirmDialog(item);
                });
            }
            
            // 设置整个列表项点击事件（点击非按钮区域时打开终端）
            convertView.setOnClickListener(v -> {
                openWebView(item.url);
            });
            
            return convertView;
        }
    }
}
