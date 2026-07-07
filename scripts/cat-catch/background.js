// CatCatch - 后台 Service Worker
// 拦截所有 m3u8 URL 请求并存储，供 popup 查看

const STORAGE_KEY = 'catcatch_m3u8_urls';

// 拦截网络请求
chrome.webRequest.onCompleted.addListener(
  (details) => {
    const url = details.url;

    // 只处理 m3u8 请求
    if (!url.includes('.m3u8')) return;

    // 忽略 manifest master playlist（不含 .m3u8 直接包含分段）
    if (url.includes('main.m3u8') || url.includes('master.m3u8')) {
      // 这些是主播放列表，包含多种清晰度
      addUrl(url, 'master', details);
    } else if (url.includes('/hls/') || url.includes('/video/')) {
      // 媒体播放列表（包含分段）
      addUrl(url, 'media', details);
    } else {
      addUrl(url, 'other', details);
    }
  },
  { urls: ['*://*/*'] }
);

function addUrl(url, type, details) {
  const entry = {
    url,
    type,
    timestamp: Date.now(),
    tabId: details.tabId,
    frameId: details.frameId,
    // 从 URL 中提取域名
    domain: new URL(url).hostname,
  };

  chrome.storage.local.get(STORAGE_KEY, (data) => {
    const existing = data[STORAGE_KEY] || [];
    // 去重
    if (!existing.some(e => e.url === url)) {
      existing.unshift(entry); // 最新在前
      // 最多保留 100 条
      const trimmed = existing.slice(0, 100);
      chrome.storage.local.set({ [STORAGE_KEY]: trimmed });
      // 通知 popup 更新
      chrome.runtime.sendMessage({ type: 'urls_updated', count: trimmed.length });
    }
  });
}

// 清空记录
chrome.runtime.onMessage.addListener((msg) => {
  if (msg.type === 'clear_urls') {
    chrome.storage.local.set({ [STORAGE_KEY]: [] });
    chrome.runtime.sendMessage({ type: 'urls_updated', count: 0 });
  }
});

// 复制到剪贴板
chrome.runtime.onMessage.addListener((msg) => {
  if (msg.type === 'copy_url' && msg.url) {
    navigator.clipboard.writeText(msg.url).then(() => {
      chrome.runtime.sendMessage({ type: 'copy_success', url: msg.url });
    });
  }
});
