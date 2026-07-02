#!/usr/bin/env node
// canonical 一致性机械校验（第 7 轮终审 P3#14 引入）
// ──────────────────────────────────────────────────────────────────────────
// 动机：plan 集历 7 轮 review，最顽固的缺陷是「§8 裁决→§3 正文回写」靠人工纪律、
//       每轮都漏一处（CANON-01/02/03、verify_token 口径、reject 命令…「修一处漏一处」）。
// 本脚本把反复打架的 canonical 不变量固化为可执行断言，纳入 review/CI 流程，
// 把「人工 grep 全文」换成「机械全量扫描」，杜绝同型回归。
//
// 用法：node tools/check_plan_canonical.mjs    （仓库根目录执行）
// 退出码：0=全部通过；1=有违反（打印每条违反的文件:行 + 摘录）。
// 维护：新 canonical 裁决定稿时，在 RULES 增一条断言（与回写 §3 正文同步）。

import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

const DOCS_DIR = 'plan-docs/refactor_2026';

// 读取全部 Part*.md（不含 review_*.md：review 是历史快照，允许留旧表述）
const files = readdirSync(DOCS_DIR)
  .filter((f) => /^Part\d.*\.md$/.test(f))
  .map((f) => join(DOCS_DIR, f));

/** 把每个文件读成 {path, lines:[{n, text}]} */
const docs = files.map((path) => ({
  path,
  lines: readFileSync(path, 'utf8').split(/\r?\n/).map((text, i) => ({ n: i + 1, text })),
}));

/**
 * 规则类型：
 *  - forbid：任何 Part 文件命中 `pattern` 即违反（陈旧/被推翻的表述不得残留）。
 *    可选 `unless`：同一行另含 `unless` 正则则豁免（用于「已收窄/已标注」的注释）。
 *  - require：`inFile`（文件名子串）必须命中 `pattern`，否则违反（权威正文必须含某决策）。
 */
const RULES = [
  {
    id: 'CANON-authority',
    desc: '权威元规则不得回退为「以 §8 为准」（CANON-01，§3 正文即唯一权威）',
    type: 'forbid',
    pattern: /以\s*§?8\s*为准/,
    unless: /废止|不再以|曾因|临时改判|历史/,
  },
  {
    id: 'CANON-02-salt',
    desc: 'AES salt 位置 = ModelBlob.enc_salt，不得再出现「salt 随密文前缀存」(CANON-02)',
    type: 'forbid',
    pattern: /salt[^。\n]{0,12}(随)?密文前缀/,
    unless: /删|旧值|非密文前缀|杜绝|纠正/,
  },
  {
    id: 'CANON-encseed-granularity',
    desc: 'enc_seed 粒度 = 按 plugin 固定主种子，不得按 per-license 唯一加密',
    type: 'forbid',
    pattern: /enc_seed[^。\n]{0,16}per-?license/i,
    unless: /非|不是|❌|不可行|否则|而非|误|错配/,
  },
  {
    id: 'CANON-hkdf-info',
    desc: 'HKDF info = plugin_id‖model_id，不得只写 hkdf(enc_seed, plugin_id)',
    type: 'forbid',
    pattern: /hkdf\(\s*enc_seed\s*,\s*plugin_id\s*\)/i,
    unless: /model_id/,
  },
  {
    id: 'CANON-ed25519-scan',
    desc: '渠道合规验收不得断言「无 Ed25519 符号」(verify_token 留开源、ring 因 SHA-256 全渠道必链)',
    type: 'forbid',
    pattern: /(无|扫描?无|断言无)[^。\n]{0,24}Ed25519[^。\n]{0,8}符号/,
    unless: /不[扫含断]|收窄|留开源|不可达|第7轮终审|非\s*Store/,
  },
  {
    id: 'CANON-g6-errcode',
    desc: 'G6 worker 错误码 = GpuUnavailable/SessionExpired/ModelLoadFailed/EmbedDimMismatch，不得残留旧码',
    type: 'forbid',
    pattern: /\b(OrtError|GpuOom|ModelNotLoaded)\b/,
    unless: /并入|→|改|旧|纠正|取代|terminal|无\s*OrtError|仅\s*5\s*值/,
  },
  {
    id: 'REQUIRE-reject-cmd',
    desc: 'Part4 §3.5.1 权威正文须含 reject_face_candidate（DBX-01 回写）',
    type: 'require',
    inFile: 'Part4',
    pattern: /reject_face_candidate\(face_ids\[\],\s*person_id\)/,
  },
];

let violations = 0;

for (const rule of RULES) {
  if (rule.type === 'forbid') {
    for (const doc of docs) {
      for (const { n, text } of doc.lines) {
        if (rule.pattern.test(text) && !(rule.unless && rule.unless.test(text))) {
          violations++;
          console.error(`✗ [${rule.id}] ${doc.path}:${n}`);
          console.error(`    ${rule.desc}`);
          console.error(`    > ${text.trim().slice(0, 160)}`);
        }
      }
    }
  } else if (rule.type === 'require') {
    const targets = docs.filter((d) => d.path.includes(rule.inFile));
    for (const doc of targets) {
      const hit = doc.lines.some(({ text }) => rule.pattern.test(text));
      if (!hit) {
        violations++;
        console.error(`✗ [${rule.id}] ${doc.path}`);
        console.error(`    ${rule.desc}（全文未命中 ${rule.pattern}）`);
      }
    }
  }
}

if (violations === 0) {
  console.log(`✓ canonical 一致性校验通过（${RULES.length} 条不变量，${docs.length} 个 Part 文件）`);
  process.exit(0);
} else {
  console.error(`\n✗ canonical 校验失败：${violations} 处违反。修复后重跑（§8 裁决须同步回写 §3 正文）。`);
  process.exit(1);
}
