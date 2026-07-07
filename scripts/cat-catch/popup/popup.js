// CatCatch Popup Script

const STORAGE_KEY = 'catcatch_m3u8_urls';

function formatTime(ts) {
  const d = new Date(ts);
  return `${d.getHours().toString().padStart(2,'0')}:${d.getMinutes().toString().padStart(2,'0')}:${d.getSeconds().toString().padStart(2,'0')}`;
}

function showToast(msg) {
  const toast = document.getElementById('toast');
  toast.textContent = msg;
  toast.classList.add('show');
  setTimeout(() => toast.classList.remove('show'), 2000);
}

function renderURLs(urls) {
  const list = document.getElementById('urls-list');
  const empty = document.getElementById('empty');
  const count = document.getElementById('count');

  count.textContent = `${urls.length} 条记录`;
  document.getElementById('status').textContent = urls.length > 0 ? '已捕获' : '等待拦截...';

  if (urls.length === 0) {
    list.innerHTML = '';
    list.appendChild(empty);
    empty.style.display = 'block';
    return;
  }

  empty.style.display = 'none';
  list.innerHTML = '';

  urls.forEach((entry, idx) => {
    const div = document.createElement('div');
    div.className = `url-item ${entry.type}`;
    div.innerHTML = `
      <div class="domain">${entry.domain} <span style="color:#e94560">[${entry.type}]</span></div>
      <div class="url" title="${entry.url}">${entry.url}</div>
      <div class="meta">
        <span>⏱ ${formatTime(entry.timestamp)}</span>
        <span>Tab: ${entry.tabId}</span>
      </div>
      <div class="actions">
        <button class="btn btn-copy" data-url="${entry.url}" data-idx="${idx}">复制 URL</button>
        <button class="btn btn-open" data-url="${entry.url}" data-idx="${idx}">下载</button>
      </div>
    `;
    list.appendChild(div);
  });

  // Bind copy button
  list.querySelectorAll('.btn-copy').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const url = btn.dataset.url;
      navigator.clipboard.writeText(url).then(() => {
        showToast('已复制到剪贴板');
      });
    });
  });

  // Bind open/download button
  list.querySelectorAll('.btn-open').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const url = btn.dataset.url;
      // 用 x-video-downloader 下载
      window.open(`x-video-downloader://download?url=${encodeURIComponent(url)}`);
    });
  });
}

function loadURLs() {
  chrome.storage.local.get(STORAGE_KEY, (data) => {
    renderURLs(data[STORAGE_KEY] || []);
  });
}

document.getElementById('clear-btn').addEventListener('click', () => {
  chrome.runtime.sendMessage({ type: 'clear_urls' });
  renderURLs([]);
  showToast('已清空');
});

// Listen for updates from background
chrome.runtime.onMessage.addListener((msg) => {
  if (msg.type === 'urls_updated') {
    loadURLs();
  }
  if (msg.type === 'copy_success') {
    showToast('已复制');
  }
});

// Initial load
loadURLs();
