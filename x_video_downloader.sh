#!/bin/bash

# X.com Video Downloader - 交互式脚本
# 将X.com视频URL转换为真实下载地址并下载

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# 获取脚本所在目录
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_FILE="$SCRIPT_DIR/x_video_downloader.log"

log() {
    echo -e "$1" | tee -a "$LOG_FILE"
}

log_error() {
    echo -e "${RED}$1${NC}" | tee -a "$LOG_FILE"
}

log_success() {
    echo -e "${GREEN}$1${NC}" | tee -a "$LOG_FILE"
}

log_info() {
    echo -e "${CYAN}$1${NC}" | tee -a "$LOG_FILE"
}

# 检查依赖
check_dependencies() {
    if ! command -v yt-dlp &> /dev/null; then
        log_error "错误: yt-dlp 未安装"
        log_info "请运行: brew install yt-dlp"
        exit 1
    fi

    if ! command -v fzf &> /dev/null; then
        log_error "错误: fzf 未安装"
        log_info "请运行: brew install fzf"
        exit 1
    fi
}

# 提取视频信息
extract_video_info() {
    local url="$1"
    log_info "正在获取视频信息..."

    # 获取JSON格式的视频信息
    video_json=$(yt-dlp -j "$url" 2>/dev/null)

    if [ $? -ne 0 ]; then
        log_error "无法获取视频信息，请检查URL是否正确"
        return 1
    fi

    # 提取视频标题
    title=$(echo "$video_json" | jq -r '.title // "Unknown Title"' 2>/dev/null || echo "Unknown Title")

    # 提取上传者
    uploader=$(echo "$video_json" | jq -r '.uploader // "Unknown"' 2>/dev/null || echo "Unknown")

    # 提取时长
    duration=$(echo "$video_json" | jq -r '.duration // 0' 2>/dev/null)
    if [ "$duration" != "0" ] && [ -n "$duration" ]; then
        # 使用 awk 处理浮点数
        minutes=$(awk -v d="$duration" 'BEGIN {printf "%d", d/60}')
        seconds=$(awk -v d="$duration" 'BEGIN {printf "%d", d%60}')
        duration_str="${minutes}:$(printf "%02d" $seconds)"
    else
        duration_str="Unknown"
    fi

    # 提取缩略图
    thumbnail=$(echo "$video_json" | jq -r '.thumbnail // ""' 2>/dev/null)

    echo ""
    log "=============================================="
    log "  视频信息"
    log "=============================================="
    log "  标题: ${YELLOW}$title${NC}"
    log "  上传者: ${YELLOW}$uploader${NC}"
    log "  时长: ${YELLOW}$duration_str${NC}"
    log "=============================================="
    echo ""

    # 存储信息供后续使用
    echo "$video_json" > /tmp/x_video_info.json
    echo "$title" > /tmp/x_video_title.txt
}

# 显示可用格式
show_formats() {
    log_info "正在获取可用格式..."

    formats=$(yt-dlp --list-formats "$url" 2>/dev/null)

    echo ""
    log "=============================================="
    log "  可用视频格式"
    log "=============================================="
    echo "$formats"
    log "=============================================="
    echo ""
}

# 选择视频格式
select_format() {
    log_info "请选择视频质量:"

    # 使用fzf让用户选择格式
    format_choice=$(yt-dlp --list-formats "$url" 2>/dev/null | \
        grep -E "^[0-9]+" | \
        fzf --prompt="选择格式: " \
            --header="分辨率 | 扩展名 | 文件大小 | 编码" \
            --height=15 \
            --border)

    if [ -z "$format_choice" ]; then
        log_error "未选择格式"
        return 1
    fi

    # 提取格式ID
    format_id=$(echo "$format_choice" | awk '{print $1}')
    format_ext=$(echo "$format_choice" | awk '{print $2}')
    format_res=$(echo "$format_choice" | awk '{print $3}')

    log_success "已选择格式: $format_res ($format_ext)"

    # 清理文件名
    video_title=$(cat /tmp/x_video_title.txt)
    safe_title=$(echo "$video_title" | sed 's/[^\w\s-]//g' | sed 's/\s\+/_/g' | cut -c1-50)

    # 下载视频
    log_info "正在下载视频..."

    if yt-dlp -f "$format_id" -o "$SCRIPT_DIR/${safe_title}.${format_ext}" "$url" 2>&1 | tee -a "$LOG_FILE"; then
        log_success "下载完成!"
        log_info "文件保存至: $SCRIPT_DIR/${safe_title}.${format_ext}"
    else
        log_error "下载失败"
        return 1
    fi
}

