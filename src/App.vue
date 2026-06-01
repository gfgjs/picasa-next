<template>
  <AppShell>
    <template #sidebar>
      <AppSidebar />
    </template>

    <template #toolbar>
      <AppToolbar
        @search="onSearch"
        @sort-change="onSortChange"
      />
    </template>

    <RouterView />

    <template #statusbar>
      <AppStatusBar />
    </template>
  </AppShell>

  <!-- Detail overlay (global) -->
  <MediaDetailOverlay />

  <!-- Toast notifications -->
  <ToastContainer />
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import { useTheme }    from './composables/useTheme'
import { useUiStore }  from './stores/uiStore'
import { useScanStore } from './stores/scanStore'
import { useMediaStore } from './stores/mediaStore'

import AppShell          from './components/layout/AppShell.vue'
import AppSidebar        from './components/sidebar/AppSidebar.vue'
import AppToolbar        from './components/layout/AppToolbar.vue'
import AppStatusBar      from './components/layout/AppStatusBar.vue'
import MediaDetailOverlay from './components/media/MediaDetailOverlay.vue'
import ToastContainer    from './components/common/ToastContainer.vue'

const ui    = useUiStore()
const scan  = useScanStore()
const media = useMediaStore()

// Init theme
useTheme()

function onSearch(query: string) {
  ui.searchQuery = query
}

function onSortChange() {
  // Trigger layout re-computation (handled via watcher in useJustifiedLayout)
}

onMounted(async () => {
  await scan.loadScanRoots()
  await media.loadStats()
})
</script>