# 抖音 Cookie 导出指南

## 步骤 1: 安装 Cookie 导出扩展

推荐使用以下扩展之一：
- **EditThisCookie** (Chrome): https://chrome.google.com/webstore/detail/editthiscookie/fngmhnnpilhplaeedifhccceomclgfbg
- **Cookie-Editor** (Chrome/Firefox): https://cookie-editor.cgagnier.org/

## 步骤 2: 登录抖音

在浏览器中打开 https://www.douyin.com 并**登录账号**

## 步骤 3: 导出 Cookies

1. 打开抖音任意页面
2. 点击扩展图标 → EditThisCookie 或 Cookie-Editor
3. 点击"Export"导出
4. 选择"Netscape"格式（不是 JSON）
5. 复制导出内容

## 步骤 4: 保存 Cookie 文件并配置到项目

```bash
# 创建目录（如果不存在）
mkdir -p ~/.config/x_video_downloader

# 保存 cookie 文件
vim ~/.config/x_video_downloader/douyin_cookies.txt
# 粘贴内容，保存退出
```

## 步骤 5: 配置到 x_video_downloader

在配置文件 `~/.config/x_video_downloader/config.toml` 中添加：

```toml
[profile]
douyin_cookie_file = "~/.config/x_video_downloader/douyin_cookies.txt"
```

## 必需 Cookies

抖音需要以下 cookies（必须包含）:

| Cookie 名称 | 说明 |
|-------------|------|
| sessionid | 登录会话（必需） |
| ttwid | 测试服 Cookie（必需） |
| sid_guard | Session 保护 |
| s_v_web_id | 浏览器指纹 |
| msToken | 行为令牌 |

## 验证 Cookie

```bash
# 测试 cookie 是否有效
yt-dlp --cookies ~/.config/x_video_downloader/douyin_cookies.txt \
  --list-formats "https://www.douyin.com/video/7231679494828376333"
```

## 快速验证脚本

```bash
# 检查 cookie 文件是否存在
ls -la ~/.config/x_video_downloader/douyin_cookies.txt

# 查看 cookie 内容（不包含敏感值）
grep -E "^#|sessionid|ttwid|sid_guard|s_v_web_id|msToken" ~/.config/x_video_downloader/douyin_cookies.txt
```

## 已知问题

如果仍失败：
1. 确保在导出后**没有重新登录**或**清除浏览器缓存**
2. sessionid 可能过期（需要重新导出）
3. 某些视频可能设置了地区限制
