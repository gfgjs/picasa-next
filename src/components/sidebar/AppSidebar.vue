<template>
  <nav class="sidebar">
    <!-- Pinned logo (above scroll) | 固定 Logo（滚动区上方） -->
    <SidebarHeader />

    <!-- Scrollable accordion area. Section headers are direct children (via the
         two-root AccordionSection fragment) so they can stick top+bottom and
         stack across the WHOLE scroll range — see AccordionSection.vue.
         可滚动的手风琴区域。区块标题（经由 AccordionSection 的两根片段）是直接子
         元素，因此能在整个滚动范围内粘顶+粘底并堆叠——见 AccordionSection.vue。 -->
    <div class="sidebar__scroll-area">
      <LibrarySection :order="0" />
      <ToolsSection :order="1" />
      <FoldersSection :order="2" />
      <ManagementSection :order="3" />
      <!--
        Add a future menu by dropping one more section here, e.g.
        <AlbumsSection :order="4" />. Expand-state persistence and sticky-header
        stacking wire up automatically through provideSidebarSections().
        新增菜单只需在此再放一个区块，例如 <AlbumsSection :order="4" />。展开态持久化
        与粘性标题堆叠会通过 provideSidebarSections() 自动接入。
      -->
    </div>

    <!-- Pinned settings/theme footer (below scroll) | 固定的设置/主题页脚（滚动区下方） -->
    <SidebarFooter />

    <!-- Shared promise-based confirm dialog, mounted once for all sections. -->
    <!-- 共享的、基于 Promise 的确认对话框，为所有区块仅挂载一次。 -->
    <ConfirmDialog />
  </nav>
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import SidebarHeader from './SidebarHeader.vue'
import SidebarFooter from './SidebarFooter.vue'
import LibrarySection from './sections/LibrarySection.vue'
import ToolsSection from './sections/ToolsSection.vue'
import FoldersSection from './sections/FoldersSection.vue'
import ManagementSection from './sections/ManagementSection.vue'
import ConfirmDialog from '../common/ConfirmDialog.vue'
import { provideSidebarSections } from '../../composables/useSidebarSections'
import { useScanStore } from '../../stores/scanStore'
import { useMediaStore } from '../../stores/mediaStore'

// Provide the accordion controller (expand state + sticky math) to all sections.
// 向所有区块提供手风琴控制器（展开状态 + 粘性计算）。
provideSidebarSections()

const scan = useScanStore()
const media = useMediaStore()

// Data init: load scan roots + media stats once. The folder tree loads itself in
// FoldersSection (its watch on scan.scanRoots is the single source of tree loads).
// 数据初始化：加载扫描根目录 + 媒体统计。文件夹树由 FoldersSection 自行加载
//（其对 scan.scanRoots 的 watch 是树加载的唯一来源）。
onMounted(async () => {
  await scan.loadScanRoots()
  await media.loadStats()
})
</script>

<style scoped>
.sidebar {
  /* Single source of truth for sticky-header height + stacking math.
     Inherited by every AccordionSection header (CSS vars pierce scoped styles).
     粘性标题高度 + 堆叠计算的唯一真值来源。被每个 AccordionSection 标题继承
     （CSS 变量可穿透 scoped 样式）。 */
  --sidebar-header-h: 36px;
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.sidebar__scroll-area {
  flex: 1;
  /* `overlay` keeps the scrollbar from shifting layout; `stable` gutter is the
     modern fallback. Plain block flow (no flex) keeps `position: sticky` robust.
     `overlay` 使滚动条不挤压布局；`stable` 槽位是现代浏览器的回退。普通块级流
     （非 flex）让 `position: sticky` 更稳健。 */
  overflow-y: overlay;
  overflow-x: hidden;
  scrollbar-gutter: stable;
}

/* VSCode-style floating scrollbar — invisible until the area is hovered. */
/* VSCode 风格悬浮滚动条——hover 滚动区前不可见。 */
.sidebar__scroll-area::-webkit-scrollbar { width: 6px; background: transparent; }
.sidebar__scroll-area::-webkit-scrollbar-track { background: transparent; }
.sidebar__scroll-area::-webkit-scrollbar-thumb { background: transparent; border-radius: 3px; }
.sidebar__scroll-area:hover::-webkit-scrollbar-thumb { background: var(--color-scrollbar-thumb); }
.sidebar__scroll-area::-webkit-scrollbar-thumb:hover { background: var(--color-scrollbar-thumb-hover); }
</style>
