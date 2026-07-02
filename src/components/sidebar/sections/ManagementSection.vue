<template>
  <!-- Only present when there are scan roots to manage — when absent, the section
       unregisters itself so the sticky stacking offsets stay gap-free. -->
  <!-- 仅在有可管理的扫描根目录时存在——不存在时区块会自行注销，使粘性堆叠偏移不留空档。 -->
  <AccordionSection
    v-if="scan.hasScanRoots"
    id="management"
    :order="order"
    :title="$t('sidebar.management')"
  >
    <div class="scan-status">
      <div v-for="root in scan.scanRoots" :key="root.id" class="scan-root">
        <div class="scan-root__info">
          <span class="scan-root__alias">{{ root.alias ?? root.path.split('/').pop() }}</span>
          <div class="scan-root__actions">
            <button
              class="btn-icon"
              :class="{ active: scan.getProgress(root.id)?.isRunning }"
              :title="
                scan.getProgress(root.id)?.isRunning ? $t('sidebar.stopScan') : $t('sidebar.rescan')
              "
              :aria-label="
                scan.getProgress(root.id)?.isRunning ? $t('sidebar.stopScan') : $t('sidebar.rescan')
              "
              @click="toggleScan(root.id)"
            >
              <Square
                v-if="scan.getProgress(root.id)?.isRunning"
                :size="14"
                color="var(--color-error)"
                fill="var(--color-error)"
              />
              <RefreshCw v-else :size="14" />
            </button>
            <button
              class="btn-icon scan-root__remove"
              :title="$t('sidebar.removeFolder')"
              :aria-label="$t('sidebar.removeFolder')"
              @click="removeRoot(root.id)"
            >
              <Trash2 :size="14" />
            </button>
          </div>
        </div>

        <div v-if="scan.getProgress(root.id)?.isRunning" class="scan-root__progress">
          <div class="progress-bar">
            <div
              class="progress-bar__fill progress-shimmer"
              :class="{ 'progress-bar__fill--discovering': isIndeterminate(root.id) }"
              :style="{ width: (isIndeterminate(root.id) ? 100 : progressPercent(root.id)) + '%' }"
            />
          </div>
          <span class="scan-root__count">
            <template v-if="scan.getProgress(root.id)?.status === 'discovering'">
              {{
                $t('sidebar.discoveringFiles', { count: scan.getProgress(root.id)?.scanned ?? 0 })
              }}
            </template>
            <template v-else-if="scan.getProgress(root.id)?.status === 'enriching'">
              <template v-if="(scan.getProgress(root.id)?.total ?? 0) > 0">
                {{
                  $t('sidebar.enrichingFiles', {
                    scanned: scan.getProgress(root.id)?.scanned ?? 0,
                    total: scan.getProgress(root.id)?.total ?? 0,
                  })
                }}
              </template>
              <template v-else>{{ $t('sidebar.enrichingStart') }}</template>
            </template>
            <template v-else>
              {{
                $t('sidebar.indexingFiles', {
                  scanned: scan.getProgress(root.id)?.scanned ?? 0,
                  total: scan.getProgress(root.id)?.total ?? 0,
                })
              }}
            </template>
          </span>
        </div>
      </div>
    </div>
  </AccordionSection>
</template>

<script setup lang="ts">
import { invokeIpc } from '../../../utils/ipc'
import { IPC } from '../../../constants/ipc'
import { useI18n } from 'vue-i18n'
import { Square, RefreshCw, Trash2 } from '@lucide/vue'
import AccordionSection from '../AccordionSection.vue'
import { useUiStore } from '../../../stores/uiStore'
import { useScanStore } from '../../../stores/scanStore'
import { useMediaStore } from '../../../stores/mediaStore'
import { useConfirm } from '../../../composables/useConfirm'

defineProps<{ order: number }>()

const ui = useUiStore()
const scan = useScanStore()
const media = useMediaStore()
const { confirm } = useConfirm()
const { t } = useI18n()

