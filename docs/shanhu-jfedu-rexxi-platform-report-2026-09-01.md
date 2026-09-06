# 京峰教育 / SHANHU 珊瑚在线 — 平台解析报告

**URL**: `https://wx-bgfcbieh.rexxi.cn/PC/#/yueke3/3529714`
**分析日期**: 2026-09-01
**结论**: **当前无法下载任何视频** — 该课程首课 2026-09-02 20:00 才开课,83 个章节的 `videos[]` 全部为空、直播流走腾讯 TRTC WebRTC、任何播放接口均需登录。

---

## 1. 平台识别

### 1.1 真正的 SaaS 后端

URL 域名 `wx-bgfcbieh.rexxi.cn` 只是**白标域名**,真实后端在 `study.wanzibook.com`(万字)。配置中心(`resshktonline.tengfacexuexi.com/web/config/requestApi/default.json`)暴露 11 个 SHANHU 集群域名:

| 域名 | 用途 |
|---|---|
| `study.wanzibook.com` | 在线 API(SHANHU_ONLINE_API) |
| `live.shkt.online` | 直播 API(SHANHU_LIVE_API) |
| `data.shkt.online` | 数据 API(SHANHU_DATA_API) |
| `livehy.shkt.online` / `onlinehy.shkt.online` | 河南节点备份 |
| `wzscrm.shkt.online` / `api.tengface.net` | CRM |
| `api.changshaxierzhenxuan1.cn` | 房产云(SHANHU_FYC_API) |
| `logistics.shkt.online` | 物流 |
| `payhy.shkt.online` | 河南支付 |

### 1.2 租户

`getWebBaseInfo?key=wx-bgfcbieh.rexxi.cn` 返回:

- **租户名**: 京峰教育-专注IT高薪在线教育培训
- **org_id**: 27632958
- **官方站**: www.jfedu.net / bbs.jfedu.net (Linux论坛)
- **微信号**: jfedu123
- **客服邮箱**: support@jfedu.net
- **品牌资源**: 腾讯云 COS / 阿里云 OSS 存储

### 1.3 配置里的危险标志

```json
{
  "SHANHU_ONLINE_API": "https://study.wanzibook.com",
  "IS_EXCEPTION_MODE": true     ← 关键:这是试用/异常模式
}
```

`IS_EXCEPTION_MODE: true` 暗示租户可能处于 demo / 沙盒模式,部分功能(`canUseVideoUrl: false`)被人为限制。即使后续开课,实际能力可能受限。

---

## 2. 课程元数据

| 字段 | 值 |
|---|---|
| course_id | 3529714 |
| class_id | 3023639(周期名 `JF2153班`) |
| 课程名 | 云计算就业班 |
| 学习人数 | 246 |
| 课节数 | 83(全部 status=1 待开课 / play_style=3) |
| 首课时间 | **2026-09-02 20:00:00 — 22:00:00** |
| 末课时间 | 2026-10 月底 |
| 课节状态 | 全部 `videos:[]`、`file:null`、`show_playback:1` |

每个课节有 `ds_id`(数据源 ID,如 626896)和 `interact_id`(play_style=4 才用,此课全空)。

**重要**: 同 org_id 下探测 `course_id` 范围 3529700~3529730 全部返回 `{"code":400,"message":"课程不存在"}`。这是该租户**唯一公开课程**。

---

## 3. 三种直播/播放模式(从 JS 反编译得出)

### 3.1 play_style=3 — 标准直播间(**本课所用**)

```js
liveAddress(e, t) {
  // e = chapter {ds_id, room_id, interact_id, play_style, ...}
  // t = response from /stu/course/liveAuth: {data: {liveToken, ...}}
  const { live_domain } = await getMiniConfig();  // 从 miniConfig.json 取 wx_h5_url
  const url = `${live_domain}/?ds_id=${e.ds_id}&token=${t.data.liveToken}&other=1&shool_token=${localStorage.getItem("token")}`;
  window.open(url);
}
```

