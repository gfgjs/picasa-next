<template>
  <div v-if="isOpen" class="modal-overlay" @click.self="close">
    <div class="modal-content">
      <h2 class="modal-title">设置</h2>
      
      <div class="setting-item">
        <div class="setting-info">
          <div class="setting-label">小文件直显阈值 (KB)</div>
          <div class="setting-desc">小于此大小的图片将不生成缩略图，直接加载原图以节省磁盘空间并提高加载速度。设为 0 则对所有图片生成缩略图。</div>
        </div>
        <input 
          type="number" 
          v-model.number="thumbSkipMaxKb" 
          min="0" 
          max="1000000"
          class="setting-input"
        />
      </div>

      <div class="modal-actions">
        <button class="btn btn-secondary" @click="close">取消</button>
        <button class="btn btn-primary" @click="save">保存</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useUiStore } from '../../stores/uiStore'

const ui = useUiStore()

const isOpen = ref(false)
const thumbSkipMaxKb = ref(200)

async function openModal() {
  isOpen.value = true
  try {
    const val = await invoke<string | null>('get_app_config', { key: 'thumb_skip_max_kb' })
    if (val) {
      thumbSkipMaxKb.value = parseInt(val, 10)
    }
  } catch (e) {
    console.error('Failed to get config:', e)
  }
}

async function save() {
  try {
    await invoke('set_app_config', { key: 'thumb_skip_max_kb', value: thumbSkipMaxKb.value.toString() })
    ui.addToast('success', '设置已保存')
    isOpen.value = false
  } catch (e) {
    ui.addToast('error', '保存失败: ' + e)
  }
}

function close() {
  isOpen.value = false
}

defineExpose({ openModal })
</script>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  backdrop-filter: blur(4px);
}
.modal-content {
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  padding: var(--spacing-xl);
  width: 400px;
  max-width: 90vw;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.3);
}
.modal-title {
  margin: 0 0 var(--spacing-lg);
  font-size: var(--font-size-lg);
  font-weight: 600;
  color: var(--color-text-primary);
}
.setting-item {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--spacing-md);
  margin-bottom: var(--spacing-lg);
}
.setting-info {
  flex: 1;
}
.setting-label {
  font-size: var(--font-size-sm);
  font-weight: 500;
  color: var(--color-text-primary);
  margin-bottom: 4px;
}
.setting-desc {
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
  line-height: 1.4;
}
.setting-input {
  width: 80px;
  padding: 6px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  background: var(--color-bg-primary);
  color: var(--color-text-primary);
  text-align: right;
  font-family: var(--font-mono);
}
.setting-input:focus {
  outline: none;
  border-color: var(--color-accent);
}
.modal-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--spacing-md);
}
.btn {
  padding: 6px 16px;
  border-radius: var(--radius-sm);
  font-size: var(--font-size-sm);
  cursor: pointer;
  border: none;
  font-weight: 500;
}
.btn-secondary {
  background: transparent;
  color: var(--color-text-secondary);
}
.btn-secondary:hover {
  background: var(--color-sidebar-hover-bg);
}
.btn-primary {
  background: var(--color-accent);
  color: #fff;
}
.btn-primary:hover {
  filter: brightness(1.1);
}
</style>
