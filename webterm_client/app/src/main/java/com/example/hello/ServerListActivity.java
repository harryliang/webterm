package com.example.hello;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.Intent;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;
import android.view.LayoutInflater;
import android.view.View;
import android.view.ViewGroup;
import android.widget.AdapterView;
import android.widget.BaseAdapter;
import android.widget.Button;
import android.widget.EditText;
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

public class ServerListActivity extends Activity {
    private static final String TAG = "ServerListActivity";
    private static final String DEFAULT_HUB_URL = "http://10.126.126.6:8080";
    
    private ListView listView;
    private ServerAdapter adapter;
    private List<ServerInfo> serverList = new ArrayList<>();
    private ExecutorService executor = Executors.newSingleThreadExecutor();
    private Handler mainHandler = new Handler(Looper.getMainLooper());
    
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setContentView(R.layout.activity_server_list);
        
        listView = findViewById(R.id.list_servers);
        adapter = new ServerAdapter();
        listView.setAdapter(adapter);
        
        listView.setOnItemClickListener((parent, view, position, id) -> {
            ServerInfo server = serverList.get(position);
            // 点击列表项时，直接打开该 server 的第一个 webterm 或显示列表
            openServerDetail(server);
        });
        
        // 长按显示更多选项
        listView.setOnItemLongClickListener((parent, view, position, id) -> {
            ServerInfo server = serverList.get(position);
            showServerOptions(server);
            return true;
        });
        
        // 下拉刷新
        findViewById(R.id.btn_refresh).setOnClickListener(v -> loadServers());
        
        // 设置 Hub 地址
        findViewById(R.id.btn_settings).setOnClickListener(v -> {
            // TODO: 打开设置对话框修改 Hub 地址
            Toast.makeText(this, "Hub: " + DEFAULT_HUB_URL, Toast.LENGTH_SHORT).show();
        });
        