新窗口加载 TRTC 播放器,WebRTC 拉流(`webrtc://` → 内部转 `.flv` + `.m3u8`)。

### 3.2 play_style=4 — 腾讯交互课堂

```js
if (4 == e.play_style) {
  window.open(`${live_domain}/tcicMiddlePage/#/pc?classid=${e.interact_id}&fans_id=${this.fans_id}&ds_id=${e.ds_id}`);
}
```

用腾讯 `tcicMiddlePage`,需要 `interact_id`(本课全空)。

### 3.3 fakeLiveJump — VOD 伪装直播

```js
fakeLiveJump(e, t) {
  const url = `${wx_h5_url}/f?ds_id=${e.ds_id}&tid=${e.room_id}&token=${t.data.liveToken}&other=1&shool_token=...`;
  window.open(url);
}
```

`/f/` 路径 + `tid=room_id` 表明这是 VOD 文件当直播流播放。

### 3.4 播放器底层

JS 引用 `TCPlayer`(`SERVER_PATH="//playvideo.qcloud.com"`)+ 自家 `convertProtocol()`:

```js
function convertProtocol(e) {
  if (e.indexOf(".sdp") > -1) {
    e = e.replace(".sdp", ".flv");
    return [e.replace(".sdp",".m3u8")];
  }
  // webrtc://host/path?sig=xxx
  // → flv:  https://host/path.flv?sig=xxx
  // → hls:  https://host/path.m3u8?sig=xxx
}
```

DRM 也可能开启(`voddrm.token.<drmToken>.<file>` 前缀在代码里出现)。

---

## 4. 鉴权体系

### 4.1 登录

- **唯一途径**: 微信扫码(`/liveapi/wap/login/getLoginCode` → 轮询 `checkCodeStatus`)
- 没有手机号 / 密码登录入口

### 4.2 关键 endpoint 鉴权矩阵

| Endpoint | 鉴权 | 备注 |
|---|---|---|
| `common/index/getWebBaseInfo?key=...` | 匿名 ✅ | 白标配置 |
| `common/index/areaCodes` | 匿名 ✅ | 区号 |
| `stu/course/getCourseInfo` | 匿名 ✅ | 课程元数据 |
| `stu/course/listClass` | 匿名 ✅ | 班次列表 |
| `stu/course/listSections` | 匿名 ✅ | 课节列表(但 `videos[]` 仅登录后填充) |
| `stu/course/getClassCode` | **登录 ❌** | 加入码 / 鉴权(参数错误也返回 400) |
| `stu/course/getdstoken` | **登录 ❌** | 返回 `{"code":100067,"message":"非法请求，请重新登陆"}` |
| `stu/course/dsAuth` | **登录 ❌** | chapter 级鉴权 |
| `stu/course/liveAuth` | **登录 ❌** | 直播鉴权 |
| `stu/course/chapterAuth`(onlinehy) | **登录 ❌** | 课节鉴权(`is_auth: 0/1`) |
| `liveapi/customer/index/qryUserConfigs` | **登录 ❌** | 租户配置,body 形如 `{uid: org_id}` |
| `liveapi/wap/login/getLoginCode` | 匿名 ✅ | 拿扫码 ticket |

### 4.3 cookie 与 localStorage

- 后端 `Set-Cookie` 在响应里(被 MCP 脱敏),但 `domain=.wanzibook.com`
- `localStorage.token` = 用户登录 token,加入直播 URL 必备
- `localStorage.tengface_org_id` / `tengface_fans_id` / `tengface_is_show` = 租户绑定信息
- 当前测试 session **无 cookie** —— 直接 anonymous 浏览模式

---

## 5. 为什么"现在无法下载"

1. **课程未开课** — 首课 2026-09-02 20:00,当前 2026-09-01
2. **83 个章节 `videos[]` 全空** — 即便登录也拿不到 `file_id`
3. **`canUseVideoUrl: false`** — 配置层面禁用直链
4. **`IS_EXCEPTION_MODE: true`** — 试用模式,实际可下到的可能性极低

---

## 6. 当课程开始后,下载策略(2026-09-02+)

### 6.1 路径 A — 手动抓 m3u8(零代码)

1. 在 camoufox 登录(微信扫码,`/liveapi/wap/login/getLoginCode`)
2. 进入 `#/yueke3/3529714`,等首课录播生成
3. 点击录播条目 → 触发 `liveAutha` → 新窗口打开
4. 新窗口 DevTools → Network → 过滤 `.m3u8` 或 `playvideo.qcloud.com`
5. 复制 m3u8 URL → `ffmpeg -i "URL" -c copy lesson.mp4`

