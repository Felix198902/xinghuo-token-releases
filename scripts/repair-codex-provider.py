#!/usr/bin/env python3
"""
Repair Codex threads broken by a dangling `model_provider` reference.

Symptom
-------
ChatGPT / Codex fails to resume a thread with:

    failed to load configuration: Model provider `custom` not found

and the UI shows "无法编辑消息" / "此对话串无法继续".

Why three layers
----------------
A dangling provider id gets固化 in three independent places. Fixing only one
of them is not enough -- the thread stays broken:

1. ~/.codex/config.toml
   `model_provider = "custom"` with no matching `[model_providers.custom]`
   table. Fixing this stops NEW threads from breaking.

2. ~/.codex/state_5.sqlite -> threads.model_provider
   The index column the thread list reads.

3. ~/.codex/sessions/**/rollout-*.jsonl   <-- the actual source of truth
   `thread/resume` replays this file. Line 0 (`session_meta`) carries
   `model_provider`, and later `thread_settings_applied` events carry
   `thread_settings.model_provider_id`. THIS is what raises the error.

Usage
-----
    # dry run: show what would change, touch nothing
    python3 repair-codex-provider.py

    # actually repair (backs everything up first, then quits/restarts ChatGPT)
    python3 repair-codex-provider.py --apply

    # repair a different provider id, or target a specific provider
    python3 repair-codex-provider.py --apply --from custom --to openai

Backups
-------
- sqlite  : <db>.bak.<timestamp>          (via sqlite3 online backup API)
- rollout : <backup-dir>/<relative path>  (original files, structure preserved)

ChatGPT MUST be quit before applying -- the app caches state in memory and
will overwrite on-disk changes otherwise. This script does that for you.
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import shutil
import sqlite3
import subprocess
import sys
import time
from datetime import datetime

CODEX_DIR = os.path.expanduser("~/.codex")
DB = os.path.join(CODEX_DIR, "state_5.sqlite")
SESSIONS = os.path.join(CODEX_DIR, "sessions")
# `openai` is a built-in provider: it needs no [model_providers.*] table.
BUILTIN_FALLBACK = "openai"


def log(msg: str = "") -> None:
    print(msg, flush=True)


# --------------------------------------------------------------------------- #
# ChatGPT process control
# --------------------------------------------------------------------------- #
def chatgpt_running() -> bool:
    r = subprocess.run(["pgrep", "-f", "ChatGPT.app/Contents/MacOS/ChatGPT"],
                       capture_output=True)
    return r.returncode == 0


def quit_chatgpt() -> None:
    log("→ 正在退出 ChatGPT ...")
    subprocess.run(["osascript", "-e", 'tell application "ChatGPT" to quit'],
                   capture_output=True)
    for _ in range(20):
        if not chatgpt_running():
            log("  已退出")
            return
        time.sleep(1)
    log("  ⚠ 5 秒后仍未退出，继续（可能残留助手进程，不影响数据文件）")


def start_chatgpt() -> None:
    log("→ 正在启动 ChatGPT ...")
    subprocess.run(["open", "-a", "ChatGPT"], capture_output=True)


# --------------------------------------------------------------------------- #
# Layer 1: config.toml
# --------------------------------------------------------------------------- #
def check_config(bad: str) -> None:
    cfg = os.path.join(CODEX_DIR, "config.toml")
    log("\n[1/3] config.toml")
    if not os.path.exists(cfg):
        log("  不存在，跳过")
        return
    try:
        import tomllib
        with open(cfg, "rb") as f:
            d = tomllib.load(f)
    except Exception as e:
        log(f"  ⚠ 解析失败: {e}")
        return
    mp = d.get("model_provider")
    if mp is None:
        log(f"  无 model_provider 键 —— 干净")
        return
    providers = d.get("model_providers", {})
    if mp in providers:
        log(f"  model_provider={mp!r} 有对应表 —— 正常")
    else:
        log(f"  ⚠ 悬空引用: model_provider={mp!r} 但无 [model_providers.{mp}]")
        log(f"    手工删除该行，或恢复备份后再跑本脚本")


# --------------------------------------------------------------------------- #
# Layer 2: sqlite threads.model_provider
# --------------------------------------------------------------------------- #
def fix_sqlite(bad: str, good: str, apply: bool) -> int:
    log("\n[2/3] state_5.sqlite -> threads.model_provider")
    if not os.path.exists(DB):
        log("  不存在，跳过")
        return 0
    con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
    dist = list(con.execute(
        "select coalesce(model_provider,'<NULL>') p, count(*) n "
        "from threads group by p order by n desc"))
    log("  当前分布:")
    for v, n in dist:
        log(f"    {v:<20} {n}")
    n = list(con.execute(
        "select count(*) from threads where model_provider=?", (bad,)))[0][0]
    con.close()
    if n == 0:
        log(f"  无 '{bad}' 记录 —— 无需修复")
        return 0
    log(f"  待修复: {n} 条 -> '{good}'")
    if not apply:
        return n
    con = sqlite3.connect(DB)
    bak = f"{DB}.bak.{datetime.now():%Y%m%d-%H%M%S}"
    dst = sqlite3.connect(bak)
    con.backup(dst)
    dst.close()
    log(f"  已备份: {bak}")
    cur = con.execute("update threads set model_provider=? where model_provider=?",
                      (good, bad))
    con.commit()
    log(f"  已更新: {cur.rowcount} 行")
    con.close()
    return n


# --------------------------------------------------------------------------- #
# Layer 3: rollout jsonl  <-- the real fix
# --------------------------------------------------------------------------- #
def fix_rollouts(bad: str, good: str, apply: bool, backup_dir: str) -> int:
    log("\n[3/3] sessions/**/rollout-*.jsonl  (真正被 resume 读取的源)")
    pats = [
        (f'"model_provider":"{bad}"'.encode(), f'"model_provider":"{good}"'.encode()),
        (f'"model_provider_id":"{bad}"'.encode(),
         f'"model_provider_id":"{good}"'.encode()),
    ]
    hits = []
    for f in sorted(glob.glob(os.path.join(SESSIONS, "**", "*.jsonl"),
                              recursive=True)):
        try:
            data = open(f, "rb").read()
        except OSError:
            continue
        cnt = sum(data.count(a) for a, _ in pats)
        if cnt:
            hits.append((f, data, cnt))
    if not hits:
        log(f"  无 '{bad}' 残留 —— 无需修复")
        return 0
    log(f"  命中 {len(hits)} 个文件")
    for f, _, cnt in hits[:5]:
        log(f"    {os.path.relpath(f, SESSIONS)[:66]}  {cnt} 处")
    if len(hits) > 5:
        log(f"    ... 其余 {len(hits)-5} 个")
    if not apply:
        return len(hits)

    total = 0
    for f, data, cnt in hits:
        rel = os.path.relpath(f, SESSIONS)
        dst = os.path.join(backup_dir, rel)
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copy2(f, dst)
        new = data
        for a, b in pats:
            new = new.replace(a, b)
        if len(new) != len(data) and bad != good:
            log(f"  ⚠ 长度变化，跳过 {rel}")
            continue
        open(f, "wb").write(new)
        total += cnt
    log(f"  已备份到: {backup_dir}")
    log(f"  已替换: {total} 处 / {len(hits)} 个文件")

    # integrity check: every touched file must still be valid JSONL
    broken = 0
    for f, _, _ in hits:
        for i, line in enumerate(open(f, encoding="utf-8")):
            line = line.strip()
            if not line:
                continue
            try:
                json.loads(line)
            except Exception:
                broken += 1
                log(f"  ⚠ JSON 损坏: {os.path.relpath(f, SESSIONS)} 第 {i} 行")
    log(f"  JSON 完整性: {'全部通过' if broken == 0 else f'{broken} 行损坏'}")

    left = 0
    for f in glob.glob(os.path.join(SESSIONS, "**", "*.jsonl"), recursive=True):
        d = open(f, "rb").read()
        left += sum(d.count(a) for a, _ in pats)
    log(f"  残留 '{bad}': {left}")
    return len(hits)


# --------------------------------------------------------------------------- #
def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--apply", action="store_true",
                    help="实际执行修复（默认只做 dry-run 检查）")
    ap.add_argument("--from", dest="src", default="custom",
                    help="要替换掉的坏 provider id（默认 custom）")
    ap.add_argument("--to", dest="dst", default=BUILTIN_FALLBACK,
                    help=f"替换成的 provider id（默认 {BUILTIN_FALLBACK}，内置无需表）")
    ap.add_argument("--backup-dir", default="/tmp/codex-rollout-backup",
                    help="rollout 文件备份目录")
    ap.add_argument("--no-restart", action="store_true",
                    help="修复后不要自动启动 ChatGPT")
    args = ap.parse_args()

    mode = "APPLY（会修改数据）" if args.apply else "DRY-RUN（只读检查）"
    log(f"Codex provider 修复工具 — {mode}")
    log(f"  {args.src} -> {args.dst}")
    if not os.path.isdir(CODEX_DIR):
        log(f"✗ 未找到 {CODEX_DIR}")
        return 1

    check_config(args.src)

    if args.apply and chatgpt_running():
        quit_chatgpt()

    n_db = fix_sqlite(args.src, args.dst, args.apply)
    n_js = fix_rollouts(args.src, args.dst, args.apply, args.backup_dir)

    log("\n" + "=" * 60)
    if not args.apply:
        if n_db or n_js:
            log(f"发现 sqlite {n_db} 条 / rollout {n_js} 个文件需要处理。")
            log("确认无误后加 --apply 执行。")
        else:
            log("未发现损坏记录 —— 一切正常。")
        return 0

    log("修复完成。")
    if not args.no_restart:
        start_chatgpt()
        log("ChatGPT 已重启，请重新打开原对话串验证。")
    else:
        log("请手动启动 ChatGPT 后验证。")
    log("回滚：把备份的 sqlite 改回 state_5.sqlite，"
        f"并把 {args.backup_dir} 下的文件拷回 sessions/（先退出 ChatGPT）。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
