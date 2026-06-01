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

import AppShell          from './components/layout/AppShell.vue'
import AppSidebar        from './components/sidebar/AppSidebar.vue'
import AppToolbar        from './components/layout/AppToolbar.vue'
import AppStatusBar      from './components/layout/AppStatusBar.vue'
import MediaDetailOverlay from './components/media/MediaDetailOverlay.vue'
import ToastContainer    from './components/common/ToastContainer.vue'

const ui    = useUiStore()

// Init theme
useTheme()

function onSearch(query: string) {
  ui.searchQuery = query
}

function onSortChange() {
  // Trigger layout re-computation (handled via watcher in useJustifiedLayout)
}

onMounted(async () => {
  // Theme init only — data loading is handled in AppSidebar.vue onMounted
  // to keep initialization sequential and avoid double-compute races.
})
</script>