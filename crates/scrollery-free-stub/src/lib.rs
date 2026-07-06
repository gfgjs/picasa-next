// crates/scrollery-free-stub/src/lib.rs
//! 开源免费桩实现（Part6 §3.9.1）。
//!
//! `FreeStubEntitlement` 始终 `Unlicensed`——开源 fork / OSS 默认 build 链接它，使所有付费插件不可用
//! （核心免费功能完整）。真实直销实现 `DirectEntitlement`（keyring 验签）在闭源 `pro`，经私有 CI
//! overlay 在商业 build 改链。**本桩不持任何密钥/验签逻辑**（删 gate 重编译也只得 `Unlicensed`）。

#![forbid(unsafe_code)]

use scrollery_plugin_api::{ActivationInfo, EntitlementProvider, LicenseError, LicenseStatus};

/// 始终「未授权」的授权源（开源默认 / 只读桩 / 单测）。
///
/// 取代原 `src-tauri/src/exotic/license.rs` 内联的 `UnlicensedSource`——下沉为独立开源 crate，
/// 与闭源 `pro` 的 `DirectEntitlement` 经 `#[cfg(feature = "pro")]` 二选一。
pub struct FreeStubEntitlement;

impl EntitlementProvider for FreeStubEntitlement {
    fn evaluate(&self, _plugin_id: &str, _sku: &str, _now: i64) -> LicenseStatus {
        LicenseStatus::Unlicensed
    }

    fn source_tag(&self) -> &'static str {
        "free"
    }

    /// 免费桩不支持激活（无验签逻辑、无凭据存储）——稳定错误码 `activation_unsupported`。
    /// 亦覆盖组合根 fail-closed 回退场景：信任根解析失败降级本桩时，激活同样被拒（R1-1）。
    fn activate(
        &self,
        _plugin_id: &str,
        _sku: &str,
        _credential: &str,
        _now: i64,
    ) -> Result<ActivationInfo, LicenseError> {
        Err(LicenseError::ActivationUnsupported)
    }

    /// 无凭据可撤，幂等成功（Part0 §9.1 设计注：「activate→Err、deactivate→Ok」）。
    fn deactivate(&self, _plugin_id: &str) -> Result<(), LicenseError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_unlicensed() {
        let stub = FreeStubEntitlement;
        assert_eq!(
            stub.evaluate("any-plugin", "any-sku", 0),
            LicenseStatus::Unlicensed
        );
        assert_eq!(stub.source_tag(), "free");
    }

    /// R1-1 契约锁：激活必须 fail-closed（稳定码 activation_unsupported），撤销幂等成功。
    #[test]
    fn activate_rejected_deactivate_idempotent() {
        let stub = FreeStubEntitlement;
        assert_eq!(
            stub.activate("any-plugin", "any-sku", "any-token", 0),
            Err(LicenseError::ActivationUnsupported)
        );
        assert_eq!(stub.deactivate("any-plugin"), Ok(()));
    }
}
