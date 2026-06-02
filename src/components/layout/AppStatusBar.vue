<template>
  <div class="statusbar__info">
    <span v-if="scan.isAnyScanRunning" class="statusbar__scanning">
      <span class="spinner" />
      {{ $t('statusbar.scanningSimple') }}
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
</style>
