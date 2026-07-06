// crates/scrollery-exotic-trust/src/lib.rs
//! 插件平台 · 开源信任根验签叶 crate（Part6 §3.9.1a 去环 ③a）。
//!
//! 承接自 `src-tauri/src/exotic/{crypto,license}` 迁入的**通用验签原语**（无秘密价值、计划明定留开源，
//! Part6 §3.9 line 553）：
//!   - [`crypto`]：`VerifyingKeyset`（Ed25519 信任根公钥集）+ `verify`/`verify_any` + base64url。
//!   - [`license`]：`verify_token`/`evaluate_token`/`LicensePayload`（纯函数，不碰 keyring）。
//!
//! **去环拓扑**（一律单向、无环）：
//!   `plugin-api`（最底叶：EntitlementProvider/LicenseStatus/LicenseError）
//!       ↑
//!   `exotic-trust`（本 crate；依赖 plugin-api 拿 LicenseError/LicenseStatus）
//!       ↑
//!   `src-tauri`（KeyringLicenseStore + registry/package/install，经 `crate::exotic::crypto` 薄壳复用）
//!   `pro`（③b：DirectEntitlement，单向依赖 plugin-api + exotic-trust，**不**依赖 src-tauri → 破环）
//!
//! **零密钥红线**：本 crate 只含公钥验签逻辑；真实**生产**公钥 `exotic-keyset-prod.json` + `builtin()`
//! 生产构造随 pro 下沉（③b）。当前内置的是**占位**公钥（非生产密钥），不触红线。

#![forbid(unsafe_code)]

pub mod crypto;
pub mod license;

// 便捷再导出：下游可 `use scrollery_exotic_trust::{VerifyingKeyset, verify_token, ...}`。
pub use crypto::{
    b64url_decode, b64url_encode, CryptoError, KeyPurpose, KeyStatus, VerifyingKeyset,
};
pub use license::{evaluate_token, verify_token, LicensePayload};
