<template>
  <div class="statusbar__info">
    <span v-if="scan.isAnyScanRunning" class="statusbar__scanning">
      <span class="spinner" />
      正在扫描...
    </span>
    <span v-else-if="media.stats">
      {{ media.totalItems.toLocaleString() }} 个项目
      <template v-if="media.stats.totalImages > 0"> · {{ media.stats.totalImages.toLocaleString() }} 张图片</template>
      <template v-if="media.stats.totalVideos > 0"> · {{ media.stats.totalVideos.toLocaleString() }} 段视频</template>
    </span>
  </div>

  <div class="statusbar__right">
    <span v-if="media.isComputingLayout" class="statusbar__computing">
      <span class="spinner" />
      计算布局...
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
</style>
