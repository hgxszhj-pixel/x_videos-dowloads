# Twitter/X Cookie 获取指南

## 为什么需要 Cookie？

使用 Cookie 认证可以：
- ✅ 提高视频下载成功率
- ✅ 访问私有账号的视频（如果你关注了）
- ✅ 绕过部分地区限制
- ✅ 避免频繁请求被限制

## 获取 Cookie 的方法

### 方法1：使用浏览器扩展（推荐）

1. **安装 Chrome 扩展**：Get cookies.txt LOCALLY
   - Chrome Web Store: https://chrome.google.com/webstore/detail/get-cookiestxt-locally/bgaddhkoddajdggbgdbmaobjcimcjgl

2. **导出 Cookie**：
   - 登录 Twitter/X 账号
   - 点击扩展图标
   - 选择 "Export" → "Netscape HTTP Cookie File"
   - 保存为 `twitter_cookies.txt`

### 方法2：使用 EditThisCookie

1. **安装扩展**：EditThisCookie
2. **导出 Cookie**：
   - 登录 Twitter
   - 点击扩展图标
   - 点击 "导出"
   - 选择 "Netscape format"

### 方法3：手动复制（较复杂）

1. 登录 Twitter/X
2. 按 F12 打开开发者工具
3. Application → Cookies
4. 复制所有 Cookie 值

## 使用方法

### CLI
```bash
# 使用 Cookie 文件
x-video-downloader "URL" --cookies twitter_cookies.txt

# 同时使用代理
x-video-downloader "URL" --cookies twitter_cookies.txt --proxy "http://127.0.0.1:7890"
```

### GUI
1. 点击 "Browse" 按钮选择 Cookie 文件
2. 然后获取视频信息

## 注意事项

⚠️ **隐私安全**：
- Cookie 文件包含登录信息，请妥善保管
- 不要分享给他人
- 使用后及时删除

⚠️ **有效期**：
- Cookie 可能会过期，需要定期更新
- 建议每周重新导出一次

⚠️ **账号安全**：
- 只导出必要的 Cookie，不要使用他人账号
- 建议使用小号进行操作