### 6.2 路径 B — 复用本项目 `tulingpython` 模式(参考 `src/drission_client.rs`)

```rust
// 伪代码:对照现有 src/platform/tulingpython.rs::list_collection_impl 写
async fn download_shanhu_recording(
    org_id: u64,
    course_id: u64,
    chapter_ds_id: u64,
    shool_token: &str,
) -> Result<PathBuf> {
    // 1. 拿 liveToken(必须有 fans_id)
    let token_resp = client.post("https://study.wanzibook.com/onlineapi/stu/course/liveAuth")
        .form(&[("org_id", org_id), ("course_id", course_id),
                ("class_id", 3023639), ("chapter_id", chapter_id)])
        .send().await?.json::<LiveAuthResp>().await?;
    let live_token = token_resp.data.liveToken;

    // 2. 拿 live_domain
    let mini: MiniConfig = client.get("https://shkt-online-live-1317978474.cos.accelerate.myqcloud.com/web/config/miniConfig.json")
        .send().await?.json().await?;
    let live_url = format!("{}/?ds_id={}&token={}&other=1&shool_token={}",
                           mini.wx_h5_url, chapter_ds_id, live_token, shool_token);

    // 3. 用 Drission headless Chromium(参考 tulingpython::get_video_info)
    //    注入 cookies + 打开 live_url → 等 TCPlayer 加载 → 拦 m3u8 URL
    //    → 用本项目 HlsDownloader 拉 .ts → ffmpeg -c copy → .mp4
}
```

### 6.3 路径 C — 等首课后用浏览器 M3U8 嗅探

```bash
# 当前架构(MiniMax-M3)已能用 camoufox-reverse 完成
# 等开课后,自动:
#   1. POST /stu/course/liveAuth (拿 liveToken)
#   2. headless 打开 live_domain URL
#   3. listen for m3u8 network request
#   4. ffmpeg 下载
```

---

## 7. 风险与待验证

| 风险 | 说明 |
|---|---|
| **无内容** | 课程 2026-09-02 才开,今天下载 = 无对象 |
| **DRM** | 代码出现 `voddrm.token.<drmToken>.<file>` 前缀,可能 Tencent VOD 启用了 simpleAES |
| **登录门槛** | 仅微信扫码,无法 headless 自动化(除非用 `ego-browser` skill + 真机扫码) |
| **`IS_EXCEPTION_MODE`** | 租户疑似试用模式,即使开课后 `canUseVideoUrl: false` 可能持续 |
| **liveToken 时效** | 推测短时(分钟级),不能缓存复用 |
| **白标域名分散** | `rexxi.cn` 与 `tengfacexuexi.com` 是 DNS 映射,可能因运营商 DNS 缓存不同步 |

---

## 8. 关键 URL 模式速查

