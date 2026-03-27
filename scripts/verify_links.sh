#!/bin/bash
# Markdown 链接验证脚本
# 验证所有 Markdown 文件中的内部链接是否有效

set -e

echo "========================================"
echo "Markdown 链接验证工具"
echo "========================================"
echo ""

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 统计
TOTAL_FILES=0
TOTAL_LINKS=0
BROKEN_LINKS=0

# 项目根目录
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# 查找所有 Markdown 文件
echo "📋 扫描项目中的 Markdown 文件..."
echo ""

mapfile -t MARKDOWN_FILES < <(find "$PROJECT_ROOT" -name "*.md" -type f \
    -not -path "*/target/*" \
    -not -path "*/.git/*" \
    -not -path "*/.qwen/*")

TOTAL_FILES=${#MARKDOWN_FILES[@]}
echo "找到 $TOTAL_FILES 个 Markdown 文件"
echo ""
echo "🔍 开始验证链接..."
echo ""

# 遍历每个 Markdown 文件
for file in "${MARKDOWN_FILES[@]}"; do
    # 跳过 reference 目录下的外部参考文档
    if [[ "$file" =~ reference/kv_cache_reference ]]; then
        continue
    fi
    
    # 获取文件相对路径
    rel_path="${file#$PROJECT_ROOT/}"
    
    # 获取文件所在目录（用于解析相对链接）
    file_dir=$(dirname "$file")
    
    # 提取文件中的所有链接
    # 匹配 [text](link) 和 [text](link#anchor) 格式
    links=$(grep -oE '\]\([^)]+\)' "$file" 2>/dev/null | sed 's/]('//g | sed 's/)$//g' || true)
    
    for link in $links; do
        # 跳过外部链接
        if [[ "$link" =~ ^http:// ]] || [[ "$link" =~ ^https:// ]] || [[ "$link" =~ ^mailto: ]]; then
            continue
        fi
        
        # 跳过锚点链接（以#开头）
        if [[ "$link" =~ ^# ]]; then
            continue
        fi
        
        TOTAL_LINKS=$((TOTAL_LINKS + 1))
        
        # 解析链接路径和锚点
        link_path="${link%%#*}"
        
        # 跳过空路径（纯锚点链接）
        if [[ -z "$link_path" ]]; then
            continue
        fi
        
        # 构建绝对路径
        if [[ "$link_path" =~ ^/ ]]; then
            # 绝对路径（相对于项目根目录）
            target_path="$PROJECT_ROOT/$link_path"
        else
            # 相对路径（相对于当前文件）
            target_path="$file_dir/$link_path"
        fi
        
        # 规范化路径（解析..和.）
        if [[ -d "$(dirname "$target_path")" ]]; then
            target_path="$(cd "$(dirname "$target_path")" && pwd)/$(basename "$target_path")"
        else
            target_path=""
        fi
        
        # 检查文件是否存在
        if [[ -z "$target_path" ]] || [[ ! -f "$target_path" ]]; then
            echo -e "${RED}❌ 失效链接${NC}"
            echo "   文件：$rel_path"
            echo "   链接：$link"
            echo "   目标：$link_path"
            echo ""
            BROKEN_LINKS=$((BROKEN_LINKS + 1))
        fi
    done
done

echo "========================================"
echo "验证结果"
echo "========================================"
echo ""
echo "Markdown 文件数：$TOTAL_FILES"
echo "内部链接总数：$TOTAL_LINKS"

if [[ $BROKEN_LINKS -eq 0 ]]; then
    echo -e "${GREEN}✅ 所有链接均有效！${NC}"
    exit 0
else
    echo -e "${RED}❌ 发现 $BROKEN_LINKS 个失效链接${NC}"
    echo ""
    echo "请检查并修复上述失效链接。"
    exit 1
fi
