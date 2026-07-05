<template>
  <!-- T16 收尾:双引擎公共行组件——bucket 与方案 A 曾各持一份逐字节对齐的行模板
       (分隔符 + 卡片),选区/拖拽/FLIP 的引擎无关性依赖两份标记严格等价,人工对齐有
       漂移风险;bucket 转默认引擎后抽为单一来源。DOM 结构与原内联模板完全一致
       (data-item-id 与全部 handlers),两引擎仅 offset-y(段起点 / renderAnchor)与
       row-will-change 不同。本组件不带样式:行根类由宿主 scoped 样式直接命中(子组件
       根节点继承父作用域属性),内部类经宿主 :deep() 命中——样式保持单源。 -->
  <div
    :class="row.rowType === 'separator' ? 'date-separator' : 'media-grid__row'"
    :style="{
      position: 'absolute',
      top: 0,
      transform: `translate3d(0, ${row.y - offsetY}px, 0)`,
      willChange: rowWillChange ? 'transform' : undefined,
      left: 0,
      right: 0,
      height: row.height + 'px',
      gap: row.rowType === 'separator' ? undefined : gap + 'px',
    }"
  >
    <!-- 日期/文件夹分隔符(folder 分组时行内 sticky) -->
    <template v-if="row.rowType === 'separator'">
      <div
        class="separator-content"
        :style="{
          position: groupBy === 'folder' ? 'sticky' : 'static',
          top: 0,
          zIndex: 5,
        }"
      >
        <component
          :is="groupBy === 'folder' ? Folder : Calendar"
          :size="18"
          class="separator-icon"
        />
        <span class="separator-text">{{ row.separatorLabel }}</span>
      </div>
    </template>

    <!-- 正常行卡片 -->
    <template v-else>
      <!-- 有意不用 v-memo(R2-3 删除):嵌套 v-for 下 memo 缓存槽按模板位置分配、被外层各行共享,
           Vue 官方明示其在 v-for 内不生效;且原 deps 不含 item.id,grid 模式同尺寸未出图卡片
           deps 全等时会错误复用他项 vnode(串位隐患)。MediaThumb 子组件 props 浅比较已提供等效跳渲。 -->
      <div
        v-for="item in row.items"
        :key="item.id"
        class="media-card"
        :data-item-id="item.id"
        :class="{
          'media-card--selection-mode': selectionMode,
          'media-card--compact': compactCells,
          'media-card--pending-delete': isPendingDelete(item.id),
        }"
        :style="{ width: item.w + 'px', height: item.h + 'px' }"
        role="button"
        tabindex="0"
        :aria-label="cardAriaLabel(item)"
        :aria-pressed="selectionMode ? isSelected(item.id) : undefined"
        @click="onCardClick(item, $event)"
        @keydown.enter.self.prevent="onCardClick(item, $event)"
        @keydown.space.self.prevent="onCardClick(item, $event)"
        @pointerdown="onCardPointerDown(item.id, $event)"
        @contextmenu.prevent="onCardContextMenu($event, item.id)"
      >
        <!-- 暂存删除标记:置灰 + 「待删除」角标,退出选择模式时统一移除(撤销可恢复)。 -->
        <div v-if="isPendingDelete(item.id)" class="media-card__pending-badge">
          {{ pendingDeleteLabel }}
        </div>
        <MediaThumb
          :id="item.id"
          :item="item"
          :w="item.w"
          :h="item.h"
          :media-type="item.mediaType"
          :is-live-photo="item.isLivePhoto"
          :duration-ms="item.durationMs"
          :thumb-status="item.thumbStatus"
          :thumb-path="item.thumbPath"
          :thumbhash="item.thumbhash"
          :file-format="item.fileFormat"
          :file-size="item.fileSize"
          :similarity="item.similarity"
          :is-favorited="item.isFavorited"
          :rating="item.rating"
          :color-label="item.colorLabel"
          :is-selected="isSelected(item.id)"
          :is-selection-mode="selectionMode"
          :cache-dir="cacheDir"
          @request-thumb="onRequestThumb"
          @cancel-thumb="onCancelThumb"
          @favorite="onFavorite"
          @rate="onRate"
          @select="onSelect(item.id)"
        />
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
// 行为直通:宿主 handler 以**函数 props** 传入(而非 emits 逐层转发)——签名零重复、
// 无中转样板;`onXxx` 名已声明为 props,Vue 不会误作 listener fallthrough。
// 本组件刻意**不带任何样式**:.media-grid__row/.date-separator(行根,继承父作用域
// 属性)由宿主 scoped 样式直接命中;.media-card/.separator-content 等内部类经宿主
// :deep() 命中——样式单源,与抽取前视觉零差异。
import MediaThumb from './MediaThumb.vue'
import { Folder, Calendar } from '@lucide/vue'
import type { LayoutRow, LayoutRowItem } from '../../types/layout'

defineProps<{
  row: LayoutRow
  /** 行 transform 基准:bucket = 所在段起点 seg.start;方案 A = renderAnchor。 */
  offsetY: number
  gap: number
  groupBy: string
  compactCells: boolean
  selectionMode: boolean
  cacheDir: string
  /** 方案 A 行提示合成层(平移模式高频重钉);bucket 行不需要。 */
  rowWillChange?: boolean
  pendingDeleteLabel: string
  isSelected: (id: number) => boolean
  isPendingDelete: (id: number) => boolean
  cardAriaLabel: (item: LayoutRowItem) => string
  onCardClick: (item: LayoutRowItem, ev: MouseEvent | KeyboardEvent) => void
  onCardPointerDown: (id: number, ev: PointerEvent) => void
  onCardContextMenu: (ev: MouseEvent, id: number) => void
  onRequestThumb: (id: number) => void
  onCancelThumb: (id: number) => void
  onFavorite: (id: number) => void
  onRate: (id: number, value: number) => void
  onSelect: (id: number) => void
}>()
</script>