// ── Scan progress display ───────────────────────────────────────────────────
// ── 扫描进度显示 ───────────────────────────────────────────────────────────
function progressPercent(rootId: number): number {
  const p = scan.getProgress(rootId)
  if (!p || !p.total || p.status === 'discovering') return 0
  return Math.round((p.scanned / p.total) * 100)
}

// A phase with no known total yet shows an indeterminate (shimmer) bar instead
// of a 0% bar: discovering (walking), or enriching before the first batch event.
// 尚无已知总数的阶段显示不确定（流光）进度条而非 0%：检索中（遍历），
// 或 enriching 在首个批次事件之前。
function isIndeterminate(rootId: number): boolean {
  const p = scan.getProgress(rootId)
  if (!p) return false
  return p.status === 'discovering' || (p.status === 'enriching' && !p.total)
}

// ── Scan controls ───────────────────────────────────────────────────────────
// ── 扫描控制 ───────────────────────────────────────────────────────────────
async function toggleScan(rootId: number) {
  const p = scan.getProgress(rootId)
  if (p?.isRunning) {
    await scan.stopScan(rootId)
  } else {
    await scan.startScan(rootId, () => {
      media.loadStats()
      // Let FoldersSection refresh the tree (counts) — it owns the tree instance.
      // 让 FoldersSection 刷新树（计数）——树实例归它所有。
      window.dispatchEvent(new CustomEvent('folder-stats-changed'))
    })
  }
}

async function removeRoot(id: number) {
  const { confirmed, checkboxValue } = await confirm({
    title: t('sidebar.removeFolder'),
    message: t('sidebar.confirmRemove'),
    confirmText: t('sidebar.removeFolder'),
    cancelText: t('common.cancel'),
    showCheckbox: true,
    checkboxLabel: t('sidebar.clearThumbnails'),
    checkboxValue: true,
  })
  if (!confirmed) return

  try {
    const result = await invokeIpc<{ cleared_count: number }>(IPC.REMOVE_SCAN_ROOT_WITH_OPTIONS, {
      id,
      clearThumbnails: checkboxValue,
    })
    if (result.cleared_count > 0) {
      ui.addToast('success', t('sidebar.thumbnailsCleared', { count: result.cleared_count }))
    }
    // Removing a root changes scan.scanRoots → FoldersSection's watch reloads the
    // tree automatically. Refresh stats here.
    // 移除根目录会改变 scan.scanRoots → FoldersSection 的 watch 自动重载树。此处刷新统计。
    await scan.loadScanRoots()
    media.loadStats()
  } catch (e) {
    ui.addToast('error', t('sidebar.removeFolderFailed') + ' ' + e)
  }
}
</script>

<style scoped>
.scan-status {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-xs);
  padding: 0 var(--spacing-md);
}
.scan-root__info {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--spacing-sm);
}
.scan-root__alias {
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.scan-root__actions {
  display: flex;
  gap: 4px;
  flex-shrink: 0;
}
.scan-root__remove {
  color: var(--color-error);
  opacity: 0.7;
}
.scan-root__remove:hover {
  opacity: 1;
}
.scan-root__progress {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  margin-top: 2px;
}
.progress-bar {
  flex: 1;
  height: 3px;
  border-radius: 2px;
  background: var(--color-border);
  overflow: hidden;
}
.progress-bar__fill {
  height: 100%;
  border-radius: 2px;
  background: var(--color-accent);
  transition: width 100ms linear;
}
.progress-bar__fill--discovering {
  width: 100% !important;
  animation: breathe 1.5s ease-in-out infinite;
}
@keyframes breathe {
  0%,
  100% {
    opacity: 0.4;
  }
  50% {
    opacity: 1;
  }
}
.scan-root__count {
  font-size: 10px;
  color: var(--color-text-tertiary);
  white-space: nowrap;
}
</style>
