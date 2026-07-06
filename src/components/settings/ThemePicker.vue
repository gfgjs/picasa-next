<template>
  <!-- 外观与主题选择(S4):外观三态 segmented + 亮/暗两组主题卡片。
       数据一律来自 themes/registry(商店解耦边界),禁止硬编码主题清单;
       钉住区的紧凑外观切换仍由 DynamicSettingControl 'theme' 分支承担。 -->
  <div class="theme-picker">
    <!-- 外观模式:亮 / 暗 / 跟随系统 -->
    <div class="theme-picker__mode" role="radiogroup" :aria-label="$t('settings.theme')">
      <button
        v-for="opt in modeOptions"
        :key="opt.value"
        class="theme-picker__mode-btn"
        :class="{ active: ui.appearance === opt.value }"
        role="radio"
        :aria-checked="ui.appearance === opt.value"
        @click="ui.setAppearance(opt.value)"
      >
        <component :is="opt.icon" :size="14" />
        <span>{{ $t(opt.labelKey) }}</span>
      </button>
    </div>

    <!-- 亮/暗槽位各自的主题卡片组 -->
    <div v-for="group in groups" :key="group.kind" class="theme-picker__group">
      <div class="theme-picker__group-label">
        {{ $t(group.labelKey) }}
        <span v-if="isGroupActive(group.kind)" class="theme-picker__active-tag">
          {{ $t('settings.themeActiveNow') }}
        </span>
      </div>
      <div class="theme-picker__grid">
        <button
          v-for="t in group.themes"
          :key="t.id"
          class="theme-card"
          :class="{ selected: selectedId(group.kind) === t.id }"
          :aria-pressed="selectedId(group.kind) === t.id"
          @click="ui.setThemeForKind(group.kind, t.id)"
        >
          <!-- 四色预览:bg 打底 + surface/text/accent 三枚色片 -->
          <span class="theme-card__swatch" :style="{ backgroundColor: t.preview.bg }">
            <span class="theme-card__chip" :style="{ backgroundColor: t.preview.surface }" />
            <span class="theme-card__chip" :style="{ backgroundColor: t.preview.text }" />
            <span class="theme-card__chip" :style="{ backgroundColor: t.preview.accent }" />
          </span>
          <span class="theme-card__name">
            {{ $t(t.nameKey) }}
            <Check
              v-if="selectedId(group.kind) === t.id"
              :size="13"
              class="theme-card__check"
            />
          </span>
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { Sun, Moon, Monitor, Check } from '@lucide/vue'
import { useUiStore } from '../../stores/uiStore'
import { themesByKind } from '../../themes/registry'
import type { AppearanceMode } from '../../types/ui'

const ui = useUiStore()

const modeOptions: { value: AppearanceMode; icon: typeof Sun; labelKey: string }[] = [
  { value: 'light', icon: Sun, labelKey: 'settings.themeLight' },
  { value: 'dark', icon: Moon, labelKey: 'settings.themeDark' },
  { value: 'system', icon: Monitor, labelKey: 'settings.themeSystem' },
]

// 注册表快照即可(内置主题编译期固定;将来外置主题接入时此处换响应式列表)
const groups: { kind: 'light' | 'dark'; labelKey: string; themes: ReturnType<typeof themesByKind> }[] = [
  { kind: 'light', labelKey: 'settings.lightThemes', themes: themesByKind('light') },
  { kind: 'dark', labelKey: 'settings.darkThemes', themes: themesByKind('dark') },
]

function selectedId(kind: 'light' | 'dark'): string {
  return kind === 'light' ? ui.lightThemeId : ui.darkThemeId
}

/** 当前实际生效的是哪个槽位(system 下随 OS)——组标签挂「当前生效」提示。 */
function isGroupActive(kind: 'light' | 'dark'): boolean {
  return ui.isDark === (kind === 'dark')
}
</script>

<style scoped>
.theme-picker {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-md);
  padding: var(--spacing-sm) var(--spacing-md) var(--spacing-md);
}

/* ── 外观模式 segmented ─────────────────────────────────────── */
.theme-picker__mode {
  display: inline-flex;
  align-self: flex-start;
  gap: 2px;
  padding: 2px;
  background: var(--color-bg-inset);
  border-radius: var(--radius-md);
}

.theme-picker__mode-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 5px 12px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
  cursor: pointer;
  transition:
    background var(--transition-fast),
    color var(--transition-fast);
}

.theme-picker__mode-btn:hover {
  color: var(--color-text-primary);
}

.theme-picker__mode-btn.active {
  background: var(--color-bg-surface);
  color: var(--color-text-primary);
  box-shadow: var(--shadow-sm);
}

/* ── 主题卡片组 ─────────────────────────────────────────────── */
.theme-picker__group {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-sm);
}

.theme-picker__group-label {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}

.theme-picker__active-tag {
  padding: 1px 6px;
  border-radius: var(--radius-full);
  background: var(--color-accent-subtle);
  color: var(--color-sidebar-active-text);
  font-size: var(--font-size-2xs);
}

.theme-picker__grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(112px, 1fr));
  gap: var(--spacing-sm);
}

.theme-card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 6px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-bg-surface);
  cursor: pointer;
  transition:
    border-color var(--transition-fast),
    box-shadow var(--transition-fast);
}

.theme-card:hover {
  /* 暗色主题下 border 加深一档不够醒目,叠一层轻阴影强化「可点」感(§6.3) */
  border-color: var(--color-border-strong);
  box-shadow: var(--shadow-sm);
}

.theme-card:active {
  transform: scale(0.98);
}

.theme-card.selected {
  border-color: var(--color-accent);
  box-shadow: 0 0 0 1px var(--color-accent);
}

/* 预览色块本身是「颜色语义」,允许内联真实 hex(来自注册表 preview,非硬编码违规) */
.theme-card__swatch {
  position: relative;
  height: 44px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--color-border-subtle);
  overflow: hidden;
}

.theme-card__chip {
  position: absolute;
  bottom: 6px;
  width: 12px;
  height: 12px;
  border-radius: var(--radius-full);
  border: 1px solid rgba(128, 128, 128, 0.35);
}

.theme-card__chip:nth-child(1) {
  left: 6px;
}
.theme-card__chip:nth-child(2) {
  left: 22px;
}
.theme-card__chip:nth-child(3) {
  left: 38px;
}

.theme-card__name {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 4px;
  font-size: var(--font-size-xs);
  color: var(--color-text-primary);
}

.theme-card__check {
  color: var(--color-accent);
  flex-shrink: 0;
}
</style>
