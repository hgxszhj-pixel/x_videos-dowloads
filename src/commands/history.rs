//! history 子命令：下载历史与书签管理

use anyhow::Result;

use crate::cli::Cli;
use crate::config::AppConfig;
use crate::history::History;
use crate::types::format_bytes;

/// 历史和书签命令
pub fn run_history_commands(cli: Cli) -> Result<()> {
    let config = AppConfig::load()?;
    let mut history = History::new(&config.output_dir)?;

    // 清空历史
    if cli.clear_history {
        history.clear()?;
        println!("下载历史已清空");
        return Ok(());
    }

    // 添加书签
    if let Some(ref url) = cli.bookmark {
        let title = cli.search.clone(); // 复用 search 作为标题
        match history.add_bookmark(url.clone(), title, None) {
            Ok(_) => println!("已添加书签: {}", url),
            Err(e) => eprintln!("添加书签失败: {}", e),
        }
        return Ok(());
    }

    // 列出书签
    if cli.bookmarks {
        let bookmarks = history.get_bookmarks();
        if bookmarks.is_empty() {
            println!("暂无书签");
        } else {
            println!("\n书签列表:");
            for b in bookmarks {
                println!("  - {} ({})", b.title.as_deref().unwrap_or(&b.url), b.url);
                if let Some(ref note) = b.note {
                    println!("    备注: {}", note);
                }
            }
        }
        return Ok(());
    }

    // 搜索历史
    if let Some(ref query) = cli.search {
        let results = history.search(query);
        if results.is_empty() {
            println!("未找到匹配的历史记录");
        } else {
            println!("\n搜索结果 ({} 条):", results.len());
            for entry in results {
                println!(
                    "  - {} ({})",
                    entry.title.as_deref().unwrap_or(&entry.url),
                    entry.downloaded_at.format("%Y-%m-%d %H:%M")
                );
            }
        }
        return Ok(());
    }

    // 查看历史
    let entries = history.get_recent(20);
    if entries.is_empty() {
        println!("暂无下载历史");
    } else {
        println!("\n最近下载 ({} 条):", entries.len());
        for entry in entries {
            let size = entry
                .file_size
                .map(format_bytes)
                .unwrap_or_else(|| "N/A".to_string());
            println!(
                "  - {} | {} | {}",
                entry.title.as_deref().unwrap_or(&entry.url),
                size,
                entry.downloaded_at.format("%Y-%m-%d %H:%M")
            );
        }
    }

    Ok(())
}
