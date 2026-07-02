<!-- src/components/exotic/ExoticActivateDialog.vue -->
<!-- 插件激活对话框（Part5 T12 增量3）：粘贴授权码 → 后端验签存 keyring。触点自持的聚焦弹窗。 -->
<!-- Plugin activation dialog (Part5 T12): paste license token → backend verifies & stores. -->
<!--
  🔴 开源/闭源边界（Part0 §10）：本弹窗只把用户输入的 token 原样交后端；验签/存储全在后端
     （activate_exotic_plugin 内先验后存，失败不覆盖现有有效 token）。前端不解析、不校验 token。
-->
<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="dialog-overlay"
      @click.self="onCancel"
      @keydown.esc.stop="onCancel"
      tabindex="-1"
    >
      <div class="dialog-content">
        <header class="dialog-header">
          <h2 class="dialog-title">{{ $t('exotic.activateTitle') }}</h2>
          <button
            class="btn-close"
            :title="$t('common.close')"
            :aria-label="$t('common.close')"
            @click="onCancel"
          >
            <X :size="18" />
          </button>
        </header>

        <main class="dialog-body">
          <p class="dialog-message">
            {{
              featureName
                ? $t('exotic.activateDesc', { name: featureName })
                : $t('exotic.activateDescGeneric')
            }}
          </p>
          <textarea
            ref="tokenRef"
            v-model="token"
            class="activate-token"
            :placeholder="$t('exotic.activateTokenPlaceholder')"
            rows="4"
            spellcheck="false"
            autocapitalize="off"
            autocomplete="off"
            @keydown.enter.exact.prevent="onSubmit"
          />
          <!-- 错误只回后端稳定 code（不含 token 材料），此处按 code 提示。 -->
          <p v-if="errorCode" class="activate-error">
            {{ $t('exotic.activateFailed', { code: errorCode }) }}
          </p>
        </main>

        <footer class="dialog-footer">
          <button class="btn btn-ghost" :disabled="gate.activating.value" @click="onCancel">
            {{ $t('detail.close') }}
          </button>
          <button class="btn btn-primary" :disabled="!canSubmit" @click="onSubmit">
            {{ gate.activating.value ? $t('exotic.activating') : $t('exotic.activateSubmit') }}
          </button>
        </footer>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { X } from '@lucide/vue'
import { useI18n } from 'vue-i18n'

import { useExoticGate } from '../../composables/useExoticGate'
import { useUiStore } from '../../stores/uiStore'
import type { IpcError } from '../../utils/ipc'

interface Props {
  open: boolean
  /** 待激活插件 id（取自已解析的 entitlement，非用户任意输入）。 */
  pluginId: string
  /** 功能名（对话框文案用）。 */
  featureName?: string
}
const props = withDefaults(defineProps<Props>(), { featureName: '' })

const emit = defineEmits<{
  (e: 'close'): void
  /** 激活成功 → 父组件据此重解析授权态（关闭 gate）。 */
  (e: 'activated'): void
}>()

const { t } = useI18n()
const ui = useUiStore()
const gate = useExoticGate()

const token = ref('')
const errorCode = ref<string | null>(null)
const tokenRef = ref<HTMLTextAreaElement | null>(null)

// 非空且未在激活中才可提交（防重复提交）。
const canSubmit = computed(() => token.value.trim().length > 0 && !gate.activating.value)

// 每次打开：清空上次输入/错误并聚焦（Teleport 组件常驻，须手动复位）。
watch(
  () => props.open,
  (isOpen) => {
    if (isOpen) {
      token.value = ''
      errorCode.value = null
      nextTick(() => tokenRef.value?.focus())
    }
  },
)

function onCancel() {
  if (gate.activating.value) return // 激活在途不允许中途关闭，避免态错乱
  emit('close')
}

async function onSubmit() {
  if (!canSubmit.value) return
  errorCode.value = null
  try {
    await gate.activate(props.pluginId, token.value.trim())
    ui.addToast('success', t('exotic.activateSuccess'))
    emit('activated')
    emit('close')
  } catch (e) {
    // IpcError 带后端稳定 code（bad_token / no_sku / …）；无 code 兜底 'unknown'。
    errorCode.value = (e as IpcError)?.code ?? 'unknown'
  }
}
</script>

<style scoped>
/* 复用项目对话框视觉约定（与 ConfirmDialog 一致）。 */
.dialog-overlay {
  position: fixed;
  inset: 0;
  z-index: 10000;
  background: color-mix(in srgb, var(--color-bg-primary) 60%, transparent);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
  display: flex;
  align-items: center;
  justify-content: center;
  animation: fadeIn 0.2s ease-out;
}
.dialog-content {
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  box-shadow: 0 10px 40px rgba(0, 0, 0, 0.2);
  width: 100%;
  max-width: 460px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  animation: slideUp 0.2s ease-out;
}
.dialog-header {
  padding: var(--spacing-md) var(--spacing-lg);
  border-bottom: 1px solid var(--color-border);
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.dialog-title {
  margin: 0;
  font-size: var(--font-size-lg);
  font-weight: 600;
  color: var(--color-text-primary);
}
.btn-close {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  color: var(--color-text-secondary);
  cursor: pointer;
  transition: all var(--transition-fast);
}
.btn-close:hover {
  background: var(--color-bg-hover);
  color: var(--color-text-primary);
}
.dialog-body {
  padding: var(--spacing-lg);
  display: flex;
  flex-direction: column;
  gap: var(--spacing-md);
}
.dialog-message {
  margin: 0;
  font-size: var(--font-size-base);
  color: var(--color-text-secondary);
  line-height: 1.5;
}

/* 授权码输入：等宽字体（token 多为 base64url），可换行不横向溢出。 */
.activate-token {
  width: 100%;
  box-sizing: border-box;
  resize: vertical;
  min-height: 84px;
  padding: var(--spacing-sm) var(--spacing-md);
  font-family: var(--font-mono);
  font-size: var(--font-size-sm);
  line-height: 1.5;
  color: var(--color-text-primary);
  background: var(--color-input-bg);
  border: 1px solid var(--color-input-border);
  border-radius: var(--radius-md);
  word-break: break-all;
}
.activate-token:focus {
  outline: none;
  border-color: var(--color-input-border-focus);
}

.activate-error {
  margin: 0;
  font-size: var(--font-size-sm);
  color: var(--color-error);
}

.dialog-footer {
  padding: var(--spacing-md) var(--spacing-lg);
  border-top: 1px solid var(--color-border);
  background: var(--color-bg-primary);
  display: flex;
  justify-content: flex-end;
  gap: var(--spacing-sm);
}
.btn:disabled {
  opacity: var(--opacity-disabled);
  cursor: not-allowed;
}

@keyframes fadeIn {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}
@keyframes slideUp {
  from {
    opacity: 0;
    transform: translateY(10px) scale(0.98);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}
</style>
