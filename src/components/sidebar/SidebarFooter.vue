<template>
  <!-- Settings + theme toggle — pinned below the scroll area. -->
  <!-- 设置 + 主题切换——固定在滚动区域下方。 -->
  <div class="sidebar-footer">
    <button
      class="btn-icon"
      :title="$t('sidebar.settings')"
      :aria-label="$t('sidebar.settings')"
      @click="ui.isSettingsOpen = true"
    >
      <Settings :size="18" />
    </button>
    <button
      class="btn-icon"
      :title="$t('sidebar.toggleTheme')"
      :aria-label="$t('sidebar.toggleTheme')"
      @click="ui.cycleTheme()"
    >
      <Sun v-if="ui.isDark" :size="18" />
      <Moon v-else :size="18" />
    </button>
  </div>
</template>

<script setup lang="ts">
import { Settings, Sun, Moon } from '@lucide/vue'
import { useUiStore } from '../../stores/uiStore'

const ui = useUiStore()
</script>

<style scoped>
.sidebar-footer {
  border-top: 1px solid var(--color-border);
  padding: var(--spacing-sm) var(--spacing-md);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--spacing-sm);
  flex-shrink: 0;
}
</style>