        // 初始加载
        loadServers();
    }
    
    private void loadServers() {
        executor.execute(() -> {
            HttpURLConnection conn = null;
            try {
                String urlString = DEFAULT_HUB_URL + "/api/servers";
                URL url = new URL(urlString);
                conn = (HttpURLConnection) url.openConnection();
                conn.setRequestMethod("GET");
                conn.setConnectTimeout(5000);
                conn.setReadTimeout(5000);
                
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
                    parseAndDisplayServers(sb.toString());
                } else {
                    showError("加载失败: " + responseCode);
                }
            } catch (Exception e) {
                showError("网络错误: " + e.getMessage());
            } finally {
                if (conn != null) {
                    conn.disconnect();
                }
            }
        });
    }
    
    private void parseAndDisplayServers(String json) {
        try {
            JSONArray array = new JSONArray(json);
            List<ServerInfo> servers = new ArrayList<>();
            
            for (int i = 0; i < array.length(); i++) {
                JSONObject obj = array.getJSONObject(i);
                ServerInfo server = new ServerInfo();
                server.id = obj.getString("id");
                server.name = obj.getString("name");
                server.user = obj.getString("user");
                server.hostname = obj.getString("hostname");
                
                // 解析 webterms
                JSONArray wtArray = obj.getJSONArray("webterms");
                for (int j = 0; j < wtArray.length(); j++) {
                    JSONObject wtObj = wtArray.getJSONObject(j);
                    WebTermInfo wt = new WebTermInfo();
                    wt.id = wtObj.getString("id");
                    wt.url = wtObj.getString("url");
                    wt.command = wtObj.getString("command");
                    wt.cwd = wtObj.optString("cwd", "");
                    server.webterms.add(wt);
                }
                
                servers.add(server);
            }
            
            mainHandler.post(() -> {
                serverList = servers;
                adapter.notifyDataSetChanged();
                Toast.makeText(this, "加载了 " + servers.size() + " 个 Server", Toast.LENGTH_SHORT).show();
            });
            
        } catch (JSONException e) {
            showError("解析错误: " + e.getMessage());
        }
    }
    
    private void openServerDetail(ServerInfo server) {
        if (server.webterms.isEmpty()) {
            // 如果没有活跃的终端，提示用户是否创建新终端
            new AlertDialog.Builder(this)
                .setTitle(server.name)
                .setMessage("该 Server 没有活跃的终端，是否创建新终端？")
                .setPositiveButton("创建", (dialog, which) -> {
                    showStartTerminalDialog(server);
                })
                .setNegativeButton("取消", null)
                .show();
            return;
        }
        
        // 如果有多个 webterms，打开列表选择
        if (server.webterms.size() > 1) {
            Intent intent = new Intent(this, WebTermListActivity.class);
            intent.putExtra("server_id", server.id);
            intent.putExtra("server_name", server.name);
            // 传递 webterms 数据
            JSONArray wtArray = new JSONArray();
            for (WebTermInfo wt : server.webterms) {
                try {
                    JSONObject obj = new JSONObject();
                    obj.put("id", wt.id);
                    obj.put("url", wt.url);
                    obj.put("command", wt.command);
                    obj.put("cwd", wt.cwd);
                    wtArray.put(obj);
                } catch (JSONException e) {
                    Log.e(TAG, "JSON error", e);
                }
            }
            intent.putExtra("webterms", wtArray.toString());
            startActivity(intent);
        } else {
            // 只有一个，直接打开
            openWebView(server.webterms.get(0).url);
        }
    }
    
    private void showServerOptions(ServerInfo server) {
        String[] options = {"打开终端", "启动新终端", "停止终端", "刷新"};
        new AlertDialog.Builder(this)
            .setTitle(server.name)
            .setItems(options, (dialog, which) -> {
                switch (which) {
                    case 0:
                        openServerDetail(server);
                        break;
                    case 1:
                        showStartTerminalDialog(server);
                        break;
                    case 2:
                        showStopTerminalDialog(server);
                        break;
                    case 3:
                        loadServers();
                        break;
                }
            })
            .show();
    }
    
    private void openWebView(String url) {
        Intent intent = new Intent(this, MainActivity.class);
        intent.putExtra("url", url);
        startActivity(intent);
    }
    
    private void showError(String message) {
        mainHandler.post(() -> {
            Toast.makeText(this, message, Toast.LENGTH_LONG).show();
        });
    }
    
    @Override
    protected void onDestroy() {
        super.onDestroy();
        executor.shutdown();
    }
    
    // 数据类
    static class ServerInfo {
        String id;
        String name;
        String user;
        String hostname;
        List<WebTermInfo> webterms = new ArrayList<>();
    }
    
    static class WebTermInfo {
        String id;
        String url;
        String command;
        String cwd;
    }
    
    // 适配器
    class ServerAdapter extends BaseAdapter {
        @Override
        public int getCount() {
            return serverList.size();
        }
        
        @Override
        public Object getItem(int position) {
            return serverList.get(position);
        }
        
        @Override
        public long getItemId(int position) {
            return position;
        }
        
        @Override
        public View getView(int position, View convertView, ViewGroup parent) {
            ViewHolder holder;
            
            if (convertView == null) {
                convertView = LayoutInflater.from(ServerListActivity.this)
                    .inflate(R.layout.item_server, parent, false);
                holder = new ViewHolder();
                holder.tvName = convertView.findViewById(R.id.tv_server_name);
                holder.tvInfo = convertView.findViewById(R.id.tv_server_info);
                holder.tvStatus = convertView.findViewById(R.id.tv_status);
                holder.btnStart = convertView.findViewById(R.id.btn_start);
                holder.btnKill = convertView.findViewById(R.id.btn_kill);
                convertView.setTag(holder);
            } else {
                holder = (ViewHolder) convertView.getTag();
            }
            
            ServerInfo server = serverList.get(position);
            holder.tvName.setText(server.name);
            holder.tvInfo.setText(server.hostname + " | " + server.user);
            holder.tvStatus.setText(server.webterms.size() + " 个会话");
            
            // 设置按钮点击事件（按钮会拦截事件，不会触发列表项点击）
            holder.btnStart.setOnClickListener(v -> {
                showStartTerminalDialog(server);
            });
            
            holder.btnKill.setOnClickListener(v -> {
                showStopTerminalDialog(server);
            });
            
            // 设置整个列表项点击事件（点击非按钮区域时打开终端）
            convertView.setOnClickListener(v -> {
                openServerDetail(server);
            });
            
            return convertView;
        }
    }
    
    static class ViewHolder {
        TextView tvName;
        TextView tvInfo;
        TextView tvStatus;
        Button btnStart;
        Button btnKill;
    }
    
    /**
     * 发送控制命令到 Hub
     * @param serverId Server ID
     * @param action 动作："start" 或 "stop"
     * @param webtermId 对于 stop 动作，指定 webterm_id
     * @param customCmd 对于 start 动作，可选的自定义命令
     * @param customArgs 对于 start 动作，可选的命令参数
     */
    private void sendControlCommand(String serverId, String action, String webtermId, 
                                     String customCmd, List<String> customArgs) {
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
                
                // 构建请求体
                JSONObject requestBody = new JSONObject();
                requestBody.put("server_id", serverId);
                
                JSONObject command = new JSONObject();
                command.put("action", action);
                
                if ("start".equals(action)) {
                    if (customCmd != null) {
                        command.put("cmd", customCmd);
                    }
                    if (customArgs != null && !customArgs.isEmpty()) {
                        JSONArray argsArray = new JSONArray();
                        for (String arg : customArgs) {
                            argsArray.put(arg);
                        }
                        command.put("args", argsArray);
                    }
                } else if ("stop".equals(action) && webtermId != null) {
                    command.put("webterm_id", webtermId);
                }
                
                requestBody.put("command", command);
                
                // 发送请求
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
                    
                    // 解析响应
                    JSONObject response = new JSONObject(sb.toString());
                    boolean success = response.getBoolean("success");
                    String message = response.getString("message");
                    
                    mainHandler.post(() -> {
                        Toast.makeText(this, message, Toast.LENGTH_LONG).show();
                        if (success) {
                            // 成功后刷新列表
                            loadServers();
                        }
                    });
                } else {
                    showError("控制命令失败: " + responseCode);
                }
            } catch (Exception e) {
                showError("发送命令错误: " + e.getMessage());
            } finally {
                if (conn != null) {
                    conn.disconnect();
                }
            }
        });
    }
    
    /**
     * 显示启动新终端的对话框
     */
    private void showStartTerminalDialog(ServerInfo server) {
        AlertDialog.Builder builder = new AlertDialog.Builder(this);
        builder.setTitle("启动新终端 - " + server.name);
        
        final EditText input = new EditText(this);
        input.setHint("输入命令（可选，默认使用系统 shell）");
        builder.setView(input);
        
        builder.setPositiveButton("启动", (dialog, which) -> {
            String cmd = input.getText().toString().trim();
            if (cmd.isEmpty()) {
                cmd = null; // 使用默认命令
            }
            sendControlCommand(server.id, "start", null, cmd, null);
        });
        
        builder.setNegativeButton("取消", (dialog, which) -> dialog.cancel());
        
        builder.show();
    }
    
    /**
     * 显示停止终端的对话框
     */
    private void showStopTerminalDialog(ServerInfo server) {
        if (server.webterms.isEmpty()) {
            Toast.makeText(this, "该 Server 没有活跃的终端", Toast.LENGTH_SHORT).show();
            return;
        }
        
        // 如果有多个 webterms，让用户选择
        String[] items = new String[server.webterms.size()];
        for (int i = 0; i < server.webterms.size(); i++) {
            WebTermInfo wt = server.webterms.get(i);
            items[i] = wt.command + " (" + wt.id.substring(0, 8) + "...)";
        }
        
        AlertDialog.Builder builder = new AlertDialog.Builder(this);
        builder.setTitle("选择要停止的终端");
        builder.setItems(items, (dialog, which) -> {
            WebTermInfo wt = server.webterms.get(which);
            sendControlCommand(server.id, "stop", wt.id, null, null);
        });
        builder.setNegativeButton("取消", null);
        builder.show();
    }
}
