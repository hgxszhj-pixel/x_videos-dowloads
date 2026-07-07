# CatCatch — m3u8 URL 拦截器

## 功能

Chrome 扩展，拦截视频页面的 m3u8 URL，支持：
- ✅ 拦截所有 m3u8 请求（主播放列表 + 媒体播放列表）
- ✅ 拦截带 sign 签名的动态 URL
- ✅ 复制 URL / 直接下载
- ✅ 最多保存 100 条记录

## 安装

1. 打开 Chrome，访问 `chrome://extensions/`
2. 开启「开发者模式」
3. 点击「加载已解压的扩展程序」
4. 选择 `scripts/cat-catch` 文件夹

## 使用流程

### 小鹅通视频下载

1. 在 Chrome 安装插件并打开插件 popup
2. 访问 `https://vip.tulingpyton.cn`
3. 登录账号
4. 进入课程，点击播放视频
5. 插件自动拦截 m3u8 URL（显示在 popup 中）
6. 点击「复制 URL」或「下载」
7. 将 m3u8 URL 粘贴到 x-video-downloader 下载

### 支持平台

- 小鹅通（vip.tulingpyton.cn）
- 所有基于小鹅通的课程站
- 其他视频平台（自动拦截所有 .m3u8 请求）

## 文件结构

```
cat-catch/
├── manifest.json       # 扩展配置
├── background.js       # 后台拦截逻辑
├── popup/
│   ├── popup.html      # 弹出界面
│   └── popup.js       # Popup 脚本
└── icons/
    └── icon*.png      # 图标
```

## 技术原理

1. 使用 `chrome.webRequest.onCompleted` 拦截所有 HTTP 请求
2. 过滤包含 `.m3u8` 的 URL
3. 存储到 `chrome.storage.local`
4. Popup 页面展示拦截结果

## 注意

- sign 签名有时效，拦截后尽快下载
- 会员专属内容需要登录账号才能播放
