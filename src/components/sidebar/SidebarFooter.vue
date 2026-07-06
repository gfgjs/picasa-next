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
    <!-- 三态循环 亮→暗→跟随系统(P2 修复:原二态循环使 system 从此处不可达)。
         图标显示当前模式本身(Sun=亮/Moon=暗/Monitor=跟随系统),而非"将切换到"的目标。 -->
    <button
      class="btn-icon"
      :title="$t('sidebar.toggleTheme')"
      :aria-label="$t('sidebar.toggleTheme')"
      @click="ui.cycleAppearance()"
    >
      <Sun v-if="ui.appearance === 'light'" :size="18" />
      <Moon v-else-if="ui.appearance === 'dark'" :size="18" />
      <Monitor v-else :size="18" />
    </button>
  </div>
</template>

<script setup lang="ts">
import { Settings, Sun, Moon, Monitor } from '@lucide/vue'
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