```
匿名可读(GET/POST form-encoded):
  https://study.wanzibook.com/onlineapi/common/index/getWebBaseInfo?key={custom_domain}
  https://study.wanzibook.com/onlineapi/common/index/areaCodes
  https://study.wanzibook.com/onlineapi/stu/course/getCourseInfo (org_id + course_id)
  https://study.wanzibook.com/onlineapi/stu/course/listClass   (org_id + course_id)
  https://study.wanzibook.com/onlineapi/stu/course/listSections (org_id + course_id + class_id + page + num)
  https://study.wanzibook.com/onlineapi/stu/course/classInfo   (org_id + classId)

登录后(JSON body):
  POST https://study.wanzibook.com/onlineapi/stu/course/liveAuth
    {course_id, class_id, chapter_id}        → {liveToken}
  POST https://study.wanzibook.com/onlineapi/stu/course/dsAuth
    {ds_id, fans_id}                          → {liveToken}
  POST https://study.wanzibook.com/onlineapi/stu/course/getdstoken
    {org_id, fans_id}                         → {liveToken}
  POST https://onlinehy.shkt.online/chapter/chapterAuth
    {org_id, course_id, class_id, chapter_id} → {is_auth}

配置中心:
  https://resshktonline.tengfacexuexi.com/web/config/requestApi/default.json
  https://resshktonline.tengfacexuexi.com/web/config/playType.json
  https://resshktonline.tengfacexuexi.com/web/config/miniConfig.json  (含 wx_h5_url / wx_h5_main_host)
  https://shkt-online-live-1317978474.cos.accelerate.myqcloud.com/web/config/miniConfig.json  (镜像)
  https://tengface-live.oss-cn-beijing.aliyuncs.com/web/config/userConfig/{org_id}.json  (租户私有)

登录流:
  GET  https://live.shkt.online/liveapi/wap/login/getLoginCode
  POST https://live.shkt.online/liveapi/wap/login/checkCodeStatus {code}

直播房间(打开新窗口):
  {wx_h5_url}/?ds_id={ds_id}&token={liveToken}&other=1&shool_token={shool_token}
  {wx_h5_url}/tcicMiddlePage/#/pc?classid={interact_id}&fans_id={fans_id}&ds_id={ds_id}
  {wx_h5_url}/f?ds_id={ds_id}&tid={room_id}&token={liveToken}&other=1&shool_token={shool_token}

底层流媒体:
  //playvideo.qcloud.com/playerv2/playermc/playerv2-main.mc.js  (TCPlayer SDK)
  webrtc://{server}/{stream}?sig=xxx  → https://{server}/{stream}.m3u8?sig=xxx
```

---

## 9. 给用户的直接建议

**今天(2026-09-01)**: 无法下载,等 2026-09-02 之后才有内容。

**2026-09-02 当晚(或之后任意时间)**:

1. **最省事**: 在 camoufox 浏览器登录 → 进 `#/yueke3/3529714` → 等课节录播出来(开课后通常几小时内出) → 点录播 → 新窗口 DevTools 找 m3u8 → `ffmpeg -c copy` 下载
2. **半自动**: 让我把上面伪代码转成 Rust,加进 `src/platform/shanhu.rs`(沿用 `tulingpython.rs` 的 Drission + HlsDownloader 模式)
3. **避开平台**: 该课是 Linux 云计算就业班(BBS 链接 bbs.jfedu.net、Linux论坛),**大量内容可能在 bbs.jfedu.net 有公开 PDF/视频**,值得先看一遍

---

## 10. 与本项目代码的衔接点

如果决定加 `ShanhuResolver`:

- `src/platform/zhixueyun.rs` 是最近的同类参考(同样多租户 SaaS、同样匿名 + 登录两层)
- `src/platform/tulingpython.rs::list_collection_impl` 的 `reqwest-fastpath` 思路可复用(直接绕过 headless SPA,先 API 拿章节列表)
- `src/drission_client.rs::diag_download_all_lessons` 的"Drission headless + HlsDownloader"是终极兜底
- 需要新增文件:
  - `src/platform/shanhu.rs`(解析器主体,~600 行)
  - `src/platform/shanhu_token/mod.rs`(cookie / liveToken 缓存,~100 行)
  - `Cargo.toml` 加 `feature = ["shanhu"]` 门控,默认开(与 zhixueyun 一致)
