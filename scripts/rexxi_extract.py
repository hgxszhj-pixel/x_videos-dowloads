#!/usr/bin/env python3
"""
rexxi_extract.py - rexxi.cn / 珊瑚课堂 课程页 m3u8 提取 + yt-dlp 下载

macOS 适配版，基于 ~/.agents/skills/rexxi-course-m3u8/SKILL.md + 本机实测 (2026-09-13)
关键差异:
  - SKILL.md 说走 liveAutha,但实测该方法返回 undefined 不发请求(因为课程用 noteCourseId 字段)
  - 改为直接 $axios.post('/stu/course/listSections' + '/stu/course/videoAuth')
  - 章节数据用 listSections API 拿全(分页 num=30),不依赖 DOM
  - 80/83 章节 videos=[] 空数组(SKILL.md 第 92 行陷阱),自动跳过
  - yt-dlp 下载并发化(multiprocessing.Pool,默认 4 worker)

用法:
    python3 scripts/rexxi_extract.py [course_url] [output_dir] [--jobs N]

前置:
    1. Chrome 调试端口 9223 监听中:
       "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \\
         --remote-debugging-port=9223 --remote-allow-origins='*' \\
         --user-data-dir=/tmp/chrome-rexxi-profile --no-first-run
    2. 用户已在 Chrome 里登录目标 rexxi 站点
"""
import argparse
import json
import multiprocessing as mp
import os
import re
import subprocess
import sys
import time
from pathlib import Path
from urllib.parse import urlparse

import requests
import websocket

CDP_HTTP = "http://127.0.0.1:9223"
DEFAULT_OUT = Path.home() / "Downloads" / "rexxi-courses" / "3529714"


def cdp_new_tab():
    return requests.put(f"{CDP_HTTP}/json/new", timeout=5).json()


def cdp_close(tab_id):
    try:
        requests.get(f"{CDP_HTTP}/json/close/{tab_id}", timeout=5)
    except Exception:
        pass


class CDP:
    def __init__(self, ws_url, timeout=30):
        self.ws = websocket.create_connection(ws_url, timeout=timeout)
        self.ws.settimeout(timeout)
        self._id = 0

    def send(self, method, params=None):
        self._id += 1
        msg = {"id": self._id, "method": method, "params": params or {}}
        self.ws.send(json.dumps(msg))
        while True:
            r = json.loads(self.ws.recv())
            if r.get("id") == self._id:
                return r

    def eval(self, expr, await_promise=False):
        r = self.send("Runtime.evaluate", {
            "expression": expr, "returnByValue": True, "awaitPromise": await_promise,
        })
        return r.get("result", {}).get("result", {}).get("value")

    def navigate(self, url):
        self.send("Page.navigate", {"url": url})

    def wait_app_mount(self, timeout=30):
        for _ in range(int(timeout * 2)):
            n = self.eval("document.querySelectorAll('#app > *').length")
            if n and n > 0:
                return True
            time.sleep(0.5)
        return False

    def wait_chapter_list(self, timeout=20):
        """等章节 title 出现(SPA 路由 + listSections API 完成)"""
        for _ in range(int(timeout * 2)):
            n = self.eval("document.querySelectorAll('.chapter-item-title').length")
            if n and n > 0:
                return True
            time.sleep(0.5)
        return False

    def close(self):
        try:
            self.ws.close()
        except Exception:
            pass


# 探测 Vue 实例 + 拿 ID + 走 listSections API(翻页)
PROBE_AND_LIST_EXPR = """
(async () => {
  const titles = document.querySelectorAll('.chapter-item-title');
  if (!titles.length) return { error: 'no chapter titles - need to navigate first' };
  let el = titles[0];
  while (el && !el.__vue__) el = el.parentElement;
  const vm = el.__vue__;
  let liveAuthaVm = vm;
  while (liveAuthaVm && typeof liveAuthaVm.liveAutha !== 'function') {
    liveAuthaVm = liveAuthaVm.$parent;
  }
  const axiosInst = liveAuthaVm.$axios;
  const class_id = liveAuthaVm.class_id ?? liveAuthaVm.$data?.class_id;
  const course_id = liveAuthaVm.$data?.noteCourseId ?? liveAuthaVm.course_id;
  const org_id = liveAuthaVm.orgId ?? liveAuthaVm.org_id ?? liveAuthaVm.$data?.orgId;

  // 翻页拿全部章节
  const allChapters = [];
  for (let page = 1; page <= 10; page++) {
    const r = await axiosInst.post('/stu/course/listSections', {
      org_id, course_id, class_id, page, num: 30
    });
    if (r?.data?.code !== 200) return { error: `listSections p${page} fail`, resp: r?.data };
    const sections = r.data.data || [];
    if (!sections.length) break;
    for (const sec of sections) {
      for (const ch of (sec.chapters || [])) {
        const videos = ch.videos || [];
        if (!videos.length) continue;  // SKILL.md 第 92 行陷阱
        allChapters.push({
          chapterId: ch.id,
          videoId: videos[0].id,
          name: ch.name,
          duration: videos[0].duration,
          view_time: videos[0].view_time,
        });
      }
    }
    if (sections.length < 30) break;
  }
  return { class_id, course_id, org_id, chapters: allChapters };
})()
"""