# 直接下载最佳质量
download_best() {
    video_title=$(cat /tmp/x_video_title.txt)
    safe_title=$(echo "$video_title" | sed 's/[^\w\s-]//g' | sed 's/\s\+/_/g' | cut -c1-50)

    log_info "正在下载最佳质量视频..."

    if yt-dlp -f "best" -o "$SCRIPT_DIR/${safe_title}.mp4" "$url" 2>&1 | tee -a "$LOG_FILE"; then
        log_success "下载完成!"
        log_info "文件保存至: $SCRIPT_DIR/${safe_title}.mp4"
    else
        log_error "下载失败"
        return 1
    fi
}

# 显示下载链接（不下载）
show_download_links() {
    log_info "正在提取真实下载地址..."

    # 获取所有格式的下载地址
    yt-dlp -j --flat-playlist "$url" 2>/dev/null | jq -r '.formats[] | "\(.format_id)\t\(.ext)\t\(.resolution)\t\(.filesize // .filesize_approx // 0)"' 2>/dev/null | \
    while IFS=$'\t' read -r ext filesize; do
        echo "$ext"
    done

    # 使用yt-dlp获取可直接下载的URL
    log ""
    log "=============================================="
    log "  可用下载地址"
    log "=============================================="

    yt-dlp -f "best" --get-url "$url" 2>/dev/null | while read line; do
        log "最佳质量: $line"
    done

    # 获取所有格式
    yt-dlp --list-formats "$url" 2>/dev/null | grep -E "^[0-9]+" | while read line; do
        format_id=$(echo "$line" | awk '{print $1}')
        ext=$(echo "$line" | awk '{print $2}')
        res=$(echo "$line" | awk '{print $3}')

        # 获取该格式的下载链接
        url_line=$(yt-dlp -f "$format_id" --get-url "$url" 2>/dev/null || echo "")
        if [ -n "$url_line" ]; then
            log ""
            log "格式 ${GREEN}$format_id${NC} - ${YELLOW}$res${NC} ($ext)"
            log "  $url_line"
        fi
    done

    log "=============================================="
    log ""
}

# 主菜单
show_menu() {
    clear
    log "=============================================="
    log "       X.com 视频下载器"
    log "=============================================="
    log ""
    log "  ${GREEN}1${NC}. 获取视频信息并查看格式"
    log "  ${GREEN}2${NC}. 下载最佳质量"
    log "  ${GREEN}3${NC}. 选择特定格式下载"
    log "  ${GREEN}4${NC}. 仅显示下载地址"
    log "  ${GREEN}5${NC}. 下载GIF"
    log "  ${GREEN}0${NC}. 退出"
    log ""
    log "=============================================="
}

# 主程序
main() {
    check_dependencies

    clear
    log "=============================================="
    log "       X.com 视频下载器"
    log "=============================================="
    log ""

    # 提示输入URL
    read -p "请输入X.com视频URL: " url

    if [ -z "$url" ]; then
        log_error "URL不能为空"
        exit 1
    fi

    # 验证URL格式
    if [[ ! "$url" =~ x\.com|twitter\.com ]]; then
        log_error "请输入有效的X.com或Twitter.com URL"
        exit 1
    fi

    # 提取视频信息
    extract_video_info "$url"

    # 显示菜单并处理选择
    while true; do
        show_menu
        read -p "请选择操作 [0-5]: " choice
        echo ""

        case $choice in
            1)
                show_formats
                ;;
            2)
                download_best
                break
                ;;
            3)
                select_format
                break
                ;;
            4)
                show_download_links
                ;;
            5)
                log_info "正在下载GIF..."
                video_title=$(cat /tmp/x_video_title.txt)
                safe_title=$(echo "$video_title" | sed 's/[^\w\s-]//g' | sed 's/\s\+/_/g' | cut -c1-50)

                if yt-dlp --convert-thumbnails gif -o "$SCRIPT_DIR/${safe_title}.gif" "$url" 2>&1 | tee -a "$LOG_FILE"; then
                    log_success "GIF下载完成!"
                    log_info "文件保存至: $SCRIPT_DIR/${safe_title}.gif"
                else
                    log_error "GIF下载失败，可能该帖子不包含GIF"
                fi
                break
                ;;
            0)
                log_info "再见!"
                exit 0
                ;;
            *)
                log_error "无效选择，请重新输入"
                ;;
        esac

        echo ""
        read -p "按回车继续..."
    done
}

# 清理临时文件
cleanup() {
    rm -f /tmp/x_video_info.json /tmp/x_video_title.txt
}

trap cleanup EXIT

# 运行主程序
main "$@"
