#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Mortal 的 mjai 機器人：tools/mortal/run.sh s42|hhim8826 [--threads N] [--q-values]
# 安裝見 tools/mortal/README.md（setup.sh）。
DIR=$(cd "$(dirname "$0")" && pwd)
exec "$DIR/.venv/bin/python" "$DIR/mortal_bot.py" "$@"
