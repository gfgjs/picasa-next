<template>
  <AccordionSection id="library" :order="order" :title="$t('sidebar.library')">
    <ul class="nav-list">
      <li v-for="album in smartAlbums" :key="album.id">
        <button
          class="nav-item"
          :class="{ active: ui.activeSmartAlbum === album.id && !ui.activeDirectoryId }"
          @click="onAlbumClick(album.id)"
        >
          <span class="nav-item__icon"><component :is="album.icon" :size="18" /></span>
          <span class="nav-item__label">{{ album.label }}</span>
          <span v-if="album.count != null" class="nav-item__count">{{ formatCount(album.count) }}</span>
        </button>
      </li>
    </ul>
  </AccordionSection>
</template>

<script setup lang="ts">
import { computed, markRaw } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ImageIcon, Heart, Sparkles, Clock, Trash2 } from '@lucide/vue'
import AccordionSection from '../AccordionSection.vue'
import { useUiStore } from '../../../stores/uiStore'
import { useMediaStore } from '../../../stores/mediaStore'
import type { SmartAlbum } from '../../../types/ui'

defineProps<{ order: number }>()

const ui = useUiStore()
const media = useMediaStore()
const router = useRouter()
const route = useRoute()
const { t } = useI18n()

// Smart albums — counts come from media stats (null = no count shown).
// 智能相册——计数来自媒体统计（null = 不显示计数）。
const smartAlbums = computed(() => [
  { id: 'all'         as const, icon: markRaw(ImageIcon), label: t('sidebar.allPhotos'),     count: media.stats?.totalItems },
  { id: 'favorites'   as const, icon: markRaw(Heart),     label: t('sidebar.favorites'),     count: media.stats?.totalFavorited },
  { id: 'live-photos' as const, icon: markRaw(Sparkles),  label: t('sidebar.livePhotos'),    count: media.stats?.totalLivePhotos },
  { id: 'recent'      as const, icon: markRaw(Clock),     label: t('sidebar.recentlyAdded'), count: null },
  { id: 'trash'       as const, icon: markRaw(Trash2),    label: t('sidebar.trash'),         count: media.stats?.totalDeleted },
])

function formatCount(n: number | undefined | null): string {
  if (n == null) return ''
  if (n >= 1000) return (n / 1000).toFixed(1) + 'k'
  return String(n)
}

function onAlbumClick(albumId: SmartAlbum) {
  ui.setSmartAlbum(albumId)
  if (route.path !== '/') router.push('/')
}
</script>

<style scoped>
.nav-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 0 var(--spacing-xs);
}
.nav-item {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  width: 100%;
  padding: 6px var(--spacing-sm);
  border-radius: var(--radius-md);
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
  transition: background-color var(--transition-fast), color var(--transition-fast);
  text-align: left;
}
.nav-item:hover {
  background: var(--color-sidebar-hover-bg);
  color: var(--color-text-primary);
}
.nav-item.active {
  background: var(--color-sidebar-active-bg);
  color: var(--color-sidebar-active-text);
  font-weight: 600;
}
.nav-item__icon {
  width: 20px;
  display: inline-flex;
  justify-content: center;
}
.nav-item__label { flex: 1; }
.nav-item__count {
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
  font-variant-numeric: tabular-nums;
}
</style>