def sanitize_filename(name):
    name = re.sub(r'[\\/:*?"<>|]', '_', name)
    return name.strip().strip('.') or 'unnamed'


def download_one(item):
    """worker 函数 — 给 multiprocessing 用,返回 (name, status, msg)"""
    name, m3u8, _ch = item
    out_path = Path(item[2].get('_out_dir', '')) / f"{name}.mp4" if '_out_dir' in item[2] else None
    # 实际 out_dir 由调用方通过 env 传
    out_dir = Path(os.environ.get("REXXI_OUT_DIR", "."))
    tp = out_dir / f"{name}.mp4"
    if tp.exists() and tp.stat().st_size > 1024 * 1024:
        return (name, "skip", f"{tp.stat().st_size // 1024 // 1024} MB")
    cmd = ["yt-dlp", "-o", str(tp),
           "--no-part", "--no-mtime", "--no-warnings",
           "--quiet", "--no-progress", m3u8]
    try:
        r = subprocess.run(cmd, timeout=1800, check=True, capture_output=True)
        if tp.exists():
            sz = tp.stat().st_size / 1024 / 1024
            return (name, "ok", f"{sz:.0f} MB")
        return (name, "fail", "no output file")
    except subprocess.TimeoutExpired:
        return (name, "timeout", "1800s")
    except subprocess.CalledProcessError as e:
        err = e.stderr.decode(errors='replace')[-300:]
        return (name, "fail", f"exit {e.returncode}: {err}")


def resolve_m3u8(mp4_url):
    """mp4Url 302 → 火山引擎直链"""
    try:
        r = requests.get(mp4_url, allow_redirects=False, timeout=10)
        if r.status_code in (301, 302) and r.headers.get('Location'):
            return r.headers['Location']
        host = urlparse(mp4_url).hostname
        appid = host.split('.')[0] if host else None
        if appid:
            r2 = requests.get(f"{mp4_url}?appid={appid}&all_play_url=true", timeout=10)
            data = r2.json()
            play_urls = data.get('data', {}).get('play_urls') or data.get('play_urls') or []
            if play_urls:
                return play_urls[0].get('url')
    except Exception as e:
        print(f"  ⚠️ resolve error: {e}")
    return None


