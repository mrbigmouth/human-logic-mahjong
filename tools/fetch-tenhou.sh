#!/usr/bin/env bash
# 下載天鳳鳳凰卓的牌譜（mjlog XML 與 JSON 兩種格式）。
#
# 用法：tools/fetch-tenhou.sh <日期時 YYYYMMDDHH ...> [-n 每小時最多幾場] [-o 輸出目錄] [-r 規則]
#   例：tools/fetch-tenhou.sh 2026100700 2026100701 -n 20
#   規則預設「四鳳南喰赤」（四人半莊、有食斷、有赤）；東風戰用「四鳳東喰赤」。
#
# 牌譜清單來自 https://tenhou.net/sc/raw/（只保留最近幾天的每小時清單）。
# 下載的牌譜放在 data/tenhou/（已 gitignore）；每場存成 <牌譜 ID>.xml 與 <牌譜 ID>.json。
# 請保持低頻率（每場之間暫停），不要對天鳳伺服器造成負擔。
set -euo pipefail

per_hour=20
out=data/tenhou
rule="四鳳南喰赤"
hours=()
while [ $# -gt 0 ]; do
  case "$1" in
    -n) per_hour="$2"; shift 2 ;;
    -o) out="$2"; shift 2 ;;
    -r) rule="$2"; shift 2 ;;
    *) hours+=("$1"); shift ;;
  esac
done
[ ${#hours[@]} -gt 0 ] || { echo "用法：$0 <YYYYMMDDHH ...> [-n 每小時場數] [-o 目錄] [-r 規則]" >&2; exit 1; }
mkdir -p "$out"

for h in "${hours[@]}"; do
  ids=$(curl -sf "https://tenhou.net/sc/raw/dat/scc${h}.html.gz" | gunzip \
    | grep "$rule" | grep -o 'log=[0-9a-z-]*' | cut -d= -f2 | head -n "$per_hour") || {
    echo "無法取得 $h 的清單（可能已超過保留期限）" >&2; continue; }
  for id in $ids; do
    [ -s "$out/$id.xml" ] || curl -sf "https://tenhou.net/0/log/?$id" -o "$out/$id.xml"
    [ -s "$out/$id.json" ] || curl -sf -A "Mozilla/5.0" -H "Referer: https://tenhou.net/6/?log=$id" \
      "https://tenhou.net/5/mjlog2json.cgi?$id" -o "$out/$id.json"
    echo "$id"
    sleep 1
  done
done
