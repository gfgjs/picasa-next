<template>
  <div class="statusbar__info">
    <span v-if="scan.isAnyScanRunning" class="statusbar__scanning">
      <span class="spinner" />
      {{ $t('statusbar.scanningSimple') }}
    </span>
    <span v-else-if="scan.thumbGenProgress.isRunning" class="statusbar__scanning" title="正在后台生成缩略图，不影响正常浏览" style="cursor: help; display: flex; align-items: center; gap: 4px;">
      <span class="spinner" />
      {{ $t('settings.genStatusRunning', { generated: scan.thumbGenProgress.generated, total: scan.thumbGenProgress.total }) }}
      <span v-if="scan.thumbGenProgress.currentItem" style="opacity: 0.8; max-width: 150px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; display: inline-block;">({{ scan.thumbGenProgress.currentItem }})</span>
      <button @click="scan.stopFullThumbnailGeneration()" class="statusbar__stop-btn" title="停止生成" style="background: none; border: none; cursor: pointer; color: inherit; padding: 2px; display: flex; align-items: center; border-radius: 4px; opacity: 0.8;">
        <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"></rect></svg>
      </button>
    </span>
    <span v-else-if="scan.autoThumbInFlight > 0" class="statusbar__scanning" title="正在生成视口缩略图" style="cursor: help;">
      <span class="spinner" />
      视口缩略图: 处理中 {{ scan.autoThumbInFlight }} 项
      <template v-if="scan.autoThumbQueueSize > 0"><span style="opacity: 0.8; margin-left: 4px;">(排队 {{ scan.autoThumbQueueSize }})</span></template>
    </span>
    <span v-else-if="media.stats">
      {{ $t('statusbar.items', { count: media.totalItems.toLocaleString() }) }}
      <template v-if="media.stats.totalImages > 0"> · {{ $t('statusbar.images', { count: media.stats.totalImages.toLocaleString() }) }}</template>
      <template v-if="media.stats.totalVideos > 0"> · {{ $t('statusbar.videos', { count: media.stats.totalVideos.toLocaleString() }) }}</template>
    </span>
  </div>

  <div class="statusbar__right">
    <span v-if="media.isComputingLayout" class="statusbar__computing">
      <span class="spinner" />
      {{ $t('statusbar.computingLayout') }}
    </span>
    <span class="statusbar__version">v0.1.0</span>
  </div>
</template>

<script setup lang="ts">
import { useScanStore } from '../../stores/scanStore'
import { useMediaStore } from '../../stores/mediaStore'

const scan  = useScanStore()
const media = useMediaStore()
</script>

<style scoped>
.statusbar__info {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  flex: 1;
  overflow: hidden;
}
.statusbar__scanning {
  display: flex;
  align-items: center;
  gap: var(--spacing-xs);
  color: var(--color-accent);
}
.statusbar__right {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
}
.statusbar__computing {
  display: flex;
  align-items: center;
  gap: var(--spacing-xs);
  color: var(--color-text-tertiary);
}
.statusbar__version {
  color: var(--color-text-tertiary);
  font-family: var(--font-mono);
  font-size: 11px;
}
.statusbar__stop-btn:hover {
  background: var(--color-bg-hover) !important;
  opacity: 1 !important;
}
</style>