def main():
    parser = argparse.ArgumentParser(description="rexxi 课程视频提取 + yt-dlp 并发下载")
    parser.add_argument("url", nargs="?",
                        default="https://wx-bgfcbieh.rexxi.cn/PC/#/yueke3/3529714")
    parser.add_argument("output_dir", nargs="?", default=str(DEFAULT_OUT))
    parser.add_argument("--jobs", "-j", type=int,
                        default=min(4, max(1, mp.cpu_count())),
                        help=f"并发下载数(默认 min(4, cpu_count)={min(4, max(1, mp.cpu_count()))})")
    args = parser.parse_args()
    url = args.url
    out_dir = Path(args.output_dir)
    jobs = args.jobs
    out_dir.mkdir(parents=True, exist_ok=True)
    print(f"⚙️  jobs={jobs}")

    print(f"📂 Output: {out_dir}")
    print(f"🌐 URL: {url}")

    target = cdp_new_tab()
    cdp = CDP(target['webSocketDebuggerUrl'], timeout=30)

    try:
        cdp.navigate(url)
        if not cdp.wait_app_mount(timeout=30):
            print("❌ #app 未挂载")
            return
        # 等章节列表渲染(listSections API 完成)
        if not cdp.wait_chapter_list(timeout=20):
            print("⚠️  章节列表未渲染,继续(可能鉴权失效)")

        token = cdp.eval("localStorage.getItem('token')") or ""
        if not token or len(str(token)) < 10:
            print(f"❌ 未登录: token={token!r}")
            return
        print(f"✅ Token {len(token)} chars")

        # 走 API 拿章节列表
        info = cdp.eval(PROBE_AND_LIST_EXPR, await_promise=True)
        if not info or info.get('error'):
            print(f"❌ {info}")
            return
        chapters = info['chapters']
        print(f"✅ class_id={info['class_id']}, course_id={info['course_id']}")
        print(f"📚 有回看的章节: {len(chapters)} 个")

        if not chapters:
            print("⚠️  本课程还没有任何回看")
            return

        # 写章节清单(备用)
        ch_file = out_dir / "chapters.json"
        with open(ch_file, "w") as f:
            json.dump(chapters, f, ensure_ascii=False, indent=2)
        for c in chapters:
            print(f"   - {c['name']}  ({c['duration']//60} 分钟)")

        # 用 $axios 逐个调 videoAuth(在 CDP 端并发 5)
        print(f"\n🔄 逐个 videoAuth + 解析 m3u8...")
        items = []
        ok_count = 0
        for i, ch in enumerate(chapters, 1):
            print(f"   [{i}/{len(chapters)}] {ch['name']}", end=" ... ", flush=True)
            mp4url = cdp.eval(f"""
(async () => {{
  const titles = document.querySelectorAll('.chapter-item-title');
  let el = titles[0];
  while (el && !el.__vue__) el = el.parentElement;
  const vm = el.__vue__;
  let liveAuthaVm = vm;
  while (liveAuthaVm && typeof liveAuthaVm.liveAutha !== 'function') liveAuthaVm = liveAuthaVm.$parent;
  const axiosInst = liveAuthaVm.$axios;
  try {{
    const r = await axiosInst.post('/stu/course/videoAuth', {{
      course_id: {info['course_id']}, class_id: {info['class_id']},
      chapter_id: {ch['chapterId']}, video_id: {ch['videoId']},
    }});
    if (r?.data?.code === 200 && r?.data?.data?.video) return r.data.data.video.mp4Url;
    return {{ error: r?.data?.msg || ('code=' + r?.data?.code) }};
  }} catch (e) {{ return {{ error: String(e).slice(0, 200) }}; }}
}})()
""", await_promise=True)
            if not mp4url or isinstance(mp4url, dict):
                err = mp4url.get('error', 'unknown') if isinstance(mp4url, dict) else 'null'
                print(f"❌ videoAuth: {err}")
                continue
            real = resolve_m3u8(mp4url)
            if not real:
                print(f"❌ 302/JSON 失败")
                continue
            safe = sanitize_filename(ch['name'])
            items.append((safe, real, ch))
            ok_count += 1
            print(f"✓")
        print(f"✅ videoAuth {ok_count}/{len(chapters)} 成功")

        if not items:
            print("❌ 无可下载视频")
            return

        # 写 urls.txt
        urls_file = out_dir / "urls.txt"
        with open(urls_file, "w") as f:
            for name, m3u8, _ in items:
                f.write(f"{m3u8};{name}\n")
        print(f"📝 urls.txt: {urls_file}")

        # yt-dlp 并发下载
        print(f"\n⬇️  yt-dlp 下载到 {out_dir}/ (并发 {jobs})")
        # 通过 env 把 out_dir 传给 worker(multiprocessing.Pool 不便传 Path)
        os.environ["REXXI_OUT_DIR"] = str(out_dir)
        # 已有文件用 size > 1MB 判定完整
        todo = [(name, m3u8, ch) for name, m3u8, ch in items
                if not (out_dir / f"{name}.mp4").exists()
                or (out_dir / f"{name}.mp4").stat().st_size < 1024 * 1024]
        skip_count = len(items) - len(todo)
        if skip_count:
            print(f"   ↪ {skip_count} 个已下载,跳过")
        if not todo:
            print("   ✓ 全部已完成")
        else:
            print(f"   待下载 {len(todo)} 个")
            # mp.Pool + imap_unordered 边下边报
            with mp.Pool(processes=jobs) as pool:
                for name, status, msg in pool.imap_unordered(download_one, todo):
                    sym = {"ok": "✓", "skip": "↪", "timeout": "⏱", "fail": "❌"}.get(status, "?")
                    print(f"   {sym} {name}: {msg}")

        print("\n📁 最终:")
        for p in sorted(out_dir.glob("*.mp4")):
            sz = p.stat().st_size / 1024 / 1024
            print(f"   {sz:6.0f} MB  {p.name}")

    finally:
        cdp.close()
        cdp_close(target['id'])


if __name__ == "__main__":
    main()
