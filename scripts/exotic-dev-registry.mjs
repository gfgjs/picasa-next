// scripts/exotic-dev-registry.mjs
// 插件商店 dev registry 生成器(开发期专用;Part8 D1 签发端的本地原型)。
//
// 产出(全部落 <repo>/.dev-registry/,已 gitignore,cargo clean 不波及):
//   dev-release.pem / dev-license.pem  Ed25519 私钥(仅本机 dev 用,勿外传)
//   dev-keyset.json                    公钥集(host 经 PICASA_EXOTIC_DEV_KEYSET 注入,debug-only)
//   exotic-image-psd.zip               插件包(package-manifest.json+sig+plugin-manifest+psd-worker.exe)
//   index.json / index.sig             签名 Registry index(package_url=file:///…,debug-only 放行)
//   seq                                单调序号(registry_sequence/package_sequence 防回滚基线)
//
// 用法:node scripts/exotic-dev-registry.mjs
// 前置:cargo build -p psd-worker(需 target/debug/psd-worker.exe)
//
// 安全边界:dev 密钥/file:// 传输仅在 debug 构建 + 两个环境变量显式开启时生效
// (PICASA_EXOTIC_DEV_FILE_URLS=1 + PICASA_EXOTIC_DEV_KEYSET=<dev-keyset.json>);
// Release 构建两条旁路整体不编入(SEC-02 姿态)。验签/sha256/zip 白名单等校验
// 与生产完全同一套代码,本工具产物须能通过 installer.rs 的 #[ignore] 核验测试。

import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const out = path.join(repo, '.dev-registry');
fs.mkdirSync(out, { recursive: true });

const PLUGIN_ID = 'exotic-image-psd';
const TARGET = 'x86_64-pc-windows-msvc';
const PROTOCOL_VERSION = 2; // 与 crates/exotic-protocol frame.rs PROTOCOL_VERSION 同步

// ── 1. 密钥对(存在即复用,保证 keyset 稳定) ─────────────────────────────────
function ensureKey(name) {
  const pemPath = path.join(out, `${name}.pem`);
  if (fs.existsSync(pemPath)) {
    return crypto.createPrivateKey(fs.readFileSync(pemPath));
  }
  const { privateKey } = crypto.generateKeyPairSync('ed25519');
  fs.writeFileSync(pemPath, privateKey.export({ type: 'pkcs8', format: 'pem' }));
  return privateKey;
}
const releaseKey = ensureKey('dev-release');
const licenseKey = ensureKey('dev-license');

// spki DER 尾 32 字节 = Ed25519 裸公钥(keyset 用标准 base64)。
const rawPub = (priv) =>
  crypto.createPublicKey(priv).export({ type: 'spki', format: 'der' }).subarray(-32);

const keyset = {
  schema: 1,
  keys: [
    {
      key_id: 'dev-release-local',
      purpose: 'release',
      public_key_b64: rawPub(releaseKey).toString('base64'),
      status: 'active',
      not_before: 0,
      not_after: null,
    },
    {
      key_id: 'dev-license-local',
      purpose: 'license',
      public_key_b64: rawPub(licenseKey).toString('base64'),
      status: 'active',
      not_before: 0,
      not_after: null,
    },
  ],
};
fs.writeFileSync(path.join(out, 'dev-keyset.json'), JSON.stringify(keyset, null, 2));

// ── 2. worker 载荷 ────────────────────────────────────────────────────────────
const workerExe = path.join(repo, 'target', 'debug', 'psd-worker.exe');
if (!fs.existsSync(workerExe)) {
  console.error(`缺 ${workerExe}\n先构建:cargo build -p psd-worker`);
  process.exit(1);
}
const workerBytes = fs.readFileSync(workerExe);
const sha256 = (buf) => crypto.createHash('sha256').update(buf).digest('hex');

// ── 3. 单调序号(registry_sequence 与 package_sequence 共用,防回滚基线) ─────
const seqPath = path.join(out, 'seq');
const seq = (fs.existsSync(seqPath) ? parseInt(fs.readFileSync(seqPath, 'utf8'), 10) : 0) + 1;
fs.writeFileSync(seqPath, String(seq));
const version = `1.0.${seq}`;

// ── 4. 包内清单对 + 签名 ──────────────────────────────────────────────────────
// plugin-manifest.json:插件自声明 formats/capabilities,安装时须为 Catalog 子集。
const pluginManifest = Buffer.from(
  JSON.stringify({ plugin_id: PLUGIN_ID, formats: ['psd'], capabilities: ['thumbnail'] }, null, 2)
);
// package-manifest.json:安装真相(清单即白名单;kind=worker 项即运行期 exe 坐标)。
const packageManifest = Buffer.from(
  JSON.stringify(
    {
      schema: 1,
      key_id: 'dev-release-local',
      plugin_id: PLUGIN_ID,
      version,
      package_sequence: seq,
      target: TARGET,
      min_host_version: '0.1.0',
      protocol_version: PROTOCOL_VERSION,
      compliance_review_id: 'dev-local',
      files: [
        {
          path: 'psd-worker.exe',
          size: workerBytes.length,
          sha256: sha256(workerBytes),
          kind: 'worker',
          executable: true,
        },
        {
          path: 'plugin-manifest.json',
          size: pluginManifest.length,
          sha256: sha256(pluginManifest),
          kind: 'resource',
          executable: false,
        },
      ],
    },
    null,
    2
  )
);
const manifestSig = crypto.sign(null, packageManifest, releaseKey); // Ed25519 对原始字节

