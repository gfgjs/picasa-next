<template>
  <div class="dialog-overlay" @click.self="cancel" tabindex="-1" @keydown.esc.stop="cancel" ref="overlayRef">
    <div class="dialog-content">
      <header class="dialog-header">
        <h2 class="dialog-title">{{ title }}</h2>
        <button class="btn-close" title="取消" @click="cancel">
          <X :size="18" />
        </button>
      </header>
      
      <main class="dialog-body" style="height: 320px; overflow-y: auto; padding: 0;">
        <div v-if="folderTree.nodes.value.length === 0 && !folderTree.loading.value" class="empty-state">
          暂无可选文件夹
        </div>
        <div class="tree-container" v-else>
          <button
            v-for="node in folderTree.nodes.value"
            :key="node.id"
            class="tree-item"
            :class="{ active: selectedNodeId === node.id }"
            :style="{ paddingLeft: (node.depth * 16 + 12) + 'px' }"
            @click="selectedNodeId = node.id; selectedNode = node"
          >
            <span class="tree-arrow" @click.stop="folderTree.toggleNode(node)">
              <ChevronRight v-if="node.hasChildren" :size="14" class="tree-chevron" :class="{ expanded: node.expanded }" />
              <span v-else class="tree-chevron-spacer" />
            </span>
            <span class="tree-icon"><Folder :size="15" /></span>
            <span class="tree-label" :title="getNodePath(node)">
              <span class="name">{{ node.name }}</span>
              <span class="path">{{ getNodePath(node) }}</span>
            </span>
          </button>
        </div>
      </main>

      <footer class="dialog-footer" style="display: flex; justify-content: space-between; align-items: center;">
        <div style="display: flex; gap: 8px;">
          <button class="btn btn-secondary" :disabled="!selectedNodeId" @click="createNewFolderHere" title="在当前选中的文件夹内新建子文件夹">在此处新建</button>
          <button class="btn btn-secondary" @click="createNewGlobalFolder" title="选择任意本地位置新建文件夹">在其他位置新建...</button>
        </div>
        <div style="display: flex; gap: 8px;">
          <button class="btn btn-secondary" @click="cancel">取消</button>
          <button class="btn btn-primary" :disabled="!selectedNodeId" @click="confirm">确定</button>
        </div>
      </footer>
    </div>

    <FolderCreateDialog
      v-if="folderCreateDialog.isOpen"
      :base-path="folderCreateDialog.basePath"
      @close="folderCreateDialog.isOpen = false"
      @created="onFolderCreated"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, nextTick } from 'vue'
import { X, ChevronRight, Folder } from '@lucide/vue'
import { useFolderTree } from '../../composables/useFolderTree'
import { useScanStore } from '../../stores/scanStore'
import FolderCreateDialog from './FolderCreateDialog.vue'
import type { DirNode } from '../../types/media'

const props = defineProps<{
  title: string
}>()

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'confirm', targetNode: DirNode): void
}>()

const scan = useScanStore()
const folderTree = useFolderTree()

const selectedNodeId = ref<number | null>(null)
const selectedNode = ref<DirNode | null>(null)
const overlayRef = ref<HTMLElement | null>(null)

const folderCreateDialog = ref({
  isOpen: false,
  basePath: ''
})

onMounted(async () => {
  if (scan.scanRoots.length === 0) {
    await scan.loadScanRoots()
  }
  await folderTree.loadRoots(scan.scanRoots)
  
  // Auto-select the first node if nothing is selected
  if (!selectedNodeId.value && folderTree.nodes.value.length > 0) {
    selectedNodeId.value = folderTree.nodes.value[0].id
    selectedNode.value = folderTree.nodes.value[0]
  }

  nextTick(() => {
    overlayRef.value?.focus()
  })
})

function cancel() {
  emit('close')
}

function confirm() {
  if (selectedNode.value) {
    emit('confirm', selectedNode.value)
  }
}

function createNewFolderHere() {
  if (selectedNode.value && selectedNode.value.absPath) {
    folderCreateDialog.value.basePath = selectedNode.value.absPath
    folderCreateDialog.value.isOpen = true
  }
}

function createNewGlobalFolder() {
  folderCreateDialog.value.basePath = ''
  folderCreateDialog.value.isOpen = true
}

function getNodePath(node: DirNode): string {
  if (node.absPath) return node.absPath
  const r = scan.scanRoots.find(root => root.id === node.rootId)
  if (r) {
    return node.relPath ? `${r.path}/${node.relPath}` : r.path
  }
  return node.relPath || ''
}

async function onFolderCreated() {
  // refresh tree
  await scan.loadScanRoots()
  await folderTree.loadRoots(scan.scanRoots)
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
  max-width: 500px;
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

.empty-state {
  padding: var(--spacing-xl);
  text-align: center;
  color: var(--color-text-tertiary);
}

.tree-container {
  display: flex;
  flex-direction: column;
  padding: var(--spacing-xs) 0;
}

.tree-item {
  display: flex;
  align-items: center;
  width: 100%;
  padding: 6px 12px;
  border: none;
  background: transparent;
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
  cursor: pointer;
  text-align: left;
}

.tree-item:hover {
  background: var(--color-bg-hover);
}

.tree-item.active {
  background: var(--color-accent);
  color: #fff;
}

.tree-arrow {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  cursor: pointer;
  border-radius: var(--radius-sm);
}

.tree-arrow:hover {
  background: color-mix(in srgb, currentColor 10%, transparent);
}

.tree-chevron {
  transition: transform 0.2s;
}

.tree-chevron.expanded {
  transform: rotate(90deg);
}

.tree-chevron-spacer {
  width: 14px;
}

.tree-icon {
  margin: 0 8px 0 4px;
  display: flex;
  align-items: center;
}

.tree-label {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: var(--font-size-sm);
  color: var(--color-text-primary);
  flex: 1;
  text-align: left;
  overflow: hidden;
}

.tree-label .name {
  white-space: nowrap;
  flex-shrink: 0;
}

.tree-label .path {
  font-size: 11px;
  color: var(--color-text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  direction: rtl; /* Truncate from left if too long to keep folder name visible at end */
  text-align: left;
}

.dialog-footer {
  padding: var(--spacing-md) var(--spacing-lg);
  border-top: 1px solid var(--color-border);
  background: var(--color-bg-primary);
}

@keyframes fadeIn {
  from { opacity: 0; }
  to { opacity: 1; }
}

@keyframes slideUp {
  from { opacity: 0; transform: translateY(10px) scale(0.98); }
  to { opacity: 1; transform: translateY(0) scale(1); }
}
</style>
