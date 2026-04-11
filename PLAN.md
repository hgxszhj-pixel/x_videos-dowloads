# X Video Downloader 性能优化与架构改进计划

## 一、项目概述

本项目是使用 Rust 开发的 x.com (Twitter) 视频下载器，支持 CLI 和 GUI 两种模式。项目使用 yt-dlp 作为后端进行视频信息获取和下载，同时实现了原生的 HTTP 下载功能。

## 二、问题列表（按优先级排序）

### 高优先级问题

#### 1. 下载模块内存低效 (src/downloader.rs:228)

**问题描述**:
在并行片段下载完成后合并文件时，代码使用 `tokio::fs::read()` 将整个临时片段文件读入内存，然后再写入输出文件：

```rust
let data = tokio::fs::read(&temp_path).await?;
output_file.write_all(&data).await?;
```

这种方式在下载大文件时会导致内存使用翻倍。

**影响**:
- 内存使用效率低
- 大文件下载时可能触发 OOM

**解决方案**:
使用 `tokio::io::copy()` 流式合并文件

---

### 中优先级问题

#### 2. yt-dlp 随机数生成弱 (src/yt_dlp.rs:100-104)

**问题描述**:
使用 `SystemTime::now()` 的纳秒时间戳作为随机种子，随机性可预测。

**解决方案**:
使用 `rand` crate 的 `ThreadRng`

#### 3. GUI 无实时进度更新 (src/gui.rs)

**问题描述**:
download_progress 只是模拟值 (0.0 -> 1.0)，不是真实进度。

**解决方案**:
解析 yt-dlp 的 --progress 输出

---

### 低优先级问题

#### 4. 重复代码 - sanitize_filename

**问题描述**:
`sanitize_filename` 函数在 gui.rs 和 main.rs 中重复定义。

#### 5. 类型缺少 Copy trait

**问题描述**:
所有结构体派生了 Clone 但没有 Copy。

#### 6. 无配置管理系统

**问题描述**:
所有配置都在 CLI 参数中硬编码，没有配置文件支持。

---

## 三、实施计划

### 阶段一：关键修复
1. 流式文件合并（内存优化） ✅ 已完成
2. 添加 Copy trait

### 阶段二：功能改进
3. 改进随机数生成 ✅ 已完成
4. 实时进度更新 ✅ 已完成（main.rs已实现）
5. Cookie认证支持 ✅ 已完成（CLI + GUI）

### 阶段三：代码整理
6. 消除重复代码 ✅ 已完成
7. 添加配置管理

---

## 四、审核结论

### 已完成
- ✅ 重试机制（指数退避）
- ✅ 分段并行下载（4线程）
- ✅ 连接池优化
- ✅ 流式文件合并（内存优化）
- ✅ 随机User-Agent生成（rand crate）
- ✅ 实时进度显示（解析yt-dlp输出）
- ✅ Cookie认证支持（CLI + GUI）
- ✅ 消除重复代码（sanitize_filename）

### 待处理（优先级排序）
| 优先级 | 问题 | 状态 |
|--------|------|------|
| LOW | 配置管理系统 | 📋 |
| LOW | 添加Copy trait | 📋 |