// ── 5. 存储式 zip(method=0;高压缩比检查天然不触发,布局与生产解包白名单一致)──
function crc32(buf) {
  let c,
    crc = 0xffffffff;
  for (let i = 0; i < buf.length; i++) {
    c = (crc ^ buf[i]) & 0xff;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    crc = (crc >>> 8) ^ c;
  }
  return (crc ^ 0xffffffff) >>> 0;
}
function storeZip(entries) {
  const locals = [],
    centrals = [];
  let offset = 0;
  for (const [name, data] of entries) {
    const n = Buffer.from(name, 'utf8');
    const crc = crc32(data);
    const head = Buffer.alloc(30);
    head.writeUInt32LE(0x04034b50, 0);
    head.writeUInt16LE(20, 4); // version needed
    head.writeUInt16LE(0, 6); // flags
    head.writeUInt16LE(0, 8); // method=store
    head.writeUInt32LE(0, 10); // dos time/date(固定 0,确定性产物)
    head.writeUInt32LE(crc, 14);
    head.writeUInt32LE(data.length, 18);
    head.writeUInt32LE(data.length, 22);
    head.writeUInt16LE(n.length, 26);
    head.writeUInt16LE(0, 28);
    locals.push(head, n, data);

    const cen = Buffer.alloc(46);
    cen.writeUInt32LE(0x02014b50, 0);
    cen.writeUInt16LE(20, 4); // made by
    cen.writeUInt16LE(20, 6); // needed
    cen.writeUInt16LE(0, 8);
    cen.writeUInt16LE(0, 10);
    cen.writeUInt32LE(0, 12);
    cen.writeUInt32LE(crc, 16);
    cen.writeUInt32LE(data.length, 20);
    cen.writeUInt32LE(data.length, 24);
    cen.writeUInt16LE(n.length, 28);
    // extra/comment/disk/attrs 全 0
    cen.writeUInt32LE(offset, 42);
    centrals.push(cen, n);
    offset += 30 + n.length + data.length;
  }
  const centralStart = offset;
  const centralBuf = Buffer.concat(centrals);
  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0);
  eocd.writeUInt16LE(entries.length, 8);
  eocd.writeUInt16LE(entries.length, 10);
  eocd.writeUInt32LE(centralBuf.length, 12);
  eocd.writeUInt32LE(centralStart, 16);
  return Buffer.concat([...locals, centralBuf, eocd]);
}
const zipBytes = storeZip([
  ['package-manifest.json', packageManifest],
  ['package-manifest.sig', manifestSig],
  ['plugin-manifest.json', pluginManifest],
  ['psd-worker.exe', workerBytes],
]);
const zipPath = path.join(out, `${PLUGIN_ID}.zip`);
fs.writeFileSync(zipPath, zipBytes);

// ── 6. 签名 index ─────────────────────────────────────────────────────────────
const now = Math.floor(Date.now() / 1000);
const fileUrl = 'file:///' + zipPath.replace(/\\/g, '/');
const index = Buffer.from(
  JSON.stringify(
    {
      schema: 1,
      key_id: 'dev-release-local',
      sequence: seq,
      generated_at: now,
      expires_at: now + 30 * 86400,
      plugins: [
        {
          plugin_id: PLUGIN_ID,
          version,
          package_sequence: seq,
          media_kind: 'image',
          formats: ['psd'],
          capabilities: ['thumbnail'],
          sku: 'psd-engine-2026',
          min_host_version: '0.1.0',
          target: TARGET,
          package_url: fileUrl,
          package_size: zipBytes.length,
          package_sha256: sha256(zipBytes),
        },
      ],
    },
    null,
    2
  )
);
fs.writeFileSync(path.join(out, 'index.json'), index);
fs.writeFileSync(path.join(out, 'index.sig'), crypto.sign(null, index, releaseKey));

const regBase = 'file:///' + out.replace(/\\/g, '/');
console.log(`dev registry 已生成(seq=${seq}, version=${version})
  keyset : ${path.join(out, 'dev-keyset.json')}
  index  : ${path.join(out, 'index.json')}
  zip    : ${zipPath}(${(zipBytes.length / 1048576).toFixed(1)} MB)

在启动 tauri dev 的同一 PowerShell 里设置:
  $env:PICASA_EXOTIC_DEV_FILE_URLS = '1'
  $env:PICASA_EXOTIC_DEV_KEYSET = '${path.join(out, 'dev-keyset.json')}'
  $env:PICASA_REGISTRY_BASE = '${regBase}'
需要授权 gate 也放行时(激活/运行付费插件),dev 构建加 feature:
  npm run tauri dev -- -- --features exotic-dev-fixtures
产物自检(生产同套校验器):
  $env:PICASA_EXOTIC_DEV_FILE_URLS='1'; cargo test -p picasa-next --lib dev_registry_artifacts -- --ignored`);
