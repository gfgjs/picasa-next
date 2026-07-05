// scripts/exotic-issue-license.mjs
// License token 签发器(内测形态;Part8 D1 签发端的本地原型,2026-07-05)。
//
// 产出一枚 `payload.sig` 形态的激活 token(exotic-trust §5.2),测试者在应用
// 「插件商店 → 激活」里粘贴即可解锁付费插件(内测安装包的信任根须含对应内测公钥,
// 即经 PICASA_EXOTIC_KEYSET_FILE 注入 internal-keyset.json 的构建)。
//
// 用法:
//   node scripts/exotic-issue-license.mjs                      # PSD 插件,永久授权
//   node scripts/exotic-issue-license.mjs --days 30            # 30 天后过期
//   node scripts/exotic-issue-license.mjs --plugin X --sku Y   # 其他插件/SKU
//
// 密钥:.internal-signing/internal-license.pem(缺失则自动生成——但那样生成的新键
// 必须重跑 exotic-internal-registry.mjs 让 keyset 收录后重新构建安装包才可验过,
// 故常规顺序是先跑 registry 生成器再签发)。

import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { ensureKey, signLicenseToken } from './lib/exotic-signing.mjs';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const out = path.join(repo, '.internal-signing');
fs.mkdirSync(out, { recursive: true });

// 极简参数解析(仅 --key value 形态;脚本级工具不引依赖)。
const args = process.argv.slice(2);
function argOf(flag, fallback) {
  const i = args.indexOf(flag);
  return i >= 0 && i + 1 < args.length ? args[i + 1] : fallback;
}
const pluginId = argOf('--plugin', 'exotic-image-psd');
const sku = argOf('--sku', 'psd-engine-2026');
const days = argOf('--days', null);

const licensePem = path.join(out, 'internal-license.pem');
const existedBefore = fs.existsSync(licensePem);
const licenseKey = ensureKey(licensePem);
if (!existedBefore) {
  console.warn(
    '⚠ 新生成了 internal-license.pem——须重跑 exotic-internal-registry.mjs 更新 keyset 并重建安装包,token 才能验过。'
  );
}

const now = Math.floor(Date.now() / 1000);
const token = signLicenseToken({
  licenseKey,
  keyId: 'license-internal-2026-07',
  licenseId: `lic-internal-${crypto.randomUUID()}`,
  pluginId,
  sku,
  issuedAt: now,
  // 留 1 小时时钟偏差余量:测试机时钟略慢也不至于 NotYetValid。
  notBefore: now - 3600,
  // null = 永久授权(v3 首发主路径);--days N 则 N 天后过期。
  expiresAt: days ? now + parseInt(days, 10) * 86400 : null,
});

console.log(`内测 License token 已签发(plugin=${pluginId}, sku=${sku}, ${days ? `${days} 天有效` : '永久'})
在应用「插件商店 → 该插件 → 激活」中粘贴下面整行:

${token}`);
