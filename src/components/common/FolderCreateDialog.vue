<template>
  <div
    class="dialog-overlay"
    @click.self="cancel"
    tabindex="-1"
    @keydown.esc.stop="cancel"
    ref="overlayRef"
  >
    <div class="dialog-content">
      <header class="dialog-header">
        <h2 class="dialog-title">
          {{ isGlobal ? t('folderCreate.titleGlobal') : t('sidebar.newSubfolder') }}
        </h2>
        <button
          class="btn-close"
          :title="t('common.cancel')"
          :aria-label="t('common.cancel')"
          @click="cancel"
        >
          <X :size="18" />
        </button>
      </header>

      <main class="dialog-body">
        <div v-if="isGlobal" class="form-group">
          <label>{{ t('folderCreate.basePath') }}</label>
          <div style="display: flex; gap: 8px">
            <input
              type="text"
              class="input-text"
              v-model="selectedBasePath"
              readonly
              :placeholder="t('folderCreate.basePathPlaceholder')"
              style="flex: 1"
            />
            <button class="btn btn-secondary" @click="selectBasePath">
              {{ t('folderCreate.choose') }}
            </button>
          </div>
        </div>
        <div v-else class="form-group">
          <label>{{ t('folderCreate.parentPath') }}</label>
          <input type="text" class="input-text" :value="basePath" readonly disabled />
        </div>

        <div class="form-group">
          <label>{{ t('folderCreate.folderName') }}</label>
          <input
            type="text"
            class="input-text"
            v-model="folderName"
            :placeholder="t('folderCreate.folderNamePlaceholder')"
            autofocus
            @keyup.enter="create"
          />
        </div>

        <div v-if="errorMessage" class="error-message">
          {{ errorMessage }}
        </div>
      </main>

      <footer class="dialog-footer">
        <button class="btn btn-secondary" @click="cancel">{{ t('common.cancel') }}</button>
        <button class="btn btn-primary" :disabled="!canCreate" @click="create">
          {{ t('folderCreate.create') }}
        </button>
      </footer>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import { X } from '@lucide/vue'
import { open } from '@tauri-apps/plugin-dialog'
import { invokeIpc } from '../../utils/ipc'
import { IPC } from '../../constants/ipc'
import { useUiStore } from '../../stores/uiStore'

const props = defineProps<{
  basePath: string // If empty, it's global create
}>()

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'created'): void
}>()

const { t } = useI18n()
const ui = useUiStore()

const isGlobal = computed(() => !props.basePath)
const selectedBasePath = ref(props.basePath || '')
const folderName = ref('')
const errorMessage = ref('')
const overlayRef = ref<HTMLElement | null>(null)

onMounted(() => {
  nextTick(() => {
    overlayRef.value?.focus()
  })
})

const canCreate = computed(() => {
  return selectedBasePath.value.trim() !== '' && folderName.value.trim() !== ''
})

async function selectBasePath() {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title: t('folderCreate.chooseBaseDir'),
    })
    if (selected) {
      selectedBasePath.value = typeof selected === 'string' ? selected : selected[0]
      errorMessage.value = ''
    }
  } catch (e) {
    errorMessage.value = t('sidebar.chooseDirFailed', { error: String(e) })
  }
}

function cancel() {
  emit('close')
}

async function create() {
  if (!canCreate.value) return
  errorMessage.value = ''

  try {
    await invokeIpc(IPC.CREATE_PHYSICAL_FOLDER, {
      basePath: selectedBasePath.value,
      folderName: folderName.value.trim(),
    })
    ui.addToast('success', t('folderCreate.createSuccess'))
    emit('created')
    emit('close')
  } catch (e) {
    errorMessage.value = String(e)
  }
}
</script>

<style scoped>
.dialog-overlay {
  position: fixed;
  inset: 0;
  z-index: 9999;
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

.form-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.form-group label {
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
}

.input-text {
  padding: 8px 12px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-bg-primary);
  color: var(--color-text-primary);
  font-size: var(--font-size-sm);
}
.input-text:disabled,
.input-text[readonly] {
  background: var(--color-bg-secondary);
  color: var(--color-text-tertiary);
}

.error-message {
  font-size: var(--font-size-sm);
  color: var(--color-error);
  margin-top: 4px;
}

.dialog-footer {
  padding: var(--spacing-md) var(--spacing-lg);
  border-top: 1px solid var(--color-border);
  background: var(--color-bg-primary);
  display: flex;
  justify-content: flex-end;
  gap: var(--spacing-sm);
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
