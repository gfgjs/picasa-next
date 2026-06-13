<template>
  <!-- App logo + title — pinned above the scroll area. -->
  <!-- 应用 Logo + 标题——固定在滚动区域上方。 -->
  <div class="sidebar-header">
    <span class="sidebar-header__icon"><Aperture :size="20" /></span>
    <span class="sidebar-header__text">Picasa Next</span>
  </div>
</template>

<script setup lang="ts">
import { Aperture } from '@lucide/vue'
</script>

<style scoped>
.sidebar-header {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  padding: var(--spacing-md);
  flex-shrink: 0;
}
.sidebar-header__icon {
  display: inline-flex;
  color: var(--color-accent);
}
.sidebar-header__text {
  font-size: var(--font-size-md);
  font-weight: 700;
  color: var(--color-text-primary);
  letter-spacing: -0.3px;
}
</style>
